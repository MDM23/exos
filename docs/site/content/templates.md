# Templates

`view!` takes real HTML: the tags and attributes you would write in a `.html`
file. Void elements are void (`<br>`, not `<br/>`). A braced block is Rust, and
text is a string literal (`<p>"Hello"</p>`). Written bare, text would reach the
macro as Rust tokens with the spacing rearranged, turning `50% off` into
`50 % off`, so bare text is a compile error instead.

```rust
view! {
    <ul class="files">
        { entries.iter().map(row).collect::<Vec<_>>() }
    </ul>
}
```

It compiles to `String` pushes, so the static parts are string literals in the
binary and nothing is parsed at runtime.

Everything interpolated is escaped. [`Markup`] is the only type that is not,
and it is the only way to emit raw HTML, so "unescaped" is greppable.

## Attributes

`Option<T>` drops the attribute entirely when `None`, because `aria-current=""`
is not the same as no `aria-current`:

```rust
fn current(path: &str, href: &str) -> Option<&'static str> {
    (path == href).then_some("page")
}

view! {
    <a href="/" aria-current={ current(path, "/") }>"Files"</a>
}
```

A bare `bool` renders the *text* `"true"` or `"false"`, which is what you want
for `data-favorite="false"`, so CSS can match both states. For genuine HTML
boolean attributes, `Flag` gives present-or-absent:

```rust
view! {
    <input disabled={ Flag(is_locked) }>
}
```

That distinction is not cosmetic. Writing `.dot[data-online]` in CSS matches
`data-online="false"` too, which is a bug this project shipped once and had to
fix.

[`Markup`]: https://docs.rs/exos/latest/exos/struct.Markup.html
