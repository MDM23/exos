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
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
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
 * The numbers the fixture pins, chosen to sit on the edges of grouping.
 *
 * 999 and 1000 straddle the first separator, 1000 and 12345 straddle the
 * languages that want two digits before one appears, 1234567 is where an Indic
 * pattern stops matching a Western one, and the negatives are there because a
 * minus sign is a symbol like any other and three languages write it with a
 * character nobody would guess.
 */
const NUMBERS = [0, 1, -1, 12, 999, 1000, -1000, 12345, 1234567, 12345678, 1234567890];

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
const numbersVersion = require("cldr-numbers-full/package.json").version;

// Two packages, one release. A table built from a rule in one and a separator
// in another is a table nobody could reason about afterwards.
if (version !== numbersVersion) {
    throw new Error(`cldr-core is ${version} and cldr-numbers-full is ${numbersVersion}`);
}
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
//                                NUMBER SYMBOLS
// -----------------------------------------------------------------------------

// What it takes to write a whole number: the digits of whichever numbering
// system the locale uses by default, the separator between groups, how big a
// group is, how many digits there have to be before the first separator
// appears, and the minus sign. The decimal separator and the percent sign are
// not here, because a count is a whole number and a column nothing reads is a
// column nothing checks.

const numberingSystems = cldr("supplemental/numberingSystems.json").supplemental.numberingSystems;
const languageAlias = cldr("supplemental/aliases.json").supplemental.metadata.alias.languageAlias;
const NUMBERS_MAIN = join(dirname(require.resolve("cldr-numbers-full/package.json")), "main");

/**
 * The locale CLDR keeps `tag`'s numbers under, or `und` where it keeps none.
 *
 * A tag is tried whole, then as whatever CLDR replaced it with, then with a
 * subtag dropped, which is how `sh` reaches Serbian in Latin script and `jw`
 * reaches Javanese. Four of the tags with plural rules end at the root, and
 * they are the reason this answers rather than refusing.
 */
function numbersUnder(tag) {
    let candidate = tag;

    for (;;) {
        if (existsSync(join(NUMBERS_MAIN, candidate, "numbers.json"))) {
            return candidate;
        }

        const replacement = languageAlias[candidate.replaceAll("-", "_")]?._replacement;

        if (replacement !== undefined) {
            return numbersUnder(replacement.replaceAll("_", "-"));
        }

        const shorter = candidate.lastIndexOf("-");

        if (shorter < 0) {
            return "und";
        }

        candidate = candidate.slice(0, shorter);
    }
}

/** How `tag` writes a whole number. */
function symbols(tag) {
    const under = numbersUnder(tag);
    const path = join(NUMBERS_MAIN, under, "numbers.json");
    const numbers = JSON.parse(readFileSync(path, "utf8")).main[under].numbers;

    const system = numbers.defaultNumberingSystem;
    const declared = numberingSystems[system];

    if (declared === undefined || declared._type !== "numeric") {
        throw new Error(`${tag} counts in ${system}, which is not ten digits`);
    }

    const digits = [...declared._digits];

    if (digits.length !== 10) {
        throw new Error(`${system} has ${digits.length} digits`);
    }

    // The pattern says how wide a group is: `#,##0.###` groups by three, and
    // the Indic `#,##,##0.###` groups the first three and then by two.
    const integer = numbers[`decimalFormats-numberSystem-${system}`].standard.split(".")[0];
    const groups = integer.split(",");
    const grouping = groups.length > 1 ? groups.at(-1).length : 0;
    const secondary = groups.length > 2 ? groups.at(-2).length : grouping;

    return {
        under,
        digits,
        group: numbers[`symbols-numberSystem-${system}`].group,
        minus: numbers[`symbols-numberSystem-${system}`].minusSign,
        grouping,
        secondary,
        minimum: Number(numbers.minimumGroupingDigits ?? 1),
    };
}

/** `value` as `symbols` writes it, which is what the fixture pins. */
function written(symbols, value) {
    const digits = [...String(Math.abs(value))].map((digit) => symbols.digits[Number(digit)]);
    const sign = value < 0 ? symbols.minus : "";

    // Grouping waits until there are enough digits before the separator, which
    // is why Polish writes 1000 and then 12 345.
    if (symbols.grouping === 0 || digits.length <= symbols.grouping + symbols.minimum - 1) {
        return sign + digits.join("");
    }

    let cut = digits.length - symbols.grouping;
    const grouped = [digits.slice(cut).join("")];

    while (cut > 0) {
        const start = Math.max(0, cut - symbols.secondary);
        grouped.unshift(digits.slice(start, cut).join(""));
        cut = start;
    }

    return sign + grouped.join(symbols.group);
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

const table = tags.map((tag) => ({
    tag,
    direction: direction(tag),
    symbols: symbols(tag),
    rules: rules(tag),
}));

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

function symbolsSource({ digits, group, minus, grouping, secondary, minimum }) {
    return `Symbols { digits: ${string(digits.join(""))}, group: ${string(group)}, \
minus: ${string(minus)}, grouping: ${grouping}, secondary_grouping: ${secondary}, \
minimum_grouping_digits: ${minimum} }`;
}

function tableSource() {
    const entries = table.map(({ tag, direction, symbols, rules }) => {
        const emitted = rules.map(({ category, samples, clauses }) => {
            const condition = clauses
                .map((clause) => `&[${clause.map(testSource).join(", ")}]`)
                .join(", ");

            return `Rule { category: Category::${upper(category)}, \
samples: ${string(samples)}, condition: &[${condition}] }`;
        });

        return `Entry { tag: ${string(tag)}, direction: Direction::${direction}, \
symbols: ${symbolsSource(symbols)}, rules: &[${emitted.join(", ")}] }`;
    });

    return `//! The table itself.
//!
//! Generated by \`generate.mjs\`, beside this crate's manifest, from cldr-core
//! and cldr-numbers-full ${version}. Edit that, not this, and commit what it
//! writes.
//!
//! Every condition here has already been collapsed for whole-number counts: the
//! operands that describe a fraction are zero, so the relations over them are
//! decided during generation and what is left compares the count itself. That
//! is why most languages arrive with one or two comparisons, and why the five
//! whose "many" only ever applies to a decimal do not carry it at all.

use crate::entry::{Category, Direction, Entry, Rule, Symbols, Test};

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
    // Written by hand rather than through JSON.stringify's indentation, so that
    // one locale is one line and a CLDR bump that moves one language shows up
    // as one line of diff.
    const categories = table.map(({ tag, rules }) => {
        const row = COUNTS.map((count) => categoryOf(rules, count)).join(" ");
        return `    ${string(tag)}: ${string(row)}`;
    });

    // Only the languages CLDR gives numbers of their own. The four that end at
    // the root are left out rather than pinned to it: the table writes them the
    // way the root writes them, which is a fallback rather than a claim about
    // the language, and ICU has data for one of them that we do not.
    const numbers = table
        .filter(({ symbols }) => symbols.under !== "und")
        .map(({ tag, symbols }) => {
            const row = NUMBERS.map((value) => string(written(symbols, value)));
            return `    ${string(tag)}: [${row.join(", ")}]`;
        });

    return `{
  "cldr": ${string(version)},
  "counts": [${COUNTS.join(", ")}],
  "categories": {
${categories.join(",\n")}
  },
  "numbers": [${NUMBERS.join(", ")}],
  "written": {
${numbers.join(",\n")}
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
    const emitted = join(ROOT, "crates/exos-cldr/src/table.rs");
    execFileSync("rustfmt", ["--edition", "2024", emitted], { stdio: "inherit" });
} catch (cause) {
    throw new Error("rustfmt has to be on the path; run this inside `nix develop`", { cause });
}

const rooted = table.filter(({ symbols }) => symbols.under === "und").map(({ tag }) => tag);

console.log(
    `${table.length} locales from CLDR ${version}, ${COUNTS.length} counts and ` +
        `${NUMBERS.length} numbers each`
);
console.log(`${rooted.length} of them count in the root's numbers: ${rooted.join(", ")}`);
