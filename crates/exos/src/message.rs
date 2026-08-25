//! Text defined in Rust, in every language an application declares.
//!
//! [`messages!`](crate::messages) is the macro, and it writes almost all of
//! this: a function per message, a `match` over the locale, and inside each of
//! its arms a `match` over whatever the message branches on. What lives here
//! are the two domains that generated code is generic over. A count is any
//! whole number, and [`Count::magnitude`] is what a plural rule asks about. An
//! [`Enumerable`] domain is what makes an application's own enum legal in an
//! arm.
//!
//! ```
//! exos::locales! {
//!     De = "de",
//!     #[fallback]
//!     En = "en",
//! }
//!
//! exos::messages! {
//!     clear_selection {
//!         De = "Auswahl aufheben",
//!         En = "Clear selection",
//!     }
//!
//!     items_selected(count: Plural) {
//!         De { One } = "{count} Element ausgewählt",
//!         De { _ }   = "{count} Elemente ausgewählt",
//!         En { One } = "{count} item selected",
//!         En { _ }   = "{count} items selected",
//!     }
//!
//!     accept_terms(terms: Slot) {
//!         De = "Bitte die {terms}Nutzungsbedingungen{/terms} annehmen.",
//!         En = "Please accept the {terms}terms of service{/terms}.",
//!     }
//! }
//!
//! # fn main() {
//! exos::with_scope(|| {
//!     exos::scope().set(Locale::De);
//!
//!     assert_eq!(clear_selection(), "Auswahl aufheben");
//!     assert_eq!(items_selected(1), "1 Element ausgewählt");
//!     assert_eq!(items_selected(3), "3 Elemente ausgewählt");
//!
//!     let accepted = accept_terms(|inner| exos::view! {
//!         <a href="/terms">{ inner }</a>
//!     });
//!
//!     assert_eq!(
//!         accepted.as_str(),
//!         "Bitte die <a href=\"/terms\">Nutzungsbedingungen</a> annehmen.",
//!     );
//! });
//! # }
//! ```
//!
//! # Slots
//!
//! A sentence with a link in it cannot be composed from two messages: the link
//! lands somewhere else in the next language, and a translator handed half a
//! sentence has been handed something they cannot check. A slot keeps the
//! sentence whole, including the words inside the link, and asks the call site
//! only for the wrapper, which is an `FnOnce(Markup) -> Markup`.
//!
//! A message with a slot in it answers with [`Markup`](crate::Markup) rather
//! than a `String`. Its own words are escaped while this crate compiles and an
//! interpolated value is escaped where it is written, so the only structure a
//! translation can carry is a slot that was declared in Rust.
//!
//! # A count the browser has
//!
//! The argument decides where the message is resolved. Handed a number, it
//! answers with the sentence; handed a [`Js`](crate::Js) expression, it answers
//! with one of its own, and the sentence is picked in the browser:
//!
//! ```ignore
//! items_selected(3)                    // String,     resolved here
//! items_selected(picked.get().len())   // Js<String>, resolved there
//! ```
//!
//! What crosses is the variants of that one message in the locale the request
//! resolved to, and the language's tag. `Intl.PluralRules` picks between them
//! and `Intl.NumberFormat` writes the number, which is the agreement the
//! [fixture](../../../crates/exos-cldr/fixture.json) holds both sides to.
//!
//! Only a count crosses. A message with more than one, and a message with a
//! slot, keep the signature they have: the first has no single answer type to
//! give and the second would have to build nodes rather than text.

use core::fmt::Display;
use std::cell::RefCell;

use serde_json::{Map, Value};

use crate::{Js, Symbols, fnv::Fnv1a, locale::PluralCategory};

// -----------------------------------------------------------------------------
//                                 THE DOMAINS
// -----------------------------------------------------------------------------

/// What keeps [`Count`] implementable by the numbers listed here.
mod sealed {
    /// Named nowhere else, which is the whole of what it does.
    pub trait Count {}
}

/// A whole number a message can count.
///
/// Every integer type is one, so a call site hands over whatever it already
/// has: a `usize` from `len()`, an `i32` out of a row, a literal.
///
/// ```
/// # exos::locales! { #[fallback] En = "en" }
/// # exos::messages! { files(count: Plural) { En { One } = "{count} file", En { _ } = "{count} files" } }
/// # fn main() { exos::with_scope(|| {
/// let picked: Vec<u32> = vec![7, 9];
///
/// assert_eq!(files(picked.len()), "2 files");
/// assert_eq!(files(1), "1 file");
/// # }); }
/// ```
///
/// It is sealed. What a message needs from a count is fixed by CLDR rather
/// than by us, and the type is the whole of what decides where a message is
/// resolved, so it is not somewhere an application should be able to reach.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a count",
    label = "a count is a whole number",
    note = "an expression for a count projects the message into the browser, \
            which a message with a slot or with two counts cannot be"
)]
pub trait Count: Copy + Display + sealed::Count {
    /// The count without its sign, which is CLDR's `n`.
    ///
    /// A plural rule asks about the absolute value: a language that has a
    /// singular puts -1 in it just as it puts 1 there.
    fn magnitude(self) -> u64;

    /// Whether the count is below zero.
    ///
    /// The sign decides nothing about which arm a message takes, and only
    /// which character the number is written with; see
    /// [`Symbols`](crate::Symbols), where that is not always a hyphen.
    fn is_negative(self) -> bool;
}

/// Implements [`Count`] for the whole numbers a count arrives as.
///
/// `u128` and `i128` are left out. A count large enough to need one is not a
/// number of things anybody is pluralising, and the alternative is a
/// conversion that quietly truncates.
macro_rules! counts {
    (unsigned $($number:ty),*) => {
        $(
            impl sealed::Count for $number {}

            impl Count for $number {
                fn magnitude(self) -> u64 {
                    u64::from(self)
                }

                fn is_negative(self) -> bool {
                    false
                }
            }

            counted!($number);
        )*
    };
    (signed $($number:ty),*) => {
        $(
            impl sealed::Count for $number {}

            impl Count for $number {
                fn magnitude(self) -> u64 {
                    u64::from(self.unsigned_abs())
                }

                fn is_negative(self) -> bool {
                    self < 0
                }
            }

            counted!($number);
        )*
    };
}

/// A number resolves a message here.
///
/// One impl per type rather than one over [`Count`], because a blanket impl
/// bounded on a trait and the [`Js`] impl below overlap as far as coherence
/// can see, whatever the sealing says.
macro_rules! counted {
    ($number:ty) => {
        impl Counted for $number {
            type Answer = String;

            fn resolve(
                self,
                symbols: Symbols,
                here: impl FnOnce(u64, String) -> String,
                _: impl FnOnce(&str) -> Js<String>,
            ) -> String {
                here(Count::magnitude(self), symbols.number(self))
            }
        }
    };
}

counts!(unsigned u8, u16, u32, u64);
counts!(signed i8, i16, i32, i64);

counted!(usize);
counted!(isize);

/// And an expression resolves it there, whatever whole number it yields.
impl<T: Count> Counted for Js<T> {
    type Answer = Js<String>;

    fn resolve(
        self,
        _: Symbols,
        _: impl FnOnce(u64, String) -> String,
        there: impl FnOnce(&str) -> Js<String>,
    ) -> Js<String> {
        there(self.source())
    }
}

// The two whose width is the machine's are written out, since neither
// conversion above holds on every target `u64::from` would have to cover.
impl sealed::Count for usize {}

impl Count for usize {
    fn magnitude(self) -> u64 {
        self as u64
    }

    fn is_negative(self) -> bool {
        false
    }
}

impl sealed::Count for isize {}

impl Count for isize {
    fn magnitude(self) -> u64 {
        self.unsigned_abs() as u64
    }

    fn is_negative(self) -> bool {
        self < 0
    }
}

/// A domain a message can branch on.
///
/// Interpolating a value asks nothing of its type beyond `Display`, but
/// *branching* on one asks that the values can be listed: a message picks a
/// string per value, so there has to be a knowable set of them. A fieldless
/// enum qualifies and derives this:
///
/// ```
/// use exos::Enumerable as _;
///
/// #[derive(Clone, Copy, exos::Enumerable)]
/// enum Assignee {
///     Me,
///     Somebody,
/// }
///
/// assert_eq!(Assignee::ALL.len(), 2);
/// ```
///
/// A message never names this trait, since the generated code does. Reading
/// [`ALL`](Enumerable::ALL) yourself is what asks for the import above.
///
/// [`bool`] is one too, so a message branching on a flag needs nothing
/// declared. Plural categories are another, and `locales!` implements this for
/// each language's own.
pub trait Enumerable: Copy + 'static {
    /// Every value, in the order they were declared.
    const ALL: &'static [Self];
}

impl Enumerable for bool {
    const ALL: &'static [Self] = &[false, true];
}

// -----------------------------------------------------------------------------
//                               THE PROJECTION
// -----------------------------------------------------------------------------

/// A count a message was given.
///
/// A number resolves the message here and answers with the sentence. A
/// [`Js`] expression cannot, since the count is whatever the browser holds by
/// the time anybody reads it, so the message answers with an expression and
/// the variants cross with the page.
///
/// It is the argument type alone that decides, which is what lets one call
/// site serve both sides. Nothing implements this beyond the whole numbers
/// [`Count`] covers and `Js` over one of them.
pub trait Counted: Sized {
    /// What a message answers when it is given this.
    type Answer;

    /// Runs whichever half of the message this count is for.
    ///
    /// `here` is handed the magnitude a plural rule asks about and the count
    /// written the way the language writes one; `there` is handed the source
    /// of the expression the browser will read it from. Called by the
    /// `messages!` expansion, which is the only thing that has both halves.
    #[doc(hidden)]
    fn resolve(
        self,
        symbols: Symbols,
        here: impl FnOnce(u64, String) -> String,
        there: impl FnOnce(&str) -> Js<String>,
    ) -> Self::Answer;
}

/// Where a count sits in a projected sentence.
///
/// A message is split on this before it crosses, so what the browser gets is
/// the text around the number rather than a placeholder it has to find. It
/// never reaches a page, and a translation holding one loses nothing but the
/// character.
#[doc(hidden)]
pub const HOLE: &str = "\u{0}";

thread_local! {
    /// What has been projected and not yet written into an element.
    ///
    /// Drained by [`Attributes::render`](crate::Attributes), so the entries a
    /// template projects ride out on the element whose expression reads them.
    /// A table entry is keyed by what is in it, so where it lands and how
    /// often it lands there change nothing.
    static PROJECTED: RefCell<Map<String, Value>> = RefCell::new(Map::new());
}

/// Projects one message's variants and hands back the expression reading them.
///
/// Called by the `messages!` expansion. `say` is the text per plural category,
/// each split on [`HOLE`] into the parts the count goes between.
#[doc(hidden)]
pub fn project(lang: &'static str, say: Vec<(PluralCategory, String)>, count: &str) -> Js<String> {
    let mut entry = Map::new();

    for (category, text) in say {
        let parts: Vec<Value> = text.split(HOLE).map(Into::into).collect();
        entry.insert(category.as_str().to_owned(), Value::Array(parts));
    }

    let mut table = Map::new();
    table.insert(String::from("lang"), lang.into());
    table.insert(String::from("say"), Value::Object(entry));

    let key = named(&table);
    let read = format!("msg({}, {count})", crate::quote_js(&key));

    PROJECTED.with(|projected| projected.borrow_mut().insert(key, Value::Object(table)));

    Js::raw(read)
}

/// What a message is called in the table, which is what it says.
///
/// A content hash, so the same sentence projected from two places is one entry
/// and a patch carrying an entry the document already has merges rather than
/// collides. It is not a name anything has to agree on across a deploy: a
/// document and the entries it carries always arrive together.
fn named(table: &Map<String, Value>) -> String {
    let mut hash = Fnv1a::new();
    core::hash::Hasher::write(
        &mut hash,
        Value::Object(table.clone()).to_string().as_bytes(),
    );

    format!("m{:08x}", core::hash::Hasher::finish(&hash) >> 32)
}

/// Every entry projected since the last element wrote one out.
pub(crate) fn projected() -> Option<String> {
    PROJECTED.with(|projected| {
        let taken = core::mem::take(&mut *projected.borrow_mut());

        match taken.is_empty() {
            true => None,
            false => Some(Value::Object(taken).to_string()),
        }
    })
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_is_asked_for_its_magnitude_because_that_is_what_a_rule_asks() {
        assert_eq!(3_u8.magnitude(), 3);
        assert_eq!(3_usize.magnitude(), 3);
        assert_eq!(u64::MAX.magnitude(), u64::MAX);
    }

    /// A language with a singular puts -1 in it, so the sign is dropped on the
    /// way to the rule and kept on the way to the text.
    #[test]
    fn a_negative_count_falls_where_its_magnitude_does() {
        assert_eq!((-1_i32).magnitude(), 1);
        assert_eq!(i64::MIN.magnitude(), 9_223_372_036_854_775_808);
        assert_eq!(format!("{}", -1_i32), "-1");
    }

    #[test]
    fn a_flag_is_a_domain_without_anything_being_declared() {
        assert_eq!(bool::ALL, &[false, true]);
    }

    /// One variant, split where the count goes. The parts are what the browser
    /// joins the number between, so a sentence that writes the count twice
    /// needs nothing said about how often.
    #[test]
    fn a_projected_message_crosses_as_the_text_around_its_count() {
        drop(projected());

        let read = project(
            "en",
            vec![
                (PluralCategory::One, format!("{HOLE} item")),
                (PluralCategory::Other, format!("{HOLE} of {HOLE} items")),
            ],
            "$.picked.length",
        );

        let written = projected().expect("it was recorded");
        let entries: Map<String, Value> =
            serde_json::from_str(&written).expect("the entries are an object");

        let (key, entry) = entries.iter().next().expect("one entry");

        assert_eq!(entry["lang"], "en");
        assert_eq!(entry["say"]["one"], serde_json::json!(["", " item"]));
        assert_eq!(
            entry["say"]["other"],
            serde_json::json!(["", " of ", " items"])
        );

        // Read by name, so the expression and the entry cannot be about
        // different sentences.
        assert_eq!(read.source(), format!("msg(\"{key}\", $.picked.length)"));
    }

    /// An entry is named by what it says, so the same sentence projected twice
    /// is one entry and a document never carries two copies of it.
    #[test]
    fn the_same_sentence_projected_twice_is_one_entry() {
        drop(projected());

        let say = || vec![(PluralCategory::Other, format!("{HOLE} items"))];

        let first = project("en", say(), "$.a");
        let second = project("en", say(), "$.b");
        let entries = projected().expect("both were recorded");

        assert_eq!(
            first.source().split(',').next(),
            second.source().split(',').next(),
            "the same sentence is the same key",
        );

        assert_eq!(entries.matches("\"lang\"").count(), 1, "{entries}");
    }

    /// And the language is part of what it says, since the same call site
    /// serves every language the application has.
    #[test]
    fn one_sentence_in_two_languages_is_two_entries() {
        let say = || vec![(PluralCategory::Other, format!("{HOLE} items"))];

        let english = project("en", say(), "$.a");
        let german = project("de", say(), "$.a");

        drop(projected());

        assert_ne!(english.source(), german.source());
    }

    /// Nothing to write out where nothing projected, which is every element on
    /// every page that has no message crossing on it.
    #[test]
    fn an_element_that_projected_nothing_carries_nothing() {
        drop(projected());

        assert_eq!(projected(), None);
    }
}
