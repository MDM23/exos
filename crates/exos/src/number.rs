//! Writing a whole number the way a language writes one.
//!
//! A count is the one number a message knows is a number, so it is the one
//! that goes in with the language's own digits and separators rather than with
//! Rust's. The symbols come from the same vendored CLDR table the plural rules
//! do, and the same committed fixture holds them to what `Intl.NumberFormat`
//! answers with, because the two halves of a page have to write a number the
//! same way or the reader sees it change.
//!
//! ```
//! # exos::locales! { De = "de", #[fallback] En = "en", Hi = "hi" }
//! # fn main() {
//! assert_eq!(Locale::De.number(1_234_567), "1.234.567");
//! assert_eq!(Locale::En.number(1_234_567), "1,234,567");
//!
//! // Not every language groups by three, and not every one starts at a
//! // thousand.
//! assert_eq!(Locale::Hi.number(1_234_567), "12,34,567");
//! # }
//! ```

use crate::Count;

/// The symbols a language writes a whole number with.
///
/// Written by [`locales!`](crate::locales) out of the vendored table, and
/// reached through `Locale::symbols`. The fields are public because the macro
/// fills them in and there is nothing here to uphold; what a locale writes is
/// CLDR's answer rather than ours.
///
/// Enough to write a whole number and no more. The decimal separator and the
/// percent sign are not vendored, because nothing reads them yet.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Symbols {
    /// The ten digits of whichever numbering system the language counts in by
    /// default, in order. Western Arabic for most, and not for Persian,
    /// Burmese or Bengali.
    pub digits: &'static str,
    /// What goes between groups of digits.
    pub group: &'static str,
    /// What goes in front of a count below zero. Rarely a plain hyphen: some
    /// languages put a directionality mark in front of theirs, which is what
    /// keeps a minus on the correct side of a number in a right-to-left line.
    pub minus: &'static str,
    /// How many digits are in the group furthest to the right, or zero where
    /// the language does not group at all.
    pub grouping: u8,
    /// How many are in each group after that. It differs from
    /// [`grouping`](Self::grouping) only in the Indic pattern, which writes
    /// 12,34,567.
    pub secondary_grouping: u8,
    /// How many digits there have to be before the first separator appears.
    /// Polish writes 1000 and then 12 345.
    pub minimum_grouping_digits: u8,
}

impl Symbols {
    /// `count` written with these symbols.
    ///
    /// ```
    /// # exos::locales! { #[fallback] En = "en", Fa = "fa" }
    /// # fn main() {
    /// assert_eq!(Locale::Fa.symbols().number(-12_345), "‎−۱۲٬۳۴۵");
    /// # }
    /// ```
    #[must_use]
    pub fn number(&self, count: impl Count) -> String {
        let mut written = String::new();

        if count.is_negative() {
            written.push_str(self.minus);
        }

        let mut alphabet = ['0'; 10];

        for (slot, digit) in alphabet.iter_mut().zip(self.digits.chars()) {
            *slot = digit;
        }

        // Least significant first, which is the order they come out in.
        let mut digits = [0_usize; 20];
        let mut length = 0;
        let mut rest = count.magnitude();

        loop {
            digits[length] = (rest % 10) as usize;
            length += 1;
            rest /= 10;

            if rest == 0 {
                break;
            }
        }

        let grouping = usize::from(self.grouping);
        let minimum = usize::from(self.minimum_grouping_digits);

        // A language that groups by three and then by two is the Indic
        // pattern; every other language repeats the first size.
        let secondary = match usize::from(self.secondary_grouping) {
            0 => grouping,
            secondary => secondary,
        };

        // Grouping waits for the digit the language wants before it, which is
        // why Polish writes 1000 and then 12 345.
        let grouped = grouping > 0 && length >= grouping + minimum;

        for index in (0..length).rev() {
            written.push(alphabet[digits[index]]);

            // `index` is what is left to write, so it is also the distance
            // from the right-hand end, which is where groups are measured.
            let boundary = index == grouping
                || (index > grouping && (index - grouping).is_multiple_of(secondary));

            if grouped && index > 0 && boundary {
                written.push_str(self.group);
            }
        }

        written
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// German: three digits to a group, a full stop between them, and grouping
    /// from a thousand.
    const DE: Symbols = Symbols {
        digits: "0123456789",
        group: ".",
        minus: "-",
        grouping: 3,
        secondary_grouping: 3,
        minimum_grouping_digits: 1,
    };

    #[test]
    fn a_number_below_the_first_group_is_written_as_it_is() {
        assert_eq!(DE.number(0), "0");
        assert_eq!(DE.number(7), "7");
        assert_eq!(DE.number(999), "999");
    }

    #[test]
    fn groups_are_separated_from_the_right() {
        assert_eq!(DE.number(1000), "1.000");
        assert_eq!(DE.number(12_345), "12.345");
        assert_eq!(DE.number(1_234_567_890), "1.234.567.890");
    }

    #[test]
    fn the_sign_is_the_languages_own() {
        let arabic = Symbols {
            minus: "\u{61c}-",
            ..DE
        };

        assert_eq!(DE.number(-1000), "-1.000");
        assert_eq!(arabic.number(-1), "\u{61c}-1");
    }

    /// Polish groups from ten thousand, so the separator waits for a digit
    /// that German does not wait for.
    #[test]
    fn a_language_can_ask_for_a_digit_before_it_groups() {
        let polish = Symbols {
            group: "\u{a0}",
            minimum_grouping_digits: 2,
            ..DE
        };

        assert_eq!(polish.number(1000), "1000");
        assert_eq!(polish.number(12_345), "12\u{a0}345");
    }

    /// The Indic pattern: three at the end, two for everything above.
    #[test]
    fn the_groups_above_the_first_can_be_a_different_size() {
        let hindi = Symbols {
            group: ",",
            secondary_grouping: 2,
            ..DE
        };

        assert_eq!(hindi.number(1000), "1,000");
        assert_eq!(hindi.number(1_234_567), "12,34,567");
        assert_eq!(hindi.number(123_456_789), "12,34,56,789");
    }

    #[test]
    fn the_digits_are_the_languages_own() {
        let bengali = Symbols {
            digits: "০১২৩৪৫৬৭৮৯",
            group: ",",
            secondary_grouping: 2,
            ..DE
        };

        assert_eq!(bengali.number(1_234_567), "১২,৩৪,৫৬৭");
    }

    /// A count is asked for its magnitude, so the widest number there is has
    /// somewhere to go.
    #[test]
    fn the_largest_count_there_is_survives() {
        assert_eq!(DE.number(u64::MAX), "18.446.744.073.709.551.615");
        assert_eq!(DE.number(i64::MIN), "-9.223.372.036.854.775.808");
    }
}
