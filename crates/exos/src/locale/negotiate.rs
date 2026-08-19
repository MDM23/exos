//! Reading `Accept-Language`, and matching it against what was declared.
//!
//! RFC 4647 lookup, which is the scheme that answers with one tag rather than
//! with a list: each range the browser sent is tried whole and then with its
//! subtags dropped one at a time, and the first declared locale that comes up
//! wins. `de-CH` therefore reaches an application that declared `de`, and `de`
//! does not reach one that declared only `de-AT`, because a range is only ever
//! shortened. An application that wants to answer a bare `de` declares it.

use crate::LocaleSet;

/// The quality of a range nothing said anything about, in thousandths.
///
/// Whole numbers rather than a float because `q` carries three decimal places
/// at most, and because sorting is the one thing floats are awkward at.
const FULL: u16 = 1000;

/// The declared locale `header` asks for, if it asks for one that exists.
///
/// `None` where nothing in it names a declared locale, which is the caller's
/// cue to fall back. A malformed entry is dropped rather than failing the
/// header, since the ranges around it are still perfectly good answers.
pub(crate) fn lookup<L: LocaleSet>(header: &str) -> Option<L> {
    let mut ranges: Vec<(&str, u16)> = header.split(',').filter_map(entry).collect();

    // Stable, so ranges of equal quality keep the order they were sent in,
    // which is the order the browser meant them in.
    ranges.sort_by_key(|(_, quality)| core::cmp::Reverse(*quality));

    ranges.into_iter().find_map(|(range, _)| matching(range))
}

/// One entry of the header: the range it names and how much it is wanted.
///
/// `None` for an entry with no range, an unreadable `q`, or `q=0`, which is the
/// spelling for "not this one" and is therefore not a preference to try.
fn entry(entry: &str) -> Option<(&str, u16)> {
    let mut parts = entry.split(';');
    let range = parts.next()?.trim();

    if range.is_empty() {
        return None;
    }

    let mut quality = FULL;

    for parameter in parts {
        if let Some(value) = weight(parameter.trim()) {
            quality = self::quality(value)?;
        }
    }

    (quality > 0).then_some((range, quality))
}

/// The value of a `q=` parameter, if that is what this parameter is.
fn weight(parameter: &str) -> Option<&str> {
    let (name, value) = parameter.split_once('=')?;

    name.trim().eq_ignore_ascii_case("q").then(|| value.trim())
}

/// A `q` value in thousandths, which is all the precision HTTP allows.
fn quality(text: &str) -> Option<u16> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));

    let whole: u16 = match whole {
        "" => 0,
        digits => digits.parse().ok()?,
    };

    if whole > 1 || fraction.len() > 3 || !fraction.bytes().all(|digit| digit.is_ascii_digit()) {
        return None;
    }

    let mut thousandths = whole * FULL;
    let mut place = FULL / 10;

    for digit in fraction.bytes() {
        thousandths += u16::from(digit - b'0') * place;
        place /= 10;
    }

    (thousandths <= FULL).then_some(thousandths)
}

/// The locale `range` names, dropping subtags from it until one is declared.
fn matching<L: LocaleSet>(range: &str) -> Option<L> {
    // A reader who accepts anything is asking for whatever this application
    // would have answered with anyway.
    if range == "*" {
        return Some(L::FALLBACK);
    }

    let mut range = range;

    loop {
        if let Some(locale) = declared(range) {
            return Some(locale);
        }

        range = shorter(range)?;
    }
}

/// The locale declared with exactly this tag.
fn declared<L: LocaleSet>(tag: &str) -> Option<L> {
    L::ALL
        .iter()
        .copied()
        .find(|locale| locale.tag().eq_ignore_ascii_case(tag))
}

/// `range` with its last subtag dropped, or `None` where there is nothing left
/// to drop.
fn shorter(range: &str) -> Option<&str> {
    let (head, _) = range.rsplit_once('-')?;

    match head.rsplit_once('-') {
        // A singleton goes with the subtag that followed it: `zh-Hant-CN-x` is
        // a range nothing can be declared as, so trying it would be a step
        // spent on nothing.
        Some((rest, singleton)) if singleton.len() == 1 => Some(rest),
        // The whole range was private use, and none of it is a language.
        _ if head.len() == 1 => None,
        _ => Some(head),
    }
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    // The set lives with the tests of the module above, because both halves of
    // resolution are checked against the same declaration.
    use crate::locale::tests::Tongue;

    fn asked(header: &str) -> Option<Tongue> {
        lookup(header)
    }

    #[test]
    fn a_declared_tag_is_matched_whole() {
        assert_eq!(asked("de"), Some(Tongue::De));
        assert_eq!(asked("pt-BR"), Some(Tongue::PtBr));
        assert_eq!(asked("zh-Hant"), Some(Tongue::ZhHant));
    }

    #[test]
    fn a_tag_is_matched_however_it_is_spelled() {
        assert_eq!(asked("DE"), Some(Tongue::De));
        assert_eq!(asked("pt-br"), Some(Tongue::PtBr));
        assert_eq!(asked("ZH-hant"), Some(Tongue::ZhHant));
    }

    /// The whole point of lookup over an exact match: a browser asks for the
    /// region it is in, and the application declared the language.
    #[test]
    fn a_range_is_shortened_until_something_is_declared() {
        assert_eq!(asked("de-CH"), Some(Tongue::De));
        assert_eq!(asked("de-CH-1996"), Some(Tongue::De));
        assert_eq!(asked("zh-Hant-TW"), Some(Tongue::ZhHant));
    }

    /// And the direction it does not go, which is worth pinning because it is
    /// the surprise: `pt` is not `pt-BR`, and an application that wants to
    /// answer a bare `pt` declares it.
    #[test]
    fn a_range_is_never_lengthened() {
        assert_eq!(asked("pt"), None);
        assert_eq!(asked("zh"), None);
    }

    #[test]
    fn a_language_that_was_not_declared_is_no_answer_at_all() {
        assert_eq!(asked("fr"), None);
        assert_eq!(asked("fr-CA, ja"), None);
        assert_eq!(asked(""), None);
    }

    #[test]
    fn the_most_wanted_range_wins_whatever_order_it_arrived_in() {
        assert_eq!(asked("fr;q=0.9, de;q=0.8, en;q=0.7"), Some(Tongue::De));
        assert_eq!(asked("de;q=0.3, en;q=0.9"), Some(Tongue::En));
        assert_eq!(asked("de;q=0.30, en;q=0.301"), Some(Tongue::En));
    }

    /// A browser writes its first choice without a `q`, and it outranks every
    /// entry that carries one.
    #[test]
    fn a_range_with_no_quality_is_wanted_most() {
        assert_eq!(asked("de, en;q=0.9"), Some(Tongue::De));
        assert_eq!(asked("en;q=0.9, de"), Some(Tongue::De));
    }

    #[test]
    fn ranges_of_equal_quality_are_tried_in_the_order_they_were_sent() {
        assert_eq!(asked("de;q=0.8, en;q=0.8"), Some(Tongue::De));
        assert_eq!(asked("en;q=0.8, de;q=0.8"), Some(Tongue::En));
        assert_eq!(asked("de, en"), Some(Tongue::De));
    }

    /// `q=0` is how a browser says it does not want a language, so it is not a
    /// preference to be tried.
    #[test]
    fn a_refused_range_is_not_an_answer() {
        assert_eq!(asked("de;q=0"), None);
        assert_eq!(asked("de;q=0.000, en;q=0.1"), Some(Tongue::En));
    }

    #[test]
    fn a_wildcard_is_whatever_this_application_would_have_answered() {
        assert_eq!(asked("*"), Some(Tongue::FALLBACK));
        assert_eq!(asked("fr, *"), Some(Tongue::FALLBACK));
        // Only once the ranges that name something have been tried.
        assert_eq!(asked("de, *"), Some(Tongue::De));
        assert_eq!(asked("*;q=0.5, de;q=0.9"), Some(Tongue::De));
    }

    #[test]
    fn whitespace_around_the_parts_is_ignored() {
        assert_eq!(asked(" fr ; q=0.9 , de ; q=0.8 "), Some(Tongue::De));
        assert_eq!(asked("fr;Q=0.1,de;q=0.2"), Some(Tongue::De));
    }

    /// A header nobody can read is not a reason to answer nothing: the entries
    /// around the broken one still say what the reader wants.
    #[test]
    fn an_entry_that_cannot_be_read_is_dropped_rather_than_the_header() {
        assert_eq!(asked("de;q=x, en"), Some(Tongue::En));
        assert_eq!(asked("de;q=1.5, en"), Some(Tongue::En));
        assert_eq!(asked("de;q=0.1234, en;q=0.1"), Some(Tongue::En));
        assert_eq!(asked(",,de"), Some(Tongue::De));
    }

    /// Extensions and private use are dropped with the singleton that
    /// introduces them, which is what RFC 4647 asks for and what keeps a step
    /// from being spent on a range nothing can be declared as.
    #[test]
    fn a_singleton_goes_with_what_follows_it() {
        assert_eq!(shorter("zh-Hant-CN-x-private"), Some("zh-Hant-CN"));
        assert_eq!(shorter("de-CH-1996"), Some("de-CH"));
        assert_eq!(shorter("de-CH"), Some("de"));
        assert_eq!(shorter("de"), None);
        assert_eq!(shorter("x-private"), None);

        assert_eq!(asked("zh-Hant-CN-x-private"), Some(Tongue::ZhHant));
        assert_eq!(asked("x-private"), None);
    }

    #[test]
    fn a_quality_is_read_to_the_thousandth() {
        assert_eq!(quality("1"), Some(1000));
        assert_eq!(quality("1.0"), Some(1000));
        assert_eq!(quality("1.000"), Some(1000));
        assert_eq!(quality("0"), Some(0));
        assert_eq!(quality("0.5"), Some(500));
        assert_eq!(quality(".5"), Some(500));
        assert_eq!(quality("0.001"), Some(1));
        assert_eq!(quality("0.85"), Some(850));

        assert_eq!(quality("1.001"), None, "above one is not a quality");
        assert_eq!(quality("2"), None);
        assert_eq!(quality("0.0001"), None, "and neither is a fourth place");
        assert_eq!(quality("high"), None);
        assert_eq!(quality("-1"), None);
    }
}
