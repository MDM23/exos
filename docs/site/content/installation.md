# Installation

exos is not on crates.io yet, so depend on the repository:

```toml
[dependencies]
axum = "0.8"
exos = { git = "https://github.com/MDM23/exos" }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

`axum` is there because [`exos::app()`](hello-exos) hands back an application
axum serves rather than a server of its own, `serde` because a
[model](models) is an ordinary `Deserialize` type, and `tokio` because that is
what serves it. exos adds nothing else you have to install: there is no CLI, no
project template, no bundler and no build script.

The compiler is the only requirement, and it must be at least 1.88.

## What is not needed

No `npm install`, no `node_modules`, no `package.json`, and no watcher process
beside `cargo run`. The client runtime is compiled into the `exos` crate and
served from it, so there is no copy of it in your project and no version of it
to keep in step. Your own stylesheets and scripts are bundled by
[`asset!`](assets) while your crate compiles.

That holds in a deploy too. What you ship is the binary `cargo build --release`
produced, with every asset embedded in it, and nothing beside it.

## Where to go next

[Hello, exos](hello-exos) is the smallest application that runs. [How it
works](how-it-works) is the one page that makes the rest of this guide make
sense, and is worth reading before the reference pages rather than after them.
