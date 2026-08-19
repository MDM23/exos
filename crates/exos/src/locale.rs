//! Which language a request is in, and what the document says it is.
//!
//! The set of languages is generated: [`locales!`](crate::locales) turns a list
//! of tags into a `Locale` enum, and gives each language a module holding
//! exactly the plural categories CLDR gives it. [`LocaleSet`] is the part of
//! that this crate can name, and is what lets [`locale`] answer with a type
//! exos has never seen.
//!
//! ```
//! exos::locales! {
//!     De = "de",
//!     #[fallback]
//!     En = "en",
//! }
//!
//! # fn main() {
//! exos::with_scope(|| {
//!     // Nothing knows any better, so the fallback.
//!     assert_eq!(exos::locale::<Locale>(), Locale::En);
//!
//!     // What an application does once it has resolved who is reading.
//!     exos::scope().set(Locale::De);
//!     assert_eq!(exos::locale::<Locale>(), Locale::De);
//! });
//! # }
//! ```
//!
//! # How a request reaches one
//!
//! In order, first hit wins:
//!
//! 1. A locale in the [request scope](crate::scope), put there by the
//!    application.
//! 2. `Accept-Language`, matched against the declared tags by RFC 4647 lookup.
//! 3. The locale `locales!` marked `#[fallback]`.
//!
//! Step 3 always succeeds, which is why [`locale`] answers with a locale rather
//! than an `Option`: there is no such thing as a request in no language, and a
//! caller made to handle one would only write the fallback out again.
//!
//! A response that reached step 2 says so with `Vary: Accept-Language`, and one
//! the application decided does not, because it did not vary by the header and
//! should not claim to.
//!
//! # The override, and why nothing is persisted
//!
//! A signed-in reader's language lives in their profile. The application
//! already resolves a session name to a viewer once per request, and that is
//! where the override belongs:
//!
//! ```
//! # exos::locales! { De = "de", #[fallback] En = "en" }
//! # fn main() {
//! # struct Viewer { locale: Locale }
//! # let viewer = Viewer { locale: Locale::De };
//! # exos::with_scope(|| {
//! exos::scope().set(viewer.locale);
//! # });
//! # }
//! ```
//!
//! exos writes that nowhere and has no place to write it. It holds a session's
//! name and none of its contents, a copy of a preference drifts from the
//! profile that owns it the moment the reader changes their language on another
//! device, and whether a language cookie needs consent is a question about a
//! jurisdiction rather than about a framework. An application that wants an
//! anonymous language switcher writes one cookie and reads it in step 1, in
//! code it can point at.

use core::sync::atomic::{AtomicBool, Ordering};

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, header},
    middleware::Next,
    response::Response,
};

use crate::{Attributes, IntoAttributes};

mod negotiate;

// -----------------------------------------------------------------------------
//                                THE LOCALE SET
// -----------------------------------------------------------------------------

/// The languages an application declared.
///
/// Implemented by the `Locale` enum [`locales!`](crate::locales) generates and
/// by nothing else, which is what lets [`locale`] hand back a type this crate
/// cannot name. Everything here is also an inherent item on the generated enum,
/// so an application writes `Locale::FALLBACK` and `locale.tag()` without
/// importing anything, and exos reaches the same answers through the bound.
pub trait LocaleSet: Copy + Sealed + Send + Sync + 'static {
    /// Every declared locale, in the order they were declared.
    const ALL: &'static [Self];

    /// The locale a request answers with when nothing better is known about
    /// who is reading.
    const FALLBACK: Self;

    /// The tag this locale was declared with.
    ///
    /// It is what `lang` on the document carries, and what the browser hands to
    /// `Intl`, so the two halves of a page agree on the language by agreeing on
    /// this string.
    fn tag(self) -> &'static str;

    /// Which way this locale's script runs.
    fn direction(self) -> Direction;
}

/// What keeps [`LocaleSet`] implementable by `locales!` alone.
///
/// Re-exported at the crate root so the macro can name it. There is no reason
/// to name it yourself, and a locale set written by hand would be one exos
/// could not add a method to without breaking it.
pub trait Sealed {}

/// Which way a script runs.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Direction {
    /// Latin, Cyrillic, Greek, Han, Devanagari and most of the rest.
    LeftToRight,
    /// Arabic, Hebrew, Thaana, N'Ko, Adlam and the others CLDR marks.
    RightToLeft,
}

/// A CLDR plural category, named the same way in every language.
///
/// A message never branches on this. It branches on the locale's own `Plural`,
/// which holds only the categories that language actually has, so that a
/// missing translation is a non-exhaustive match. This is the shared spelling
/// of the same answer, for the places that cross locales: a category projected
/// into the browser, where `Intl.PluralRules` answers with these very strings,
/// and a test comparing the two.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PluralCategory {
    // CLDR's order, which is the order a language's rules are tried in, rather
    // than alphabetical.
    /// `zero`, which is not simply the count 0. Latvian puts every count
    /// ending in 0 here, and English does not use the category at all.
    Zero,
    /// `one`, the singular where a language has one.
    One,
    /// `two`, the dual, in the handful of languages with one.
    Two,
    /// `few`, the paucal.
    Few,
    /// `many`, which several Slavic languages use for most counts.
    Many,
    /// `other`, the category every language has and most counts fall in.
    Other,
}

impl Direction {
    /// What HTML's `dir` attribute calls this.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LeftToRight => "ltr",
            Self::RightToLeft => "rtl",
        }
    }
}

impl PluralCategory {
    /// The CLDR keyword, which is what `Intl.PluralRules` answers with.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Few => "few",
            Self::Many => "many",
            Self::One => "one",
            Self::Other => "other",
            Self::Two => "two",
            Self::Zero => "zero",
        }
    }
}

// -----------------------------------------------------------------------------
//                                  RESOLUTION
// -----------------------------------------------------------------------------

/// The language this request is in.
///
/// The module docs say where the answer comes from and why it is never an
/// `Option`. The type is the application's, so the call site names it:
///
/// ```
/// # exos::locales! { De = "de", #[fallback] En = "en" }
/// # fn main() {
/// # exos::with_scope(|| {
/// assert_eq!(exos::locale::<Locale>(), Locale::En);
///
/// // Or nothing at all, where the type is already known, which is most places.
/// let locale: Locale = exos::locale();
/// assert_eq!(locale.tag(), "en");
/// # });
/// # }
/// ```
///
/// Resolving happens once per request. The answer is kept in the scope, so a
/// page rendering a hundred messages reads the header once, and an application
/// that overrides the locale still wins, because its own value is looked at
/// first.
///
/// # Panics
///
/// If there is no request, or if the caller is inside a live fragment, exactly
/// as [`scope`](crate::scope) does and for the same reasons. A fragment renders
/// again from whatever publishes it, so a language it read out of the request
/// would be the language of whoever happened to trigger the publish.
#[must_use]
pub fn locale<L: LocaleSet>() -> L {
    let scope = crate::scope();

    // The application's own answer first, because it is the only one that knows
    // who is reading.
    if let Some(chosen) = scope.get::<L>() {
        return *chosen;
    }

    if let Some(resolved) = scope.get::<Resolved<L>>() {
        return resolved.0;
    }

    let resolved = scope
        .get::<Accepted>()
        .and_then(|accepted| accepted.negotiate())
        .unwrap_or(L::FALLBACK);

    scope.set(Resolved(resolved));

    resolved
}

/// What this request asked for, and whether resolution read it.
///
/// Lives in the request [scope](crate::scope), like the session, which is what
/// keeps two requests in flight from answering in each other's language.
#[derive(Debug)]
struct Accepted {
    /// `Accept-Language`, as it arrived. Kept as it came rather than parsed,
    /// since most requests are for an asset and never ask.
    header: Option<HeaderValue>,

    /// Whether a locale was resolved out of the header, which is the question
    /// `Vary` answers. Set even where the header was absent: a request that
    /// carried one would have been answered differently, and that is what a
    /// cache has to be told.
    consulted: AtomicBool,
}

impl Accepted {
    /// The locale this request asked for, if it asked for a declared one.
    fn negotiate<L: LocaleSet>(&self) -> Option<L> {
        self.consulted.store(true, Ordering::Relaxed);

        negotiate::lookup(self.header.as_ref()?.to_str().ok()?)
    }
}

/// The locale this request resolved to, once it has been worked out.
///
/// Its own type rather than the locale itself, so that reading the answer back
/// is never mistaken for the application having said it: the two are looked up
/// in that order, and only one of them means "somebody decided this".
#[derive(Debug)]
struct Resolved<L>(L);

/// Reads the header on the way in and says so on the way out.
///
/// Mounted by [`app`](crate::app) inside the request scope, which it writes to.
/// It awaits nothing of its own: a request that never resolves a locale costs a
/// header lookup and a clone of what it found.
pub(crate) async fn layer(request: Request, next: Next) -> Response {
    let scope = crate::scope();

    scope.set(Accepted {
        header: request.headers().get(header::ACCEPT_LANGUAGE).cloned(),
        consulted: AtomicBool::new(false),
    });

    let accepted = scope
        .get::<Accepted>()
        .expect("it was set on the line above; nothing else writes this type");

    let mut response = next.run(request).await;

    // Appended rather than inserted, because an application varying by
    // something of its own is not saying anything about this header.
    if accepted.consulted.load(Ordering::Relaxed) && !varies(response.headers()) {
        response
            .headers_mut()
            .append(header::VARY, HeaderValue::from_static("Accept-Language"));
    }

    response
}

/// Whether the response already says it varies by the language asked for.
fn varies(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::VARY)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|name| {
            let name = name.trim();

            name == "*" || name.eq_ignore_ascii_case("accept-language")
        })
}

// -----------------------------------------------------------------------------
//                                 THE DOCUMENT
// -----------------------------------------------------------------------------

/// The `lang` and `dir` attributes of a document. See [`lang`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lang {
    tag: &'static str,
    direction: Direction,
}

impl IntoAttributes for Lang {
    fn write(self, attributes: &mut Attributes) {
        attributes.set("lang", self.tag);

        if self.direction == Direction::RightToLeft {
            attributes.set("dir", self.direction.as_str());
        }
    }
}

/// What `<html>` says about the language it was rendered in.
///
/// ```
/// # exos::locales! { #[fallback] En = "en", He = "he" }
/// # fn main() {
/// # exos::with_scope(|| {
/// let locale: Locale = exos::locale();
///
/// let document = exos::view! {
///     <html { exos::lang(locale) }><body></body></html>
/// };
///
/// assert_eq!(document.as_str(), "<html lang=\"en\"><body></body></html>");
/// # });
/// # }
/// ```
///
/// It is not decoration. The browser hands `document.documentElement.lang` to
/// every `Intl` call the runtime makes, so the attribute is the contract
/// between the two halves of a page, and a document that carries the wrong one
/// formats its numbers and dates in the wrong language.
///
/// `dir` comes with it where the script runs right to left, and is left off
/// where it does not, since that is what HTML already means by its absence.
///
/// The locale is passed rather than resolved, so that a document rendered in a
/// language which is not the request's, which is what a language switcher's
/// preview is, still says which one it is in.
#[must_use]
pub fn lang(locale: impl LocaleSet) -> Lang {
    Lang {
        tag: locale.tag(),
        direction: locale.direction(),
    }
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;
    use crate::{detached, with_scope};

    /// A locale set as `locales!` would generate one, written out here so that
    /// what these tests cover is the resolving rather than the macro, and so
    /// that the tags are the awkward ones: a right-to-left script, a language
    /// declared with a region, and one declared with a script.
    ///
    /// Shared with the negotiation tests in the module below, which are the
    /// other half of the same question.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum Tongue {
        Ar,
        De,
        En,
        PtBr,
        ZhHant,
    }

    impl Sealed for Tongue {}

    impl LocaleSet for Tongue {
        const ALL: &'static [Self] = &[Self::Ar, Self::De, Self::En, Self::PtBr, Self::ZhHant];
        const FALLBACK: Self = Self::En;

        fn tag(self) -> &'static str {
            match self {
                Self::Ar => "ar",
                Self::De => "de",
                Self::En => "en",
                Self::PtBr => "pt-BR",
                Self::ZhHant => "zh-Hant",
            }
        }

        fn direction(self) -> Direction {
            match self {
                Self::Ar => Direction::RightToLeft,
                _ => Direction::LeftToRight,
            }
        }
    }

    /// A request that arrived with `header`, as the layer would leave it.
    fn asking(header: Option<&str>) -> Accepted {
        Accepted {
            header: header.map(|header| HeaderValue::from_str(header).expect("a valid header")),
            consulted: AtomicBool::new(false),
        }
    }

    fn headers(vary: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();

        for value in vary {
            headers.append(
                header::VARY,
                HeaderValue::from_str(value).expect("a valid header"),
            );
        }

        headers
    }

    // ---- resolving ----------------------------------------------------------

    #[test]
    fn a_request_that_says_nothing_is_in_the_fallback_language() {
        with_scope(|| assert_eq!(locale::<Tongue>(), Tongue::En));
    }

    #[test]
    fn the_application_is_asked_before_the_browser_is() {
        with_scope(|| {
            crate::scope().set(asking(Some("de")));
            crate::scope().set(Tongue::Ar);

            assert_eq!(locale::<Tongue>(), Tongue::Ar);
        });
    }

    #[test]
    fn what_the_browser_asked_for_is_answered_where_nothing_overrode_it() {
        with_scope(|| {
            crate::scope().set(asking(Some("de-CH, en;q=0.8")));

            assert_eq!(locale::<Tongue>(), Tongue::De);
        });
    }

    #[test]
    fn a_language_this_application_does_not_have_falls_back() {
        with_scope(|| {
            crate::scope().set(asking(Some("fr, ja;q=0.8")));

            assert_eq!(locale::<Tongue>(), Tongue::En);
        });
    }

    /// The header is read once per request however many messages a page has,
    /// and an override still wins afterwards, because the application's own
    /// value is looked at before the answer that was kept.
    #[test]
    fn the_answer_is_kept_and_an_override_still_beats_it() {
        with_scope(|| {
            crate::scope().set(asking(Some("de")));

            assert_eq!(locale::<Tongue>(), Tongue::De);
            assert!(crate::scope().get::<Resolved<Tongue>>().is_some());

            crate::scope().set(Tongue::Ar);
            assert_eq!(locale::<Tongue>(), Tongue::Ar);
        });
    }

    /// Resolution has to have read the header to say the response varies by it.
    #[test]
    fn reading_the_header_is_what_marks_it_as_read() {
        with_scope(|| {
            crate::scope().set(asking(Some("de")));
            let accepted = crate::scope().get::<Accepted>().expect("it was set");

            assert!(!accepted.consulted.load(Ordering::Relaxed));

            let _ = locale::<Tongue>();
            assert!(accepted.consulted.load(Ordering::Relaxed));
        });
    }

    /// A request with no header still varies by it, because one that carried
    /// it would have been answered differently.
    #[test]
    fn a_request_that_sent_no_header_still_consulted_it() {
        with_scope(|| {
            crate::scope().set(asking(None));

            assert_eq!(locale::<Tongue>(), Tongue::En);
            assert!(
                crate::scope()
                    .get::<Accepted>()
                    .expect("it was set")
                    .consulted
                    .load(Ordering::Relaxed)
            );
        });
    }

    #[test]
    fn an_override_never_reads_the_header() {
        with_scope(|| {
            crate::scope().set(asking(Some("de")));
            crate::scope().set(Tongue::Ar);

            let _ = locale::<Tongue>();

            assert!(
                !crate::scope()
                    .get::<Accepted>()
                    .expect("it was set")
                    .consulted
                    .load(Ordering::Relaxed)
            );
        });
    }

    /// A header no browser would send, and which nothing here should panic on.
    #[test]
    fn a_header_that_is_not_text_is_no_answer_rather_than_a_failure() {
        with_scope(|| {
            crate::scope().set(Accepted {
                header: Some(HeaderValue::from_bytes(&[0xff, 0xfe]).expect("a valid header")),
                consulted: AtomicBool::new(false),
            });

            assert_eq!(locale::<Tongue>(), Tongue::En);
        });
    }

    #[test]
    #[should_panic(expected = "no request scope")]
    fn asking_outside_a_request_is_a_panic() {
        let _ = locale::<Tongue>();
    }

    /// Until the locale is part of a topic, a fragment reading it would render
    /// in the language of whoever triggered the publish.
    #[test]
    #[should_panic(expected = "a live fragment cannot read the request scope")]
    fn a_live_fragment_cannot_reach_it() {
        with_scope(|| detached(locale::<Tongue>));
    }

    // ---- what the response says ---------------------------------------------

    #[test]
    fn a_response_that_already_varies_by_the_header_is_left_alone() {
        assert!(varies(&headers(&["Accept-Language"])));
        assert!(varies(&headers(&["accept-language"])));
        assert!(varies(&headers(&["Cookie, Accept-Language"])));
        assert!(varies(&headers(&["Cookie", "Accept-Language"])));
        assert!(varies(&headers(&["*"])));
    }

    #[test]
    fn varying_by_something_else_says_nothing_about_this_header() {
        assert!(!varies(&HeaderMap::new()));
        assert!(!varies(&headers(&["Cookie"])));
        assert!(!varies(&headers(&["Accept-Language-Other"])));
    }

    // ---- the document -------------------------------------------------------

    #[test]
    fn a_document_carries_the_tag_it_was_rendered_in() {
        let mut attributes = Attributes::new();
        lang(Tongue::PtBr).write(&mut attributes);

        assert_eq!(attributes.render(), " lang=\"pt-BR\"");
    }

    /// `dir` where the script needs it, and nowhere else: absent already means
    /// left to right, and an attribute that says what the default says is one
    /// more thing to keep in step.
    #[test]
    fn only_a_right_to_left_script_says_which_way_it_runs() {
        let mut attributes = Attributes::new();
        lang(Tongue::Ar).write(&mut attributes);

        assert_eq!(attributes.render(), " lang=\"ar\" dir=\"rtl\"");

        let mut attributes = Attributes::new();
        lang(Tongue::De).write(&mut attributes);

        assert!(!attributes.render().contains("dir"));
    }

    #[test]
    fn a_direction_is_spelled_the_way_the_attribute_is() {
        assert_eq!(Direction::LeftToRight.as_str(), "ltr");
        assert_eq!(Direction::RightToLeft.as_str(), "rtl");
    }

    #[test]
    fn a_category_is_spelled_the_way_cldr_spells_it() {
        assert_eq!(PluralCategory::Zero.as_str(), "zero");
        assert_eq!(PluralCategory::One.as_str(), "one");
        assert_eq!(PluralCategory::Two.as_str(), "two");
        assert_eq!(PluralCategory::Few.as_str(), "few");
        assert_eq!(PluralCategory::Many.as_str(), "many");
        assert_eq!(PluralCategory::Other.as_str(), "other");
    }
}
