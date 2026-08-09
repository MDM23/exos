//! Bundles the client runtime into the crate.
//!
//! `runtime.js` ships as bytes inside the rlib, so a consumer needs no npm, no
//! bundler and no step of their own.

fn main() -> Result<(), exos_build::Error> {
    exos_build::Assets::new()
        .js_bundle("exos.js", &["js/runtime.js", "js/sortable.js"])?
        .emit()
}
