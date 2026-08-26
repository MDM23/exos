//! Macros for [exos](https://docs.rs/exos).
//!
//! Everything here is re-exported from the `exos` crate. Depend on that, not
//! on this.

use proc_macro::TokenStream;

mod asset;
mod enumerable;
mod escape;
mod guard;
mod live;
mod locales;
mod messages;
mod model;
mod profile;
mod route;
mod valid;
mod view;

/// Builds an asset and returns a handle to it.
///
/// Write it where the asset is referenced. There is no build script and no
/// registration step: the file is processed while this crate compiles, its
/// bytes go into the binary, and what comes back is an `exos::Asset` naming
/// the hash they were embedded under. In a view it renders as the URL it is
/// served from; `bytes()` on it is the same file again, to compute whatever
/// its content decides.
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
/// A stylesheet's `url()`s are assets in their own right. Each is resolved
/// against the stylesheet that wrote it, embedded, and rewritten to the hashed
/// name it is served under, so a background image needs no call of its own.
/// What this crate cannot reach is left as written: another host, a `data:`
/// URI, a path from the server root. One inside a custom property is refused,
/// because a browser resolves that one against the page the `var()` is used on
/// and no URL is right on every route.
///
/// The extension also picks the `Content-Type`. For one the web has no name
/// for, say so:
///
/// ```ignore
/// exos::asset!("data/blob.xyz", "application/octet-stream")
/// ```
///
/// Referencing the same file from several places is free: it is embedded and
/// registered once, and every call site gets the same handle. A file that does
/// not exist, or one whose extension implies nothing and was not given a
/// content type, is a compile error here rather than a 404 later.
#[proc_macro]
pub fn asset(input: TokenStream) -> TokenStream {
    asset::expand(input.into()).into()
}

/// Compiles real HTML into a sequence of string pushes.
///
/// The input is HTML: the tags and attributes you would write in a `.html`
/// file. Void elements are void, so `<br>` and `<link rel="...">` are written
/// the way HTML writes them rather than the XML-flavoured `<br/>` a generic
/// tag parser would insist on.
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
/// Text is a string literal: `<p>"Hello"</p>`. Written bare it would arrive
/// here as Rust tokens with the spacing rearranged, turning `50% off` into
/// `50 % off`, so bare text is a compile error instead.
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

/// Registers the middleware every page is served through.
///
/// ```ignore
/// #[exos::guard]
/// async fn guard(request: Request, next: Next) -> Response {
///     let Some(user) = whoever(exos::session().id()).await else {
///         return Redirect::to(&login::url()).into_response();
///     };
///
///     exos::scope().set(user);
///
///     next.run(request).await
/// }
/// ```
///
/// It is ordinary axum middleware, and the attribute decides only where it is
/// mounted: inside the layers `exos::app` puts up, so `exos::session`,
/// `exos::locale` and `exos::scope` all answer here, and around the routes the
/// application declared and nothing else. Whatever it writes into the scope is
/// readable from a view, which is a plain function and can extract nothing.
///
/// exos's own endpoints are outside it. A stream and a field check answer the
/// client runtime rather than a browser that could follow a redirect, and a
/// subscription is already bound to the session that was served the fragment.
/// So is a request matching no route, which stays a 404 instead of becoming a
/// redirect to the sign-in form.
///
/// One per application. Middleware order is meaning and link order is not an
/// order, so a second one is a panic at startup rather than a coin toss;
/// anything that does not need the session or the scope is a `.layer` on the
/// router `exos::app` returns.
#[proc_macro_attribute]
pub fn guard(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    guard::expand(item.into()).into()
}

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
/// The topic is derived from the module, the function name and the argument
/// values, so the server owns it end to end and there is no name to invent,
/// keep in step, or collide with. Every argument must be `Hash`. The module is
/// in the hash rather than in the readable half of the id, and it is what lets
/// two modules each hold a `status`; the cost is that moving a fragment renames
/// its topic, which is a deploy that drops the documents holding the old name.
///
/// Calling the function names the fragment; the body runs when something asks
/// for its markup, which is once per template it appears in and once per
/// publish. So an argument is held until then and handed to the body by clone
/// on each render, which is why every argument must be `Clone` as well as
/// `Hash`. Borrowing one is fine, since the fragment does not outlive the call
/// that named it.
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
///   mapping a count to one of them. Those categories are an
///   [`Enumerable`](derive@Enumerable) domain like any other a message branches
///   on.
/// - Per locale, the symbols it writes a whole number with, and `number`,
///   which writes one: `Locale::De.number(1_234_567)` is `1.234.567`.
/// - An implementation of `exos::LocaleSet`, which is how
///   `exos::locale::<Locale>()` answers with a type exos has never seen. It
///   delegates to the items above, so nothing has to be imported to ask a
///   locale for its tag.
/// - One hidden alias per locale, keyed by the variant, which is how
///   [`messages!`](messages) reaches a language's categories from an arm that
///   names `De` rather than `"de"`.
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
/// Counts are whole numbers. The operands CLDR uses to describe the digits
/// after a decimal point are therefore zero, which is what collapses most
/// languages to one or two comparisons, why the five whose `many` applies only
/// to a fraction do not carry that category at all, and why the symbols
/// vendored alongside are the ones a whole number needs and not the decimal
/// separator.
#[proc_macro]
pub fn locales(input: TokenStream) -> TokenStream {
    locales::expand(input.into()).into()
}

/// Declares text, in every language the application is built in.
///
/// ```ignore
/// exos::messages! {
///     clear_selection {
///         De = "Auswahl aufheben",
///         En = "Clear selection",
///     }
///
///     items_selected(count: Plural) {
///         De { One } = "{count} Element ausgewählt",
///         De { _ }   = "{count} Elemente ausgewählt",
///         En { One } = "{count} item selected",
///         En { _ }   = "{count} items selected",
///     }
/// }
/// ```
///
/// It writes `clear_selection()` and `items_selected(count)` where the block
/// stands, as public functions and nothing else. Both read `exos::locale()` and
/// answer with a `String`, so a message interpolated into a [`view!`](view) is
/// escaped like any other string, and no part of one is ever parsed as HTML.
///
/// Write it wherever the text is used. The macro may be invoked as often as an
/// application likes, so messages live next to the feature that says them
/// rather than in one file every branch touches. Where they want a prefix, the
/// block goes in a module of the crate's own naming, and `t::clear_selection()`
/// is a `mod t` around it.
///
/// # Arms
///
/// An arm names a locale as a variant of `crate::Locale`, then one pattern per
/// parameter. They read like a `match`: in order, first wins, with `_` and `..`
/// as wildcards. That order carries meaning, so arms are the one list in this
/// repository that is not kept alphabetical. A bare name is a value rather than
/// a binding, so `De { One }` is the category and not a catch-all under a
/// misleading name; write a locale with no braces at all where it says one
/// thing whatever the parameters are.
///
/// A parameter declared as `Plural` is a count. It arrives as any whole number
/// and is branched on through the categories of the language being rendered,
/// which are that language's own: `de::Plural` has `One` and `Other` while
/// `ar::Plural` has six. It is also written into the sentence the way that
/// language writes a number, so German says 1.234 where English says 1,234.
/// Any other parameter is interpolated by `{name}`
/// wherever the translation puts it, as often as it likes or not at all, and
/// branched on where an arm names one of its values, which asks that its type
/// derive [`Enumerable`](derive@Enumerable).
///
/// # Slots
///
/// A sentence with a link in it is one message rather than three, because the
/// link lands somewhere else in the next language. A parameter declared as
/// `Slot` is a wrapper, `FnOnce(Markup) -> Markup`, and the translation says
/// which words go inside it:
///
/// ```ignore
/// exos::messages! {
///     accept_terms(terms: Slot) {
///         De = "Bitte die {terms}Nutzungsbedingungen{/terms} annehmen.",
///         En = "Please accept the {terms}terms of service{/terms}.",
///     }
/// }
///
/// accept_terms(|inner| view! { <a href="/terms">{ inner }</a> })
/// ```
///
/// So the href, the classes and the routing stay in Rust while the words stay
/// in the sentence. `{b}` and `{i}` are the same mechanism with the wrapper
/// already written, as `<strong>` and `<em>`, since emphasis falls on
/// different words in different languages and there is nothing there for a
/// call site to decide.
///
/// A message with a slot in it answers with `exos::Markup` instead of a
/// `String`. Its own words are escaped while your crate compiles, so no part
/// of a message string is ever parsed as HTML and a translation cannot
/// introduce an element by being edited. Three more things are checked here:
/// slots are balanced, every declared slot wraps something in every arm, and
/// nothing branches on a slot, which is a wrapper rather than a value.
///
/// # What the compiler checks
///
/// A proc macro cannot see another invocation, so "every message covers every
/// locale" is not a check this macro performs. It does not have to be: the
/// generated `match` over `crate::Locale` and the `match` over each language's
/// categories leave the work to rustc.
///
/// | what is wrong | what the compiler says |
/// | --- | --- |
/// | a locale is missing from a message | non-exhaustive match on `Locale` |
/// | a category is missing for a locale | non-exhaustive match on `de::Plural` |
/// | a category does not exist in that language | no variant `de::Plural::Few` |
/// | a branching enum gained a variant | non-exhaustive match, at every message |
///
/// Adding a locale to [`locales!`](locales) therefore breaks every `messages!`
/// block in the workspace until it is translated, which is the point.
///
/// `crate::Locale` is a convention rather than a lookup, and
/// `exos::messages!(in path::to::Locale { ... })` is the way out of it, for a
/// locale set that is somewhere else.
#[proc_macro]
pub fn messages(input: TokenStream) -> TokenStream {
    messages::expand(input.into()).into()
}

/// Marks an enum a message can branch on.
///
/// ```ignore
/// #[derive(Clone, Copy, exos::Enumerable)]
/// enum Assignee {
///     Me,
///     Somebody,
/// }
/// ```
///
/// Interpolating a value asks nothing of its type beyond `Display`. Branching
/// on one asks that its values can be listed, since a message picks a string
/// per value, and that is what this says: `Assignee::ALL` is every value, in
/// the order they were declared.
///
/// A variant carries nothing, because a message would otherwise need a string
/// for every value of whatever it carried.
#[proc_macro_derive(Enumerable)]
pub fn enumerable(item: TokenStream) -> TokenStream {
    enumerable::expand(item.into()).into()
}
