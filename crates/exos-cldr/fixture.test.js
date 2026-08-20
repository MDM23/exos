// The vendored plural table, checked from the browser's side.
//
// The other half of this is `crates/exos/tests/cldr.rs`, which reads the same
// file and asserts that what `locales!` generates reproduces it. It sits over
// there because reading the fixture from Rust takes the macro, and the macro
// takes exos. Neither suite needs the other to have run, and that is the whole
// arrangement: the fixture was derived from the CLDR release we vendored, so a
// `cargo test` failure means the macro stopped agreeing with the table, and a
// failure here means ICU stopped agreeing with it.
//
// No DOM, so no jsdom. `Intl` is part of the language rather than of the
// document, and it is the same implementation a browser would answer with.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const fixture = JSON.parse(readFileSync(join(HERE, "fixture.json"), "utf8"));

/**
 * Whether ICU here knows `tag` itself rather than something it falls back to.
 *
 * A tag it does not know still answers, out of whatever locale it resolved to
 * instead, so asking is the difference between checking a language and
 * checking a stand-in for it. Lookup rather than best fit: it is the matching
 * `Accept-Language` uses, and it says yes for `de-AT` on the strength of `de`,
 * which is the answer we want.
 *
 * Asked of whichever half is doing the checking, because the two do not know
 * the same languages: plural rules reach further than numbers do.
 */
function known(tag, intl) {
    return intl.supportedLocalesOf([tag], { localeMatcher: "lookup" }).length > 0;
}

test("every count in the fixture lands in the category ICU gives it", () => {
    const { counts } = fixture;
    let checked = 0;

    for (const [tag, row] of Object.entries(fixture.categories)) {
        if (!known(tag, Intl.PluralRules)) {
            continue;
        }

        const rules = new Intl.PluralRules(tag, { localeMatcher: "lookup" });
        const expected = row.split(" ");

        assert.equal(expected.length, counts.length, `${tag} has a row of the wrong length`);

        for (const [index, count] of counts.entries()) {
            assert.equal(
                rules.select(count),
                expected[index],
                `${tag} disagrees about ${count}: CLDR ${fixture.cldr} in the table, ` +
                    `ICU ${process.versions.icu} here`,
            );
        }

        checked += 1;
    }

    // A tag ICU has never heard of is skipped rather than failed, since a
    // vendored CLDR newer than this ICU is a legitimate state to be in. What
    // is not legitimate is that going unnoticed until the suite is checking
    // almost nothing, so the count is asserted rather than reported.
    assert.ok(
        checked > Object.keys(fixture.categories).length * 0.9,
        `only ${checked} of ${Object.keys(fixture.categories).length} tags were checked`,
    );
});

test("every number in the fixture is written the way ICU writes it", () => {
    const { numbers, written } = fixture;
    let checked = 0;

    for (const [tag, row] of Object.entries(written)) {
        if (!known(tag, Intl.NumberFormat)) {
            continue;
        }

        const format = new Intl.NumberFormat(tag, { localeMatcher: "lookup" });

        assert.equal(row.length, numbers.length, `${tag} has a row of the wrong length`);

        for (const [index, number] of numbers.entries()) {
            assert.equal(
                format.format(number),
                row[index],
                `${tag} disagrees about ${number}: CLDR ${fixture.cldr} in the table, ` +
                    `ICU ${process.versions.icu} here`,
            );
        }

        checked += 1;
    }

    // Numbers reach fewer languages than plural rules do, so the share checked
    // is lower than the one above rather than the same number written twice.
    assert.ok(
        checked > Object.keys(written).length * 0.8,
        `only ${checked} of ${Object.keys(written).length} tags were checked`,
    );
});

test("the fixture covers every category CLDR has", () => {
    const seen = new Set(Object.values(fixture.categories).flatMap((row) => row.split(" ")));

    assert.deepEqual(
        [...seen].sort(),
        ["few", "many", "one", "other", "two", "zero"],
        "a count set that reaches no zero or no two is a count set that proves less than it looks",
    );
});

test("the fixture covers every way a number is written", () => {
    const rows = Object.values(fixture.written);
    const at = (value) => fixture.numbers.indexOf(value);

    const reaches = (what, holds) =>
        assert.ok(rows.some(holds), `no language in the fixture ${what}, so nothing proves it`);

    reaches("leaves a thousand ungrouped", (row) => row[at(1000)] === "1000");
    reaches("groups the Indic way", (row) => row[at(1234567)] === "12,34,567");
    reaches("counts in digits of its own", (row) => !/[0-9]/.test(row[at(1)]));
    reaches("writes a minus that is not a hyphen", (row) => !row[at(-1)].startsWith("-"));
});
