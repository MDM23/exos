# Signals

A signal is a piece of state in the browser, declared once in Rust:

```rust
let gone = signal(false);   // Signal<bool>
```

Put the handle in an attribute block to declare it. That element becomes its
scope:

```rust
view! {
    <li id={ row_id } {&gone}>/* ... */</li>
}
```

Scoping is lexical with the DOM as the tree: the nearest ancestor that declares
a name wins. A row already needs an `id` for morphing, so a hundred rows can
each declare their own without colliding and you never invent `gone_3`.

## Most signals have no name

The handle is the whole interface. `signal` takes no name because nothing
outside the handle should be spelling one: the store is keyed by a name derived
from the declaration site, and that key is not published. Read it with
`gone.name()` while debugging, never in a template.

A name is a contract, so it exists only where something off the page needs one,
and then it comes from a type rather than a string:

- **`#[model]` fields**, below. These are what an action's body carries and what
  a handler writes with `Effect::set`. Their names are generated too, per model
  and field rather than per call site, so that every `signals()` agrees.
- **Names a plugin owns**, such as the sortable plugin's `_order`. Those are
  written in JavaScript and reach a template as a raw expression, so `view!`
  reads the names out of the expressions in a subtree and declares whatever it
  finds as `null`.
