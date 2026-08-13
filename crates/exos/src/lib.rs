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

// The macros name everything through `::exos`, so that path has to mean this
// crate here too.
extern crate self as exos;

mod asset;
mod attributes;
mod context;
mod discover;
mod effect;
mod js;
mod keys;
mod live;
mod model;
mod render;
mod response;
mod scope;
mod signal;

pub use crate::{
    asset::{Asset, AssetSet, routes as asset_routes, runtime},
    attributes::{
        Attr, Attributes, Bind, BindKind, Class, Event, IntoAttributes, Target, attr, bind, class,
        on, on_change, on_click, on_input, on_submit, preserve, prop, show, text,
    },
    context::{data, provide, try_data},
    discover::{AssetSetEntry, RouteEntry, app},
    effect::{Effect, Step},
    js::{
        IntoJs, IntoPayload, Js, append, attr_now, call, emit, focus_now, quote_js, record, when,
    },
    keys::{Keys, keys},
    live::{Fragment, Topic, connection_count, publish},
    model::{Model, ModelFields, ModelRejection, to_wire},
    render::{AttributeValue, Flag, Markup, Render, escape_into},
    response::Page,
    scope::{Scope, detached, scope, with_scope},
    signal::{Field, Placement, Signal, signal},
};

#[doc(inline)]
pub use exos_macro::{asset, delete, get, live, model, patch, post, put, view};

// Re-exported so the macros can name them without the user taking a direct
// dependency, and so nobody has to keep versions in step with ours.
#[doc(hidden)]
pub use {axum, inventory, serde, serde_json};
