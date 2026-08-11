# exos

**E**ffects e**X**ecuted **O**ver **S**treams. A Rust web framework where the
server renders the HTML and a small runtime keeps it alive in the browser.

One binary. Just `cargo run`. No bundler, no npm, no build step.

```bash
cargo run -p files    # then open http://localhost:3000, twice
```

## What it looks like

Everything the browser does is written in Rust and checked by the compiler:

```rust
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Selection {
    picked: Vec<u32>,
}

#[exos::post("/files/archive")]
async fn archive(Json(selection): Json<Selection>) -> Effect {
    data::<Files>().update(|files| store::archive(files, &selection.picked));

    publish(&file_list());
    Effect::signals(json!({ "picked": [] })).scroll("#file-list")
}

view! {
    <section {&selection}>
        <div class="bar" {show(selection.picked.get().any())}>
            <span {text(selection.picked.get().len())}></span>
            <button {on_click(|_| archive::post(selection))}>"Archive"</button>
        </div>

        <li id="file-3">
            <input type="checkbox" value="3" {bind(&selection.picked)}>
        </li>
    </section>
}
```

Rename `picked`, change the route path, or alter the payload type, and it stops
compiling, on both sides.

## How it works

A handler closure runs **at render time, on the server**, and records what it
does. So the whole Rust language is available while building the page, and only
the values that must survive to the browser are `Js<T>`. Native control flow
deliberately cannot record: `Js<bool>` is not `bool`, so an `if` over a signal
read is a compile error at the exact spot rather than a subset discovered one
restriction at a time.

A handler answers with an `Effect`: patch this HTML, merge those signals, focus
that field. The same steps can be pushed down the live **stream** instead, so a
fragment written for click-to-update also updates from a background job with no
new machinery. Both directions are server-sent events, which means one parser
on the client and one code path on the server.

Templates are **real HTML**. Void elements are void, attributes go where
attributes go, and a braced block is Rust.

## The one design decision

Everything else follows from a single rule:

> The runtime never assumes the DOM stopped changing.

Events are delegated, so nothing is bound to an element and one rendered ten
minutes from now is already wired. Bindings genuinely need per-element state,
so a `MutationObserver` applies them on insert and disposes them on removal.
That is what makes navigation, live updates and server-driven patches compose
instead of excluding one another.

## Crates

- [`exos`](crates/exos) is the runtime library, and returns a plain
  `axum::Router` so it composes into an axum application rather than replacing
  one.
- [`exos-macro`](crates/exos-macro) holds `view!`, the route attributes,
  `#[model]` and `#[live]`. Depend on `exos`, which re-exports them.
- [`exos-build`](crates/exos-build) is the build-time asset pipeline: bundle
  CSS, minify JS, content-hash, embed.

The [guide](docs/guide.md) walks through the whole surface, and
[`examples/files`](examples/files) exercises it in one page.

## Status

Early. The shape is settled and the pieces work together, but this has not
carried a real application yet. Known gaps, roughly in priority order:

- **Live tokens are not a MAC.** `Topic::token` uses `DefaultHasher`, which is
  not a cryptographic primitive. Before it guards anything real it wants
  HMAC-SHA256 with a configured key, bound to a session so it proves *this*
  viewer was served the fragment.
- **No form validation API.** The `Effect` shape is right for it, errors as
  signals reaching only the requester, but the way rules are expressed is not
  designed.
- **No localization.** The plan is to project a message's variants for the
  active locale and let `Intl.PluralRules` pick, so catalogs stay on the
  server.
- **Expressions are compiled with `new Function`**, which a strict CSP without
  `unsafe-eval` blocks. A precompiled mode is the answer.
- **Morphing is hand-rolled.** It keys on id and preserves input state, but
  [idiomorph](https://github.com/bigskysoftware/idiomorph) is better tested.
- **No CSRF handling.** Actions are same-origin `fetch` with a custom header,
  which is a start and not a policy.

## License

MIT or Apache-2.0, at your option.
