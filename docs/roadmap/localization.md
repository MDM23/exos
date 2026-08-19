# Localization

Messages defined in Rust, rendered wherever the fact they need is known.

Status: the first half of [stage 1](#stage-1-the-locale) is built.
`exos::locales!` declares the set and generates each language's plural
categories, out of the CLDR table [exos-cldr](../../crates/exos-cldr) vendors as
ordinary source, and a committed fixture holds `cargo test` and `npm test` to
the same answers. Resolution, `exos::locale()`, `Accept-Language` and the
document's `lang` and `dir` attributes are not built, and neither is anything
above stage 1. The shape below, one macro with
match-like arms and one call site that works on both sides, predates exos: it
was settled while the framework was still a prototype, and lived in a guide
draft that was cut when the guide was rewritten against code that existed. It
is recorded here so it stops being remembered and starts being reviewed.

Stages 1 to 3 and stage 5 wait on nothing. [Sessions and
identity](sessions-and-identity.md) already supplies the one thing they need
from elsewhere, which is a place for an application to say who a request is,
and that place is built.

Stage 4, which is live fragments, waits on [dimensions](dimensions.md). That
document exists because of this one: a locale threaded through every fragment
signature by hand is the version of stage 4 that can be built today, and it is
enough worse than the version with dimensions that it is worth waiting for.

## What it is for

A page says "3 items selected" in English and "3 Elemente ausgewählt" in
German, where the 3 came from a checkbox ticked a moment ago and no request has
happened since. A toast says "you were outbid" to one bidder in their language
while the lot's price patches identically for everybody watching it. A row says
"3 minutes ago" and keeps saying something true a minute later, in a time zone
the server was never told.

Three properties, in the order they constrain the design:

- **Messages are defined anywhere and used as ordinary functions.** Not a
  catalog file, not a key looked up at runtime, not a string that compiles when
  it is wrong.
- **Parameters both interpolate and branch.** `{count}` substitutes; a plural
  category, an enum or a bool selects which string is used at all. Branching is
  what makes this harder than string formatting, and it is why a catalog cannot
  simply be handed to the client.
- **The server renders what it can decide, and projects the rest.** A message
  fixed at render time is text in the document. A message that depends on
  client state crosses as its variants, and the browser picks.

## Two facts, and where they part

Everything below follows from one asymmetry.

| | the locale | the time zone |
| --- | --- | --- |
| known to the server | yes, from the request or the reader's profile | no |
| known to the browser | yes | yes |
| cost of putting it in a topic | a fan-out over the locales in use | a fragment per viewer |
| so text is decided | on the server, or projected per locale | in the browser, always |

A locale is a legitimate topic dimension, and is in fact the one
[dimensions](dimensions.md) was written for. Two readers in two languages are
looking at two different renders of one fact, there are as many renders as
there are languages in use, and [`Topic::new`](../../crates/exos/src/live.rs)
hashes what a fragment varies over already, so per-locale topics cost a hash
rather than a mechanism.

A time zone is not. It is per viewer in practice, so putting it in a topic
means a fragment nobody shares, which is the topic model doing no work at all.
The server also cannot learn it without asking the browser and waiting, and the
first render happens before the answer arrives.

So: **instants project, formatted dates do not.** The server sends the moment,
the browser renders the words. Stage 5 is what that costs, which is very
little, because the runtime already re-runs bindings after a morph.

## What exos will not do

- **No translation files and no runtime catalog.** Messages are Rust, checked
  by the compiler. Nothing is loaded, parsed or hot-reloaded at run time.
- **No raw markup inside a message.** No part of a message string is ever
  parsed as HTML, so a translation cannot introduce an element by being edited.
  Structure comes from [slots](#slots-structure-without-half-sentences), which
  are declared in Rust and filled at the call site.
- **No currency formatting.** It needs per-currency symbol, position and
  rounding data, it changes more often than anything else here, and the
  decisions belong to the application. Format money yourself and pass the
  string as an interpolated parameter.
- **No ordinals and no decimal plural counts** in the stages below. Both are
  additive later; see [open questions](#open-questions).
- **No URL-prefixed locales.** `/de/...` is routing, an application can build
  it on the override hook in stage 1, and exos does not own a URL shape.
- **No translator tooling.** Grouping by message is right while a developer
  writes them and wrong for a translator working through one language. An
  export to XLIFF or PO is a build-time dump from the same data, and it can be
  written the day somebody needs it.
- **No locale switch without a navigation.** Changing language re-renders the
  document. A locale that were a signal would force every projected message to
  carry every language, which is the one thing this design exists to avoid.

## Stage 1: the locale

One declaration, at the crate root, because everything below resolves it as
`crate::Locale`:

```rust
exos::locales! {
    De = "de",
    #[fallback]
    En = "en",
}
```

The list stays alphabetical, and the fallback is marked rather than positional,
so [ordering.md](../../.claude/rules/ordering.md) applies to it like any other
list.

It generates more than a list, and that is the point of having it:

- `enum Locale`, with `ALL`, `FALLBACK`, the declared tag, the writing
  direction, `from_tag`, and `CLDR_VERSION` so an application can say which
  release its plurals came from.
- Per locale, a module named after the tag holding a `Plural` enum with
  **exactly the categories that language reaches** (`de::Plural` has `One` and
  `Other`, `ar::Plural` has six) and a `category` function mapping a count to
  one of them.

Both come from the vendored table described [below](#where-the-data-comes-from),
and only for the locales declared, so an application that ships two languages
compiles two evaluators.

Two supporting types are the framework's rather than the generated code's,
because they are the same in every application:
[`Direction`](../../crates/exos/src/locale.rs), which is what `dir` carries, and
`PluralCategory`, which is CLDR's six keywords. A message never names the
second: it branches on the locale's own `Plural`, which is what makes a missing
translation a non-exhaustive match. `Locale::category` answers in the shared
spelling instead, for the two places that cross locales, which are a projection
into the browser and a test.

**"Exactly the categories CLDR gives that language" turned out to be a
slightly different sentence from the one this document opened with.** Whole
number counts do not reach every category CLDR lists. Czech, Slovak, Manx,
Lithuanian and Samogitian each have a `many` that is nothing but "there are
digits after the decimal point", so with the operand set stage 1 supports it can
never apply, and the generated enum does not carry it. Emitting it anyway would
demand a translation of a string nothing can render, which is a string no
translator can check. The cost is that decimal counts, an [open
question](#open-questions) below, would add a variant to five languages and
break every message written in them. That is the same guarantee working, at the
moment it becomes true, rather than a silent change.

The reverse also happens and is left alone: Polish reaches `one`, `few` and
`many` but never `other` with a whole number, and `pl::Plural::Other` exists all
the same, because it is the arm the rule list ends in.

### Resolving it

In order, first hit wins:

1. A `Locale` already in the [request scope](../../crates/exos/src/scope.rs),
   put there by the application.
2. `Accept-Language`, matched against the declared tags by RFC 4647 lookup.
3. The fallback locale.

`exos::locale()` answers with a `Locale` and never an `Option`, because step 3
always succeeds. Outside a request it panics exactly as `exos::scope()` does.
Inside a live fragment it answers once `Locale` is a declared
[dimension](dimensions.md) and panics until then, which is stage 4.

### The override, and why nothing is persisted

A signed-in reader's language lives in their profile. The application already
resolves a session name to a viewer once per request, and that is where the
override belongs:

```rust
if let Some(who) = data::<Sessions>().viewer(&name).await? {
    scope().set(who.id);
    scope().set(who.locale);
}
```

exos writes that nowhere, and will not grow a place to write it. Three
reasons, in increasing order of how much they would hurt:

- **There is nothing to write to.** exos holds a session's name and none of its
  contents, which [stage 3 of sessions and
  identity](sessions-and-identity.md#stage-3-and-no-store-at-all) argues at
  length. A locale would be the first exception, and it is not special enough
  to be one.
- **A copy drifts from what owns it.** The profile is the preference. A second
  copy in a cookie disagrees with it the moment the reader changes their
  language on their phone, and the tab holding the stale copy is the one that
  looks broken. It is the same argument the runtime makes for not mirroring
  server state into a signal.
- **A cookie is not exos's to set.** Whether a language cookie needs consent is
  a question about a jurisdiction and a product, not about a framework. An
  application that wants an anonymous language switcher writes one cookie and
  reads it in step 1, in code it can point at.

### What the document carries

The resolved locale is rendered onto `<html>` as `lang` and, where the script
needs it, `dir`. Not decoration: the runtime reads
`document.documentElement.lang` for every `Intl` call in stages 3 and 5, so the
attribute is the contract between the two halves.

Where resolution actually consulted `Accept-Language`, the response gets `Vary:
Accept-Language`. Where the application overrode, it did not vary by the header
and should not claim to, which means an application serving per-reader pages
owns its own caching policy, the same hazard the guide already names for pages.

## Stage 2: messages

```rust
exos::messages! {
    clear_selection {
        De = "Auswahl aufheben",
        En = "Clear selection",
    }

    items_selected(count: Plural) {
        De { One } = "{count} Element ausgewählt",
        De { _ }   = "{count} Elemente ausgewählt",
        En { One } = "{count} item selected",
        En { _ }   = "{count} items selected",
    }

    assigned(to: Assignee, count: Plural) {
        De { .. }         = "…",
        En { Me,    One } = "{count} file assigned to you",
        En { Me,    _ }   = "{count} files assigned to you",
        En { Other, _ }   = "{count} files assigned to {to}",
    }
}
```

Generating `t::clear_selection()`, `t::items_selected(count)` and
`t::assigned(to, count)`, which return `String` and read the locale from the
request scope. A message with no slots is text, so interpolating one into a
[`view!`](../guide.md) escapes it like any other string.

Arms read like a `match`: in order, first wins, `_` and `..` as wildcards. That
order carries meaning, so arms are exempt from the alphabetical rule while the
messages and the locales around them are not.

**The macro is usable more than once**, so messages live next to the feature
that uses them rather than in one file that every branch touches.

### Why the checking works across invocations

A proc macro cannot see another invocation, so "every message covers every
locale" cannot be a check the macro performs. It does not have to be. The macro
generates a `match` per message over `crate::Locale`, and inside each arm a
`match` over that locale's category enum, and then rustc does the work:

| what is wrong | what the compiler says |
| --- | --- |
| a locale is missing from a message | non-exhaustive match on `Locale` |
| a category is missing for a locale | non-exhaustive match on `de::Plural` |
| a category does not exist in that language | no variant `de::Plural::Few` |
| a branching enum gained a variant | non-exhaustive match, at every message |

Adding a locale to `locales!` therefore breaks every `messages!` block in the
workspace until it is translated, which is the strongest guarantee available
and costs no tooling. It is also the sharpest cost in this document, and the
one most likely to be argued about later.

`crate::Locale` is a convention rather than a lookup, with
`exos::messages!(in path::to::Locale { ... })` as the escape hatch. The
convention is what keeps the common case free of a path in every block, and the
escape hatch exists because the convention breaks for a library crate.

**Messages belong to an application.** A library cannot know the locale set it
will be compiled into, and a library that wants translatable text takes it as a
parameter. That is a limit, stated rather than designed around, because the
alternative is a runtime registry and this design exists to avoid one.

### Branching versus interpolating

A parameter that is **branched on** must have a finite, enumerable domain. A
parameter that is only **interpolated** can be anything that renders.

`Plural` and `bool` qualify. An application enum qualifies by deriving
`exos::Enumerable`, which is what makes `Assignee::Me` legal in an arm and, in
stage 3, what lets the server walk the domain to project it. The rule is
carried by a trait bound rather than by a check inside the macro, so getting it
wrong is an ordinary trait error at the call site.

Interpolated numbers are formatted with the locale's symbols, generated from
the same vendored table. That is the server half of the agreement stage 3 makes
with `Intl.NumberFormat`.

An interpolated parameter appears wherever the translation puts it, as many
times as it likes, or not at all. Word order is the translation's business and
nothing in the call site knows about it.

### Slots: structure without half sentences

A sentence with a link in it cannot be composed from two messages. The link
lands in a different place in German, and a translator handed `"Please accept
the "` and `" before continuing"` has been handed two things that are not
sentences and cannot be checked.

A **slot** keeps the sentence whole, including the words inside the link, and
lets the call site supply only the wrapper:

```rust
exos::messages! {
    accept_terms(terms: Slot) {
        De = "Bitte {terms}Nutzungsbedingungen{/terms} akzeptieren.",
        En = "Please accept the {terms}terms of service{/terms}.",
    }

    unread(count: Plural) {
        En { One } = "You have {b}{count} unread{/b} message",
        En { _ }   = "You have {b}{count} unread{/b} messages",
    }
}
```

```rust
t::accept_terms(|inner| view! { <a href="/terms">{inner}</a> })
```

A `Slot` parameter is a wrapper, `FnOnce(Markup) -> Markup`, so the href, the
classes and the routing stay in Rust while the words stay in the sentence.
`{b}` and `{i}` are the same mechanism with a built-in wrapper, since emphasis
falls on different words in different languages and there is nothing for a call
site to decide about it.

**The escaping rule is not weakened, which is the point.** The macro splits the
string at compile time into literal text and slot boundaries, and the generated
code writes escaped text and calls the wrapper. Nothing from a message string
is ever parsed as HTML, so the only structure a translation can express is a
slot that was declared in Rust.

Three more compile-time checks follow: slots are balanced, every declared slot
is used exactly once in every arm, and no arm names a slot that was not
declared. A translation that drops the link fails the build rather than
shipping a sentence nobody can click.

A message with a slot returns `Markup` rather than `String`, and interpolates
through the one unescaped path in
[render.rs](../../crates/exos/src/render.rs).

## Stage 3: projecting a message

The argument type decides where the message is resolved, which is the whole
trick and the reason one call site can serve both sides:

```rust
t::items_selected(3)                   // String,     resolved here
t::items_selected(sel.picked.len())    // Js<String>, resolved in the browser
t::assigned(who, sel.picked.len())     // Js<String>, only count projected
```

Only the dimensions that are actually client-side are enumerated. `who` is a
server value on that third line, so one message survives the crossing rather
than all of them.

### What crosses, and where it lands

Not an inline literal per binding. The runtime compiles an expression once and
[caches it by its source
string](../../crates/exos/js/runtime.js), so a variant table written into every
row's attribute would compile one function per row. Instead a message projects
into a document-level table, keyed by a hash of the message, the locale and the
projected dimensions, and the binding references the key:

```html
<span data-text="msg('a3f1', $.picked.length)"></span>
```

One compiled function for a thousand rows, and identical messages on a page
collapse to one entry. Because the key is a content hash, a patch carrying its
own table entries merges them idempotently, which is what makes a fragment
self-contained without a second mechanism.

`msg`, `plural` and `num` join the helper set the runtime already passes into
compiled expressions alongside `get`, `post` and `attr`. They are parameters
rather than generated source, so the precompiled mode that [loose
ends](loose-ends.md) wants for a strict CSP keeps working: a table entry is a
call, not a program.

Substitution writes text into `textContent` or into an attribute value, never
into parsed HTML, so a projected message cannot become markup by accident.

### The cross product

Several branching client parameters multiply. With `Enumerable` the domain
sizes are const, so the product is a const expression and the generated code
carries a const assertion: exceeding the cap is a compile error naming the
message, rather than a hundred variants nobody notices in the payload.

### A message with a slot does not project

Not in this stage, and stated as a limit rather than left to be discovered. The
projection writes text through `data-text`, and a slot would need the runtime to
build nodes and interleave them with text parts instead. Passing a `Js<T>`
argument to a message that has a slot is a compile error saying exactly that,
which is better than a sentence that renders its own tags as words.

The composition that does work is to project the text and wrap it at the call
site, since a wrapper is markup the server already renders.

## Stage 4: fragments, and rendering outside a request

**Depends on [dimensions](dimensions.md)**, and is the reason that document was
written.

A live fragment renders through `detached` and cannot read the request scope,
so `t::` cannot reach the locale there. Being a topic dimension is how it
reaches it, and `locales!` registers `Locale` as one by itself:

```rust
exos::locales! { … }                // registers the topic dimension

#[exos::live]
fn lot(id: LotId) -> Markup { … }   // signature unchanged
```

Automatic rather than a second declaration, because the failure mode decides
it. An application that declares its languages and then forgets to declare the
dimension delivers German markup to an English reader, silently, which is the
class of bug the topic invariant exists to make impossible. What it costs is a
live fragment with no words in it rendering once per watched language instead
of once, and that is waste rather than wrongness.

`exos::locale()` then answers inside the body, the value is hashed into the
topic alongside `id`, and `exos::scope()` still panics there, so the invariant
reads the same as it always did with one more term in it.

The version without dimensions is a `locale: Locale` parameter on every live
fragment in the application. It works, and it is what stage 4 said before the
dimension design existed, but it puts a fact about the viewer into the argument
list of every fragment that renders a word, and an application will get one of
them wrong.

Three consequences worth being explicit about.

**A publish fans out.** One raise renders the fragment once per locale being
watched, and each render reaches the tabs subscribed to that topic. The cost is
real and it is proportional to languages in use rather than to viewers, which
is the distinction that makes it acceptable.

**A patch cannot arrive in the wrong language.** The locale is in the address,
so a page rendered in German subscribes to the German topic. A mismatch between
what a page was rendered as and what a publisher computes delivers nothing at
all, which is visible, rather than the wrong words, which is not.

**Out-of-band renders name the locale.** A directed effect carrying text is
built by a sender who is not inside the recipient's request, so the recipient's
locale is looked up by the application at the send site, exactly like any other
fact about them, and passed explicitly. That gives every message two call forms,
the scope-reading one and the explicit one, and the naming of the second is an
open question below.

## Stage 5: dates, times and relative time

The server sends the instant. The browser formats it.

```html
<time datetime="2026-08-19T09:00:00Z" data-text="date($el.dateTime, 'long')">
  2026-08-19T09:00:00Z
</time>
```

The element's own text is the ISO instant, so a crawler and a reader without
the runtime see something true rather than something wrong. `date`, `time` and
`ago` join the helper set, backed by `Intl.DateTimeFormat` and
`Intl.RelativeTimeFormat` keyed off `document.documentElement.lang`.

Nothing new is needed to survive a patch. A binding owns what it writes, and
the runtime re-runs an element's bindings after a morph, so a fragment that
arrives with the ISO text is reformatted the moment it lands. Relative time
needs one interval that re-runs the visible `ago` bindings, and that is the
only piece of state this stage adds.

In a message, an `Instant` parameter makes the whole message resolve in the
browser, which is the type system expressing that the zone is not a server
fact:

```rust
t::due(when)    // Js<String>, even in a server render
```

So a message carrying a date cannot be used where only a `String` will do, in a
`<title>` or an email body. That is a compile error rather than a wrong time,
and the escape hatch is honest: an application that knows a reader's zone,
because it is in their profile, formats the date itself and passes the string
as an interpolated parameter. Email has no browser and was always going to work
that way.

exos does not ask the browser for its zone. It could, on the stream's `GET`,
and it would still be wrong for the first render, which is the one that matters.

## Where the data comes from

CLDR, vendored into [exos-cldr](../../crates/exos-cldr/src/table.rs) as
committed generated source, produced by
[generate.mjs](../../crates/exos-cldr/generate.mjs) beside it, which a
maintainer runs with `npm run cldr` when CLDR is bumped. Not fetched during a
build: a build that reaches the network is the leak
[nix.md](../../.claude/rules/nix.md) is about, and a build script that parses
CLDR would pay for it on every clean checkout. What release a checkout is on is
a line in `package-lock.json` rather than whatever a network answered with that
afternoon.

Three slices, and nothing else:

- **Cardinal plural rules**, which are a small expression language over the
  operands `n, i, v, w, f, t, c`. With integer counts only, `v` through `c` are
  zero and most languages collapse to one or two comparisons.
- **Writing direction**, for `dir` on the document.
- **Number symbols**: decimal separator, group separator, minus sign, percent,
  grouping sizes. Not vendored yet. They arrive with the number interpolation in
  stage 2, since a column nothing reads is a column nothing checks.

### What the first two slices measured

The collapse is what the rest of stage 1 was staged on, so here is what it
actually came to, against cldr-core 48.2.0:

- **223 locales**, in 32 distinct rule shapes. `und` is dropped: it is the tag
  for a language nobody has determined, and the one tag ICU cannot check a
  fixture row for.
- **186 of the 223 ask two comparisons or fewer.** Thirty-three have a single
  category and never look at the count, 120 ask one question, 33 ask two. The
  tail is Slavic and Celtic: Polish, Russian, Belarusian and Ukrainian ask
  seven, Breton eight, and Cornish ten.
- **The whole table is 5,400 lines of Rust**, 160 KB, and takes about 100 ms to
  compile. That is the reason it is its own crate rather than a module of
  [exos-macro](../../crates/exos-macro): the data changes twice a year and the
  macro changes whenever it is worked on, so the two should not recompile each
  other. An application still carries only the locales it declared.
- **Every rule list ends in an unconditional `other`**, and nothing before it is
  unconditional, which is what lets the generated function be a chain of `if`s
  ending in an `else`. The generator refuses to write a table where that stops
  being true rather than emitting unreachable code.

A tag is looked up whole and then by dropping subtags, so `de-AT` finds `de` and
`pt-PT` finds itself, which is one of two tags in CLDR's table with a subtag at
all. Direction comes from the script CLDR expects the language to be written in,
except where the tag names a script itself: `pa` is Gurmukhi and `pa-Arab` is
not, and both resolve to the same plural rules.

## Testing

The interesting risk is not that the generated code is wrong today. It is that
our vendored CLDR and the browser's ICU disagree tomorrow, in one language,
about one number.

A committed fixture makes that a test failure in two places rather than a bug
report. [fixture.json](../../crates/exos-cldr/fixture.json) holds, for every tag
in the table, the category of each of 97 counts, chosen to sit on the edges the
rules have: a run through the first hundred, runs around 100 and 1000 for the
teens exceptions Slavic and Celtic rules carry, and a tail for the millions rule
French and Breton use.

Both suites read it and neither needs the other to have run.
[crates/exos/tests/cldr.rs](../../crates/exos/tests/cldr.rs) declares every tag
in one `locales!` and asserts the generated evaluator reproduces the file, which
also means `cargo test` compiles the evaluator for all 223 languages rather than
for the two an example would carry. It sits in the exos crate rather than beside
the table because reading the fixture from Rust takes the macro, and the macro
takes exos.
[fixture.test.js](../../crates/exos-cldr/fixture.test.js) asserts
`Intl.PluralRules` reproduces it, in plain node rather than in jsdom, since
`Intl` belongs to the language rather than to the document. Both name the
language and the count they
disagreed about, and both name the CLDR release they were reading. A tag ICU has
never heard of is skipped rather than failed, because a vendored CLDR newer than
the runtime's ICU is a legitimate state to be in, and the number skipped is
asserted so that the check cannot quietly degrade to checking nothing. Today it
skips none.

The fixture holds no formatted numbers yet. It will when the symbols are
vendored, and a fixture half that only one side can check would not be one.

Beyond that: negotiation is a unit test, a fragment rendering in two locales
produces two topics, and the compile errors in stage 2 are worth a `trybuild`
case each, since an error message that stops naming the missing locale is the
kind of regression nothing else catches.

## What it costs

- **Adding a locale breaks the build until every message is translated.** The
  guarantee and the cost are the same sentence. An application mid-translation
  has no way to ship, and the pressure will be to add a fallback that silently
  renders English. Refusing that is the decision; it should be refused on
  purpose rather than by omission.
- **A publish renders once per locale in use**, and a page that is served in
  eight languages has eight of every live topic. That cost belongs to
  [dimensions](dimensions.md), which is where it is paid and bounded, but it
  arrives here because localization is what asks for it first.
- **Message text lives in Rust source**, so a translator cannot touch it until
  the export exists.
- **A vendored table nobody reviews by eye.** The fixture is the only thing
  standing between it and a quiet wrong answer in a language none of us reads,
  which is why it was the first thing built rather than the last.
- **`crate::Locale` by convention** is magic, in a codebase that has mostly
  avoided it. It buys one absent path per block.
- **Slots put a second syntax inside a string.** `{count}` interpolates and
  `{terms}…{/terms}` wraps, and a translator has to keep the pair intact. The
  build catches a dropped one, which is the mitigation, but it is still a
  notation somebody has to be taught.

## Open questions

- **What the explicit-locale call form is called.** A trait method on `Locale`
  reads well at the call site (`locale.items_selected(3)`) and costs an import;
  a second free function (`t::items_selected_in(locale, 3)`) costs a name. The
  choice is not obvious and does not block stages 1 to 3.
- **What the built-in slots are and what they render.** `{b}` and `{i}` as
  `<strong>` and `<em>` is the obvious pair, and the argument against a longer
  list is that every entry is a decision about semantics made on a
  translator's behalf.
- **Whether a slot can project later.** It needs the runtime to interleave text
  parts with cloned nodes rather than write a string, which is a real piece of
  machinery and worth building only if a projected sentence with a link turns
  out to be common.
- **Whether exos sets `Vary: Accept-Language` itself** or leaves the header to
  the application, given that an override means the response did not vary by it.
- **Ordinals** ("3rd"), a separate CLDR table and a second parameter type.
- **Decimal counts** ("1.5 hours"), which need the full operand set rather than
  the integer collapse in stage 1, and which would hand `many` back to the five
  languages that lose it there, breaking every message written in them.
- **Currency**, which is listed as out of scope and will be asked for anyway.
