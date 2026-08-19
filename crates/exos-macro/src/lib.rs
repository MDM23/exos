//! Macros for [exos](https://docs.rs/exos).
//!
//! Everything here is re-exported from the `exos` crate. Depend on that, not
//! on this.

use proc_macro::TokenStream;

mod asset;
mod live;
mod locales;
mod model;
mod profile;
mod route;
mod view;

/// Builds an asset and returns the URL it is served from.
///
/// Write it where the asset is referenced. There is no build script and no
/// registration step: the file is processed while this crate compiles, its
/// bytes go into the binary, and the URL comes back as a `&'static str` with
/// the content hash already in it.
///
/// ```ignore
/// view! {
///     <link rel="stylesheet" href={ exos::asset!("css/app.css") }>
///     <script defer src={ exos::runtime() }></script>
/// }
/// ```
///
/// The path is relative to the crate root, next to `Cargo.toml`, and the
/// extension picks the pipeline. A `.css` file is bundled through its
/// `@import`s; a `.js` or `.mjs` file is bundled through its `import`s;
/// anything else is embedded byte for byte. Release builds minify and debug
/// builds do not.
///
/// The extension also picks the `Content-Type`. For one the web has no name
/// for, say so:
///
/// ```ignore
/// exos::asset!("data/blob.xyz", "application/octet-stream")
/// ```
///
/// Referencing the same file from several places is free: it is embedded and
/// registered once, and every call site gets the same URL. A file that does
/// not exist, or one whose extension implies nothing and was not given a
/// content type, is a compile error here rather than a 404 later.
#[proc_macro]
pub fn asset(input: TokenStream) -> TokenStream {
    asset::expand(input.into()).into()
}

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
/// runtime. Interpolated values go through `exos::Render` and are escaped;
/// `Markup` is the only exception.
///
/// An attribute written as a braced block contributes `exos::Attributes`,
/// which is how handlers and signal handles attach to an element.
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
/// `Model<Selection>`, the template binds `selection.picked`, and renaming the
/// field breaks both. Alongside the struct it generates a handle whose fields
/// are signals, a typed token per field for error reporting, and the
/// implementations that let the handle declare itself on an element and be
/// sent as a payload.
///
/// A field name never leaves the server. Both the signal and the payload key
/// are named after the model and the field, so nothing outside the generated
/// pair can name either, which is what leaves both free to change; see
/// [`Model`](../exos/struct.Model.html). A `serde` rename is refused for that
/// reason, since it would change what the server expects and nothing the
/// client sends.
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
/// `Markup` to `exos::Fragment`. That one value does both jobs: put it in a
/// template to render it, or hand it to `exos::publish` to broadcast it.
///
/// The topic is derived from the function name and the argument values, so the
/// server owns it end to end and there is no name to invent, keep in step, or
/// collide with. Every argument must be `Hash`.
#[proc_macro_attribute]
pub fn live(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    live::expand(item.into()).into()
}

/// Declares the languages an application is built in.
///
/// ```ignore
/// exos::locales! {
///     De = "de",
///     #[fallback]
///     En = "en",
/// }
/// ```
///
/// Write it once, at the crate root, because everything localization decides
/// resolves `crate::Locale`. The list stays alphabetical and the fallback is
/// marked rather than positional; it is the locale a request answers with when
/// nothing better is known, which is what lets resolution return a `Locale`
/// rather than an `Option`.
///
/// It generates more than the list, and that is the point of having it:
///
/// - `enum Locale`, with `ALL`, `FALLBACK`, the tag each locale was declared
///   with, the writing direction, and `from_tag`.
/// - Per locale, a module named after the tag holding a `Plural` enum with
///   **exactly the categories CLDR gives that language**, so `de::Plural` has
///   `One` and `Other` while `ar::Plural` has six, and a `category` function
///   mapping a count to one of them.
/// - An implementation of `exos::LocaleSet`, which is how
///   `exos::locale::<Locale>()` answers with a type exos has never seen. It
///   delegates to the items above, so nothing has to be imported to ask a
///   locale for its tag.
///
/// Both come from the CLDR table [`exos-cldr`](https://docs.rs/exos-cldr)
/// vendors as ordinary source, so nothing is fetched or parsed while an
/// application builds, and only the locales that were declared are compiled
/// into it.
///
/// A tag is matched by dropping subtags, so `de-AT` resolves through `de`,
/// while a tag naming its own script means that script: `pa` is written left
/// to right and `pa-Arab` is not. A language CLDR has no rules for is a
/// compile error here rather than a wrong plural later.
///
/// Counts are whole numbers in this stage. The operands CLDR uses to describe
/// the digits after a decimal point are therefore zero, which is what collapses
/// most languages to one or two comparisons, and why the five whose `many`
/// applies only to a fraction do not carry that category at all.
#[proc_macro]
pub fn locales(input: TokenStream) -> TokenStream {
    locales::expand(input.into()).into()
}
