//! Expansion of the method attributes.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, LitStr};

/// Expands one method attribute into the function, its registration and the
/// typed caller that goes with it.
pub(crate) fn expand(attribute: TokenStream, item: TokenStream, method: &str) -> TokenStream {
    let path = match syn::parse2::<LitStr>(attribute) {
        Ok(path) => path,
        Err(error) => return error.to_compile_error(),
    };

    if !path.value().starts_with('/') {
        return syn::Error::new(path.span(), "a route path starts with `/`").to_compile_error();
    }

    let function = match syn::parse2::<ItemFn>(item) {
        Ok(function) => function,
        Err(error) => return error.to_compile_error(),
    };

    let name = &function.sig.ident;
    let method = syn::Ident::new(method, name.span());

    quote! {
        #function

        // An anonymous const so the submission cannot collide with anything
        // the user named, and so it stays out of the docs.
        const _: () = {
            ::exos::inventory::submit! {
                ::exos::RouteEntry::new(#path, || ::exos::axum::routing::#method(#name))
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand_ok(attribute: &str, item: &str) -> String {
        let attribute: TokenStream = attribute.parse().expect("valid attribute");
        let item: TokenStream = item.parse().expect("valid item");
        expand(attribute, item, "get").to_string()
    }

    #[test]
    fn keeps_the_function_and_adds_a_registration() {
        let expanded = expand_ok(r#""/files""#, "async fn files() {}");

        assert!(expanded.contains("async fn files"));
        assert!(expanded.contains("RouteEntry"));
        assert!(expanded.contains("routing :: get"));
    }

    #[test]
    fn rejects_a_path_that_does_not_start_with_a_slash() {
        let expanded = expand_ok(r#""files""#, "async fn files() {}");
        assert!(expanded.contains("compile_error"));
    }
}
