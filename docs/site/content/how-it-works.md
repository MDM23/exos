# How it works

Server-rendered HTML with a small client runtime. Everything is ordinary Rust:
the only JavaScript you write is [the escape hatch](expressions#the-escape-hatch),
and you rarely reach for it.

Two rules explain most of the design.

1. **The runtime never assumes the DOM stopped changing.** Events are
   delegated, bindings are applied by a `MutationObserver`. Markup that arrives
   ten minutes after page load is already wired.
2. **Anything crossing to the browser is a typed Rust value**, never a string
   you have to keep in agreement with something elsewhere.

## Handlers run on the server, at render time

A handler closure runs while the page is being built, and records what it does.
So the whole Rust language is available: `match`, `?`, `data::<T>()`, string
building, all of it runs on the server. Only the values that must survive to
the browser are `Js<T>`.

Native control flow deliberately cannot record. `Js<bool>` is not `bool`, so an
`if` over a signal read is a compile error at the exact spot rather than a
subset of the language discovered one restriction at a time. [Handlers](handlers)
says what to write instead.

## One answer shape, in both directions

A handler answers with an [`Effect`](effects): patch this HTML, merge those
signals, focus that field. The same steps can be pushed down the live
[stream](live-fragments) instead, so a fragment written for click-to-update also
updates from a background job with no new machinery. Both directions are
server-sent events, which means one parser on the client and one code path on
the server.

That one format is why the awkward cases need no new machinery either. A
handler too slow to answer at once
[streams its effects](effects#streaming-a-slow-answer) as it computes them. A
handler that [refuses](effects#refusing-with-an-effect) answers with the status
a refusal deserves *and* with what the page should do about it, because an
effect is applied whatever status carries it.

## The one design decision

Everything else follows from a single rule:

> The runtime never assumes the DOM stopped changing.

Events are delegated, so nothing is bound to an element and one rendered ten
minutes from now is already wired. Bindings genuinely need per-element state,
so a `MutationObserver` applies them on insert and disposes them on removal.
That is what makes navigation, live updates and server-driven patches compose
instead of excluding one another.

## The crates

- `exos` is the runtime library, and returns a plain `axum::Router` so it
  composes into an axum application rather than replacing one.
  `Router::new().nest("/admin", exos::app())` needs no configuration: exos works
  out [where it was mounted](routes#serving-under-a-prefix) and puts that in
  front of every URL it writes, and the browser runtime finds it from the URL it
  was itself loaded from.
- `exos-macro` holds `view!`, `asset!`, the route attributes, `#[model]` and
  `#[live]`. Depend on `exos`, which re-exports them.
- `exos-build` is the asset pipeline `asset!` calls while your crate compiles:
  bundle CSS, bundle and minify JS, content-hash, embed. There is no build
  script.
- `exos-cldr` is the slice of CLDR `locales!` reads while it expands: cardinal
  plural rules, writing direction and the symbols a whole number is written
  with, vendored as committed source by a script a maintainer runs. No build
  reaches the network for it, and nothing of it reaches a binary except the
  languages an application declared.
