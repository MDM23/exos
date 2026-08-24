# Models

When the same fields are both client state and what an action sends, declare
them once:

```rust
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Selection {
    picked: Vec<u32>,
}

let selection = Selection::signals();   // selection.picked: Signal<Vec<u32>>
```

The handler takes `Model<Selection>`. Rename `picked` and both sides stop
compiling. `selection.` autocompletes.

Two models with a field of the same name are two signals, which also means
nesting one scope inside another cannot silently shadow.

A model's fields are declared on the **document**, wherever the element holding
the handle happens to sit. That follows from what they are for: a handler
answers with `Effect::set`, the client applies it against the document root,
and a field declared into an element's scope would be a different signal of the
same name. So put the handle wherever the markup it belongs to is, and the
write still lands:

```rust
view! {
    <form {&draft} {on_submit(|_| add::post(draft))}>
        <input {bind(&draft.title)}>
    </form>
}
```

The two kinds render as two attributes, `data-signals` for the element's own
and `data-signals-root` for the document's, so which is which is visible in the
markup rather than being a rule to remember. A `signal` handle is not reachable
from a handler at all, and a debug build says so rather than writing a signal
nothing reads.

## What a model's state lasts for

A model's state is the page's. One model is one value however many elements
declare it, they cannot disagree because the starting value is always the
model's `Default`, and it outlives the element that carried the declaration:
that is what lets a list declare one editing buffer and every row use it.

Two consequences worth knowing before reusing a model type:

- **One model is one instance.** Two comment boxes each with their own draft
  are two model types, not two declarations of one. Element signals are the
  ones that repeat.
- **A navigation re-seeds it.** The document that arrives declares what its
  signals start as, so a page does not inherit what the last one was holding.
  Names the new document does not mention keep their value, and a patch never
  re-seeds anything, since a patch is an update to the page you are on.

Anything `Serialize + Deserialize` can be a signal: `bool`, numbers, `String`,
`Vec<T>`, and nested models.

## The wire is private

A field name never leaves the server. Both the signal and the payload key are
named after the model and the field, so the call above compiles to:

```js
post('/files/archive', {"sc523a195": $.sc523a195, "s70c556ff": $.s70c556ff})
```

An action route is not a public API. Its response is a stream of DOM patches,
so there was never anything useful to integrate against, and the point of
keeping the request private is not to stop anyone: it is that nothing outside
the generated pair can depend on the shape, so the shape stays free to change.
Batching several actions into one request, sending only what changed,
versioning the envelope. A payload someone has written into a script is a
payload that cannot move again.

Two consequences worth stating plainly.

**This is not authorization.** An opaque key is a "do not depend on this"
marker, in the way an unstable ABI is. The keys are sitting in `data-signals`
for anyone who opens the inspector. Every route still authorizes for itself.

**A body written elsewhere keeps `Json`.** The sortable plugin posts
`{ order: $._order }`, which JavaScript writes by hand, so `Reorder` is a plain
`Deserialize` struct behind `Json<Reorder>` and its field names are legible on
purpose. The extractor a handler names is what says which of the two it is.

Where the server already knows a body, `exos::to_wire` builds one, which is
also how a test posts to its own action:

```rust
let body = exos::to_wire(&Selection { picked: vec![], fail: true });
```

Writing `Json<Selection>` on an action still compiles, because a model is an
ordinary `Deserialize` type. It fails at runtime with a missing field, since
the keys that arrive are not the ones serde is looking for.

## Repeating groups

A form with rows in it is one submission, and the rows are the browser's until
it is made. The rows are a field, and a row is a model:

```rust
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Order {
    #[valid(required)]
    reference: String,

    #[valid(required)]
    lines: Rows<Line>,
}

#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Line {
    #[valid(required)]
    sku: String,
}
```

`each` writes the row once and it is used twice: for the rows the form opens
with, and as the `<template>` a new row is cloned from.

```rust
{ form.lines.each(|line| view! {
    <li {line}>
        <input {bind(&line.sku)}>
        <p class="error" {text(line.sku.error())}></p>
        <button type="button" {on_click(|_| form.lines.remove())}>"Remove"</button>
    </li>
}) }

<button type="button" {on_click(|_| form.lines.add())}>"Add a line"</button>
```

That is the whole of it. No ids, no routes, no list on the server: `add` clones
the template and `remove` takes an element off the page, and neither is a
request.

**A row needs no name because a clone is its own scope.** The runtime keys
signals per element, so every row declares the same field name and holds its own
value, which is the rule that has always given [a row's own
signal](signals#most-signals-have-no-name) its own value. The submission reads
the rows out of the group when it is sent, in the order they are on screen.

**`{line}` goes on the row's root element.** It declares the row's fields and
marks where one row ends, so it has to sit on the outermost element of the row.

**What the form opens with is the model's `Default`.** One blank `Line` in
`Order::default()` is one blank row on screen; a form editing something existing
builds the same shape from it.

**A row's rules are the row's.** `required` on `Line::sku` is checked per row
and the message lands on that row. `required` on `lines` is about how many rows
there are, and `form.lines.error()` is where that one goes.

**Rows are numbered by position, not identity.** A message about the third row
is written under the third row, so removing a row would leave every message
after it about a different one. They are retired rather than renumbered, and
the next submit says what is wrong with the rows as they then are. That is the
price of never inventing an id, and editing a row clears its own message
either way.

## Rules on a model

A rule about the shape of one value is written on the field it is about:

```rust
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Signup {
    #[valid(required, length = 2..=40)]
    name: String,

    #[valid(required, email)]
    email: String,

    invoice: bool,

    #[valid(required_with = invoice)]
    vat: String,
}
```

**Nothing calls a validator.** `Model<Signup>` already refuses a body it cannot
read, so it refuses one that breaks a rule the same way: a `422` carrying an
effect that writes what is wrong into the model's own record and moves the
caret to the first field that has something wrong with it. A handler body runs
only against a value whose shape held, and no call site can forget a rule
because there is no call site.

**The same declaration answers in the browser.** `bind` carries the field's
rules to the control, which asks them while somebody types and writes the
answer into the same record a refusal writes. One slot per field, whichever
side decided what is in it, so a template reads one place:

```rust
view! {
    <input {bind(&form.email)}>
    <p {show(form.email.invalid())} {text(form.email.error())}></p>
}
```

A bound control also marks itself with `aria-invalid="true"`, which is a
standard attribute rather than a class of ours, so a screen reader is told what
the border says and the simplest usable form needs no error markup at all:

```css
[aria-invalid="true"] {
  border-color: red;
}
```

Nothing speaks until a field has been edited, because a form that is red before
it is read is worse than no validation. A message from the server shows
whenever there is one, since the server only speaks after a submit.

**A rule exos cannot know is a `Refusal`.** Whether a code is spent or a name
is taken needs the data, so it stays ordinary Rust in the handler and answers
in the same shape, and nothing a viewer sees says which side decided:

```rust
#[exos::post("/signup")]
async fn signup(Model(form): Model<Signup>) -> Result<Effect, Refusal<Signup>> {
    let mut refusal = Refusal::new();

    if !data::<Codes>().accepts(&form.code) {
        refusal.add(Signup::CODE, "That code is not one of ours.");
    }

    if !refusal.is_empty() {
        return Err(refusal);
    }

    /* ... */
}
```

`Signup::CODE` is a token rather than a name, so renaming the field breaks that
line instead of quietly addressing nothing.

**A refusal about the submission is `say`.** Whether these two are a login is a
question about the pair, and answering it on the password says something the
server does not know. It lands in the same record under a key no field has:

```rust
#[exos::post("/login")]
async fn login(Model(form): Model<Login>) -> Result<Effect, Refusal<Login>> {
    let Some(account) = accounts().authenticate(&form).await else {
        let mut refusal = Refusal::new();
        refusal.say(wrong_credentials());
        return Err(refusal);
    };

    /* ... */
}
```

A template reads it the way it reads a field's, and no control marks itself for
it, because it is about none of them:

```rust
view! {
    <p {show(form.refused())} {text(form.refusal())}></p>
}
```

**exos ships no message text**, because an application's languages are its own
and belong in [`messages!`](languages#messages) where the compiler holds them
to every locale. A violation is a value, and one function turns one into a
sentence:

```rust
exos::complaints(|field, violation| match (field, violation) {
    ("vat", Violation::Required) => String::from("An invoice needs a VAT id."),
    (_, Violation::Required) => String::from("This is needed."),
    _ => String::from("That does not look right."),
});
```

The field arrives under the name it is declared with, which never leaves the
server, so an application can answer per field where the general sentence is
not good enough.

One thing to watch. `required_with` gates a rule on another field being filled
in, and nothing holds that gate and whatever `show`s the section together. A
section revealed on more than the gate names is validated while hidden, and the
submit then fails with a message nobody can see.

## What a form asks about itself

Two questions about the model as a whole, each one read rather than a fold over
however many fields it has:

```rust
view! {
    <button type="submit" {attr("disabled", !form.valid())}>"Register"</button>
}
```

`form.valid()` is that same record, empty. It answers for every rule of every
field, for a rule about a row, and for whatever the server decided, because all
of them land in the one place. `form.dirty()` is whether any control writing
into it has been edited.

**A form nobody has filled in is valid**, because nothing has judged it yet.
That is deliberate: a submit button disabled before anybody has had a chance to
be wrong hides the way forward, and the submit is what the first messages come
back on.

What it costs is that a message has to be retirable, or a form could reach a
state it cannot be submitted out of. Each of them is: editing a field retires
what was said about it, editing a gate retires what was said about the fields
it arms, adding or removing a row retires what was said about how many there
are, and any edit at all retires what was said about the submission, which was
about the values that were sent.

A field's own rules retire only what they said themselves. They run again over
the record whenever anything writes into it, and a field a handler refused is
one they find nothing wrong with, so anything else would be a refusal deleting
itself on the microtask it arrived on.
