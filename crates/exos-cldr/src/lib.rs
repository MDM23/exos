//! The slice of CLDR [exos](https://docs.rs/exos) vendors.
//!
//! Cardinal plural rules and writing direction, for every locale CLDR has
//! rules for, as ordinary Rust rather than as data parsed at build time or at
//! run time. [`exos_macro`](https://docs.rs/exos-macro) reads it while
//! `locales!` expands and writes the rules of the declared locales into the
//! application; nothing here ends up in a binary.
//!
//! ```
//! # use exos_cldr::{Category, Direction, Entry};
//! # fn main() -> Result<(), Box<dyn core::error::Error>> {
//! let entry = Entry::lookup("de-AT").ok_or("cldr has rules for de")?;
//!
//! assert_eq!(entry.tag, "de");
//! assert_eq!(entry.direction_of("de-AT"), Direction::LeftToRight);
//! assert_eq!(entry.rules.last().map(|rule| rule.category), Some(Category::Other));
//! # Ok(())
//! # }
//! ```
//!
//! # It is not a plural rule engine
//!
//! There is no function here that takes a count. A rule is a
//! [`Rule::condition`] to be turned into source, because the point of vendoring
//! the table is that an application compiles the two comparisons its languages
//! need rather than carrying an evaluator and 223 locales it does not.
//!
//! Every condition has already been collapsed for whole-number counts, which is
//! what makes it two comparisons. See [`Test`].
//!
//! # Where it comes from
//!
//! `generate.mjs`, in this crate, run with `npm run cldr` from the workspace
//! root when the pinned `cldr-core` is bumped. It writes [`table`](self)'s
//! source and the fixture both halves of the test suite check it against. A
//! build never reaches the network, and a clean checkout never parses CLDR.

mod entry;
mod table;

pub use crate::{
    entry::{Category, Direction, Entry, Rule, Test},
    table::{LOCALES, VERSION},
};
