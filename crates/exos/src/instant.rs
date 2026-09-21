//! A point in time, and the three ways a page reads one.
//!
//! The server sends the instant and the browser formats it. That is not a
//! division of labour anybody chose: a zone is a fact about the reader, it
//! reaches the server only after the first render, and the first render is the
//! one that matters. The browser has the whole IANA database and it has ICU,
//! so the half holding the fact does the work.
//!
//! ```
//! # use exos::{Instant, Markup, When, view};
//! # fn row(at: Instant) -> Markup {
//! view! {
//!     <td>{ When::date(at) }</td>
//!     <td>"Posted " { When::ago(at) }</td>
//! }
//! # }
//! ```
//!
//! [`When`] is the whole `<time>` element, since there is only one way to
//! write it and every call site would otherwise write that way by hand. The
//! element's own text is the instant itself, so a crawler and a reader without
//! the runtime see something true rather than something wrong.
//!
//! # A time that belongs to a place
//!
//! Not every time is read in the reader's zone: a kickoff is at the venue, and
//! every viewer of that fixture list has to see the same clock time. That is
//! not a second mechanism, only a zone travelling as the string an application
//! already has, so [`When::zone`] takes one and nothing else has to.

use core::{
    fmt::{self, Display},
    str::FromStr,
};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Attributes, Js, Render, attributes::BindKind, quote_js, render::AttributeValue, text};

/// Milliseconds in a day, which is the only unit conversion here that is not a
/// multiple of sixty.
const DAY: i64 = 86_400_000;

// -----------------------------------------------------------------------------
//                                  THE INSTANT
// -----------------------------------------------------------------------------

/// A moment, as milliseconds since the Unix epoch.
///
/// Deliberately not a calendar type. exos never does arithmetic on a date and
/// never formats one, so what it needs is the one representation both sides
/// agree on: UTC, written and read as RFC 3339. An application that computes
/// with dates keeps doing that in whichever crate it already uses and converts
/// at the edge.
///
/// ```
/// # use exos::Instant;
/// let at: Instant = "2026-08-19T09:00:00Z".parse().expect("a valid instant");
///
/// assert_eq!(at.millis(), 1_787_130_000_000);
/// assert_eq!(at.to_string(), "2026-08-19T09:00:00Z");
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Instant(i64);

impl Instant {
    /// The instant this many milliseconds after the epoch.
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// Milliseconds since the epoch, for handing to whichever date crate the
    /// application keeps.
    #[must_use]
    pub const fn millis(self) -> i64 {
        self.0
    }

    /// Now, by the system clock.
    ///
    /// # Panics
    ///
    /// If the system clock is further than about 292 million years from the
    /// epoch, which is not a clock this crate has anything useful to say to.
    #[must_use]
    pub fn now() -> Self {
        Self::from(SystemTime::now())
    }
}

impl From<SystemTime> for Instant {
    fn from(time: SystemTime) -> Self {
        let millis = match time.duration_since(UNIX_EPOCH) {
            Ok(since) => i64::try_from(since.as_millis()).unwrap_or(i64::MAX),
            Err(before) => -i64::try_from(before.duration().as_millis()).unwrap_or(i64::MAX),
        };

        Self(millis)
    }
}

impl Display for Instant {
    /// RFC 3339, in UTC, with the milliseconds left off where there are none.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day) = civil(self.0.div_euclid(DAY));
        let clock = self.0.rem_euclid(DAY);
        let (seconds, millis) = (clock / 1000, clock % 1000);

        write!(
            formatter,
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
        )?;

        if millis != 0 {
            write!(formatter, ".{millis:03}")?;
        }

        formatter.write_str("Z")
    }
}

/// What a string that is not an instant is answered with.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("expected an RFC 3339 instant, such as `2026-08-19T09:00:00Z`")]
pub struct NotAnInstant;

impl FromStr for Instant {
    type Err = NotAnInstant;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse(text).ok_or(NotAnInstant)
    }
}

/// Every shape a `datetime-local` control and an ISO timestamp arrive in: a
/// date, a clock with or without seconds and fraction, and an offset that is
/// `Z`, absent, or written out.
fn parse(text: &str) -> Option<Instant> {
    let (date, rest) = text.split_once(['T', 't', ' '])?;

    let mut fields = date.splitn(3, '-');
    let year: i64 = fields.next()?.parse().ok()?;
    let month: i64 = fields.next()?.parse().ok()?;
    let day: i64 = fields.next()?.parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let (clock, offset) = split_offset(rest)?;

    let mut fields = clock.splitn(3, ':');
    let hour: i64 = fields.next()?.parse().ok()?;
    let minute: i64 = fields.next()?.parse().ok()?;

    let written = fields.next().unwrap_or("0");

    let (second, millis): (i64, i64) = match written.split_once('.') {
        Some((second, fraction)) => (second.parse().ok()?, fraction_millis(fraction)?),
        None => (written.parse().ok()?, 0),
    };

    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let seconds = hour * 3600 + minute * 60 + second;

    Some(Instant(
        days(year, month, day) * DAY + seconds * 1000 + millis - offset,
    ))
}

/// The clock and how far ahead of UTC it was written, in milliseconds.
///
/// A naive wall clock, which is what `datetime-local` sends, is read as UTC.
/// The browser corrects one in the reader's zone before it ever reaches here;
/// what is left is a value typed into a plain text field or written by hand.
fn split_offset(rest: &str) -> Option<(&str, i64)> {
    if let Some(clock) = rest.strip_suffix(['Z', 'z']) {
        return Some((clock, 0));
    }

    let Some(sign) = rest.rfind(['+', '-']) else {
        return Some((rest, 0));
    };

    let (clock, offset) = rest.split_at(sign);
    let (hours, minutes) = offset[1..].split_once(':')?;
    let ahead = hours.parse::<i64>().ok()? * 3_600_000 + minutes.parse::<i64>().ok()? * 60_000;

    Some((
        clock,
        if offset.starts_with('-') {
            -ahead
        } else {
            ahead
        },
    ))
}

/// A fractional second as milliseconds, truncated rather than rounded: a
/// timestamp carrying microseconds is not claiming the next millisecond.
fn fraction_millis(fraction: &str) -> Option<i64> {
    if fraction.is_empty() || !fraction.bytes().all(|digit| digit.is_ascii_digit()) {
        return None;
    }

    let mut millis = 0;

    for place in 0..3 {
        millis =
            millis * 10 + i64::from(fraction.as_bytes().get(place).copied().unwrap_or(b'0') - b'0');
    }

    Some(millis)
}

/// Days from the epoch to a civil date, and back.
///
/// Howard Hinnant's algorithm, which is the shortest correct one and is here
/// rather than in a dependency because it is the only calendar arithmetic this
/// crate does. It shifts the year to start in March, so the leap day is the
/// last day of it and every other month keeps a fixed length.
fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = of_era * 365 + of_era / 4 - of_era / 100 + of_year;

    era * 146_097 + day_of_era - 719_468
}

fn civil(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days - era * 146_097;
    let of_year = (of_era - of_era / 1460 + of_era / 36524 - of_era / 146_096) / 365;
    let year = of_year + era * 400;
    let day_of_year = of_era - (365 * of_year + of_year / 4 - of_year / 100);
    let month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month + 2) / 5 + 1;

    (
        year + i64::from(month >= 10),
        month + if month < 10 { 3 } else { -9 },
        day,
    )
}

// -----------------------------------------------------------------------------
//                                  ON THE PAGE
// -----------------------------------------------------------------------------

/// A time on a page, which is the `<time>` element and what fills it.
///
/// There is one way to write that element and every call site would write the
/// same way, so this is the element rather than the expression that goes on
/// one:
///
/// ```
/// # use exos::{Instant, Markup, When, view};
/// # fn row(at: Instant) -> Markup {
/// view! { <p>"Due " { When::date(at) }</p> }
/// # }
/// ```
///
/// which renders the instant as its own text, with the binding that rewrites
/// it in the reader's language:
///
/// ```html
/// <p>Due <time datetime="2026-08-19T09:00:00Z"
///              data-text="date(&quot;2026-08-19T09:00:00Z&quot;)">2026-08-19T09:00:00Z</time></p>
/// ```
///
/// For a date somewhere other than in an element of its own, a `title` or an
/// `aria-label`, [`to_js`](When::to_js) hands over the expression alone.
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use = "a time does nothing until it is rendered into a template"]
pub struct When {
    at: Instant,
    /// Which of the runtime's three helpers writes it.
    helper: &'static str,
    /// Left out where the helper's own default is wanted, which is the medium
    /// date, the short time and the long relative form.
    style: Option<&'static str>,
    zone: Option<String>,
}

impl When {
    /// The date, written the way this reader's language writes one.
    pub const fn date(at: Instant) -> Self {
        Self::with(at, "date")
    }

    /// The time of day.
    pub const fn time(at: Instant) -> Self {
        Self::with(at, "time")
    }

    /// How long ago it was, kept current while the page is open.
    pub const fn ago(at: Instant) -> Self {
        Self::with(at, "ago")
    }

    /// A time for a message to read, which says for itself which of the three
    /// ways it reads one.
    ///
    /// ```
    /// # exos::locales! { #[fallback] En = "en" }
    /// # exos::messages! { kickoff_at(when: Time) { En = "Kickoff at {when}" } }
    /// # use exos::{Instant, When};
    /// # fn main() { exos::with_scope(|| {
    /// # exos::scope().set(Locale::En);
    /// let at = Instant::from_millis(1_787_130_000_000);
    /// let said = kickoff_at(When::of(at).zone("Europe/Berlin"));
    ///
    /// assert!(said.source().contains("time(\"2026-08-19T09:00:00Z\""));
    /// # }); }
    /// ```
    ///
    /// Rendered on its own it is the date, since something has to be, and a
    /// call site that means one says [`date`](When::date).
    pub const fn of(at: Instant) -> Self {
        Self::date(at)
    }

    /// Which helper reads it, imposed by a message's own declaration.
    ///
    /// Called by the `messages!` expansion, which is where `Date`, `Time` and
    /// `Ago` are written down. A sentence reads "due on" or "posted" whoever
    /// calls it, so the declaration wins over whatever the call site built.
    #[doc(hidden)]
    pub const fn read_as(mut self, helper: &'static str) -> Self {
        self.helper = helper;
        self
    }

    /// The zone the instant belongs to, where that is not the reader's own.
    ///
    /// A difference between two instants is the same difference in every zone,
    /// so [`ago`](When::ago) ignores one.
    pub fn zone(mut self, zone: impl Into<String>) -> Self {
        self.zone = Some(zone.into());
        self
    }

    /// The shortest form the language has.
    pub const fn short(mut self) -> Self {
        self.style = Some("short");
        self
    }

    /// The longest form that is still a sentence rather than a table cell.
    pub const fn long(mut self) -> Self {
        self.style = Some("long");
        self
    }

    /// The expression alone, for the places that are not an element of their
    /// own.
    ///
    /// ```
    /// # use exos::{Instant, When};
    /// let at = Instant::from_millis(1_787_130_000_000);
    ///
    /// assert_eq!(
    ///     When::date(at).to_js().source(),
    ///     "date(\"2026-08-19T09:00:00Z\")",
    /// );
    /// ```
    pub fn to_js(&self) -> Js<String> {
        let at = quote_js(&self.at.to_string());
        let helper = self.helper;

        let style = match self.style {
            Some(style) => quote_js(style),
            None if self.zone.is_some() => String::from("undefined"),
            None => return Js::raw(format!("{helper}({at})")),
        };

        Js::raw(match &self.zone {
            Some(zone) => format!("{helper}({at}, {style}, {})", quote_js(zone)),
            None => format!("{helper}({at}, {style})"),
        })
    }

    const fn with(at: Instant, helper: &'static str) -> Self {
        Self {
            at,
            helper,
            style: None,
            zone: None,
        }
    }
}

/// So a message that reads a time takes the instant itself where nothing has
/// to be said about how it is written, which is most call sites.
impl From<Instant> for When {
    fn from(at: Instant) -> Self {
        Self::of(at)
    }
}

impl Render for When {
    fn render_to(&self, out: &mut String) {
        let mut attributes = Attributes::new();
        attributes.set("datetime", self.at.to_string());
        crate::IntoAttributes::write(text(self.to_js()), &mut attributes);

        out.push_str("<time");
        out.push_str(&attributes.render());
        out.push('>');
        self.at.render_to(out);
        out.push_str("</time>");
    }
}

// -----------------------------------------------------------------------------
//                                 THE PLUMBING
// -----------------------------------------------------------------------------

impl Render for Instant {
    fn render_to(&self, out: &mut String) {
        out.push_str(&self.to_string());
    }
}

impl AttributeValue for Instant {
    type Output<'value> = &'value Self;

    fn attribute_value(&self) -> Option<&Self> {
        Some(self)
    }
}

/// A control writing an instant hands back a wall clock, which only the
/// browser can turn into one.
impl BindKind for Instant {
    const KIND: &'static str = "instant";
}

impl Serialize for Instant {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Instant {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instant_is_written_and_read_as_the_same_utc_string() {
        let text = "2026-08-19T09:00:00Z";
        let at: Instant = text.parse().expect("a valid instant");

        assert_eq!(at.to_string(), text);
        assert_eq!(at.millis(), 1_787_130_000_000);
    }

    #[test]
    fn the_epoch_and_the_dates_around_it_survive_the_round_trip() {
        for text in [
            "1970-01-01T00:00:00Z",
            "1969-12-31T23:59:59Z",
            "2000-02-29T12:00:00Z",
            "2024-02-29T23:59:59.999Z",
            "2100-03-01T00:00:00Z",
        ] {
            let at: Instant = text.parse().expect(text);
            assert_eq!(at.to_string(), text);
        }
    }

    /// A `datetime-local` control sends no offset and no seconds, and a value
    /// typed by hand may carry either.
    #[test]
    fn a_wall_clock_without_an_offset_is_read_as_utc() {
        assert_eq!(
            "2026-08-19T09:00".parse::<Instant>().expect("valid"),
            "2026-08-19T09:00:00Z".parse::<Instant>().expect("valid"),
        );
    }

    #[test]
    fn an_offset_is_taken_off_on_the_way_in() {
        let berlin: Instant = "2026-08-19T11:00:00+02:00".parse().expect("valid");

        assert_eq!(berlin.to_string(), "2026-08-19T09:00:00Z");

        let behind: Instant = "2026-08-19T05:00:00-04:00".parse().expect("valid");

        assert_eq!(behind.to_string(), "2026-08-19T09:00:00Z");
    }

    /// Truncated rather than rounded: a timestamp carrying microseconds is not
    /// claiming the next millisecond.
    #[test]
    fn a_fraction_longer_than_milliseconds_is_cut_rather_than_rounded() {
        let at: Instant = "2026-08-19T09:00:00.123789Z".parse().expect("valid");

        assert_eq!(at.to_string(), "2026-08-19T09:00:00.123Z");
    }

    #[test]
    fn what_is_not_an_instant_is_refused() {
        for text in [
            "",
            "2026-08-19",
            "2026-13-01T00:00:00Z",
            "2026-08-19T24:00:00Z",
            "2026-08-19T09:60:00Z",
            "yesterday",
        ] {
            assert!(text.parse::<Instant>().is_err(), "{text}");
        }
    }

    /// The expression names the helper and carries the instant as a literal,
    /// so the element reading it needs nothing else on it.
    #[test]
    fn a_zone_travels_as_the_string_the_application_already_has() {
        let at = Instant::from_millis(1_787_130_000_000);

        assert_eq!(
            When::time(at).to_js().source(),
            "time(\"2026-08-19T09:00:00Z\")",
        );
        assert_eq!(
            When::time(at).zone("Europe/Berlin").to_js().source(),
            "time(\"2026-08-19T09:00:00Z\", undefined, \"Europe/Berlin\")",
        );
        assert_eq!(
            When::ago(at).short().to_js().source(),
            "ago(\"2026-08-19T09:00:00Z\", \"short\")",
        );
    }

    /// One call and one element, because there is only one way to write that
    /// element: the instant as its own text, and the binding that rewrites it.
    #[test]
    fn a_time_renders_as_the_element_that_carries_it() {
        let at = Instant::from_millis(1_787_130_000_000);

        assert_eq!(
            When::date(at).render().as_str(),
            "<time datetime=\"2026-08-19T09:00:00Z\" \
             data-text=\"date(&quot;2026-08-19T09:00:00Z&quot;)\">2026-08-19T09:00:00Z</time>",
        );
    }

    /// A zone reaches the expression as a quoted string, escaped on the way
    /// into the attribute like everything else a template writes.
    #[test]
    fn a_zone_that_is_not_a_zone_cannot_leave_the_attribute() {
        let at = Instant::from_millis(0);
        let rendered = When::time(at).zone("\" onload=\"x").render();

        assert!(!rendered.as_str().contains("onload=\"x"), "{rendered}");
    }

    #[test]
    fn an_instant_goes_over_the_wire_as_what_it_prints() {
        let at = Instant::from_millis(1_787_130_000_000);
        let written = serde_json::to_string(&at).expect("serializes");

        assert_eq!(written, "\"2026-08-19T09:00:00Z\"");
        assert_eq!(
            serde_json::from_str::<Instant>(&written).expect("round trips"),
            at,
        );
    }
}
