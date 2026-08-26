//! Expansion of the `#[guard]` attribute.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;

/// Expands the attribute into the function and its registration.
pub(crate) fn expand(item: TokenStream) -> TokenStream {
    let function = match syn::parse2::<ItemFn>(item) {
        Ok(function) => function,
        Err(error) => return error.to_compile_error(),
    };

    if function.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            function.sig.fn_token,
            "a guard awaits the rest of the request, so it must be async",
        )
        .to_compile_error();
    }

    let name = &function.sig.ident;

    quote! {
        #function

        // An anonymous const so the submission cannot collide with anything
        // the user named, and so it stays out of the docs.
        const _: () = {
            ::exos::inventory::submit! {
                // `route_layer` rather than `layer`: middleware that answers
                // early must not run for a request that matched no route, or
                // every 404 becomes a redirect to the sign-in form.
                ::exos::GuardEntry::new(|router| {
                    router.route_layer(::exos::axum::middleware::from_fn(#name))
                })
            }
        };
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
        let item: TokenStream = item.parse().expect("valid item");
        expand(item).to_string()
    }

    #[test]
    fn keeps_the_function_and_adds_a_registration() {
        let expanded = expand_ok("async fn guard(request: Request, next: Next) -> Response {}");

        assert!(expanded.contains("async fn guard"));
        assert!(expanded.contains("GuardEntry"));
        assert!(expanded.contains("middleware :: from_fn"));
        assert!(
            expanded.contains("route_layer"),
            "a guard answers early, so it must not turn a 404 into a redirect"
        );
    }

    /// Otherwise the mistake surfaces as a trait bound on a function the
    /// author never wrote.
    #[test]
    fn refuses_one_that_cannot_await_the_rest_of_the_request() {
        let expanded = expand_ok("fn guard(request: Request, next: Next) -> Response {}");

        assert!(expanded.contains("compile_error"));
        assert!(expanded.contains("must be async"));
    }
}
