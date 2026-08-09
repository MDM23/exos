//! Macros for [exos](https://docs.rs/exos).
//!
//! Everything here is re-exported from the `exos` crate. Depend on that, not
//! on this.

use proc_macro::TokenStream;

mod live;
mod model;
mod route;
mod view;

/// Compiles real HTML into a sequence of string pushes.
///
/// The input is HTML: the tags, attributes and text you would write in a
/// `.html` file. Void elements are void, so `<br>` and `<link rel="...">` are
/// written the way HTML writes them rather than the XML-flavoured `<br/>` a
/// generic tag parser would insist on.
///
/// The one addition is that a braced block is Rust:
///
/// ```ignore
/// view! {
///     <ul class="files">
///         { entries.iter().map(row).collect::<Vec<_>>() }
///     </ul>
/// }
/// ```
///
/// Static parts become string literals in the binary, so nothing is parsed at
/// runtime. Interpolated values go through [`Render`](exos::Render) and are
/// escaped; `Markup` is the only exception.
///
/// An attribute written as a braced block contributes
/// [`Attributes`](exos::Attributes), which is how handlers and signal handles
/// attach to an element.
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    view::expand(input.into()).into()
}

/// Generates one method attribute.
macro_rules! method_attribute {
    ($name:ident, $http:literal) => {
        #[doc = concat!("Registers a handler for `", $http, "`.")]
        ///
        /// ```ignore
        /// #[exos::post("/files/{id}/favorite")]
        /// async fn favorite(Path(id): Path<u32>, Json(body): Json<Controls>) -> Effect {
        ///     /* ... */
        /// }
        /// ```
        ///
        /// The attribute is the whole registration: there is no second list,
        /// and no way to add a handler and forget to mount it.
        ///
        /// It also generates a module of the same name holding a typed caller,
        /// so a template writes `favorite::post(entry.id, &controls)` and has
        /// the URL, the path parameter's type and the payload type all
        /// checked. A module and a function may share a name, so the module
        /// sits beside the handler rather than shadowing it.
        #[proc_macro_attribute]
        pub fn $name(attribute: TokenStream, item: TokenStream) -> TokenStream {
            route::expand(attribute.into(), item.into(), stringify!($name)).into()
        }
    };
}

method_attribute!(delete, "DELETE");
method_attribute!(get, "GET");
method_attribute!(patch, "PATCH");
method_attribute!(post, "POST");
method_attribute!(put, "PUT");

/// Marks a type that is both client state and a request body.
///
/// ```ignore
/// #[exos::model]
/// #[derive(Default, Deserialize, Serialize)]
/// struct Selection {
///     picked: Vec<u32>,
/// }
///
/// let selection = Selection::signals();   // selection.picked: Signal<Vec<u32>>
/// ```
///
/// Declaring the fields once is the point: the handler takes
/// `Json<Selection>`, the template binds `selection.picked`, and renaming the
/// field breaks both. Alongside the struct it generates a handle whose fields
/// are signals, a typed token per field for error reporting, and the
/// implementations that let the handle declare itself on an element and be
/// sent as a payload.
#[proc_macro_attribute]
pub fn model(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    model::expand(item.into()).into()
}

/// Marks a fragment that keeps itself up to date.
///
/// ```ignore
/// #[exos::live]
/// fn presence(user: u32) -> Markup {
///     view! { <span class="dot" data-online={ online(user) }></span> }
/// }
/// ```
///
/// The function keeps its signature; only its return type changes, from
/// `Markup` to [`Fragment`](exos::Fragment). That one value does both jobs:
/// put it in a template to render it, or hand it to
/// [`publish`](exos::publish) to broadcast it.
///
/// The topic is derived from the function name and the argument values, so the
/// server owns it end to end and there is no name to invent, keep in step, or
/// collide with. Every argument must be `Hash`.
#[proc_macro_attribute]
pub fn live(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    live::expand(item.into()).into()
}
