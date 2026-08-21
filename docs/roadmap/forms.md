# Forms

Rules written once, checked on both sides, and the one round trip that carries
what only the server knows.

Status: not built, and this is the design rather than the chores. It moves an
edge the [guide](../guide.md) had already closed, which is the first section
below. Stage 0 is an example rather than an API, for the reason [sessions and
identity](sessions-and-identity.md) records in its stage 3: the narrow version
is the reversible one, and nothing here has been written against yet.

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

The tree contains one form. It has one field, and its whole answer to a refusal
is `Effect::none()` with a comment explaining why the field keeps what was
typed. Everything the roadmap knows about refusals came from business rules in
[`examples/playlist`](../../examples/playlist), not from fields.

So the first move is an example that is a form and nothing else, written with
today's surface: several fields of different types, a section that only applies
when a checkbox is ticked, repeating rows added by a button, one rule the server
alone can answer, and a search field beside it. Every stage below is a guess
until that exists, and the guesses most likely to be wrong are stages 4 and 5.

The second thing to build in it is a **searchable dropdown**, because it is the
control every application needs, because none of them get it right, and because
it is where exos's edges sit closest together. A listbox that filters as it is
typed into, from the client for a short list and from the server for a long one,
with optional multiple selection.

Most of it is cheaper than it looks. Multiple selection is a `Vec<T>` model
field with checkbox bindings, which
[`examples/playlist`](../../examples/playlist) already does with no per-row
bookkeeping. Server-side filtering is stage 3's debounce patching a fragment,
the same mechanism as the search field beside it, which is the second time one
debounce pays for two features. And client-side filtering needs none of the
client-side loop exos refuses to grow: the server renders every option, each
carrying its own `show` condition over the query, so filtering is per-element
visibility. The same trick shows the chosen labels back in the closed control,
since a chip is an option that reveals itself once it is picked. **Render
everything and gate it with `show`** is how exos answers this whole class of
widget, and its one limit is how many options that stays reasonable for, which
stage 0 should measure rather than argue about.

What it will find is the dismissal and the keyboard. Both are gaps in the
vocabulary rather than in the design, and none of them needs a document:

- **Nothing can say "focus left this widget".** Delegation dispatches to the
  nearest element carrying the attribute, so a click outside a dropdown finds no
  handler at all, and a `data-on-click` on `<body>` is shadowed by every inner
  click handler rather than running after it. `focusout` bubbles and is the
  accessible answer anyway, but deciding whether focus went somewhere inside
  means reading `relatedTarget` and asking the DOM, and
  [`Event`](../../crates/exos/src/attributes/handler.rs) exposes neither.
- **The combinators cannot do a listbox's arithmetic.** Moving the active option
  with the arrow keys is an index clamped to a length, and
  [combinator.rs](../../crates/exos/src/js/combinator.rs) has `plus` and `minus`
  but no `min`, `max` or clamp. `aria-activedescendant` is an id and an index
  concatenated, and there is no concatenation on `Js<String>` either.
- **A client-side filter is case-sensitive**, because `contains` is what there
  is and there is no `lower`.
- **The element carrying the binding is not an `<input>`.** A combobox is a
  `div` with `role="combobox"`, so stage 2's `aria-invalid` has to be written by
  whatever carries `bind` rather than by whatever looks like a form control.
  Worth knowing before that stage decides otherwise.

That is the method the rest of this roadmap was built by. Stage 4 of [sessions
and identity](sessions-and-identity.md) argued for a `reconnect` step until
[`examples/auction`](../../examples/auction) showed it loses a race it cannot
win.

## Stage 1: a rule is a value the server checks

Rules are declared on the model, because that is the one place the template, the
body and the handler already agree on:

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

A rule is a value with two evaluators:

```rs
pub trait Rule<T> {
    /// Checked on the server, against the value that arrived.
    fn check(&self, value: &T) -> bool;

    /// The same question as an expression, for the browser to answer while it
    /// is being typed. A rule with no client half returns `None` and is
    /// checked on the server alone.
    fn js(&self, value: Js<T>) -> Option<Js<bool>> {
        let _ = value;
        None
    }
}
```

The client half is already affordable: `trim`, `is_empty`, `len`, `eq`, `gt` and
the rest of [combinator.rs](../../crates/exos/src/js/combinator.rs) are what
shape rules are made of. And "server-only" needs no separate concept, since it
is a rule that did not implement the second method. One vocabulary, two tiers.

**exos ships no message text.** It cannot: an application's languages are its
own, and [localization](localization.md) exists so that a string in a page is a
`messages!` arm the compiler holds to every locale. So a violation is a value
(`Required`, `TooShort { min }`) and the application turns it into text in one
function it writes, once, with the same macro as everything else it says.

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

Where an error lands decides how much of this is new machinery, and the answer
is almost none. `#[model]` generates one more document signal, `errors`, holding
a map from a field's generated name to the text for it. The server's refusal
writes it with `Effect::set`, which merges, and a template reads one field's
error through the handle:

```rs
view! {
    <label>
        "Email"
        <input type="email" {bind(&form.email)}>
    </label>

    <p class="error" {text(form.error(Signup::EMAIL))}></p>
}
```

`Field<M>` is the token that makes that check at compile time, and it already
exists in [signal.rs](../../crates/exos/src/signal.rs), generated per field by
`#[model]` and so far used by nothing. It was written for this.

The key in the map is the field's **generated** name, not `email`, so [the wire
stays private](../guide.md#the-wire-is-private) and an error map is addressable
only through a handle, like everything else a model owns.

Client-side checking then needs no new runtime feature except one flag, because
a checked rule is an ordinary recorded expression: the error a field shows is
`its client rules, then whatever the server last said`. What is new is
**dirty**, and it belongs to the binding, which is the one thing that already
knows a control changed. Two rules fall out of it, and both are about not
shouting at somebody mid-word:

- **A field says nothing until it is dirty.** An untouched field is not a wrong
  field, and a form that is red before it is read is worse than no validation.
  A submit marks every field dirty at once, which is what makes the first
  submission show everything.
- **Editing a field clears the server's error for it.** Otherwise "that email is
  taken" hangs under a field while somebody types a different one, and the
  client cannot answer a rule it does not own.

Precedence is the client's answer first, because it is the fresher of the two.

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

## Stage 3: one debounce, three features

The mechanism is a debounced call, and it reads like the recorder's existing
`when`:

```rs
on_input(|_| debounce(300, || search::post(&query)))
```

The key is the call site, hashed the way
[`signal`](../../crates/exos/src/signal.rs) hashes its own, so nothing invents a
name for it.

That one thing delivers three of this document's wishes, which is the sign it is
the right shape rather than three features wearing a coat: a search field that
submits as it is typed, a server rule that answers while a field is still being
edited, and any other action that should not fire per keystroke.

**Debouncing alone is not enough and this is the part that gets forgotten.** Two
requests in flight can answer in the other order, and the older one then paints
over the newer. So a debounce key also carries last-response-wins: a reply older
than the newest request under that key is dropped. Without it a search bar shows
the results for a prefix of what is in the box, intermittently, on a slow
connection, which is a bug nobody can reproduce.

Nothing else about a search field is new. The query is a model field, the
handler patches a fragment, and `aria-busy` on the element already says the work
is happening.

## Stage 4: rules that only sometimes apply

A section that appears when a box is ticked has to be checked while it is
showing and ignored while it is not. That is a gate on a rule, and it is written
on the field it gates:

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

**The gate is not a rule, and that matters for the trait.** A `Rule<T>` sees one
field's value, and giving it the whole model to read a sibling would put a model
type parameter on every rule that never uses one. So `required_with` expands in
the macro to the ordinary `required` rule under a condition, on both sides:
`check` runs it only when the sibling is present, and the recorded expression is
the same question with the same guard in front of it. The trait stays as stage 1
draws it.

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

## Stage 5: repeating groups

The biggest piece by a distance, and the only one here that fights an edge
rather than moving it. exos has no client-side loop and should not grow one, so
a row added by a button is **rendered by the server** and arrives as a patch,
like every other list in the tree.

What that needs is a model field of `Vec<Row>` and a way to name one row's
field, so that a binding, an error key and a rule all address the same thing:

```rs
form.lines.at(line.id).quantity   // Signal<u32>
```

Keyed by the row's own id rather than by its index, because removing the second
of five rows renumbers three signals and every error under them. The id is
already in the markup, since a row needs one for morphing.

The error map's keys stop being flat here, and that is the decision this stage
turns on: an error belongs to a field of a row, so the key is a path. That is
worth designing against a real form rather than in advance, which is the second
reason stage 0 comes first.

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

- **How a server-only rule is written.** Either a second route the application
  writes that answers with errors and nothing else, which duplicates the model's
  declaration, or a rule on the model that names an async function and lets the
  macro generate the route, which is consistent with "nothing calls the
  validator" and drags async into the model layer. The second is more in
  keeping and less obviously right.
- **Whether a message may vary with what is typed.** A client-side message is
  baked at render time, so "at least 3 characters" is free while "3 characters
  too many" needs a count that only the browser has. That is exactly the
  crossing [localization](localization.md) still owes, so client-side messages
  are fixed strings until it lands, and this is the second thing that wants it.
- **Whether `Model<T>` validating is one extractor or two.** One means a handler
  cannot accidentally skip it. Two (`Model<T>` and a checked wrapper) means the
  refusal is visible in the signature. `Result<Model<T>, _>` may make the
  question moot.
- **What a form does with a rule it cannot show.** A violation on a field with
  no error element in the template is silent today by construction. A debug
  build should probably say so, the way `Effect::set` already asserts against a
  signal nothing can read.
