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
mod asset;
mod attributes;
mod base;
mod context;
mod csrf;
mod discover;
mod effect;
mod fnv;
mod hex;
mod identity;
mod instant;
mod js;
mod keys;
mod live;
mod locale;
mod message;
mod model;
mod number;
mod render;
mod response;
mod rows;
mod scope;
mod session;
mod signal;
mod valid;
mod welcome;

pub use crate::{
    app::{App, app},
    asset::{Asset, AssetSet, Embedded, routes as asset_routes, runtime},
    attributes::{
        Attr, Attributes, Bind, BindKind, Class, Event, EventType, IntoAttributes, Link, Target,
        attr, bind, class, on, on_change, on_click, on_dblclick, on_focusout, on_input, on_keydown,
        on_submit, preserve, prop, show, text,
    },
    base::{base_path, segment, segments, url},
    context::{data, provide, try_data},
    discover::{AssetSetEntry, RouteEntry},
    effect::{Effect, EffectStream, Step},
    identity::{Audience, Audiences, Resolution},
    instant::{Instant, NotAnInstant, When},
    js::{
        IntoJs, IntoPayload, Js, append, attr_now, call, debounce, emit, focus_now, quote_js,
        record, when,
    },
    keys::Keys,
    live::{
        Fragment, Frame, Kind, Sent, Topic, connected, connection_count, deliver, disconnect,
        publish, send,
    },
    locale::{Direction, Lang, LocaleSet, PluralCategory, lang, locale},
    message::{Count, Counted, Enumerable},
    model::{Model, ModelFields, ModelRejection, nested_rows, to_wire},
    number::Symbols,
    render::{AttributeValue, Flag, Markup, Render, escape_display_into, escape_into},
    response::Page,
    rows::{Row, RowModel, Rows, RowsOf},
    scope::{Scope, detached, scope, with_scope},
    session::{Id, Session, session},
    signal::{Bound, Field, Placement, Signal, signal},
    valid::{CheckEntry, Errors, Length, Presence, Refusal, Validate, Violation},
};

// Named by the `#[model]` expansion, which has to reach them from anywhere.
#[doc(hidden)]
pub use crate::{
    message::{HOLE, project},
    valid::{all_valid, any_dirty, asked, chain, complaint, email_js, is_email, model_refusal},
};

#[doc(inline)]
pub use exos_macro::{
    Enumerable, asset, delete, get, live, locales, messages, model, patch, post, put, view,
};

// What keeps `LocaleSet` implementable by `locales!` alone, which has to be
// able to name it.
#[doc(hidden)]
pub use crate::locale::Sealed;

// Re-exported so the macros can name them without the user taking a direct
// dependency, and so nobody has to keep versions in step with ours.
#[doc(hidden)]
pub use {axum, inventory, serde, serde_json};
