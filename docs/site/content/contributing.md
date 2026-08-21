# Contributing

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
there. Node is a dev dependency of the repository and of nothing built with it;
an application still needs no bundler and no npm.

CI runs both, plus `cargo clippy` over every target, `cargo doc` with warnings
denied so the doc links cannot rot, `cargo audit` against the lockfile, and a
build with the oldest supported compiler. CI runs the same shell, so a green
pipeline and a green checkout mean the same thing.

## This site

The documentation is an exos application, in `docs/site`, and the pages are
ordinary markdown in `docs/site/content`:

```bash
cargo run -p exos-docs
```

A debug build reads the content off disk, so editing a page and reloading the
browser is the whole cycle. A release build embeds every page in the binary,
which is what gets deployed.

Adding a page is two edits: the file, and a line in
`docs/site/content/navigation.md`, which is the sidebar. A page missing from
the navigation is a test failure rather than a page nobody can reach.

## License

[MIT](https://github.com/MDM23/exos/blob/main/LICENSE-MIT) or
[Apache-2.0](https://github.com/MDM23/exos/blob/main/LICENSE-APACHE), at your
option.
