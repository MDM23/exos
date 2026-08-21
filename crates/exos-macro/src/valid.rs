//! The `#[valid(...)]` attribute a model's fields carry.
//!
//! Parsed by [`rules`], taken off the struct by [`strip`], and turned into the
//! server's half by [`check`]. The browser's half is generated beside it, from
//! the same list, which is the whole point: two spellings of one rule are what
//! this exists to prevent.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, ExprRange, FieldsNamed, Ident, ItemStruct, Meta, RangeLimits, Token, Type};

/// What one field is checked against.
pub(crate) struct Rules {
    /// The field it is about.
    pub(crate) field: Ident,
    /// Its type, which decides which impl answers each question.
    pub(crate) ty: Type,
    /// What was declared, in the order it was written.
    pub(crate) rules: Vec<Rule>,
}

/// One rule.
pub(crate) enum Rule {
    /// Filled in at all.
    Required,
    /// Between `least` and `most`, counted the way JavaScript counts.
    Length {
        /// The shortest this may be.
        least: usize,
        /// The longest.
        most: usize,
    },
    /// Shaped like an address.
    Email,
}

/// Reads every field's `#[valid(...)]`.
pub(crate) fn rules(fields: &FieldsNamed) -> syn::Result<Vec<Rules>> {
    let mut declared = Vec::new();

    for field in &fields.named {
        let Some(ident) = field.ident.clone() else {
            continue;
        };

        let mut rules = Vec::new();

        for attribute in &field.attrs {
            if !attribute.path().is_ident("valid") {
                continue;
            }

            for meta in attribute
                .parse_args_with(syn::punctuated::Punctuated::<Meta, Token![,]>::parse_terminated)?
            {
                rules.push(rule(&meta)?);
            }
        }

        if !rules.is_empty() {
            declared.push(Rules {
                field: ident,
                ty: field.ty.clone(),
                rules,
            });
        }
    }

    Ok(declared)
}

/// One entry of a `#[valid(...)]` list.
fn rule(meta: &Meta) -> syn::Result<Rule> {
    match meta {
        Meta::Path(path) if path.is_ident("required") => Ok(Rule::Required),
        Meta::Path(path) if path.is_ident("email") => Ok(Rule::Email),

        Meta::NameValue(pair) if pair.path.is_ident("length") => {
            let Expr::Range(range) = &pair.value else {
                return Err(syn::Error::new_spanned(
                    &pair.value,
                    "a length is a range, as in `length = 2..=40`",
                ));
            };

            let (least, most) = bounds(range)?;
            Ok(Rule::Length { least, most })
        }

        other => Err(syn::Error::new_spanned(
            other,
            "unknown rule; this macro knows `required`, `email` and `length = a..=b`",
        )),
    }
}

/// Both ends of a length range, each defaulting to no limit at all.
fn bounds(range: &ExprRange) -> syn::Result<(usize, usize)> {
    let least = match &range.start {
        Some(start) => literal(start)?,
        None => 0,
    };

    let most = match &range.end {
        Some(end) => {
            let end = literal(end)?;

            match range.limits {
                RangeLimits::Closed(_) => end,
                // An exclusive end is legal Rust and means something different
                // by one, which is exactly the kind of thing nobody notices in
                // a message. Refused rather than translated.
                RangeLimits::HalfOpen(_) => {
                    return Err(syn::Error::new_spanned(
                        range,
                        "a length range includes its end, so write `..=` rather than `..`",
                    ));
                }
            }
        }
        None => usize::MAX,
    };

    if least > most {
        return Err(syn::Error::new_spanned(
            range,
            "a length range runs upwards",
        ));
    }

    Ok((least, most))
}

/// One end of it.
fn literal(expr: &Expr) -> syn::Result<usize> {
    match expr {
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(int),
            ..
        }) => int.base10_parse(),
        other => Err(syn::Error::new_spanned(other, "a length bound is a number")),
    }
}

/// Takes every `#[valid(...)]` off the struct that is about to be emitted.
///
/// The attribute is this macro's, so leaving one behind is an error from rustc
/// about a tool it has never heard of, pointing at a line the author did write
/// and cannot fix.
pub(crate) fn strip(input: &mut ItemStruct) {
    if let syn::Fields::Named(fields) = &mut input.fields {
        for field in &mut fields.named {
            field.attrs.retain(|attr| !attr.path().is_ident("valid"));
        }
    }
}

/// The server's half: what runs inside `Validate::validate`.
pub(crate) fn check(declared: &[Rules], key: impl Fn(&Ident) -> String) -> TokenStream {
    let checks = declared.iter().map(|Rules { field, ty, rules }| {
        let key = key(field);
        let label = field.to_string();

        let questions = rules.iter().map(|rule| match rule {
            Rule::Required => quote! {
                if !::exos::Presence::is_present(&self.#field) {
                    __errors.add(#key, #label, ::exos::Violation::Required);
                }
            },

            // Shape rules skip an absent value, so an empty optional field is
            // silent and an empty required one says one thing rather than two.
            Rule::Length { least, most } => quote! {
                if ::exos::Presence::is_present(&self.#field) {
                    let __measured = ::exos::Length::measure(&self.#field);

                    if __measured < #least {
                        __errors.add(#key, #label, ::exos::Violation::TooShort { least: #least });
                    } else if __measured > #most {
                        __errors.add(#key, #label, ::exos::Violation::TooLong { most: #most });
                    }
                }
            },

            Rule::Email => quote! {
                if ::exos::Presence::is_present(&self.#field)
                    && !::exos::is_email(&self.#field)
                {
                    __errors.add(#key, #label, ::exos::Violation::Malformed);
                }
            },
        });

        // Named so that a rule on a field whose type cannot answer the
        // question points at the field rather than at the expansion.
        let _ = ty;

        quote! { #(#questions)* }
    });

    quote! { #(#checks)* }
}
