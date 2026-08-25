//! Expansion of the `#[model]` attribute.

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Fields, Ident, ItemStruct};

use crate::valid;

/// Expands a struct into itself plus its signal handle, field tokens and
/// payload implementations.
pub(crate) fn expand(item: TokenStream) -> TokenStream {
    let mut input = match syn::parse2::<ItemStruct>(item) {
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

    let name = input.ident.clone();

    // Read before the struct is emitted, and taken off it: `valid` is this
    // macro's word and rustc knows nothing about it.
    let rules = match valid::rules(fields, |field| signal_name(&name, field)) {
        Ok(rules) => rules,
        Err(error) => return error.to_compile_error(),
    };

    valid::strip(&mut input);

    let Fields::Named(fields) = &input.fields else {
        unreachable!("the fields were named a moment ago")
    };

    let visibility = &input.vis;
    let handle = Ident::new(&format!("{name}Signals"), name.span());

    let names: Vec<Ident> = fields
        .named
        .iter()
        .filter_map(|field| field.ident.clone())
        .collect();

    let types: Vec<syn::Type> = fields.named.iter().map(|field| field.ty.clone()).collect();

    // Which fields hold rows of another model, and of which. A `Rows<Line>`
    // field is a handle over many `Line`s rather than a signal over one value,
    // so it is the one field the pieces below all treat differently.
    let rows: Vec<Option<syn::Type>> = types.iter().map(rows_of).collect();
    let labels: Vec<String> = names.iter().map(ToString::to_string).collect();
    let keys: Vec<String> = names
        .iter()
        .map(|field| signal_name(&name, field))
        .collect();

    let tokens = names.iter().zip(&labels).map(|(ident, label)| {
        let constant = Ident::new(&ident.to_string().to_uppercase(), ident.span());
        let summary = format!("The `{label}` field, as a token.");

        quote! {
            #[doc = #summary]
            pub const #constant: ::exos::Field<#name> = ::exos::Field::new(#label);
        }
    });

    let handle_docs = format!("Signal handles for every field of [`{name}`].");

    // Where this model's errors live. Hashed like a field so it looks like
    // nothing special, off a name no field can spell.
    let state = signal_name(&name, &format_ident!("__state"));
    let checks = valid::check(&rules);

    // The rules only the server can answer: one entry per checked field for
    // the route to resolve, and the same functions awaited at submit.
    let entries = valid::entries(&rules, &state);
    let awaited = valid::checks(&rules);

    // The same rules, asked the other way round. One list, two readers, which
    // is the whole reason they are declared rather than written twice.
    let asked: Vec<TokenStream> = names
        .iter()
        .map(|field| valid::ask(&rules, field))
        .collect();

    // And, for a field that gates others, which they are. Editing a gate
    // changes which rules there are, so what they said goes with the edit.
    let arming: Vec<String> = names
        .iter()
        .map(|field| valid::arms(&rules, field))
        .collect();

    // A checked field is addressed by this model and its own name, so the
    // control carries the model it is checked by rather than the record it
    // writes into: for a field of a row those are two different models.
    let checking: Vec<&str> = names
        .iter()
        .map(|field| match valid::checked(&rules, field) {
            true => state.as_str(),
            false => "",
        })
        .collect();

    // What one field is on the handle, how it is built, what it sends, and
    // what a row of it is validated against. A `Rows` field answers all four
    // differently, so they are built together rather than four matches apart.
    let held: Vec<TokenStream> = types
        .iter()
        .zip(&rows)
        .map(|(ty, row)| match row {
            Some(row) => quote! { ::exos::RowsOf<#row> },
            None => quote! { ::exos::Bound<#ty> },
        })
        .collect();

    let built: Vec<TokenStream> = keys
        .iter()
        .zip(&rows)
        .zip(&asked)
        .zip(&labels)
        .zip(&arming)
        .zip(&checking)
        .map(
            |(((((key, row), asked), label), arming), checking)| match row {
                // The rows the model opens with, which is what `each` renders
                // before the template. Everything after that is the browser's.
                Some(_) => quote! {
                    ::exos::RowsOf::new(
                        #key,
                        #state,
                        __initial
                            .get(#label)
                            .and_then(::exos::serde_json::Value::as_array)
                            .cloned()
                            .unwrap_or_default(),
                    )
                },
                None => quote! {{
                    let __signal = ::exos::Signal::with_value(
                        #key,
                        __initial
                            .get(#label)
                            .cloned()
                            .unwrap_or(::exos::serde_json::Value::Null),
                        // On the document, not on whichever element declares the
                        // handle: a handler answers with Effect::set, which the
                        // client applies against the document root.
                        ::exos::Placement::Document,
                    );

                    let __asked = #asked;

                    ::exos::Bound::new(__signal, #state, __asked)
                        .arming(#arming)
                        .checking(#checking)
                }},
            },
        )
        .collect();

    // A rows field declares nothing: a row's signals belong to the row, and
    // the handle putting them on the form would name every row it was told
    // about wherever the form's own handle happened to sit.
    let declared: Vec<&Ident> = names
        .iter()
        .zip(&rows)
        .filter(|(_, row)| row.is_none())
        .map(|(field, _)| field)
        .collect();

    // The renaming does not stop at the top level. A row is a model with keys
    // of its own, so a body whose rows kept their field names is one serde
    // cannot read and one the client never sends.
    let nests: Vec<TokenStream> = labels
        .iter()
        .zip(&rows)
        .filter_map(|(label, row)| {
            let row = row.as_ref()?;

            Some(quote! {
                #label => ::exos::nested_rows::<#row>(__value, __outwards),
            })
        })
        .collect();

    let sent: Vec<TokenStream> = names
        .iter()
        .zip(&rows)
        .map(|(field, row)| match row {
            Some(_) => quote! { ::exos::RowsOf::payload(&self.#field) },
            None => quote! { self.#field.get().source() },
        })
        .collect();

    // A row's own rules run under a prefix naming where the row sits, so a
    // message about it lands on the key that row's control reads.
    let walked: Vec<TokenStream> = names
        .iter()
        .zip(&rows)
        .zip(&keys)
        .filter_map(|((field, row), key)| {
            row.as_ref()?;

            Some(quote! {
                for (__at, __row) in ::exos::Rows::iter(&self.#field).enumerate() {
                    ::exos::Validate::validate_into(
                        __row,
                        &::std::format!("{}{}.{}.", __prefix, #key, __at),
                        __errors,
                    );
                }
            })
        })
        .collect();

    // The same walk, for the rules a row can only have answered by the server.
    // Sequential rather than joined: a check is a query, and a form that adds
    // rows freely would otherwise decide how many run at once.
    let checked: Vec<TokenStream> = names
        .iter()
        .zip(&rows)
        .zip(&keys)
        .filter_map(|((field, row), key)| {
            row.as_ref()?;

            Some(quote! {
                for (__at, __row) in ::exos::Rows::iter(&self.#field).enumerate() {
                    ::exos::Validate::check_into(
                        __row,
                        &::std::format!("{}{}.{}.", __prefix, #key, __at),
                        __errors,
                    )
                    .await;
                }
            })
        })
        .collect();

    // Every field as one row holds it: on the row's own element, so that a
    // clone of the template is its own scope and nothing has to name it.
    let within: Vec<TokenStream> = keys
        .iter()
        .zip(&rows)
        .zip(&asked)
        .zip(&labels)
        .zip(&arming)
        .zip(&checking)
        .map(
            |(((((key, row), asked), label), arming), checking)| match row {
                // Rows of rows would need a group inside a group, and nothing has
                // asked for one. Left empty rather than silently addressing the
                // wrong signals.
                Some(_) => quote! {
                    ::exos::RowsOf::new(#key, __state, ::std::vec::Vec::new())
                },
                None => quote! {{
                    let __signal = ::exos::Signal::with_value(
                        #key,
                        __initial
                            .get(#label)
                            .cloned()
                            .unwrap_or(::exos::serde_json::Value::Null),
                        ::exos::Placement::Element,
                    );

                    let __asked = #asked;

                    ::exos::Bound::row(__signal, __state, __asked, __group)
                        .arming(#arming)
                        .checking(#checking)
                }},
            },
        )
        .collect();

    quote! {
        #input

        #[doc = #handle_docs]
        #[derive(::core::clone::Clone, ::core::fmt::Debug)]
        #visibility struct #handle {
            #(
                #[doc = concat!("The `", #labels, "` field.")]
                pub #names: #held,
            )*
        }

        impl #name {
            #(#tokens)*

            /// Signal handles for this model's fields, named after them.
            pub fn signals() -> #handle
            where
                Self: ::core::default::Default + ::exos::serde::Serialize,
            {
                let __initial = ::exos::serde_json::to_value(Self::default())
                    .unwrap_or(::exos::serde_json::Value::Null);

                #handle { #(#names: #built,)* }
            }
        }

        impl #handle {
            /// Whether nothing in this model is currently complaining.
            ///
            /// One read of the record every message lands in, so a row and a
            /// refusal count for as much as a rule the browser answered. A
            /// form nobody has touched is valid: nothing has judged it yet.
            pub fn valid(&self) -> ::exos::Js<bool> {
                ::exos::all_valid(#state)
            }

            /// Whether any of its controls has been edited.
            pub fn dirty(&self) -> ::exos::Js<bool> {
                ::exos::any_dirty(#state)
            }

            /// What a handler said about the submission rather than about one
            /// field of it, through `Refusal::say`.
            ///
            /// Empty while there is nothing to say, so a template reads it the
            /// way it reads a field's own message.
            pub fn refusal(&self) -> ::exos::Js<::std::string::String> {
                ::exos::model_refusal(#state)
            }

            /// Whether there is one.
            ///
            /// About the model itself, where [`valid`](Self::valid) is about
            /// everything anything has left in the record.
            pub fn refused(&self) -> ::exos::Js<bool> {
                !self.refusal().is_empty()
            }
        }

        impl ::exos::RowModel for #name {
            type Handle = #handle;

            fn row(
                __initial: &::exos::serde_json::Value,
                __state: &'static str,
                __group: &'static str,
            ) -> #handle {
                #handle { #(#names: #within,)* }
            }

            fn keys() -> &'static [&'static str] {
                &[#(#keys),*]
            }
        }

        impl ::exos::IntoAttributes for &#handle {
            fn write(self, __attributes: &mut ::exos::Attributes) {
                #(::exos::IntoAttributes::write(&self.#declared, __attributes);)*

                // The record every field's error is read out of. Declared
                // beside them rather than as a field of its own, because it is
                // not one: nothing sends it and nothing binds to it.
                ::exos::IntoAttributes::write(
                    &::exos::Signal::<::exos::Errors>::with_value(
                        #state,
                        ::exos::serde_json::Value::Object(
                            ::exos::serde_json::Map::new()
                        ),
                        ::exos::Placement::Document,
                    ),
                    __attributes,
                );
            }
        }

        impl ::exos::Validate for #name {
            const STATE: &'static str = #state;

            fn validate_into(&self, __prefix: &str, __errors: &mut ::exos::Errors) {
                #checks
                #(#walked)*
            }

            fn check_into(
                &self,
                __prefix: &str,
                __errors: &mut ::exos::Errors,
            ) -> impl ::core::future::Future<Output = ()> + ::core::marker::Send {
                async move {
                    #awaited
                    #(#checked)*
                }
            }
        }

        #entries

        impl ::exos::IntoAttributes for #handle {
            fn write(self, __attributes: &mut ::exos::Attributes) {
                ::exos::IntoAttributes::write(&self, __attributes);
            }
        }

        impl ::exos::ModelFields for #name {
            const FIELDS: &'static [(&'static str, &'static str)] =
                &[#((#keys, #labels)),*];

            fn nested(
                __field: &str,
                __value: ::exos::serde_json::Value,
                __outwards: bool,
            ) -> ::exos::serde_json::Value {
                match __field {
                    #(#nests)*
                    _ => __value,
                }
            }
        }

        impl ::exos::IntoPayload<#name> for #handle {
            fn payload(&self) -> ::std::string::String {
                let __fields: ::std::vec::Vec<::std::string::String> = ::std::vec![
                    #(
                        ::std::format!("{}: {}", ::exos::quote_js(#keys), #sent),
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

/// The row model of a `Rows<T>` field, if that is what this is.
///
/// Matched on the name rather than resolved, which is what a macro can do: a
/// type aliased to something else called `Rows` would be taken for one, and a
/// `Rows` aliased to another name would not. Both are visible at the
/// declaration, which is where somebody reading this is standing.
fn rows_of(ty: &syn::Type) -> Option<syn::Type> {
    let syn::Type::Path(path) = ty else {
        return None;
    };

    let segment = path.path.segments.last()?;

    if segment.ident != "Rows" {
        return None;
    }

    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };

    match arguments.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner.clone()),
        _ => None,
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
        assert!(expanded.contains("picked : :: exos :: Bound"));
        assert!(expanded.contains("fail : :: exos :: Bound"));
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

    /// A handler answers with `Effect::set`, which the client applies against
    /// the document, so a field declared into an element's scope would be a
    /// different signal of the same name.
    #[test]
    fn a_field_is_declared_on_the_document_rather_than_on_an_element() {
        let expanded = expand_ok("struct Draft { sku: String }");

        assert!(expanded.contains(":: exos :: Placement :: Document"));
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

    /// The whole form's validity is the record read, not the rules folded: a
    /// fold could only reach the fields the document declares, which is every
    /// field except the rows and every verdict except the server's.
    #[test]
    fn a_handle_answers_for_the_whole_model() {
        let expanded = expand_ok("struct Draft { sku: String }");
        let state = signal_name(&format_ident!("Draft"), &format_ident!("__state"));

        assert!(expanded.contains("fn valid"), "{expanded}");
        assert!(
            expanded.contains(&format!("all_valid (\"{state}\")")),
            "{expanded}"
        );
        assert!(
            expanded.contains(&format!("any_dirty (\"{state}\")")),
            "{expanded}"
        );
        assert!(
            expanded.contains(&format!("model_refusal (\"{state}\")")),
            "{expanded}"
        );
    }

    /// A gate names a sibling, so a typo is a message at the attribute rather
    /// than a missing field in an expansion nobody wrote.
    #[test]
    fn a_gate_naming_no_field_is_refused() {
        let expanded = expand_ok(
            "struct Draft { invoice: bool, #[valid(required_with = invioce)] vat: String }",
        );

        assert!(expanded.contains("compile_error"));
    }

    /// And when it names one, both halves read it: the server through the
    /// sibling field, the browser through that field's signal.
    #[test]
    fn a_gate_reaches_both_halves() {
        let expanded = expand_ok(
            "struct Draft { invoice: bool, #[valid(required_with = invoice)] vat: String }",
        );
        let armed = signal_name(&format_ident!("Draft"), &format_ident!("invoice"));

        assert!(
            expanded.contains("is_present (& self . invoice)"),
            "{expanded}"
        );
        assert!(
            expanded.contains(&format!(r#"raw ("$.{armed}")"#)),
            "{expanded}"
        );
    }

    /// A rule the server alone can answer reaches three places from one
    /// declaration: the route's entry, the submit that runs the same function,
    /// and the control that has to know there is a round trip to make.
    #[test]
    fn a_checked_field_is_registered_awaited_and_carried() {
        let expanded = expand_ok("struct Draft { #[valid(checked_by = coupon)] code: String }");
        let state = signal_name(&format_ident!("Draft"), &format_ident!("__state"));
        let key = signal_name(&format_ident!("Draft"), &format_ident!("code"));

        assert!(
            expanded.contains(&format!(r#"CheckEntry :: new ("{state}" , "{key}""#)),
            "{expanded}"
        );
        assert!(
            expanded.contains("coupon (:: core :: clone :: Clone"),
            "{expanded}"
        );
        assert!(
            expanded.contains(&format!(r#"checking ("{state}")"#)),
            "{expanded}"
        );
    }

    /// And a field without one carries nothing, so the control makes no
    /// request it has no rule for.
    #[test]
    fn an_unchecked_field_carries_no_model_to_ask() {
        let expanded = expand_ok("struct Draft { sku: String }");

        assert!(expanded.contains(r#"checking ("")"#), "{expanded}");
        assert!(!expanded.contains("CheckEntry"), "{expanded}");
    }

    /// A rule that names something other than a function is refused where it
    /// is written rather than inside an expansion nobody wrote.
    #[test]
    fn a_check_that_names_no_function_is_refused() {
        let expanded =
            expand_ok(r#"struct Draft { #[valid(checked_by = "coupon")] code: String }"#);

        assert!(expanded.contains("compile_error"));
    }

    /// Other serde attributes are none of this macro's business.
    #[test]
    fn an_unrelated_serde_attribute_is_left_alone() {
        let expanded = expand_ok(r#"#[serde(default)] struct Draft { sku: String }"#);

        assert!(!expanded.contains("compile_error"));
    }
}
