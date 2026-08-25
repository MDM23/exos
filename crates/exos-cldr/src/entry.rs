//! A row of the table, and what its parts mean.

use crate::table::LOCALES;

/// One locale CLDR has cardinal plural rules for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Entry {
    /// The tag as CLDR spells it.
    pub tag: &'static str,
    /// Which way the script the language is usually written in runs.
    ///
    /// [`Entry::direction_of`] is the answer to prefer, since a tag may name a
    /// script this does not know about.
    pub direction: Direction,
    /// How the language writes a whole number.
    pub symbols: Symbols,
    /// The categories in the order they are tried, ending in an unconditional
    /// [`Category::Other`].
    pub rules: &'static [Rule],
}

/// How a locale writes a whole number.
///
/// Enough to write one and no more: the decimal separator and the percent sign
/// are not vendored, because a count is a whole number and a column nothing
/// reads is a column nothing checks.
///
/// Number symbols belong to a locale rather than to a language, and these are
/// keyed by language like everything else here, so a region that writes numbers
/// differently from the language it belongs to is written the language's way.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Symbols {
    /// The ten digits of whichever numbering system the locale counts in by
    /// default, in order, which is Western Arabic for most languages and is
    /// not for Persian, Burmese or Bengali.
    pub digits: &'static str,
    /// What goes between groups of digits.
    pub group: &'static str,
    /// What goes in front of a count below zero. Rarely a plain hyphen: three
    /// languages put a directionality mark in front of theirs.
    pub minus: &'static str,
    /// How many digits are in the group furthest to the right, or zero where
    /// the language does not group at all.
    pub grouping: u8,
    /// How many are in each group after that, which differs from
    /// [`grouping`](Self::grouping) only in the Indic pattern: 12,34,567.
    pub secondary_grouping: u8,
    /// How many digits there have to be before the first separator appears.
    /// Polish writes 1000 and then 12 345.
    pub minimum_grouping_digits: u8,
}

/// One category, and what makes a count fall in it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rule {
    /// The category this rule names.
    pub category: Category,
    /// CLDR's own integer samples, as it writes them: `2~4, 22~24, 1002, …`.
    /// Empty where no whole number reaches the category, which happens to
    /// Polish and `other`.
    pub samples: &'static str,
    /// Clauses, any one of which holding makes the rule apply. Empty means it
    /// always applies, which is how the last rule of every locale is written.
    pub condition: &'static [&'static [Test]],
}

/// One comparison against the count.
///
/// The count is a whole number, so CLDR's operands for the digits after a
/// decimal point are all zero, and the relations over them were decided when
/// the table was generated rather than being carried into an application. What
/// is left compares `n`, which is why most languages arrive with one or two of
/// these and five of them have no `many`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Test {
    /// What the count is reduced modulo first, if anything.
    pub modulus: Option<u64>,
    /// Inclusive ranges the count has to fall in.
    pub ranges: &'static [(u64, u64)],
    /// Whether it has to fall outside them instead.
    pub negated: bool,
}

/// A CLDR plural category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    /// `few`, the paucal.
    Few,
    /// `many`, which several Slavic languages use for most counts.
    Many,
    /// `one`, the singular where a language has one.
    One,
    /// `other`, the category every language has.
    Other,
    /// `two`, the dual.
    Two,
    /// `zero`, which is not simply the count 0.
    Zero,
}

/// Which way a script runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// Latin, Cyrillic, Greek, Han, Devanagari and most of the rest.
    LeftToRight,
    /// Arabic, Hebrew, Thaana, N'Ko, Adlam and the others CLDR marks.
    RightToLeft,
}

impl Entry {
    /// The entry for `tag`, matched the way BCP 47 lookup matches.
    ///
    /// The table is keyed by language, with `pt-PT` and `kok-Latn` as the two
    /// exceptions, so a tag is tried whole and then with one subtag dropped at
    /// a time: `de-AT-1996` finds `de`, and `pt-BR` finds `pt` while `pt-PT`
    /// finds itself. Case is not significant in a tag, and a caller that
    /// writes `pt-pt` means the same locale as one that writes `pt-PT`.
    ///
    /// ```
    /// # use exos_cldr::Entry;
    /// assert_eq!(Entry::lookup("PT-br").map(|entry| entry.tag), Some("pt"));
    /// assert!(Entry::lookup("klingon").is_none());
    /// ```
    pub fn lookup(tag: &str) -> Option<&'static Self> {
        let mut candidate = tag;

        loop {
            if let Some(entry) = LOCALES
                .iter()
                .find(|entry| entry.tag.eq_ignore_ascii_case(candidate))
            {
                return Some(entry);
            }

            candidate = candidate.rsplit_once('-')?.0;
        }
    }

    /// Which way `tag` runs.
    ///
    /// A tag that names its own script means it: `pa` is written in Gurmukhi
    /// and `pa-Arab` is not, and the entry either of them looks up is the same
    /// one. Where the tag names no script, [`Entry::direction`] stands, which
    /// is CLDR's guess at the one the language is usually written in.
    ///
    /// ```
    /// # use exos_cldr::{Direction, Entry};
    /// # fn main() -> Result<(), Box<dyn core::error::Error>> {
    /// let entry = Entry::lookup("pa").ok_or("cldr has rules for pa")?;
    ///
    /// assert_eq!(entry.direction_of("pa"), Direction::LeftToRight);
    /// assert_eq!(entry.direction_of("pa-Arab"), Direction::RightToLeft);
    /// # Ok(())
    /// # }
    /// ```
    pub fn direction_of(&self, tag: &str) -> Direction {
        let script = tag
            .split('-')
            .skip(1)
            .find(|part| part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic()));

        match script {
            Some(script)
                if crate::table::RTL_SCRIPTS
                    .iter()
                    .any(|rtl| rtl.eq_ignore_ascii_case(script)) =>
            {
                Direction::RightToLeft
            }
            Some(_) => Direction::LeftToRight,
            None => self.direction,
        }
    }
}

impl Category {
    /// The CLDR keyword, which is what `Intl.PluralRules` answers with.
    pub const fn keyword(self) -> &'static str {
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

impl Rule {
    /// Whether this rule applies to every count, which the last rule of every
    /// locale does and no other rule may.
    pub const fn is_unconditional(&self) -> bool {
        self.condition.is_empty()
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_finds_its_own_entry() {
        assert_eq!(Entry::lookup("de").map(|entry| entry.tag), Some("de"));
        assert_eq!(Entry::lookup("pt-PT").map(|entry| entry.tag), Some("pt-PT"));
    }

    /// Lookup drops subtags until something matches, which is what makes a
    /// region or a variant free to declare.
    #[test]
    fn a_tag_with_subtags_finds_its_language() {
        assert_eq!(Entry::lookup("de-AT").map(|entry| entry.tag), Some("de"));
        assert_eq!(
            Entry::lookup("de-AT-1996").map(|entry| entry.tag),
            Some("de")
        );
        assert_eq!(Entry::lookup("pt-BR").map(|entry| entry.tag), Some("pt"));
    }

    /// A tag is case insensitive, so the case an application writes is its own
    /// business rather than something the table decides.
    #[test]
    fn case_is_not_significant() {
        assert_eq!(Entry::lookup("PT-pt").map(|entry| entry.tag), Some("pt-PT"));
    }

    #[test]
    fn a_language_nobody_has_rules_for_is_not_found() {
        assert!(Entry::lookup("xx").is_none());
        assert!(Entry::lookup("").is_none());
    }

    /// Direction follows the script the tag names, and only falls back to the
    /// language's usual one where the tag names none.
    #[test]
    fn a_script_subtag_decides_the_direction() {
        let entry = Entry::lookup("pa").expect("pa is a language");

        assert_eq!(entry.direction_of("pa"), Direction::LeftToRight);
        assert_eq!(entry.direction_of("pa-Arab"), Direction::RightToLeft);
        assert_eq!(entry.direction_of("pa-arab-PK"), Direction::RightToLeft);
    }

    #[test]
    fn a_language_written_right_to_left_says_so_without_a_script() {
        let entry = Entry::lookup("ar-EG").expect("ar is a language");

        assert_eq!(entry.direction_of("ar-EG"), Direction::RightToLeft);
    }

    /// The five languages whose `many` needs a decimal point do not carry it,
    /// because a count is a whole number here and a category nothing can reach
    /// is a translation nobody can check.
    #[test]
    fn a_category_no_integer_reaches_is_not_in_the_table() {
        let categories: Vec<Category> = Entry::lookup("cs")
            .expect("cs is a language")
            .rules
            .iter()
            .map(|rule| rule.category)
            .collect();

        assert_eq!(
            categories,
            [Category::One, Category::Few, Category::Other],
            "cs has a `many`, and it is `v != 0`"
        );
    }

    /// Every rule list ends in an arm that always applies, which is what lets
    /// the generated function end in an `else`.
    #[test]
    fn every_locale_ends_in_an_unconditional_other() {
        for entry in LOCALES {
            let last = entry.rules.last().expect("a locale has rules");

            assert_eq!(last.category, Category::Other, "{}", entry.tag);
            assert!(last.is_unconditional(), "{}", entry.tag);
        }
    }

    /// Nothing before the last arm may be unconditional: it would make every
    /// rule after it dead, and the generated `else` unreachable.
    #[test]
    fn nothing_before_the_last_rule_always_applies() {
        for entry in LOCALES {
            for rule in &entry.rules[..entry.rules.len() - 1] {
                assert!(
                    !rule.is_unconditional(),
                    "{}/{:?}",
                    entry.tag,
                    rule.category
                );
            }
        }
    }

    /// Ten of them, always. The formatter picks a digit by the value it is
    /// writing, so a numbering system that arrived with nine would write the
    /// wrong number rather than fail.
    #[test]
    fn every_locale_counts_in_ten_digits() {
        for entry in LOCALES {
            assert_eq!(entry.symbols.digits.chars().count(), 10, "{}", entry.tag);
        }
    }

    /// A language that groups at all groups by something and separates with
    /// something, since a size of zero would put a separator between every
    /// digit and an empty separator would put none anywhere.
    #[test]
    fn a_language_that_groups_says_how_and_how_wide() {
        for entry in LOCALES {
            if entry.symbols.grouping == 0 {
                continue;
            }

            assert!(entry.symbols.secondary_grouping > 0, "{}", entry.tag);
            assert!(!entry.symbols.group.is_empty(), "{}", entry.tag);
        }
    }

    /// The tags are sorted, so a diff after a CLDR bump reads as a diff.
    #[test]
    fn the_table_is_sorted_by_tag() {
        let tags: Vec<&str> = LOCALES.iter().map(|entry| entry.tag).collect();
        let mut sorted = tags.clone();
        sorted.sort_unstable();

        assert_eq!(tags, sorted);
    }
}
