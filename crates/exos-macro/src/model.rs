//! Expansion of the `#[model]` attribute.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, Ident, ItemStruct};

/// Expands a struct into itself plus its signal handle, field tokens and
/// payload implementations.
pub(crate) fn expand(item: TokenStream) -> TokenStream {
    let input = match syn::parse2::<ItemStruct>(item) {
        Ok(input) => input,
        Err(error) => return error.to_compile_error(),
    };

    let Fields::Named(fields) = &input.fields else {
        return syn::Error::new_spanned(
            &input.fields,
            "a model needs named fields, because they become the signal names",
        )
        .to_compile_error();
    };

    let name = &input.ident;
    let visibility = &input.vis;
    let handle = Ident::new(&format!("{name}Signals"), name.span());

    let names: Vec<Ident> = fields
        .named
        .iter()
        .filter_map(|field| field.ident.clone())
        .collect();

    let types: Vec<syn::Type> = fields.named.iter().map(|field| field.ty.clone()).collect();
    let labels: Vec<String> = names.iter().map(ToString::to_string).collect();

    let tokens = names.iter().zip(&labels).map(|(ident, label)| {
        let constant = Ident::new(&ident.to_string().to_uppercase(), ident.span());
        let summary = format!("The `{label}` field, as a token.");

        quote! {
            #[doc = #summary]
            pub const #constant: ::exos::Field<#name> = ::exos::Field::new(#label);
        }
    });

    let handle_docs = format!("Signal handles for every field of [`{name}`].");

    quote! {
        #input

        #[doc = #handle_docs]
        #[derive(::core::clone::Clone, ::core::fmt::Debug)]
        #visibility struct #handle {
            #(
                #[doc = concat!("The `", #labels, "` signal.")]
                pub #names: ::exos::Signal<#types>,
            )*
        }

        impl #name {
            #(#tokens)*

            /// Signal handles for this model's fields, named after them.
            #[must_use]
            pub fn signals() -> #handle
            where
                Self: ::core::default::Default + ::exos::serde::Serialize,
            {
                let __initial = ::exos::serde_json::to_value(Self::default())
                    .unwrap_or(::exos::serde_json::Value::Null);

                #handle {
                    #(
                        #names: ::exos::Signal::with_value(
                            #labels,
                            __initial
                                .get(#labels)
                                .cloned()
                                .unwrap_or(::exos::serde_json::Value::Null),
                        ),
                    )*
                }
            }
        }

        impl ::exos::IntoAttributes for &#handle {
            fn write(self, __attributes: &mut ::exos::Attributes) {
                #(::exos::IntoAttributes::write(&self.#names, __attributes);)*
            }
        }

        impl ::exos::IntoAttributes for #handle {
            fn write(self, __attributes: &mut ::exos::Attributes) {
                ::exos::IntoAttributes::write(&self, __attributes);
            }
        }

        impl ::exos::IntoPayload<#name> for #handle {
            fn payload(&self) -> ::std::string::String {
                let __fields: ::std::vec::Vec<::std::string::String> = ::std::vec![
                    #(
                        ::std::format!(
                            "{}: {}",
                            ::exos::quote_js(#labels),
                            self.#names.get().source(),
                        ),
                    )*
                ];

                ::std::format!("{{{}}}", __fields.join(", "))
            }
        }

        /// A concrete value stands in for the handle when the server already
        /// knows what the body should be.
        impl ::exos::IntoPayload<#name> for #name
        where
            #name: ::exos::serde::Serialize,
        {
            fn payload(&self) -> ::std::string::String {
                ::exos::serde_json::to_string(self)
                    .unwrap_or_else(|_| ::std::string::String::from("{}"))
            }
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
    fn generates_a_handle_holding_one_signal_per_field() {
        let expanded = expand_ok("struct Selection { picked: Vec<u32>, fail: bool }");

        assert!(expanded.contains("struct SelectionSignals"));
        assert!(expanded.contains("picked : :: exos :: Signal"));
        assert!(expanded.contains("fail : :: exos :: Signal"));
    }

    #[test]
    fn generates_a_token_per_field() {
        let expanded = expand_ok("struct Draft { sku: String }");
        assert!(expanded.contains("const SKU"));
    }

    #[test]
    fn a_tuple_struct_is_refused_because_fields_become_names() {
        assert!(expand_ok("struct Selection(Vec<u32>);").contains("compile_error"));
    }
}
