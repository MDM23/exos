# Handlers

A handler is a Rust closure. It runs at render time, on the server, and what it
records becomes JavaScript:

```rust
view! {
    <button {on_click(move |_| {
        gone.set(true);
        delete_file::post(entry_id, selection);
    })}>"Delete"</button>
}
```

That renders as:

```html
<button data-on-click="$.s1f4c20a9 = true; post('/files/3/delete', {...})">
```

## How that works

The closure body is ordinary Rust. `gone.set(true)` does not set anything, it
appends a statement to the script being built. `entry_id` is a plain `u32` at
render time and is baked into the JavaScript as a literal.

You get the whole Rust language at render time: `match`, `?`, `data::<T>()`,
string building, all of it runs on the server while rendering. Only values that
must survive to the browser are `Js<T>`.

Native control flow deliberately does not record. `gone.get()` is `Js<bool>`,
not `bool`, so this does not compile:

```rust
if gone.get() { /* ... */ }   // error: expected bool, found Js<bool>
```

That is the intended failure: loud, at compile time, at the exact spot. For
branching in the browser, use `when`:

```rust
when(selection.picked.get().any(), |()| archive::post(selection));
```

Sequencing is free, since consecutive statements record in order, which covers
the large majority of handlers.

## Which events there are

`on_change`, `on_click`, `on_dblclick`, `on_focusout`, `on_input`, `on_keydown`
and `on_submit` are the shorthands; `on(EventType::PointerUp, ..)` names the
rest. The set is closed on purpose. One listener per type sits on `document`,
so an event nobody listens for would be a handler in the DOM that never fires,
and a string would let that be a typo rather than a compile error.

Two of those names are the delegated form rather than the familiar one:
`focusout` bubbles and `blur` does not, and the same goes for `focusin` against
`focus`.

An event of your own is registered from JavaScript and named with `Custom`,
which is a promise that you called `listen` for it:

```js
window.exos.listen("swipe");
```

```rust
on(EventType::Custom("swipe"), |_| archive::post(selection))
```

## The event

```rust
on_change(|event| selection.fail.set(event.target().checked()))
```

`event.target().value()` is `Js<String>` and `.checked()` is `Js<bool>`.
Nothing is read at render time; these build expressions.

## Moving the focus

`focus_now` is the client half of `Effect::focus`, for a control the same click
has just revealed:

```rust
on_dblclick(move |_| {
    editing.set(true);
    focus_now(&format!("#edit-{id}"));
})
```

It waits for the bindings the handler scheduled. While the handler runs, the
field is still hidden, and a hidden element cannot take focus, so focusing it
there would silently do nothing.
