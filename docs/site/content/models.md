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

**exos ships no message text**, because an application's languages are its own
and belong in [`messages!`](languages#messages) where the compiler holds them to every
locale. A violation is a value, and one function turns one into a sentence:

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
