//! Expansion of the method attributes.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{FnArg, Ident, ItemFn, LitStr, PathArguments, Type};

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
    let method = Ident::new(method, name.span());
    let caller = caller(&function, &path, &method);

    quote! {
        #function

        // An anonymous const so the submission cannot collide with anything
        // the user named, and so it stays out of the docs.
        const _: () = {
            ::exos::inventory::submit! {
                ::exos::RouteEntry::new(#path, || ::exos::axum::routing::#method(#name))
            }
        };

        #caller
    }
}

/// Builds `mod <name> { pub fn <method>(..) }` from the handler's signature.
///
/// A module and a function may share a name, since they live in different
/// namespaces, so `favorite::post(..)` sits beside `async fn favorite`.
///
/// Only the extractors a caller can supply are understood: `Path<T>` becomes a
/// positional argument, and `Model<T>` or `Json<T>` becomes the payload.
/// Anything else is server-side and simply is not part of the client-visible
/// signature.
fn caller(function: &ItemFn, path: &LitStr, method: &Ident) -> TokenStream {
    let name = &function.sig.ident;
    let (path_types, body_type) = extractors(function);
    let (format, arguments) = template(path, name.span());

    // A path parameter the signature does not account for would silently
    // produce a broken URL, so refuse rather than guess.
    if arguments.len() != path_types.len() {
        let message = format!(
            "the path has {} parameter(s) but the handler destructures {} through `Path<..>`, \
             so the typed caller cannot be generated",
            arguments.len(),
            path_types.len()
        );

        return quote! { compile_error!(#message); };
    }

    // Built as one list: appending the body after `#(#params,)*` would leave a
    // stray comma whenever either side is empty.
    let path_parameters: Vec<TokenStream> = arguments
        .iter()
        .zip(&path_types)
        .map(|(ident, ty)| quote! { #ident: #ty })
        .collect();

    let mut parameters = path_parameters.clone();

    let body = match &body_type {
        Some(ty) => {
            parameters.push(quote! { body: &impl ::exos::IntoPayload<#ty> });
            quote! { ::std::option::Option::Some(::exos::IntoPayload::payload(body)) }
        }
        None => quote! { ::std::option::Option::None },
    };

    let method_name = method.to_string();
    let summary = format!(
        "Records a call to `{} {}`. Only valid inside a handler.",
        method_name.to_uppercase(),
        path.value()
    );

    let url_summary = format!(
        "The URL of `{}`, under the application's base.",
        path.value()
    );

    quote! {
        #[doc = concat!("Typed client-side caller for [`", stringify!(#name), "`].")]
        #[allow(non_snake_case, unused_imports)]
        pub mod #name {
            use super::*;

            #[doc = #url_summary]
            ///
            /// Built from the route's own path and the types its handler
            /// destructures, so a renamed route or a changed parameter breaks
            /// every link to it rather than producing one that 404s.
            #[must_use]
            pub fn url(#(#path_parameters),*) -> ::std::string::String {
                ::exos::url(::std::format!(#format, #(#arguments),*))
            }

            #[doc = #summary]
            pub fn #method(#(#parameters),*) {
                ::exos::call(#method_name, &url(#(#arguments),*), #body);
            }
        }
    }
}

/// The `Path<T>` and body types in a handler's signature.
///
/// Both body extractors count. `Model<T>` is the one an action uses, and
/// `Json<T>` stays for a body something other than this crate's client wrote;
/// which of them a handler names does not change what the caller looks like,
/// because [`IntoPayload`] is what decides the shape either way.
fn extractors(function: &ItemFn) -> (Vec<syn::GenericArgument>, Option<syn::GenericArgument>) {
    let mut path_types = Vec::new();
    let mut body_type = None;

    for input in &function.sig.inputs {
        let FnArg::Typed(typed) = input else {
            continue;
        };
        let Type::Path(type_path) = &*typed.ty else {
            continue;
        };
        let Some(segment) = type_path.path.segments.last() else {
            continue;
        };

        let inner = match &segment.arguments {
            PathArguments::AngleBracketed(arguments) => arguments.args.first().cloned(),
            _ => None,
        };

        match (segment.ident.to_string().as_str(), inner) {
            ("Path", Some(inner)) => path_types.push(inner),
            ("Json" | "Model", Some(inner)) => body_type = Some(inner),
            _ => {}
        }
    }

    (path_types, body_type)
}

/// Turns `/files/{id}/favorite` into a format string and its arguments.
fn template(path: &LitStr, span: Span) -> (String, Vec<Ident>) {
    let value = path.value();
    let mut format = String::new();
    let mut arguments = Vec::new();

    for segment in value.split('/').skip(1) {
        format.push('/');

        if segment.starts_with('{') && segment.ends_with('}') {
            format.push_str("{}");
            arguments.push(Ident::new(&format!("p{}", arguments.len()), span));
        } else {
            format.push_str(segment);
        }
    }

    (format, arguments)
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    fn expand_ok(attribute: &str, item: &str) -> String {
        let attribute: TokenStream = attribute.parse().expect("valid attribute");
        let item: TokenStream = item.parse().expect("valid item");
        expand(attribute, item, "get").to_string()
    }

    fn parts(path: &str) -> (String, usize) {
        let literal = LitStr::new(path, Span::call_site());
        let (format, arguments) = template(&literal, Span::call_site());
        (format, arguments.len())
    }

    #[test]
    fn keeps_the_function_and_adds_a_registration() {
        let expanded = expand_ok(r#""/files""#, "async fn files() {}");

        assert!(expanded.contains("async fn files"));
        assert!(expanded.contains("RouteEntry"));
        assert!(expanded.contains("routing :: get"));
    }

    #[test]
    fn generates_a_module_holding_the_typed_caller() {
        let expanded = expand_ok(r#""/files""#, "async fn files() {}");

        assert!(expanded.contains("pub mod files"));
        assert!(expanded.contains("pub fn get"));
    }

    /// A link to a route is built from the same path and the same types the
    /// caller is, so renaming the route breaks every link to it rather than
    /// leaving one that 404s.
    #[test]
    fn generates_a_url_from_the_path_parameters_alone() {
        let expanded = expand_ok(
            r#""/files/{id}""#,
            "async fn show(Path(id): Path<u32>, Model(body): Model<Draft>) {}",
        );

        assert!(expanded.contains("pub fn url (p0 : u32) -> :: std :: string :: String"));
        assert!(
            !expanded.contains("pub fn url (p0 : u32 , body"),
            "a link has no body to send"
        );
    }

    /// One place decides what a route's URL is. Prefixing in both would put the
    /// application's base on twice the moment it stopped being empty.
    #[test]
    fn the_caller_asks_for_the_url_rather_than_building_a_second_one() {
        let expanded = expand_ok(r#""/files/{id}""#, "async fn show(Path(id): Path<u32>) {}");

        assert!(
            expanded.contains("call (\"get\" , & url (p0)"),
            "{expanded}"
        );
        assert_eq!(
            expanded.matches("format !").count(),
            1,
            "the path template is written out once"
        );
    }

    /// A handler naming either body extractor gets the same typed caller, so
    /// moving one to `Model<T>` is not a change to its call sites.
    #[test]
    fn either_body_extractor_becomes_the_payload() {
        for extractor in ["Json", "Model"] {
            let handler = format!("async fn save({extractor}(body): {extractor}<Draft>) {{}}");
            let expanded = expand_ok(r#""/drafts""#, &handler);

            assert!(
                expanded.contains("IntoPayload < Draft >"),
                "{extractor}: {expanded}"
            );
        }
    }

    #[test]
    fn rejects_a_path_that_does_not_start_with_a_slash() {
        assert!(expand_ok(r#""files""#, "async fn files() {}").contains("compile_error"));
    }

    #[test]
    fn refuses_when_the_signature_does_not_account_for_a_path_parameter() {
        let expanded = expand_ok(r#""/files/{id}""#, "async fn show() {}");
        assert!(expanded.contains("compile_error"));
    }

    #[test]
    fn a_static_path_needs_no_arguments() {
        assert_eq!(parts("/files"), (String::from("/files"), 0));
    }

    #[test]
    fn each_parameter_becomes_a_placeholder() {
        assert_eq!(
            parts("/files/{id}/favorite"),
            (String::from("/files/{}/favorite"), 1)
        );
        assert_eq!(parts("/a/{x}/b/{y}"), (String::from("/a/{}/b/{}"), 2));
    }

    #[test]
    fn the_root_path_survives() {
        assert_eq!(parts("/"), (String::from("/"), 0));
    }
}
