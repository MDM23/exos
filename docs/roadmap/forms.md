# Forms

Rules written once, checked on both sides, and the one round trip that carries
what only the server knows.

Status: stages 0, 1, 2, 3, 5 and the gate half of 4 are built, aggregation
included. What is left is patterns (1a), `required_when` and stage 6.

[`examples/signup`](../../examples/signup) is the form written by hand against
the surface that existed before any of this, so what the stages are worth is
measured rather than argued: each one says what it saves and what it was wrong
about, and each one that lands takes something back out of the example. It moves
an edge the [guide](../site/content/models.md#rules-on-a-model) had already
closed, which is the next section.

## What this reopens

The guide lists "no client-side validation rules, they round-trip, debounced"
among the edges. That is being moved deliberately: a rule about the **shape** of
a value (required, a length, a pattern, required only when a box is ticked) is
answered in the browser while it is typed, and only a rule that needs the server
(this email is taken, this coupon is spent) costs a request.

The reason the edge held is sound and does not go away: a mirrored rule is a
rule written twice, and two copies drift. What answers it is that neither copy
is written by hand. A rule is a value and exos writes both of its halves, so
there is one declaration and no way to disagree with it.

The other half of the guide's line is a claim with nothing behind it: nothing in
the tree debounces anything. That is stage 3, and it turns out not to be about
validation at all.

## Stage 0: write one by hand

**Done**, as [`examples/signup`](../../examples/signup): a registration form
with fields of several types, a section that applies only when a box is ticked,
a searchable multi-select, repeating rows added by a button, and one rule the
server alone can answer. Written entirely with today's surface, so what a form
costs without any help is a thing to read rather than a thing to argue about.

It was built first because every stage below it was a guess, which is the method
the rest of this roadmap was built by: stage 4 of [sessions and
identity](sessions-and-identity.md) argued for a `reconnect` step until
[`examples/auction`](../../examples/auction) showed it loses a race it cannot
win. The guesses named as most likely to be wrong were stages 4 and 5, and stage
5 was the one that moved.

### What it cost

The numbers stage 2 is written against. Thirteen model fields for a form with
six, because a message can only be written where a handler can reach it. A
report function rewriting every message on every reply, empty ones included,
since a message nobody clears outlives the value it described. A table pairing
each field with the id its input was rendered under, so a refusal can move the
caret. Two error elements per field, because no expression can choose between
the client's complaint and the server's. And a forty-line helper to render one
labelled input.

### What was cheaper than it looked

The widget half. **Render everything and gate it with `show`** turned out to
answer the whole class: the server renders every option and every chip once,
each carrying its own condition, so filtering and showing what is picked are
both per-element visibility and neither needs the client-side loop exos refuses
to grow. Multiple selection is a `Vec<T>` with checkbox bindings and no per-row
bookkeeping, as [`examples/playlist`](../../examples/playlist) already showed.
The limit is that the whole list sits in the document whether or not it is on
screen, which is right for nine options and wrong for nine thousand.

### What it found

One bug, fixed rather than recorded: **`Js<String>::contains` could not be
called at all.** Its bound asked for an expression yielding an expression, which
nothing implements, and it had no test. That single call is the whole of a
client-side filter, so this was not a corner of the API.

Four gaps in the vocabulary, each left with a comment in the example where it
bit. None needs a document:

- **Nothing can say "focus left this widget".** Delegation dispatches to the
  nearest element carrying the attribute, so a click outside a dropdown reaches
  no handler at all, and a `data-on-click` on `<body>` is shadowed by every
  inner click handler rather than running after it. `focusout` bubbles and is
  the accessible answer anyway, but deciding whether focus went somewhere inside
  means reading `relatedTarget` and asking the DOM, and
  [`Event`](../../crates/exos/src/attributes/handler.rs) exposes neither. The
  example closes its dropdown with a button.
- **No case folding on `Js<String>`**, so the filter went through `Js::raw`. A
  search that matches only the capitalisation somebody happened to type is not a
  search. **Closed**: `to_lowercase`, and the example's escape hatch with it.
- **No concatenation on `Js<String>`**, so "3 selected" was three elements.
  **Closed**, and on every `Js<T>` rather than on `Js<String>`: the piece that
  needs joining is usually the one that is not a string, and `concat` builds a
  template literal for the same reason, since `+` over two numbers adds them.
  What `aria-activedescendant` still wants is the clamped index arithmetic
  [combinator.rs](../../crates/exos/src/js/combinator.rs) has no `min` or `max`
  for.
- **`attr` cannot write `aria-invalid="true"`.** The runtime writes an empty
  attribute for a true boolean, and `aria-invalid=""` is read as false, so the
  hook [stage 2](#stage-2-the-same-rule-in-the-browser) commits to cannot be
  said today. That makes it the binding's to write directly rather than an
  expression's, which is what that stage says; the example marked its invalid
  controls with a class until stage 2 landed.

And one thing stage 1 has to change rather than extend: **`ModelRejection`
answers with a text body.** A malformed body comes back as `missing field
\`name\`` in `text/plain`, which the client logs and the page ignores. Stage 1
wants that path carrying an `Effect`, which is a change to a type that already
exists.

## Stage 1: a rule is a value the server checks

**Done.** Rules are declared on the model, because that is the one place the
template, the body and the handler already agree on:

```rs
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Signup {
    #[valid(required, length = 3..=40)]
    name: String,

    #[valid(required, email)]
    email: String,

    invoice: bool,

    #[valid(required)]
    vat_id: String,
}
```

**Nothing calls the validator.** [`Model<T>`](../../crates/exos/src/model.rs) is
already the extractor and already refuses a body it cannot read, so it refuses a
body that breaks a rule the same way: a `422` carrying an `Effect` that writes
the failures into a signal. A handler body therefore runs only against a value
whose shape held, and a rule cannot be forgotten at a call site because there is
no call site. Where a handler wants to see what failed, `Result<Model<T>, _>` is
axum's existing escape hatch and costs exos nothing.

**exos ships no message text.** It cannot: an application's languages are its
own, and [localization](localization.md) exists so that a string in a page is a
`messages!` arm the compiler holds to every locale. So a violation is a value
(`Required`, `TooShort { least }`) and the application turns it into text in one
function it writes, once, with the same macro as everything else it says.

### What it found

**A rule is not a trait object with two evaluators.** This stage drew a
`Rule<T>` with a `check` and an optional `js`, and there is no such trait in
[valid.rs](../../crates/exos/src/valid.rs). The declarations are read by the
macro and never become values, so a rule needs no runtime representation at all:
what the two halves share is not a rule type but a *question about a type*.
[`Presence`](../../crates/exos/src/valid.rs) and `Length` are those questions,
each answering twice in one impl block, and that is what a shape rule is made
of. The vocabulary is smaller than the trait would have been and the two halves
still cannot drift, which was the whole requirement.

**A length has to be counted the way JavaScript counts.** `String::len` is
bytes, `chars().count()` is scalar values, and the browser counts UTF-16 code
units. The three agree until the first emoji, at which point a form accepts what
it had already shown as too long. `Length for String` counts `encode_utf16()`
for that reason, which is the kind of thing only building it finds.

**A handler still refuses, and it has to look identical when it does.** A rule
the model cannot hold, such as whether a discount code is spent, ends in a
`Refusal<Signup>` that writes the same record with the same status and the same
caret move, so nothing a viewer sees says which side decided. That was not in
this stage's drawing, and it is most of what makes the extractor's own refusal
usable rather than a second mechanism beside it.

## Stage 1a: patterns, and the subset that makes them safe

A custom pattern is the one shape rule whose two halves are written in different
languages, so it is the one that can quietly disagree. It is still worth having,
and the subset that makes it safe is a real boundary rather than a hope.

**The engine is [`regex-lite`](https://docs.rs/regex-lite), and it is chosen for
what it lacks.** Its Perl classes are ASCII-only: `\d`, `\s`, `\w` and `\b`
cover ASCII codepoints and nothing else, `\p{...}` does not exist, and
case-insensitivity is ASCII-only. That is JavaScript's rule, exactly. The full
`regex` crate is the wrong choice here *because* it is better: its `\d` is
Unicode-aware, so `[0-9]{5}` behaves but `\d{5}` accepts an Arabic-Indic numeral
on the server that the browser had already refused, and that bug is invisible
until somebody types one. It also costs nothing: `regex-lite` has no
dependencies at all, where `regex` brings `regex-automata`, `regex-syntax`,
`aho-corasick` and `memchr`.

**The macro is what makes the subset a build error rather than a divergence.**
A pattern is written in Rust source, so `#[model]` sees it while it expands: it
can parse it, refuse what the subset excludes, and emit both forms itself. A
construct the two engines disagree about therefore fails to compile, at the
attribute, which is the same promise the rest of exos makes.

Four differences to close, and only one of them needs work:

- **Perl classes.** ASCII on both sides already, by the choice of engine. This
  is the one that would have hurt and it is closed by construction.
- **Lookaround and backreferences.** `regex-lite` has no backtracking, so it
  refuses them on its own. The exclusion is free and the error message is the
  engine's.
- **`.`** excludes `\n` in Rust and `\n`, `\r`, `\u2028` and `\u2029` in
  JavaScript. The macro lowers it to an explicit class so that one pattern means
  one thing. This is the whole of the work.
- **Flags.** None are offered. `i` is ASCII-only in `regex-lite` and Unicode
  case-folding in JavaScript, which is the first divergence wearing a hat, and
  the other flags are meaningless for a single value.

**Anchoring is implicit at both ends**, because `is_match` and `RegExp.test`
both search rather than match and a validating pattern always means the whole
value. HTML's own `pattern` attribute settled this the same way, wrapping as
`^(?:...)$`, so the rule is already the one people expect.

**A pattern is named**, which is what gives its failure something to say:

```rs
exos::pattern!(POSTCODE = r"[0-9]{5}");

#[valid(required, matches = POSTCODE)]
postcode: String,
```

The macro validates the subset at that declaration rather than at every use, the
name is reusable across models, and the violation carries it, so the
application's message function gets one arm per pattern in the same shape as
every other message it writes. An anonymous pattern could only produce "wrong
format", which is the error message everybody hates.

That leaves the named rules (`email`, `url`, `digits`) worth shipping anyway, as
two hand-written halves each. They are what most forms actually reach for, they
say what they mean at the call site, and their messages are better than a
pattern's can ever be.

## Stage 2: the same rule in the browser

**Done.** Nothing here is declared by hand, and that was the requirement rather
than a nicety. Writing [`examples/signup`](../../examples/signup) by hand cost
thirteen model fields for a form with six, a `report` function rewriting every
message on every reply including the empty ones, and a table pairing each field
with the id its input was rendered under so that a refusal could move the caret.
None of that was the form's doing. It was the shape of what was missing.

**AngularJS had this right and it is worth naming what it had.** An `ngModel`
kept a record per field, `$error`, `$dirty`, `$touched`, `$pending` and
`$valid`, put `ng-invalid` and `ng-dirty` on the control, and aggregated
validity up to the form. A template wrote
`signup.email.$touched && signup.email.$error.required` and declared none of it.
Two things to take from that and two to leave.

**Take the record.** `#[model]` generates one more document signal per model
holding the validation state, keyed by each field's generated name, and the
handle reads it per field:

```rs
view! {
    <input type="email" {bind(&form.email)}>
    <p class="error" {text(form.email.error())}></p>
}
```

That is a validated field, whole. No error field on the model, no `touched`
signal, no second element for the client's own complaint, no precedence to
arrange at the call site and nothing to clear. The key in the record is the
field's **generated** name rather than `email`, so [the wire stays
private](../site/content/models.md#the-wire-is-private) and the state is
reachable only through a handle, like everything else a model owns.

**Take the control's own marking**, which is the `aria-invalid` below and is
`ng-invalid` with a better name.

**Leave the string names.** `signup.email.$error.required` is three strings a
rename breaks in silence, which is the failure exos exists to make impossible.
The field is `form.email`, the same handle the binding took, and
[`Field<M>`](../../crates/exos/src/signal.rs) is there for the places that need
to name one to a function.

**Leave the `$parsers` and `$formatters` pipeline.** A view value and a model
value transforming into each other in both directions was the most confusing
part of that API, and here the server is the authority anyway.

**One record, written by whichever side last judged the value.** The server
writes it with a single `Effect::set` carrying the whole thing, so a reply that
fixes a field clears it by not mentioning it. The control writes its own slot as
it is typed into. Dirty is the client's alone and never leaves the browser: the
binding already knows a control changed, so a reply cannot clobber what somebody
is typing and a submission does not carry state the server would throw away.

**Which is what makes `error()` gated rather than the template gating it.** A
client rule says nothing until its field is dirty, because an untouched field is
not a wrong field and a form that is red before it is read is worse than no
validation at all. A server message shows whenever there is one, because the
server speaks only after a submission, so the arrival of a message is already
the evidence that one happened. AngularJS needed `$submitted` for exactly this
and exos needs no flag for it.

**Validity aggregates, and it costs nothing.** Built, and cheaper than this
paragraph first claimed: `form.valid()` is the record, empty, so
`{attr("disabled", !form.valid())}` on the submit button is the whole of what
AngularJS needed a form controller for. `form.dirty()` is beside it. What it
deliberately does not do is disable a form that has never been touched, which
hides the button before anybody has had a chance to be wrong.

**A bound control marks itself, and the marking is not exos's to name.**
[`bind`](../../crates/exos/src/attributes/helper.rs) already carries the field's
name and is already per-element state the runtime applies on insert and disposes
on removal, so it is the one place that knows both which control this is and
whether its field is currently in error. It writes `aria-invalid` there:

```css
input[aria-invalid="true"] {
    border-color: var(--error);
}
```

That is the whole styling story, and it is deliberately a standard attribute
rather than an `exos-invalid` class. A screen reader is told the same thing the
border says, the selector is one every stylesheet already knows how to write,
and exos adds no vocabulary to a language that had a word for this. The
consequence worth stating: **the simplest usable form has no error markup at
all.** Fields turn red on their own, and a template adds a `<p>` only where the
reason is worth words.

Two things it does not do. It does not mark a field that is not dirty, since
that is the rule above and this is the same state seen from the DOM. And it does
not write `aria-describedby` at the error element, which would complete the
accessible pairing and needs an id on both halves; that is a second decision and
probably belongs to whatever helper renders the message.

### What it found

**Two owners do not need two stores.** This stage drew the client's verdict and
the server's as separate things, which is where the two rules above came from:
one to clear a stale server message on edit, one to decide which of the two
wins. Neither survived. The control writes its answer into the same slot the
refusal writes, so recomputing it *is* clearing the old one, and there is
nothing for a precedence rule to choose between. That deleted a `forget()`
helper, a `dirty` parameter threaded through the expression, and the second
error element per field the stage-0 example needed.

**Dirty has to be a signal.** Held in a plain `Set`, it is not reactive: an
effect subscribes to what it reads, so a rule gated on one evaluates once and
never again. It lives in the signal store under a `~dirty/` prefix for that
reason, which a client test caught and nothing else would have.

**The aggregation is not the rules folded, it is the record read.** This stage
promised one expression the macro would build out of every field's rules, and
that would have been both more code and wrong: an expression on the form can
only reach the fields the document declares, which leaves out every row, and it
can only ask the rules the browser has, which leaves out everything the server
alone decided. The record already holds all of it, keyed the same way, so
`valid()` is `Object.keys` of one signal and does not grow with the model.
`dirty()` is the same shape: one flag beside the per-field ones the binding
already writes, rather than a fold over them.

**A form gated on the record has to be a form that can be submitted again**, and
that is what building this found. Every message needs something that retires it,
or the button that reads them locks. A field's own edit already did that where
the field had rules of its own; a field only the server can judge had none to
recompute, so its message outlived every value it was ever about. Two more had
the same shape and neither was visible until a button depended on them: a gate
that shuts leaves complaints about a section nothing on screen can reach, and a
group that changes shape leaves messages about rows that have moved. So an edit
retires what was said about the field **and about the fields it arms**, and a
row added or removed retires what was said about the rows from there on. That is
one rule seen three times: a verdict about a value nothing is asking about any
more is worse than no verdict.

**`aria-invalid` is written out rather than toggled**, which is stage 0's fourth
finding landing where it was aimed. `toggleAttribute` produces `aria-invalid=""`
and an empty token attribute reads as *false*, so the binding sets the literal
string. The example's stylesheet dropped its `.invalid` class for the standard
selector and the template dropped the `class(...)` block with it.

## Stage 3: one debounce, three features

**Done.** The mechanism is a debounced call, and it reads like the recorder's
existing `when`:

```rs
on_input(|_| debounce(300, || search::post(&query)))
```

The key is the call site, hashed the way
[`signal`](../../crates/exos/src/signal.rs) hashes its own, so nothing invents a
name for it.

That one thing was drawn as delivering three of this document's wishes, which
was the sign it is the right shape rather than three features wearing a coat: a
search field that submits as it is typed, a server rule that answers while a
field is still being edited, and any other action that should not fire per
keystroke.

**Debouncing alone is not enough and this is the part that gets forgotten.** Two
requests in flight can answer in the other order, and the older one then paints
over the newer. So a debounce key also carries last-response-wins: a reply older
than the newest request under that key is dropped. Without it a search bar shows
the results for a prefix of what is in the box, intermittently, on a slow
connection, which is a bug nobody can reproduce.

Nothing else about a search field is new. The query is a model field, the
handler patches a fragment, and `aria-busy` on the element already says the work
is happening.

### What it found

**A call site is not enough of a key.** It had to be the call site *and* the
element, resolved through the same DOM scopes a signal resolves through. A
helper called once per row is one call site, so a key that stopped there would
put every row on one timer, and typing in the second row would cancel the save
the first was about to make. That is the same thing
[`signal`](../../crates/exos/src/signal.rs) already says about names, arrived at
from the other direction, so the runtime walks to the nearest declaring element
and prefixes with what it finds. A client test fails without it.

**It delivers two of the three wishes, and the third is stage 5's wall.** A
server rule cannot answer while one field is being edited, because the generated
caller sends a model and only a model: checking the discount code alone would
mean posting the whole form, which runs every declared rule and lights up every
field somebody has not reached yet. That is not the debounce's doing. It is
[the missing projection](#stage-5-repeating-groups) seen from a third side, and
it is now an edge in the guide rather than a claim this stage can make.

**The key reaches the request through the frame it was armed in.** A debounced
body runs synchronously inside the timer, and a request reads the key before its
first `await`, so last-response-wins needed no plumbing through every helper an
expression might call on the way. The example the roadmap reached for is the
attendee rows, which now save as they are typed rather than on the way out, and
each row's timer is its own.

## Stage 4: rules that only sometimes apply

**Done, as `required_with`.** A section that appears when a box is ticked has to
be checked while it is showing and ignored while it is not. That is a gate on a
rule, and it is written on the field it gates:

```rs
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Signup {
    invoice: bool,

    #[valid(required_with = invoice)]
    vat_id: String,
}
```

```rs
view! {
    <fieldset {show(form.invoice.get())}>/* ... */</fieldset>
}
```

**Rejected: a named group**, declared on the model as
`#[valid(group(billing = invoice))]` and used by both the rules and the
`show`. Its one argument was that the visibility condition and the validation
condition are then provably the same, and a hidden field can never block a
submit nobody can see. It is not worth what it costs: a second concept that
only forms have, a generated constant per group, and an attribute grammar that
grows a scope. `required_with = invoice` says the same thing in the place a
reader is already looking, and the coupling it gives up is one line of a
template away from the rule it has to agree with.

What that gives up is real and is the application's to watch. Nothing checks
that a gate and a `show` agree, so a section revealed by
`invoice && !domestic` whose fields are gated on `invoice` alone is validated
while hidden, and the submit fails with an error nobody can see. The backstop is
stage 2's open question rather than a rule here: a violation on a field with no
error element on screen is worth a debug-build complaint.

**The gate is not a rule, and that is what keeps the vocabulary small.** A rule
asks one question about one value, and letting one read a sibling would drag the
model into every rule that never looks at one. So `required_with` expands to the
ordinary `required` rule under a condition, on both sides: the server runs it
only when the sibling is present, and the recorded expression is the same
question with the same guard in front of it. `Presence` answers the guard, so
the gate needed nothing that shape rules had not already brought.

**Presence is defined per type, explicitly, and this is where it can go wrong.**
An empty `String` is absent, and trimmed, so whitespace does not arm a gate. A
`false` is absent, which is what makes the checkbox above read correctly. `None`
is absent and an empty `Vec` is absent. A number is the trap: JavaScript makes
`0` falsy, so a client half written as `$.qty ? ... : ...` disagrees with a
server that treats zero as a value like any other. The generated expression
therefore tests presence the way the Rust type defines it and never leans on
truthiness, and a plain numeric field, being always present, cannot usefully arm
a gate at all. `Option<T>` is how a number opts in.

`required_when` is the other half and wants a value rather than presence:
`#[valid(required_when(country = "DE"))]`. Equality against a literal is as far
as an attribute should go. Anything that wants the combinators wants a
[server-only rule](#stage-1-a-rule-is-a-value-the-server-checks) and a round
trip, which is the same answer this document gives everywhere else.

**It is not built, and waits for something that wants it.** The machinery is the
gate above with a comparison in place of the presence test, so it is a rule arm
and no new concept, and nothing in the tree needs one yet. Building it now would
be a second spelling of a stage that already works, tested against a field
invented to test it.

### What it found

**A gate needs every field, not only the ruled ones.** The macro had been
collecting rules per field and dropping the fields that declared none, which is
exactly the set a gate points into: `invoice` carries no rules and is what two
other fields are gated on. Reading every field and keeping its wire name beside
it is also what let the key function move to one caller, so the server's half
and the browser's half stopped each deriving it.

**And it has to check the name it was given.** `required_with = invioce` would
otherwise expand into a field access on a struct the author never wrote, and
rustc would point at the expansion. It is refused at the attribute instead,
which is the promise the rest of the macro makes.

## Stage 5: repeating groups

**Done.** This was drawn as the biggest piece by a distance. Writing it by hand
in [`examples/signup`](../../examples/signup) made it the narrowest stage here,
because the half that looked hard turned out to be already built and the half
nobody mentioned is the whole of the work.

**What already works is per-row client state.** A row holds its own
[`signal`](../../crates/exos/src/signal.rs), every row declares the same
generated name because it comes from one call site, and every row is its own
scope, since the runtime keys a scope per element rather than per id. A
declaration is never overwritten, so a patch that re-renders the entire list
leaves half-typed text exactly where it was. None of that needed designing and
none of it is on this stage's list.

**What is missing is a name.** A binding names one signal, and nothing can name
the third row's field, so the rows cannot be part of the submission at all. That
is two failures rather than one:

- **`bind` on a collection is wrong rather than absent** for anything but a
  checkbox. The runtime writes the whole array into the field comma-joined and
  typing writes that string back over all of it, which reads as a broken page
  rather than as a missing feature.
- **A row's own signal cannot be sent.** The generated caller sends a model and
  only a model, so a row that wants to save copies its signal into a one-field
  model first and posts that. The example does exactly this, and the transport
  model is a type that exists for no other reason.

The draft here asked for a projection, `form.lines.at(line.id).quantity`, keyed
by the row's own id. That was built and thrown away; what replaced it is below,
and the short version is that the paragraph above already had the answer in it.
A row is its own scope, so a row needs no name:

```rs
{ form.lines.each(|line| view! {
    <li {line}><input {bind(&line.sku)}></li>
}) }

<button {on_click(|_| form.lines.add())}>"Add a line"</button>
```

**What it cost to not have it is worth recording**, because it is what the
example paid before this landed. The rows were server state, so a half-filled
form was a resource: it survived a reload, two tabs shared it, and an abandoned
one would have had to expire. Every row edit was a round trip. And a message
about a row had nowhere to go, so it was one message for the group.

The error record's keys stop being flat here, which is the one decision this
stage still turns on: a message belongs to a field of a row, so the key is a
path.

### What it found

**A projection is the wrong shape, and ids were the wrong question.** The draft
above asks for `form.lines.at(id).quantity`, which was built and then thrown
away: it made the caller supply ids off data the server had to keep, so adding
a row was a round trip and a half-filled form was still a resource. Every one of
those costs came from wanting a *name* for a row.

A row does not need one. **The runtime keys a signal scope per element**, so a
clone of a `<template>` is its own scope and every row can declare the same
field name and hold its own value. That is the mechanism a row's own `signal`
has used since the start; the only thing missing was that the submission could
not find them. So `each` renders the row markup twice, once into the template
and once per opening row, `add` clones and `remove` unmounts, and the payload
walks the group when the body is built. **Nothing about a row reaches the server
until submit, and the runtime needed no new concept, only three helpers.**

**Position is a good enough identity.** With scopes doing the work there is
nothing to renumber: removing a row removes its element and its signals. Only
the *messages* need an index, since a message comes back from the server keyed
by something, so a row's key is `<group>.<position>.<field>`. What that costs is
that a refusal's messages stop being about the rows they were written for as
soon as the rows move, so they are retired when one is added or removed, which
is where stage 2's aggregation arrived from the other side. It bought away ids,
routes and server state.

**The renaming stopped one level down**, and both sides were wrong in the same
direction, so every test passed while nothing worked. `to_wire` renamed the top
level and left each row's fields under their own names; the extractor undid
exactly that, so a test round-tripping through both agreed with itself. The
browser does not. Found by posting the body a rendered page actually sends.

**A blank row is the row model's `Default`, not nothing.** The template first
rendered its fields as `null`, which reads as empty in the browser and fails to
deserialize on the server, so a row added and never typed into would have
refused the whole form.

**A rule about the rows is not a rule about a row.** `required` on the
collection asks how many there are, and it is server-only: the rows are not a
signal, so there is nothing on the client to count. `RowsOf` answers `error()`
and `invalid()` for that message and cannot be `bind`ed.

### What it cost the example

Everything to do with rows came out and nothing went in. Gone: the `Roster`
store, both row routes, the `Row` transport model, the `roster_fault` rule, the
`attendees: String` field invented to hang a message on, the debounce on every
row, and the round trip per keystroke. What is left is a model, a closure and
two buttons. A half-filled form is no longer a resource on the server, because
none of it is on the server at all.

## Stage 6: the rule only the server can answer, while it is typed

[Stage 3](#stage-3-one-debounce-three-features) promised three things and
delivered two. The third was a server rule answering while a field is still
being edited, and it did not arrive, because the generated caller sends a model
and only a model. This is that stage, and it is the first of this document's
open questions answered: a server-only rule is declared on the model like every
other rule, and the macro gives it a route.

```rs
#[valid(required, checked_by = coupon)]
code: String,
```

```rs
async fn coupon(code: String) -> Result<(), String> {
    match store::accepts(&code) {
        true => Ok(()),
        false => Err(String::from("That code is not one of ours.")),
    }
}
```

The value arrives owned rather than borrowed, because the future outlives the
call and a borrow would need a lifetime the erasure below cannot hold. The
message is the application's, since a rule exos does not know cannot have a
`Violation` exos does, which is what
[`Refusal::add`](../../crates/exos/src/valid.rs) already says. It is written
inside a request, so the locale is in scope and this is the one message that may
count what it is about: "3 characters too many" is free here and still
[owed](#open-questions) on the browser's side.

### The field does not know the route, and needs no route to know

A binding is written by a template that knows nothing about which handler will
take the form, and a model can be posted into three of them. Neither the input
nor the rule can name the submit route. What answers that is that the check is
not the submit: it is addressed by the field rather than by the form, and the
field is the one thing both halves already know.

**The address is the two names the binding carries today.** A control renders
with `data-bind`, the field's generated name, and `data-bind-state`, the
model's, so `/_exos/check/{model}/{field}` asks the markup for nothing beyond a
flag saying there is a check to make, and the runtime builds the URL off the
base it was loaded from the way it already builds `/_exos/subscribe`. The
template does not change at all:

```rs
<input id="code" {bind(&form.code)}>
```

**One route, not one per field.** `#[model]` submits an entry per checked field
through [`inventory`](../../crates/exos/src/discover.rs), the way a route
attribute submits itself, each holding a shim the macro monomorphised that
deserializes the field's own type and awaits the function. The single route
resolves the pair and calls it. A route per field would be a URL space growing
with the struct to buy a map lookup either way.

### The control still writes the slot

A check answers about one value, so what comes back is a message or nothing, and
the control writes its own slot with it exactly as it does for a rule it
answered itself. It is deliberately not a record write: the whole record is
written by whatever judged the whole model, so one field's answer setting it
would clear every other message on the form. That is [stage
2](#stage-2-the-same-rule-in-the-browser)'s ownership rule one case further out,
and it is why this stage needs no new step in the effect vocabulary.

**The same function runs at submit**, in the extractor, after the shape rules
pass and only for a field whose shape held: nothing asks the database whether an
empty string is a taken address. The client's copy stays feedback, a handler
still runs against a value something checked, and a submit racing an outstanding
check is answered by the same function anyway. Two evaluators and one impl, one
level up from where [valid.rs](../../crates/exos/src/valid.rs) says it about
`Presence` and `Length`.

**The timing is already built.** Stage 3's debounce is keyed by call site and
element, so a checked field inside a row is its own timer, and
last-response-wins drops an answer about a value nobody is holding any more.

**A check in flight is `aria-busy` on the control**, which is where AngularJS's
`$pending` earns the place stage 2 declined to give it. The attribute is the one
the request path already writes and a stylesheet already knows, so the
vocabulary does not grow. It also wants [the busy marker to be
owned](loose-ends.md#a-busy-marker-is-taken-off-mid-request), which that loose
end describes and this stage would make visible on any page with a checked
field.

### What it costs, and what it rules out

**A checked field is an endpoint that answers a question about a value.** "Is
this address taken" is user enumeration with a friendlier name, and exos mounts
the route rather than the application, so it is said here rather than
discovered later. Three things hold it: it is a `POST` carrying `X-Exos` with
the session cookie, so it is same-origin and attached to a browser exos named;
the answer comes from the application's own function, which is where a rate
limit or a refusal to answer belongs; and a rule whose answer is a secret is the
wrong shape for a form that would have leaked the same answer at submit.
Same-origin is not the same as harmless, and this is the first route exos mounts
on an application's behalf that reads the application's data.

**Rejected: a partial model.** The shape this was drawn as, and the one the
README's edge describes: the caller sends `{code: "..."}` into the submit route
and something tells the extractor to check only what is there. It costs a second
body shape, a validation mode in the extractor and a handler that must be kept
from running, and then it does not answer the question it was drawn for. The
discount code is ruled on inside the handler body, which no extractor can reach,
so the rule has to move onto the model whatever the transport is. Once it has,
the partial body buys nothing. The edge is therefore retired rather than fixed:
a model is still sent whole, and no longer needs not to be.

**Kept, and not the answer: a second route the application writes.** It works
today, it stays an ordinary route, and what it costs is the rule written twice,
once where the check happens and once inside the handler that has to check it
again. That is the drift this document exists to make impossible.

**Rejected: a rule that reads a sibling.** [Stage
4](#stage-4-rules-that-only-sometimes-apply)'s answer, unchanged. A rule asks
one question about one value, and a question about two is the submit. A gate is
free here regardless, since `required_with` is a condition in front of a rule
and this is a rule.

**Not offered: a check on `Rows`.** A question about how many rows there are is
answered at submit, where the rows already are.

## What exos will not do

- **No form-encoded bodies, and therefore no CSRF token.** A form posts through
  the typed caller like every other action, so the three defences in [sessions
  and identity](sessions-and-identity.md) stage 7 all still hold. A form that
  submits without JavaScript would step outside all three at once, and buying
  that back is a token, a helper and a second body format. Not now, and possibly
  not ever.
- **No client-side authority.** The client's copy of a rule is feedback. The
  server checks everything, every time, and it is the extractor that does it so
  that no handler can decline to.
- **No message text.** See stage 1. A framework cannot hold an application's
  languages.
- **No layout, no widgets, no field components.** Templates are HTML.

## Open questions

- **Whether a message may vary with what is typed.** A client-side message is
  baked at render time, so "at least 3 characters" is free while "3 characters
  too many" needs a count that only the browser has. That is exactly the
  crossing [localization](localization.md) still owes, so client-side messages
  are fixed strings until it lands, and this is the second thing that wants it.
- **What a form does with a rule it cannot show.** A violation on a field with
  no error element in the template is silent today by construction. A debug
  build should probably say so, the way `Effect::set` already asserts against a
  signal nothing can read.
