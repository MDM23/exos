# exos

**E**ffects e**X**ecuted **O**ver **S**treams. A Rust web framework where the
server renders the HTML and a small runtime keeps it alive in the browser.

One binary. Just `cargo run`. No bundler, no npm, no build step.

```bash
cargo run -p playlist   # then open http://localhost:3000, twice
cargo run -p todos      # the classic list, on the same address
cargo run -p auction    # a sale room, best with a private window open too
```

## What it looks like

Everything the browser does is written in Rust and checked by the compiler:

```rust
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Selection {
    picked: Vec<u32>,
}

#[exos::post("/tracks/remove")]
async fn remove(Model(selection): Model<Selection>) -> Effect {
    data::<Room>().update(|tracks| store::remove(tracks, &selection.picked));

    publish(room);
    Effect::set(&Selection::signals().picked, Vec::new()).scroll("#queue")
}

view! {
    <section {&selection}>
        <div class="bar" {show(selection.picked.get().any())}>
            <span {text(selection.picked.get().len())}></span>
            <button {on_click(|_| remove::post(selection))}>"Remove"</button>
        </div>

        <li id="track-3">
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

That one format is why the awkward cases need no new machinery either. A handler
too slow to answer at once streams its effects as it computes them. A handler
that refuses answers with the status a refusal deserves *and* with what the page
should do about it, because an effect is applied whatever status carries it.

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
  one. `Router::new().nest("/admin", exos::app())` needs no configuration:
  exos works out where it was mounted and puts that in front of every URL it
  writes, and the browser runtime finds it from the URL it was itself loaded
  from.
- [`exos-macro`](crates/exos-macro) holds `view!`, `asset!`, the route
  attributes, `#[model]` and `#[live]`. Depend on `exos`, which re-exports
  them.
- [`exos-build`](crates/exos-build) is the asset pipeline `asset!` calls while
  your crate compiles: bundle CSS, bundle and minify JS, content-hash, embed.
  There is no build script.
- [`exos-cldr`](crates/exos-cldr) is the slice of CLDR `locales!` reads while it
  expands: cardinal plural rules, writing direction and the symbols a whole
  number is written with, vendored as committed source by a script a maintainer
  runs. No build reaches the network for it, and nothing of it reaches a binary
  except the languages an application declared.

The [guide](docs/site/content) walks through the whole surface, and
[`docs/site`](#the-documentation-site) is that guide as a site.
[`examples/playlist`](examples/playlist) is a listening room several browsers
share: a mark travels down the queue as tracks end, hearts and removals paint
before the server answers, and the room refuses to remove what it is playing,
so the one correction an optimistic update needs to show is a rule rather than
a simulation. The sleeve of whatever is on is an embedded asset, and the blur
that stands in for it while it loads is a sixteen-pixel version of the same
bytes, computed at startup and inlined into the page rather than kept beside
the file.
[`examples/todos`](examples/todos) is TodoMVC, where the list is a live
fragment per filter and editing a row is client state from the double click to
the save. [`examples/auction`](examples/auction) is about who anybody is: a
price is state and is published to everyone watching, being outbid is an event
and is sent to one person wherever they are. Reload it and the price is still
there while the message is not, which is the whole difference in one gesture.

## The documentation site

[`docs/site`](docs/site) is the guide as a site, and is itself an exos
application:

```bash
cargo run -p exos-docs
```

The pages are ordinary markdown in [`docs/site/content`](docs/site/content) and
the sidebar is one of them, so adding a page is a file and a line in a list. A
debug build reads them off disk, a release build embeds every one of them, and
what deploys is the binary with nothing beside it.

## Working on exos

Every tool comes from the flake, so there is nothing to install and nothing to
keep in step:

```bash
nix develop          # or direnv allow, which the .envrc already sets up
cargo test --workspace
npm test             # the client runtime, in jsdom
```

The Rust suite covers what the server renders. `npm test` covers the runtime
that keeps it alive in the browser, which is the half `cargo test` cannot
reach, and the reason it exists is that every bug that got past review lived
there. Node is a dev dependency of this repository and of nothing built with
it; an application still needs no bundler and no npm.

CI runs both, plus `cargo clippy` over every target, `cargo doc` with warnings
denied so the doc links cannot rot, `cargo audit` against the lockfile, and a
build with the oldest supported compiler.

## Status

Early. The shape is settled and the pieces work together, but this has not
carried a real application yet. Known gaps, roughly in priority order:

- **Expressions are compiled with `new Function`**, which a strict CSP without
  `unsafe-eval` blocks. A precompiled mode is the answer.
- **No CSRF token.** `SameSite=Lax` on the session cookie, the `X-Exos` header
  and JSON-only bodies are three defences rather than one, which is a policy and
  is written down in the guide. It leaks for a handler that accepts a
  form-encoded body, and that is when a token should be built.
- **A sentence with a link in it cannot cross.** A message whose count is
  client state projects: its variants in the one language the page was rendered
  in ride out with it, and `Intl.PluralRules` picks. What cannot is a message
  with a slot, which would have the runtime build elements rather than text,
  and a message with two counts, which could be on two sides at once.

Where each of those is going is written down in [docs/roadmap](docs/roadmap):
[sessions and identity](docs/roadmap/sessions-and-identity.md) is the one most
of the others waited on and is now built, so a live subscription proves the
browser presenting it was served the fragment,
[directed effects](docs/roadmap/directed-effects.md) is built as
far as pushing an effect to a person, [forms](docs/roadmap/forms.md) is built as
far as rules that answer on both sides and the one rule that answers over the
wire while a field is typed,
[localization](docs/roadmap/localization.md) is built as far as a message
whose count the browser holds,
[more than one instance](docs/roadmap/more-than-one-instance.md) is built as
far as a cluster that needs no sticky sessions and where a sign-out means the
same thing on every node, and [loose
ends](docs/roadmap/loose-ends.md) collects the smaller work that waits for
nothing.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
