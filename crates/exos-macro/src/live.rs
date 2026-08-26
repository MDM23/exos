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

    // The render is the closure below rather than a boxed one, so that naming a
    // fragment costs nothing: a page builds one per live element and throws them
    // away again. Edition 2024 captures the argument lifetimes in the opaque
    // type, which is what lets a fragment take a reference.
    let mut signature = function.sig.clone();
    signature.output = syn::parse_quote!(-> ::exos::Fragment<impl Fn() -> ::exos::Markup>);

    quote! {
        #(#attributes)*
        #visibility #signature {
            // The module is part of what identifies a fragment, so two of them
            // may be called `status` without becoming one topic, and the name
            // alone stays the readable half of the id. Borrowed, so the
            // arguments stay usable in the render below.
            let __topic = ::exos::Topic::new(
                #label,
                &(::core::module_path!(), #(&#arguments,)*),
            );

            ::exos::Fragment::new(__topic, move || {
                // Cloned per render rather than moved, so the body reads the
                // arguments it was written against and the fragment stays
                // renderable again, which is what a publish asks of it.
                #(let #arguments = ::core::clone::Clone::clone(&#arguments);)*

                // Detached, so the body cannot read the request scope. A
                // fragment renders again from whatever publishes it, where
                // there is no request, and content that differed between the
                // two would break the topic invariant.
                let __markup: ::exos::Markup = ::exos::detached(move || #body);

                __markup
            })
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

    /// Two modules may each hold a `status`, and the behaviour is asserted in
    /// [`tests/topics.rs`](../../exos/tests/topics.rs), where there are two
    /// modules to have it in. This pins where the answer comes from.
    #[test]
    fn the_module_is_part_of_what_names_a_topic() {
        let expanded = expand_ok("fn status() -> Markup { todo!() }");
        assert!(expanded.contains("module_path !"));
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
