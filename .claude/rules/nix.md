# Nix environment

The flake is the single source of every tool this project builds and tests
with.

- Everything that influences build, test or runtime results comes from
  [flake.nix](../../flake.nix), through `nix develop` or direnv: the Rust
  toolchain, `rustfmt`, `clippy`, `rust-analyzer`, `cargo-audit`, and the node
  the client runtime's tests need. There is no second place to install one.
- General working tools that only help navigate and edit the tree (`grep`,
  `git`, the editor itself) live on the developer's system. They leave no
  trace in any artifact.
- **No leaks from outside the flake.** An editor or a shell that resolves
  `cargo`, `rustc` or `node` outside the Nix store is a defect, not a
  shortcut: it produces behaviour that holds on one machine and nowhere else.
- If you notice such a leak while working (a tool resolving outside the store,
  a build that only succeeds because something is installed on the host, a
  version that disagrees with the flake), say so rather than working around
  it.
- CI runs the same shell, so a green pipeline and a green checkout mean the
  same thing. A workflow step that installs a toolchain of its own is the same
  defect wearing a different hat.

## What the shells are for

- `default` is what a contributor gets, and holds one stable toolchain with
  `rust-analyzer` and `rust-src`, plus node.
- `msrv` holds only the oldest compiler the workspace claims to support, and
  exists so CI can prove the claim. It reads `rust-version` out of
  [Cargo.toml](../../Cargo.toml), so the version lives in one place; never
  write it out a second time.

Adding a tool means adding it to a shell. A `nix run` of something that is not
in the flake is the leak this file is about.

## Tooling

Formatting is the only thing enforced, because the flake is small enough that
a linter would have more opinions about it than it has code.

- Every new or changed `.nix` file is formatted with `nix fmt` before it is
  committed. The formatter is the flake's own `formatter` output, `nixfmt-tree`
  wrapping **nixfmt**, the formatter RFC 166 settled on.
- Never format Nix through an editor plugin that resolves a formatter of its
  own; that is the same leak as any other.

## Style

The canon is the Nixpkgs manual's coding conventions. Where the formatter has
an opinion, the formatter wins and the style is not discussed.

- Avoid `rec`; bind shared values in a `let`.
- Avoid `with`, especially over a large set like `pkgs`. Write `pkgs.nodejs`,
  or `inherit (pkgs) nodejs;` where a name is used often.
- Destructure function arguments (`{ pkgs, lib }:`), and use `...` only for
  genuine pass-through.
- Name things in lowerCamelCase, and prefer `inherit x;` to `x = x;`.
- Sort attribute sets and lists alphabetically where order carries no meaning;
  inputs, package lists and system lists all qualify. See
  [ordering.md](ordering.md).
- Use `''...''` for multi-line strings and `"..."` otherwise.
- Comments explain why, wrap at 80 columns, and follow
  [text-style.md](text-style.md) like every other comment in the tree.
