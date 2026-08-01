//! Effects eXecuted Over Streams.
//!
//! A web framework where the server renders the HTML and a small runtime keeps
//! it alive in the browser. One binary, `cargo run`, no bundler and no build
//! step.
//!
//! # Rendering
//!
//! Values become markup through [`Render`], which escapes everything except
//! [`Markup`] itself:
//!
//! ```
//! # use exos::Render;
//! let name = "annual-report.pdf";
//! assert_eq!(name.render().as_str(), "annual-report.pdf");
//! assert_eq!("<script>".render().as_str(), "&lt;script&gt;");
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]

mod asset;
mod context;
mod render;

pub use crate::{
    asset::{Asset, AssetSet, routes as asset_routes},
    context::{data, provide, try_data},
    render::{AttributeValue, Flag, Markup, Render, escape_into},
};

#[doc(inline)]
pub use exos_macro::view;

/// The client runtime, bundled by this crate's build script.
pub const RUNTIME: Asset = runtime::ASSETS[0];

mod runtime {
    use crate::Asset;

    include!(concat!(env!("OUT_DIR"), "/exos_assets.rs"));
}
