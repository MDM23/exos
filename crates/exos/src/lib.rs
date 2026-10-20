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

mod app;
pub mod asset;
pub mod attributes;
mod context;
mod csrf;
mod discover;
pub mod effect;
mod fnv;
mod form;
mod hex;
pub mod identity;
pub mod instant;
pub mod js;
pub mod keys;
pub mod live;
pub mod locale;
mod message;
pub mod model;
mod number;
pub mod render;
mod response;
mod rows;
pub mod scope;
pub mod session;
pub mod signal;
pub mod url;
pub mod valid;
mod welcome;

// What a view or a handler writes all the time. Everything else is named
// through its module, so a name never has to say where it comes from.
pub use crate::{
    app::{App, app},
    attributes::{
        attr, bind, class, modal, modal_now, on, on_change, on_click, on_dblclick, on_focusout,
        on_input, on_keydown, on_pointerenter, on_submit, preserve, prop, show, text,
    },
    context::{data, provide, try_data},
    discover::{AssetSetEntry, RouteEntry},
    effect::Effect,
    form::{Form, FormKey, ReviseEntry},
    instant::Instant,
    js::{IntoJs, Js},
    locale::locale,
    message::Enumerable,
    model::{Model, ModelFields, ModelRejection},
    render::{Markup, Render},
    response::Page,
    rows::{Row, RowModel, Rows, RowsIntoIter, RowsIter, RowsOf},
    scope::scope,
    session::session,
    signal::{Signal, signal},
    url::url,
    valid::Validate,
};

// Named by the `#[model]` expansion, which has to reach them from anywhere.
#[doc(hidden)]
pub use crate::message::{HOLE, project};

#[doc(inline)]
pub use exos_macro::{
    Enumerable, asset, delete, form, get, live, locales, messages, model, patch, pattern, post,
    put, view,
};

// Re-exported so the macros can name them without the user taking a direct
// dependency, and so nobody has to keep versions in step with ours.
#[doc(hidden)]
pub use {axum, inventory, serde, serde_json};
