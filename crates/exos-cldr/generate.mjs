// Turns CLDR into the table this crate is.
//
// Run it after bumping cldr-core, from anywhere:
//
//     npm run cldr
//
// Nothing here runs during a build. What a checkout compiles is the committed
// output of this script, which is why the version of CLDR an application's
// plural rules come from is a line in a lockfile rather than whatever a
// network answered with that afternoon.
//
// Three files come out, and they are meant to be reviewed as a set:
//
//   crates/exos-cldr/src/table.rs      what `locales!` reads
//   crates/exos-cldr/fixture.json      what both test suites check against
//   crates/exos/tests/cldr/locales.rs  one declaration of every tag in it
//
// The fixture is the point. It is derived from the table beside it, so `cargo
// test` proves the code `locales!` generates reproduces the rules we vendored,
// and `npm test` proves `Intl.PluralRules` still agrees with them. A CLDR
// release that changes an answer therefore fails on the side that changed, in a
// language none of us reads.
//
// The declaration lands in the exos crate rather than here because the test
// that reads it needs the macro, and the macro needs exos.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..", "..");

/**
 * The counts the fixture pins, chosen to sit on the edges the rules have.
 *
 * Every boundary in the vendored rules is a value or a range over `n`, `n % 10`,
 * `n % 100`, `n % 1000`, `n % 100000` or `n % 1000000`, so the run through the
 * first hundred catches the small comparisons, the runs around 100 and 1000
 * catch the teens exceptions Slavic and Celtic rules carry, and the tail
 * catches the millions rule French and Breton use for "many".
 */
const COUNTS = [
    ...span(0, 25),
    30, 40, 50, 60, 62, 70, 71, 80, 81, 82, 90, 91, 99,
    ...span(100, 115), 119, 120, 121, 122, 123, 124, 125,
    200, 201, 202, 203, 300, 302, 400, 600, 800,
    ...span(1000, 1003), 1011, 1012, 1014, 1021, 1022, 1100, 1101, 1111,
    2000, 10000, 20000, 40000, 60000, 80000, 100000, 100001, 101000, 120000,
    1000000, 1000001, 2000000, 10000000,
];

/** CLDR's own order for the categories, which is the order rules are tried in. */
const CATEGORIES = ["zero", "one", "two", "few", "many", "other"];

/**
 * The tag the generated declaration marks as the fallback.
 *
 * `locales!` insists on one, and nothing the fixture checks depends on which,
 * so this is as arbitrary as it looks.
 */
const FALLBACK = "en";

function span(from, to) {
    return Array.from({ length: to - from + 1 }, (_, index) => from + index);
}

// -----------------------------------------------------------------------------
//                                 READING CLDR
// -----------------------------------------------------------------------------

function cldr(path) {
    return JSON.parse(readFileSync(require.resolve(`cldr-core/${path}`), "utf8"));
}

const version = require("cldr-core/package.json").version;
const cardinals = cldr("supplemental/plurals.json").supplemental["plurals-type-cardinal"];
const likely = cldr("supplemental/likelySubtags.json").supplemental.likelySubtags;
const scripts = cldr("scriptMetadata.json").scriptMetadata;

/** Scripts CLDR writes right to left, for a tag that names its script itself. */
const rtlScripts = Object.keys(scripts)
    .filter((script) => scripts[script].rtl === "YES")
    .sort();

/**
 * Tags CLDR has plural rules but no likely script for.
 *
 * All three name a group of languages rather than one, which is why nothing
 * guesses a script for them, and all three are written left to right. They are
 * listed rather than defaulted so that a tag arriving in this state for some
 * other reason stops the script instead of being quietly called left to right.
 */
const NO_LIKELY_SCRIPT = ["nah", "sh", "smi"];

/**
 * Which way `tag` runs, from the script it is most likely written in.
 *
 * A tag rarely says its script, and the direction is a property of the script
 * rather than of the language, so the answer comes from CLDR's own guess at
 * what an unadorned tag means: `ar` is `ar-Arab-EG`, and `Arab` is right to
 * left.
 */
function direction(tag) {
    const language = tag.split("-")[0];
    const guess = likely[tag] ?? likely[language];

    if (guess === undefined) {
        if (NO_LIKELY_SCRIPT.includes(tag)) {
            return "LeftToRight";
        }

        throw new Error(`no likely script for ${tag}`);
    }

    const script = guess.split("-")[1];
    return rtlScripts.includes(script) ? "RightToLeft" : "LeftToRight";
}

// -----------------------------------------------------------------------------
//                                 PLURAL RULES
// -----------------------------------------------------------------------------

// A rule is a small expression language over the operands `n, i, v, w, f, t, c`
// (UTS 35), and stage 1 counts in whole numbers only. That is what makes the
// table small: `v` through `c` are zero for every count exos can be handed, so
// each relation over one of them is decided here rather than at run time, and
// what survives is a comparison over the count itself.

const OPERANDS = "nivwftce";

/**
 * One `expr = range_list` relation, with the samples already cut off.
 *
 * The deprecated `is`, `in` and `within` forms are not accepted. CLDR has not
 * emitted them for years, and a rule that used one would be silently
 * mis-parsed rather than refused.
 */
function relation(source) {
    const parsed = source
        .trim()
        .match(/^([a-z])\s*(?:%|mod)\s*(\d+)\s*(!=|=)\s*(.+)$|^([a-z])\s*(!=|=)\s*(.+)$/);

    if (parsed === null) {
        throw new Error(`unparsed relation: ${source}`);
    }

    const [, modOperand, modulus, modOperator, modList, operand, operator, list] = parsed;
    const name = modOperand ?? operand;

    if (!OPERANDS.includes(name)) {
        throw new Error(`unknown operand ${name} in: ${source}`);
    }

    return {
        operand: name,
        modulus: modulus === undefined ? null : Number(modulus),
        negated: (modOperator ?? operator) === "!=",
        ranges: (modList ?? list).split(",").map((part) => {
            const [low, high = low] = part.trim().split("..");

            for (const bound of [low, high]) {
                if (!/^\d+$/.test(bound)) {
                    throw new Error(`non-integer bound ${bound} in: ${source}`);
                }
            }

            return [Number(low), Number(high)];
        }),
    };
}

/**
 * A rule as a condition over the count alone.
 *
 * CLDR writes a condition in disjunctive normal form already, so this keeps
 * that shape: a list of clauses, any of which holding makes the category
 * apply. `null` comes back where nothing can make it hold, which happens for
 * the five languages whose "many" is about a decimal point.
 */
function condition(rule) {
    const source = rule.split("@")[0].trim();

    if (source === "") {
        return [];
    }

    const clauses = [];

    for (const clause of source.split(/\bor\b/)) {
        const tests = [];
        let reachable = true;

        for (const part of clause.split(/\band\b/)) {
            const { operand, modulus, negated, ranges } = relation(part);

            if (operand === "n" || operand === "i") {
                tests.push({ modulus, negated, ranges });
                continue;
            }

            // Everything else is zero for an integer count, so the relation is
            // a constant and the clause either loses a test or dies.
            const holds = ranges.some(([low, high]) => low <= 0 && 0 <= high) !== negated;

            if (!holds) {
                reachable = false;
                break;
            }
        }

        if (!reachable) {
            continue;
        }

        // A clause with every test decided is a clause that always holds, and
        // a category that always applies ends the rule list.
        if (tests.length === 0) {
            return [];
        }

        clauses.push(tests);
    }

    return clauses.length === 0 ? null : clauses;
}

/** The categories `tag` can reach with a whole-number count, in CLDR's order. */
function rules(tag) {
    const source = cardinals[tag];
    const named = Object.keys(source).map((key) => key.replace("pluralRule-count-", ""));

    if (named.at(-1) !== "other") {
        throw new Error(`${tag} does not end in "other"`);
    }

    // The order matters: a rule list is tried top to bottom, and the generated
    // enum declares its variants the same way. CLDR has always emitted them in
    // its own canonical order, and the day it stops is a day to look rather
    // than to ship a table that reads as if nothing happened.
    const ordered = named.map((category) => CATEGORIES.indexOf(category));

    if (ordered.includes(-1)) {
        throw new Error(`${tag} has a category outside ${CATEGORIES.join(", ")}`);
    }

    if (ordered.some((rank, index) => index > 0 && rank <= ordered[index - 1])) {
        throw new Error(`${tag} lists its categories out of CLDR's order`);
    }

    const kept = [];

    for (const category of named) {
        const clauses = condition(source[`pluralRule-count-${category}`]);

        if (clauses === null) {
            continue;
        }

        kept.push({
            category,
            samples: samples(source[`pluralRule-count-${category}`]),
            clauses,
        });

        // Nothing after a category that always applies can ever be reached.
        if (clauses.length === 0 && category !== "other") {
            throw new Error(`${tag}/${category} always applies but is not last`);
        }
    }

    if (kept.at(-1).category !== "other" || kept.at(-1).clauses.length !== 0) {
        throw new Error(`${tag} has no unconditional fallback`);
    }

    return kept;
}

/** CLDR's own integer samples for a rule, which document the category. */
function samples(rule) {
    const integers = rule.match(/@integer\s*([^@]*)/);
    return integers === null ? "" : integers[1].trim().replace(/,\s*$/, "");
}

/** Which category `count` falls in, by the rules as they were collapsed. */
function categoryOf(collapsed, count) {
    for (const { category, clauses } of collapsed) {
        if (clauses.length === 0) {
            return category;
        }

        const holds = clauses.some((clause) =>
            clause.every(({ modulus, negated, ranges }) => {
                const value = modulus === null ? count : count % modulus;
                return ranges.some(([low, high]) => low <= value && value <= high) !== negated;
            })
        );

        if (holds) {
            return category;
        }
    }

    throw new Error("no category applies");
}

// -----------------------------------------------------------------------------
//                                  THE OUTPUT
// -----------------------------------------------------------------------------

// `und` is left out. It is the tag for a language nobody has determined, so an
// application declaring it is saying nothing, and it is the one tag ICU has no
// plural rules for, which would make it the one line of the fixture the
// browser half could not check.

const tags = Object.keys(cardinals)
    .filter((tag) => tag !== "und")
    .sort();

const table = tags.map((tag) => ({ tag, direction: direction(tag), rules: rules(tag) }));

const upper = (word) => word[0].toUpperCase() + word.slice(1);
const variant = (tag) => tag.split("-").map(upper).join("");
const string = (text) => `"${text.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;

function testSource({ modulus, negated, ranges }) {
    const list = ranges.map(([low, high]) => `(${low}, ${high})`).join(", ");

    return [
        "Test {",
        `modulus: ${modulus === null ? "None" : `Some(${modulus})`},`,
        `ranges: &[${list}],`,
        `negated: ${negated},`,
        "}",
    ].join(" ");
}

function tableSource() {
    const entries = table.map(({ tag, direction, rules }) => {
        const written = rules.map(({ category, samples, clauses }) => {
            const condition = clauses
                .map((clause) => `&[${clause.map(testSource).join(", ")}]`)
                .join(", ");

            return `Rule { category: Category::${upper(category)}, \
samples: ${string(samples)}, condition: &[${condition}] }`;
        });

        return `Entry { tag: ${string(tag)}, direction: Direction::${direction}, \
rules: &[${written.join(", ")}] }`;
    });

    return `//! The table itself.
//!
//! Generated by \`generate.mjs\`, beside this crate's manifest, from cldr-core
//! ${version}. Edit that, not this, and commit what it writes.
//!
//! Every condition here has already been collapsed for whole-number counts: the
//! operands that describe a fraction are zero, so the relations over them are
//! decided during generation and what is left compares the count itself. That
//! is why most languages arrive with one or two comparisons, and why the five
//! whose "many" only ever applies to a decimal do not carry it at all.

use crate::entry::{Category, Direction, Entry, Rule, Test};

/// What CLDR release the table below was generated from.
pub const VERSION: &str = ${string(version)};

/// Scripts CLDR writes right to left.
///
/// Direction belongs to a script rather than to a language, and a tag that
/// names its own script means it rather than whatever CLDR guesses for the
/// language, which is the difference between \`pa\` and \`pa-Arab\`.
pub(crate) static RTL_SCRIPTS: &[&str] = &[${rtlScripts.map(string).join(", ")}];

/// Every locale CLDR gives cardinal plural rules for, by tag.
///
/// Sorted, so a diff after a CLDR bump reads as a diff.
/// [\`Entry::lookup\`](crate::Entry::lookup) is the way in: a tag is tried whole
/// and then by dropping subtags, so \`de-AT\` finds \`de\` and \`pt-PT\` finds
/// itself.
pub static LOCALES: &[Entry] = &[${entries.join(", ")}];
`;
}

function declarationSource() {
    const declared = table.map(({ tag }) => {
        const fallback = tag === FALLBACK ? "    #[fallback]\n" : "";
        return `${fallback}    ${variant(tag)} = ${string(tag)},`;
    });

    return `// Every locale the fixture covers, declared once so that \`cargo test\` compiles
// the evaluator the macro generates for each of them rather than for the two an
// example would carry.
//
// Generated by \`cldr/generate.mjs\`. Included by \`tests/cldr.rs\`, which is where
// the fixture is read.

exos::locales! {
${declared.join("\n")}
}
`;
}

function fixture() {
    const categories = {};

    for (const { tag, rules } of table) {
        categories[tag] = COUNTS.map((count) => categoryOf(rules, count)).join(" ");
    }

    // Written by hand rather than through JSON.stringify's indentation, so that
    // one locale is one line and a CLDR bump that moves one language shows up
    // as one line of diff.
    const lines = Object.entries(categories).map(
        ([tag, row]) => `    ${string(tag)}: ${string(row)}`
    );

    return `{
  "cldr": ${string(version)},
  "counts": [${COUNTS.join(", ")}],
  "categories": {
${lines.join(",\n")}
  }
}
`;
}

function write(path, contents) {
    const target = join(ROOT, path);

    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, contents);
    console.log(`wrote ${path}`);
}

write("crates/exos-cldr/src/table.rs", tableSource());
write("crates/exos-cldr/fixture.json", fixture());
write("crates/exos/tests/cldr/locales.rs", declarationSource());

// The table is written as one long line per entry and then handed to the
// formatter the repository already uses, so the committed file is what `cargo
// fmt --check` expects and nothing here has an opinion about layout.
try {
    const written = join(ROOT, "crates/exos-cldr/src/table.rs");
    execFileSync("rustfmt", ["--edition", "2024", written], { stdio: "inherit" });
} catch (cause) {
    throw new Error("rustfmt has to be on the path; run this inside `nix develop`", { cause });
}

console.log(`${table.length} locales from cldr-core ${version}, ${COUNTS.length} counts each`);
