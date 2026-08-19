//! What every locale set has in common.
//!
//! The set itself is generated: [`locales!`](crate::locales) turns a list of
//! tags into a `Locale` enum, and gives each language a module holding exactly
//! the plural categories CLDR gives it. The two types here are what those
//! answers are spelled in, and they live in the framework rather than in the
//! generated code because they are the same in every application. A direction
//! is one of two things, and CLDR names six categories however many of them a
//! given language uses.
//!
//! ```
//! # use exos::{Direction, PluralCategory};
//! assert_eq!(Direction::RightToLeft.as_str(), "rtl");
//! assert_eq!(PluralCategory::Other.as_str(), "other");
//! ```

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
