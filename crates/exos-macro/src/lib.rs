//! Macros for [exos](https://docs.rs/exos).
//!
//! Everything here is re-exported from the `exos` crate. Depend on that, not
//! on this.

use proc_macro::TokenStream;

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
