//! Expansion of the `#[model]` attribute.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
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

    if let Some(error) = renamed(&input) {
        return error;
    }

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
    let keys: Vec<String> = names.iter().map(|field| signal_name(name, field)).collect();

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
                            #keys,
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

        impl ::exos::ModelFields for #name {
            const FIELDS: &'static [(&'static str, &'static str)] =
                &[#((#keys, #labels)),*];
        }

        impl ::exos::IntoPayload<#name> for #handle {
            fn payload(&self) -> ::std::string::String {
                let __fields: ::std::vec::Vec<::std::string::String> = ::std::vec![
                    #(
                        ::std::format!(
                            "{}: {}",
                            ::exos::quote_js(#keys),
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
                ::exos::to_wire(self)
            }
        }
    }
}

/// Refuses a `serde` rename anywhere on the model.
///
/// A model's fields never travel under their own name: the signal is named
/// after the field and the payload key is that same generated name, so a
/// rename would change what serde expects while changing nothing the client
/// sends. Rather than silently ignore it, say so. A type that needs a renamed
/// representation somewhere else needs a second type.
fn renamed(input: &ItemStruct) -> Option<TokenStream> {
    let fields = match &input.fields {
        Fields::Named(fields) => fields.named.iter().flat_map(|field| &field.attrs),
        _ => return None,
    };

    input
        .attrs
        .iter()
        .chain(fields)
        .filter(|attribute| attribute.path().is_ident("serde"))
        .find(|attribute| attribute.to_token_stream().to_string().contains("rename"))
        .map(|attribute| {
            syn::Error::new_spanned(
                attribute,
                "a model's fields do not travel under their own name, so a \
                 serde rename here changes what the server expects and nothing \
                 the client sends; give the renamed representation its own type",
            )
            .to_compile_error()
        })
}

/// What one field is called everywhere outside this crate: its signal in the
/// client store, and its key in an action's body.
///
/// Not the field name, which never leaves the server. A template reaches the
/// signal through the handle and the body is written by the generated caller,
/// so neither needs one, and a name nothing can spell is a name that stays
/// free to change. Spelling either would be a way for a rename to break the
/// browser while the build stays green.
///
/// Derived from the model and the field rather than from a call site, because
/// `signals()` is called wherever a handle is wanted, and the template that
/// declares one, the handler that writes it and the extractor that reads it
/// back all have to agree.
fn signal_name(model: &Ident, field: &Ident) -> String {
    // FNV-1a. Matches the shape `exos::signal` produces so that no name looks
    // special; beyond the shape the two need not agree, since they hash
    // different things.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;

    for byte in format!("{model}.{field}").bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }

    format!("s{:08x}", hash >> 32)
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;
    use quote::format_ident;

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

    /// The signal and the payload key are one generated name, so a rename
    /// cannot leave anything on the client pointing at the old one.
    #[test]
    fn neither_the_signal_nor_the_payload_key_is_the_field_name() {
        let expanded = expand_ok("struct Draft { sku: String }");
        let name = signal_name(&format_ident!("Draft"), &format_ident!("sku"));

        assert!(expanded.contains(&format!("with_value (\"{name}\"")));
        assert!(expanded.contains(&format!("quote_js (\"{name}\")")));
        assert!(!expanded.contains("quote_js (\"sku\")"));
    }

    /// The table the extractor renames through on the way back in. Without it
    /// a body keyed by the generated name has nothing to deserialize into.
    #[test]
    fn the_field_table_pairs_the_wire_name_with_the_field() {
        let expanded = expand_ok("struct Draft { sku: String }");
        let name = signal_name(&format_ident!("Draft"), &format_ident!("sku"));

        assert!(expanded.contains("ModelFields for Draft"));
        assert!(
            expanded.contains(&format!(r#""{name}" , "sku""#)),
            "{expanded}"
        );
    }

    /// Two models with a field of one name are two signals, so nesting one
    /// scope inside the other cannot silently shadow.
    #[test]
    fn one_field_name_under_two_models_gives_two_signals() {
        let field = format_ident!("picked");

        assert_ne!(
            signal_name(&format_ident!("Selection"), &field),
            signal_name(&format_ident!("Filter"), &field)
        );
    }

    #[test]
    fn a_tuple_struct_is_refused_because_fields_become_names() {
        assert!(expand_ok("struct Selection(Vec<u32>);").contains("compile_error"));
    }

    /// A rename would change what the server expects while changing nothing
    /// the client sends, so it is refused rather than quietly ignored.
    #[test]
    fn a_serde_rename_is_refused_wherever_it_sits() {
        let on_field = expand_ok(r#"struct Draft { #[serde(rename = "s")] sku: String }"#);
        let on_struct =
            expand_ok(r#"#[serde(rename_all = "camelCase")] struct Draft { unit_price: u32 }"#);

        assert!(on_field.contains("compile_error"));
        assert!(on_struct.contains("compile_error"));
    }

    /// Other serde attributes are none of this macro's business.
    #[test]
    fn an_unrelated_serde_attribute_is_left_alone() {
        let expanded = expand_ok(r#"#[serde(default)] struct Draft { sku: String }"#);

        assert!(!expanded.contains("compile_error"));
    }
}
