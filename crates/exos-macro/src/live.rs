//! Expansion of the `#[live]` attribute.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat};

/// Wraps a markup function so it also carries the topic that identifies it.
pub(crate) fn expand(item: TokenStream) -> TokenStream {
    let function = match syn::parse2::<ItemFn>(item) {
        Ok(function) => function,
        Err(error) => return error.to_compile_error(),
    };

    if function.sig.asyncness.is_some() {
        return syn::Error::new_spanned(
            function.sig.fn_token,
            "a live fragment renders inline and again on publish, so it must be sync; \
             read what it needs with exos::data",
        )
        .to_compile_error();
    }

    // Every parameter, in order. These are what identify the topic.
    let arguments: Vec<syn::Ident> = function
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(typed) => match &*typed.pat {
                Pat::Ident(ident) => Some(ident.ident.clone()),
                _ => None,
            },
            FnArg::Receiver(_) => None,
        })
        .collect();

    if arguments.len() != function.sig.inputs.len() {
        return syn::Error::new_spanned(
            &function.sig.inputs,
            "a live fragment's parameters must be plain names, since they identify its topic",
        )
        .to_compile_error();
    }

    let attributes = &function.attrs;
    let visibility = &function.vis;
    let body = &function.block;
    let label = function.sig.ident.to_string();

    let mut signature = function.sig.clone();
    signature.output = syn::parse_quote!(-> ::exos::Fragment);

    quote! {
        #(#attributes)*
        #visibility #signature {
            // Borrowed, so the arguments stay usable in the body below.
            let __topic = ::exos::Topic::new(#label, &(#(&#arguments,)*));

            // Detached, so the body cannot read the request scope. A fragment
            // renders again from whatever publishes it, where there is no
            // request, and content that differed between the two would break
            // the topic invariant.
            let __markup: ::exos::Markup = ::exos::detached(move || #body);

            ::exos::Fragment::new(__topic, __markup)
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    fn expand_ok(item: &str) -> String {
        expand(item.parse().expect("valid item")).to_string()
    }

    #[test]
    fn returns_a_fragment_rather_than_markup() {
        let expanded = expand_ok("fn presence(user: u32) -> Markup { todo!() }");

        assert!(expanded.contains(":: exos :: Fragment"));
        assert!(expanded.contains("Topic :: new"));
    }

    /// The topic invariant depends on a fragment rendering the same way inline
    /// and on publish, so the body is denied the one thing that differs.
    #[test]
    fn the_body_renders_detached_from_the_request_scope() {
        let expanded = expand_ok("fn presence(user: u32) -> Markup { todo!() }");
        assert!(expanded.contains(":: exos :: detached"));
    }

    #[test]
    fn an_async_fragment_is_refused() {
        let expanded = expand_ok("async fn presence() -> Markup { todo!() }");
        assert!(expanded.contains("compile_error"));
    }

    #[test]
    fn a_destructured_parameter_is_refused_because_it_names_the_topic() {
        let expanded = expand_ok("fn presence((a, b): (u32, u32)) -> Markup { todo!() }");
        assert!(expanded.contains("compile_error"));
    }
}
