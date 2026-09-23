//! The `#[valid(...)]` attribute a model's fields carry.
//!
//! Parsed by [`rules`], taken off the struct by [`strip`], and turned into the
//! server's half by [`check`]. The browser's half is generated beside it, from
//! the same list, which is the whole point: two spellings of one rule are what
//! this exists to prevent.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, ExprRange, FieldsNamed, Ident, ItemStruct, Meta, Path, RangeLimits, Token, Type};

/// What one field is checked against.
pub(crate) struct Rules {
    /// The field it is about.
    pub(crate) field: Ident,
    /// What that field is called on the wire, which is also the key its
    /// message is written under.
    pub(crate) key: String,
    /// Its type, which decides which impl answers each question.
    pub(crate) ty: Type,
    /// What was declared, in the order it was written.
    pub(crate) rules: Vec<Rule>,
}

/// One rule.
pub(crate) enum Rule {
    /// Filled in at all.
    Required,
    /// Filled in whenever a sibling field is.
    RequiredWith {
        /// The field that arms it.
        sibling: Ident,
    },
    /// Between `least` and `most`, counted the way JavaScript counts.
    Length {
        /// The shortest this may be.
        least: usize,
        /// The longest.
        most: usize,
    },
    /// Shaped like an address.
    Email,
    /// Shaped like the named pattern says.
    Matches {
        /// The pattern it is checked against.
        pattern: Path,
    },
    /// Whatever the named function says, which is the one rule with no browser
    /// half and therefore the one that costs a request.
    CheckedBy {
        /// The function that answers it.
        ask: Path,
    },
}

/// Reads every field's `#[valid(...)]`.
pub(crate) fn rules(
    fields: &FieldsNamed,
    key: impl Fn(&Ident) -> String,
) -> syn::Result<Vec<Rules>> {
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

        // Every field lands here, not only the ones carrying rules: a gate
        // names a sibling, and both halves of it are built from that
        // sibling's type and wire name.
        declared.push(Rules {
            key: key(&ident),
            field: ident,
            ty: field.ty.clone(),
            rules,
        });
    }

    gates(&declared)?;

    Ok(declared)
}

/// One entry of a `#[valid(...)]` list.
fn rule(meta: &Meta) -> syn::Result<Rule> {
    match meta {
        Meta::Path(path) if path.is_ident("required") => Ok(Rule::Required),
        Meta::Path(path) if path.is_ident("email") => Ok(Rule::Email),

        Meta::NameValue(pair) if pair.path.is_ident("required_with") => {
            let sibling = match &pair.value {
                Expr::Path(path) => path.path.get_ident().cloned(),
                _ => None,
            };

            sibling
                .map(|sibling| Rule::RequiredWith { sibling })
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        &pair.value,
                        "a gate names one field of this model, as in `required_with = invoice`",
                    )
                })
        }

        Meta::NameValue(pair) if pair.path.is_ident("matches") => match &pair.value {
            Expr::Path(pattern) => Ok(Rule::Matches {
                pattern: pattern.path.clone(),
            }),
            other => Err(syn::Error::new_spanned(
                other,
                "a shape names one pattern, as in `matches = POSTCODE`",
            )),
        },

        Meta::NameValue(pair) if pair.path.is_ident("checked_by") => match &pair.value {
            Expr::Path(ask) => Ok(Rule::CheckedBy {
                ask: ask.path.clone(),
            }),
            other => Err(syn::Error::new_spanned(
                other,
                "a server-only rule names one function, as in `checked_by = coupon`",
            )),
        },

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
            "unknown rule; this macro knows `required`, `required_with = other`, \
             `email`, `length = a..=b`, `matches = PATTERN` and `checked_by = function`",
        )),
    }
}

/// Refuses a gate naming something this model has no field for.
///
/// Checked here rather than left to the expansion, where a typo would come
/// back as a missing field on a struct the author cannot see.
fn gates(declared: &[Rules]) -> syn::Result<()> {
    for Rules { rules, .. } in declared {
        for rule in rules {
            let Rule::RequiredWith { sibling } = rule else {
                continue;
            };

            if !declared.iter().any(|other| other.field == *sibling) {
                return Err(syn::Error::new_spanned(
                    sibling,
                    format!("`{sibling}` is not a field of this model"),
                ));
            }
        }
    }

    Ok(())
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

/// The fields whose rules `field` arms, by wire name.
///
/// Read by the control the gate is on, so that editing it retires what was
/// said about the fields it gates: unticking the box takes the billing
/// complaints with it, rather than leaving them in the record where nothing
/// on screen can reach them.
pub(crate) fn arms(declared: &[Rules], field: &Ident) -> String {
    let armed = declared.iter().filter(|other| {
        other.rules.iter().any(|rule| match rule {
            Rule::RequiredWith { sibling } => sibling == field,
            _ => false,
        })
    });

    armed
        .map(|other| other.key.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The function that answers for `field`, where the server alone can.
fn ask_for<'a>(declared: &'a [Rules], field: &Ident) -> Option<&'a Path> {
    let rules = declared.iter().find(|rules| rules.field == *field)?;

    rules.rules.iter().find_map(|rule| match rule {
        Rule::CheckedBy { ask } => Some(ask),
        _ => None,
    })
}

/// Whether this field costs a round trip, for the binding that would make it.
pub(crate) fn checked(declared: &[Rules], field: &Ident) -> bool {
    ask_for(declared, field).is_some()
}

/// The route's half: one entry per checked field, resolved by the pair the
/// control carries.
///
/// A shim rather than the function itself, because what arrives off the wire is
/// a `Value` and every checked field has a type and a future of its own. The
/// deserializing and the presence guard belong to `exos::asked`, so the route
/// and the extractor cannot come to different conclusions about one value.
pub(crate) fn entries(declared: &[Rules], model: &str) -> TokenStream {
    let entries = declared.iter().filter_map(|Rules { field, key, .. }| {
        let ask = ask_for(declared, field)?;

        Some(quote! {
            ::exos::inventory::submit! {
                ::exos::CheckEntry::new(#model, #key, |__value| {
                    ::std::boxed::Box::pin(::exos::asked(__value, #ask))
                })
            }
        })
    });

    quote! { #(#entries)* }
}

/// The server's other half: the same functions, awaited at submit.
///
/// Written under `__prefix` like every other message, and guarded the way the
/// route's shim is: an absent value is not asked about, and neither is one the
/// shape rules have already refused.
pub(crate) fn checks(declared: &[Rules]) -> TokenStream {
    let checks = declared.iter().filter_map(|Rules { field, key, .. }| {
        let ask = ask_for(declared, field)?;

        Some(quote! {{
            let __key = ::std::format!("{}{}", __prefix, #key);

            if __errors.get(&__key).is_none() && ::exos::Presence::is_present(&self.#field) {
                // Cloned because the future outlives this call, which is the
                // same reason the function takes the value owned.
                if let ::core::result::Result::Err(__said) =
                    #ask(::core::clone::Clone::clone(&self.#field)).await
                {
                    __errors.said(__key, __said);
                }
            }
        }})
    });

    quote! { #(#checks)* }
}

/// The browser's half: the same rules, as one expression yielding a message.
///
/// Built from the list [`check`] reads, so the two questions are one
/// declaration. A field with no rules answers `None` and shows only whatever
/// the server said about it.
pub(crate) fn ask(declared: &[Rules], field: &Ident) -> TokenStream {
    let Some(Rules { ty, rules, .. }) = declared
        .iter()
        .find(|rules| rules.field == *field)
        .filter(|rules| !rules.rules.is_empty())
    else {
        return quote! { ::core::option::Option::None };
    };

    let label = field.to_string();

    let asked = rules.iter().flat_map(|rule| match rule {
        Rule::Required => vec![quote! {
            (
                !<#ty as ::exos::Presence>::present(__signal.get()),
                ::exos::complaint(#label, ::exos::Violation::Required),
            )
        }],

        // The same question with the sibling's presence in front of it, which
        // is the whole of a gate: a rule under a condition rather than a rule
        // that can see the model. The sibling is read as an expression rather
        // than through a handle, because `signals()` is still building the one
        // that would hand it over.
        Rule::RequiredWith { sibling } => declared
            .iter()
            .find(|gate| gate.field == *sibling)
            .map(|gate| {
                let armed = &gate.ty;
                let source = format!("$.{}", gate.key);

                quote! {
                    (
                        <#armed as ::exos::Presence>::present(::exos::Js::raw(#source))
                            .and(!<#ty as ::exos::Presence>::present(__signal.get())),
                        ::exos::complaint(#label, ::exos::Violation::Required),
                    )
                }
            })
            .into_iter()
            .collect(),

        // Guarded by presence exactly as the server's half is, so an empty
        // optional field is silent on both sides. Two entries rather than one,
        // because the two ends of a range do not say the same thing.
        Rule::Length { least, most } => vec![
            quote! {
                (
                    <#ty as ::exos::Presence>::present(__signal.get()).and(
                        <#ty as ::exos::Length>::length(__signal.get()).lt(#least as u32)
                    ),
                    ::exos::complaint(#label, ::exos::Violation::TooShort { least: #least }),
                )
            },
            quote! {
                (
                    <#ty as ::exos::Presence>::present(__signal.get()).and(
                        <#ty as ::exos::Length>::length(__signal.get()).gt(#most as u32)
                    ),
                    ::exos::complaint(#label, ::exos::Violation::TooLong { most: #most }),
                )
            },
        ],

        Rule::Email => vec![quote! {
            (
                <#ty as ::exos::Presence>::present(__signal.get())
                    .and(!::exos::email_js(&__signal.get())),
                ::exos::complaint(#label, ::exos::Violation::Malformed),
            )
        }],

        // The pattern is read here rather than baked in, so the browser's copy
        // and the server's are two readings of one declaration.
        Rule::Matches { pattern } => vec![quote! {
            (
                <#ty as ::exos::Presence>::present(__signal.get())
                    .and(!::exos::matches_js(&#pattern, &__signal.get())),
                ::exos::complaint(
                    #label,
                    ::exos::Violation::Unmatched { pattern: #pattern.name() },
                ),
            )
        }],

        // The one rule the browser cannot ask. What it does instead is send
        // the value, which the control does off its own binding.
        Rule::CheckedBy { .. } => Vec::new(),
    });

    quote! { ::exos::chain(::std::vec![#(#asked),*]) }
}

/// The server's half: what runs inside `Validate::validate_into`.
///
/// Every key is written under `__prefix`, which is empty for a model somebody
/// submits and names the row for a model that is one. That is the whole of
/// what makes a message about row three land where row three's control reads,
/// and it is one concat rather than a second generated body.
pub(crate) fn check(declared: &[Rules]) -> TokenStream {
    let checks = declared.iter().map(
        |Rules {
             field, key, rules, ..
         }| {
            let label = field.to_string();
            let key = quote! { ::std::format!("{}{}", __prefix, #key) };

            let questions = rules.iter().map(|rule| match rule {
            Rule::Required => quote! {
                if !::exos::Presence::is_present(&self.#field) {
                    __errors.add(#key, #label, ::exos::Violation::Required);
                }
            },

            Rule::RequiredWith { sibling } => quote! {
                if ::exos::Presence::is_present(&self.#sibling)
                    && !::exos::Presence::is_present(&self.#field)
                {
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

            Rule::Matches { pattern } => quote! {
                if ::exos::Presence::is_present(&self.#field)
                    && !#pattern.is_match(&self.#field)
                {
                    __errors.add(
                        #key,
                        #label,
                        ::exos::Violation::Unmatched { pattern: #pattern.name() },
                    );
                }
            },

            // Answered by [`checks`], which is awaited once these have run.
            Rule::CheckedBy { .. } => quote! {},
        });

            quote! { #(#questions)* }
        },
    );

    quote! { #(#checks)* }
}
