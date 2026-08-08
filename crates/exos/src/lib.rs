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
mod attributes;
mod context;
mod discover;
mod effect;
mod js;
mod render;
mod response;
mod signal;

pub use crate::{
    asset::{Asset, AssetSet, routes as asset_routes},
    attributes::{
        Attr, Attributes, Bind, BindKind, Class, Event, IntoAttributes, SignalScope, Target, attr,
        bind, class, on, on_change, on_click, on_input, on_submit, preserve, prop, show, text,
    },
    context::{data, provide, try_data},
    discover::{AssetSetEntry, RouteEntry, app, asset},
    effect::{Effect, Step},
    js::{IntoJs, IntoPayload, Js, append, attr_now, call, emit, quote_js, record, when},
    render::{AttributeValue, Flag, Markup, Render, escape_into},
    response::Page,
    signal::{Field, Signal},
};

#[doc(inline)]
pub use exos_macro::view;

/// The client runtime, bundled by this crate's build script.
pub const RUNTIME: Asset = runtime::ASSETS[0];

mod runtime {
    use crate::Asset;

    include!(concat!(env!("OUT_DIR"), "/exos_assets.rs"));
}

#[doc(inline)]
pub use exos_macro::{delete, get, model, patch, post, put};

// Re-exported so the macros can name them without the user taking a direct
// dependency, and so nobody has to keep versions in step with ours.
#[doc(hidden)]
pub use {axum, inventory, serde, serde_json};
