//! The vendored plural table, checked from the server's side.
//!
//! The other half of this is `fixture.test.js` in the `exos-cldr` crate, which
//! reads the same file and asserts that `Intl.PluralRules` still agrees with
//! it. Neither suite needs the other to have run, and that is the whole
//! arrangement: the fixture was derived from the CLDR release the table was
//! generated from, so a failure here means the code `locales!` writes stopped
//! reproducing the table, and a failure there means the browser stopped
//! agreeing with it.
//!
//! It lives here rather than beside the table because reading it takes the
//! macro, and the macro takes exos.
//!
//! Every locale in the table is declared, rather than the two an example would
//! carry, so what this compiles is the evaluator for each of them. A rule that
//! generates something rustc will not accept fails here in the language it
//! belongs to, rather than in an application that happens to ship it.

use exos::{Direction, PluralCategory, serde_json::Value};

include!("cldr/locales.rs");

/// What both suites check against. Read at compile time, since a fixture that
/// went missing is a broken checkout rather than a failing assertion.
const FIXTURE: &str = include_str!("../../exos-cldr/fixture.json");

fn fixture() -> Value {
    exos::serde_json::from_str(FIXTURE).expect("the fixture is json")
}

/// The counts the fixture pins, in the order every row is written in.
fn counts(fixture: &Value) -> Vec<u64> {
    fixture["counts"]
        .as_array()
        .expect("the fixture has counts")
        .iter()
        .map(|count| count.as_u64().expect("a count is a whole number"))
        .collect()
}

/// The numbers it pins, which go below zero where a count never does.
fn numbers(fixture: &Value) -> Vec<i64> {
    fixture["numbers"]
        .as_array()
        .expect("the fixture has numbers")
        .iter()
        .map(|number| number.as_i64().expect("a number is a whole number"))
        .collect()
}

/// The table and the fixture come out of one run of the generator, and a
/// checkout where they did not is a checkout where the rest of this file is
/// comparing two different releases of CLDR.
#[test]
fn the_fixture_was_generated_from_the_vendored_table() {
    assert_eq!(fixture()["cldr"].as_str(), Some(Locale::CLDR_VERSION));
}

#[test]
fn every_locale_in_the_table_is_covered() {
    let fixture = fixture();
    let covered = fixture["categories"].as_object().expect("categories");

    assert_eq!(covered.len(), Locale::ALL.len());

    for tag in covered.keys() {
        assert!(Locale::from_tag(tag).is_some(), "{tag} is not declared");
    }
}

/// The whole point of the arrangement: what the macro generated answers what
/// the table says, for every language and every count either suite knows about.
#[test]
fn every_count_lands_in_the_category_the_fixture_names() {
    let fixture = fixture();
    let counts = counts(&fixture);

    for (tag, row) in fixture["categories"].as_object().expect("categories") {
        let locale = Locale::from_tag(tag).expect("a declared locale");
        let expected: Vec<&str> = row
            .as_str()
            .expect("a row of categories")
            .split(' ')
            .collect();

        assert_eq!(
            expected.len(),
            counts.len(),
            "{tag} has a row of the wrong length"
        );

        for (count, expected) in counts.iter().zip(expected) {
            assert_eq!(
                locale.category(*count).as_str(),
                expected,
                "{tag} disagrees about {count}, against CLDR {}",
                Locale::CLDR_VERSION,
            );
        }
    }
}

/// The other half of the table: the digits, the separators and the sign every
/// language writes a whole number with.
#[test]
fn every_number_is_written_the_way_the_fixture_writes_it() {
    let fixture = fixture();
    let numbers = numbers(&fixture);

    for (tag, row) in fixture["written"].as_object().expect("written") {
        let locale = Locale::from_tag(tag).expect("a declared locale");
        let expected = row.as_array().expect("a row of numbers");

        assert_eq!(
            expected.len(),
            numbers.len(),
            "{tag} has a row of the wrong length"
        );

        for (number, expected) in numbers.iter().zip(expected) {
            assert_eq!(
                locale.number(*number),
                expected.as_str().expect("a written number"),
                "{tag} disagrees about {number}, against CLDR {}",
                Locale::CLDR_VERSION,
            );
        }
    }
}

/// Four languages have no numbers of their own in CLDR and count in the root's
/// instead. They are left out of the fixture rather than pinned to a fallback,
/// so what is written here is the number of claims nobody is making.
#[test]
fn the_languages_with_no_numbers_of_their_own_are_the_ones_expected() {
    let fixture = fixture();
    let written = fixture["written"].as_object().expect("written");

    let missing: Vec<&str> = Locale::ALL
        .iter()
        .map(|locale| locale.tag())
        .filter(|tag| !written.contains_key(*tag))
        .collect();

    assert_eq!(missing, ["ars", "guw", "nah", "smi"]);
}

/// A category is typed to the language it belongs to, which is what makes a
/// missing translation a non-exhaustive match rather than a wrong string.
#[test]
fn a_language_answers_in_its_own_categories() {
    assert_eq!(de::category(1), de::Plural::One);
    assert_eq!(de::category(0), de::Plural::Other);

    assert_eq!(ar::category(0), ar::Plural::Zero);
    assert_eq!(ar::category(2), ar::Plural::Two);
    assert_eq!(ar::category(11), ar::Plural::Many);

    assert_eq!(PluralCategory::from(de::category(1)), PluralCategory::One);
}

#[test]
fn a_locale_knows_the_tag_it_was_declared_with() {
    assert_eq!(Locale::De.tag(), "de");
    assert_eq!(Locale::from_tag("DE"), Some(Locale::De));
    assert_eq!(Locale::from_tag("de-AT"), None);
    assert_eq!(Locale::from_tag("klingon"), None);
}

#[test]
fn a_locale_knows_which_way_it_is_written() {
    assert_eq!(Locale::En.direction(), Direction::LeftToRight);
    assert_eq!(Locale::Ar.direction(), Direction::RightToLeft);
    assert_eq!(Locale::He.direction().as_str(), "rtl");
}

#[test]
fn the_fallback_is_the_one_that_was_marked() {
    assert_eq!(Locale::FALLBACK, Locale::En);
    assert!(Locale::ALL.contains(&Locale::FALLBACK));
}
