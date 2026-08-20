//! Expansion of `messages!`.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote, quote_spanned};
use syn::{
    Attribute, Ident, LitStr, Pat, Path, Token, Type, braced, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned as _,
    token,
};

mod text;

use crate::messages::text::Template;

/// Where `Locale` is looked for when the invocation does not say.
const CONVENTION: &str = "crate::Locale";

/// The module `locales!` puts one alias per locale in, so that an arm naming a
/// variant is enough to reach that language's categories.
const ALIASES: &str = "__locales";

/// Expands the block into a module of functions, one per message.
pub(crate) fn expand(input: TokenStream) -> TokenStream {
    match syn::parse2::<Declaration>(input).and_then(|declaration| declaration.emit()) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

/// The whole invocation.
struct Declaration {
    /// The application's locale set, by convention `crate::Locale`.
    locale: Path,
    messages: Vec<Message>,
}

/// One `items_selected(count: Plural) { … }` entry.
struct Message {
    /// Whatever documentation the message carried. The translations are added
    /// below it, so a message is documented by what it says.
    docs: Vec<Attribute>,
    name: Ident,
    parameters: Vec<Parameter>,
    arms: Vec<Arm>,
}

/// One `count: Plural` in a message's parameter list.
struct Parameter {
    name: Ident,
    kind: Kind,
}

/// What a parameter's values are.
enum Kind {
    /// A count, whose domain is the plural categories of whichever language is
    /// being rendered, and therefore a different type in every arm.
    Plural,
    /// A wrapper the call site supplies, which the words a translation puts
    /// between `{terms}` and `{/terms}` are handed to.
    Slot,
    /// Anything else, as the type was written. Boxed because a type dwarfs the
    /// variants beside it.
    Value(Box<Type>),
}

/// One `De { One } = "…"` line.
struct Arm {
    /// The locale, as a variant of the set rather than as a tag.
    locale: Ident,
    /// One pattern per parameter, or `None` where the arm was written without
    /// braces or with `..` and therefore covers every value.
    patterns: Option<Vec<Pat>>,
    text: LitStr,
}

impl Parse for Declaration {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(Token![in]) {
            input.parse::<Token![in]>()?;

            let locale = input.parse::<Path>()?;
            let body;
            braced!(body in input);

            return Ok(Self {
                locale,
                messages: Message::list(&body)?,
            });
        }

        Ok(Self {
            locale: syn::parse_str(CONVENTION)?,
            messages: Message::list(input)?,
        })
    }
}

impl Parse for Message {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let docs = input.call(Attribute::parse_outer)?;

        if let Some(attribute) = docs
            .iter()
            .find(|attribute| !attribute.path().is_ident("doc"))
        {
            return Err(syn::Error::new_spanned(
                attribute,
                "a message takes documentation and nothing else",
            ));
        }

        let name = input.parse::<Ident>()?;

        let parameters = if input.peek(token::Paren) {
            let declared;
            parenthesized!(declared in input);

            Punctuated::<Parameter, Token![,]>::parse_terminated(&declared)?
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };

        let body;
        braced!(body in input);

        Ok(Self {
            docs,
            name,
            parameters,
            arms: Punctuated::<Arm, Token![,]>::parse_terminated(&body)?
                .into_iter()
                .collect(),
        })
    }
}

impl Parse for Parameter {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse::<Ident>()?;
        input.parse::<Token![:]>()?;

        let declared = input.parse::<Type>()?;

        Ok(Self {
            name,
            kind: Kind::of(declared),
        })
    }
}

impl Parse for Arm {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let locale = input.parse::<Ident>()?;

        let patterns = if input.peek(token::Brace) {
            let content;
            braced!(content in input);

            if content.peek(Token![..]) {
                content.parse::<Token![..]>()?;
                None
            } else {
                Some(
                    Punctuated::<Pat, Token![,]>::parse_terminated_with(
                        &content,
                        Pat::parse_multi,
                    )?
                    .into_iter()
                    .collect(),
                )
            }
        } else {
            None
        };

        input.parse::<Token![=]>()?;

        Ok(Self {
            locale,
            patterns,
            text: input.parse::<LitStr>()?,
        })
    }
}

impl Kind {
    /// What a parameter declared as `declared` is.
    ///
    /// `Plural` and `Slot` are the two type names this macro reads rather than
    /// passes through. Neither stands for a type a message could name: the
    /// categories German has are not the categories Arabic has, and a slot is
    /// whatever closure the call site brings.
    fn of(declared: Type) -> Self {
        let named = |name| match &declared {
            Type::Path(path) => path.qself.is_none() && path.path.is_ident(name),
            _ => false,
        };

        if named("Plural") {
            return Self::Plural;
        }

        if named("Slot") {
            return Self::Slot;
        }

        Self::Value(Box::new(declared))
    }
}

impl Declaration {
    /// The module of functions, one per message.
    fn emit(&self) -> syn::Result<TokenStream> {
        let locale = &self.locale;
        let aliases = aliases(locale);

        let messages = self
            .messages
            .iter()
            .map(|message| message.emit(locale, &aliases))
            .collect::<syn::Result<Vec<_>>>()?;

        Ok(quote! {
            /// The messages declared beside this module, each in whatever
            /// language the request turned out to be in.
            ///
            /// Generated by `exos::messages!`. Every function here reads
            /// `exos::locale()`, so the rules that has are the rules these
            /// have: outside a request, or inside a live fragment, they panic.
            pub mod t {
                // The parameter types are written where the messages are, so
                // they are named from there.
                use super::*;

                #(#messages)*
            }
        })
    }
}

impl Message {
    /// Every message in the block, which are written one after another with
    /// nothing between them.
    fn list(input: ParseStream<'_>) -> syn::Result<Vec<Self>> {
        let mut messages = Vec::new();

        while !input.is_empty() {
            messages.push(input.parse::<Self>()?);
        }

        Ok(messages)
    }

    /// The function this message becomes.
    fn emit(&self, locale: &Path, aliases: &Path) -> syn::Result<TokenStream> {
        self.check_no_parameter_is_named_after_a_slot()?;
        self.check_every_arm_covers_the_parameters()?;

        let templates = self.templates()?;
        let branched = self.branched();

        self.check_nothing_branches_on_a_slot(&branched)?;
        self.check_every_slot_is_wrapped_around_something(&templates)?;
        self.check_every_parameter_is_read(&templates, &branched)?;

        // A sentence with a slot in it is markup, and one without is text that
        // whatever renders it escapes. It is the message rather than the arm
        // that decides, since a function has one return type and a language
        // that needs no emphasis is still the same message.
        let markup = templates
            .iter()
            .any(|template| !template.wrapped().is_empty());

        let answer = if markup {
            quote! { ::exos::Markup }
        } else {
            quote! { ::std::string::String }
        };

        let name = &self.name;
        let docs = self.documentation();
        let inputs = self.parameters.iter().map(Parameter::input);
        let assertions = self.assertions(&branched);
        let arms = self.locales(locale, aliases, &templates, &branched, markup)?;

        // The match carries the message's own span, so that a locale nothing
        // translated this into is reported at the message rather than at
        // whichever block it happens to sit in.
        let resolved = quote_spanned! { name.span() =>
            match ::exos::locale::<#locale>() {
                #(#arms)*
            }
        };

        Ok(quote! {
            #(#assertions)*

            #(#docs)*
            #[must_use]
            pub fn #name(#(#inputs),*) -> #answer {
                #resolved
            }
        })
    }

    /// Each arm's text, with every placeholder checked against the parameter
    /// list.
    fn templates(&self) -> syn::Result<Vec<Template>> {
        let slots: Vec<&Ident> = self
            .parameters
            .iter()
            .filter(|parameter| matches!(parameter.kind, Kind::Slot))
            .map(|parameter| &parameter.name)
            .collect();

        let mut templates = Vec::with_capacity(self.arms.len());

        for arm in &self.arms {
            let template = Template::parse(&arm.text, &slots)?;

            for name in template.interpolated() {
                if !self
                    .parameters
                    .iter()
                    .any(|parameter| parameter.name == *name)
                {
                    return Err(syn::Error::new(
                        arm.text.span(),
                        format!(
                            "`{{{name}}}` names no parameter of `{}`, which takes {}",
                            self.name,
                            self.declared()
                        ),
                    ));
                }
            }

            templates.push(template);
        }

        Ok(templates)
    }

    /// Refuses an arm that says a different number of things than the message
    /// takes.
    ///
    /// An arm is a pattern per parameter, in the order they were declared,
    /// which is what lets a bare `_` stand for the one it did not want to
    /// decide anything about.
    fn check_every_arm_covers_the_parameters(&self) -> syn::Result<()> {
        for arm in &self.arms {
            let Some(patterns) = &arm.patterns else {
                continue;
            };

            if patterns.len() != self.parameters.len() {
                return Err(syn::Error::new(
                    arm.locale.span(),
                    format!(
                        "`{}` takes {}, so an arm names {} of them or `..`, and this one names {}",
                        self.name,
                        self.declared(),
                        self.parameters.len(),
                        patterns.len(),
                    ),
                ));
            }
        }

        Ok(())
    }

    /// Which parameters some arm tells values of apart.
    ///
    /// A parameter every arm writes `_` for is only ever interpolated, so it
    /// stays out of the `match` and is free to be any type that renders.
    fn branched(&self) -> Vec<bool> {
        let mut branched = vec![false; self.parameters.len()];

        for arm in &self.arms {
            let Some(patterns) = &arm.patterns else {
                continue;
            };

            for (branched, pattern) in branched.iter_mut().zip(patterns) {
                *branched |= !matches!(pattern, Pat::Wild(_));
            }
        }

        branched
    }

    /// Refuses a parameter named after a slot every message already has.
    ///
    /// `{b}` and `{i}` are emphasis, and a parameter of that name would take
    /// them away from the one message that could most want them.
    fn check_no_parameter_is_named_after_a_slot(&self) -> syn::Result<()> {
        for parameter in &self.parameters {
            if text::builtin(&parameter.name).is_some() {
                return Err(syn::Error::new(
                    parameter.name.span(),
                    format!(
                        "`{}` is the built-in slot for emphasis, which every message has; give \
                         the parameter another name",
                        parameter.name
                    ),
                ));
            }
        }

        Ok(())
    }

    /// Refuses an arm that tries to tell values of a slot apart.
    ///
    /// A slot is a wrapper rather than a value. There is nothing to compare it
    /// against, and a language that wants different words inside the wrapper
    /// writes different words inside the wrapper.
    fn check_nothing_branches_on_a_slot(&self, branched: &[bool]) -> syn::Result<()> {
        for (parameter, branched) in self.parameters.iter().zip(branched) {
            if *branched && matches!(parameter.kind, Kind::Slot) {
                return Err(syn::Error::new(
                    parameter.name.span(),
                    format!(
                        "`{}` is a slot, which is a wrapper rather than a value, so no arm can \
                         tell one from another; write `_` for it",
                        parameter.name
                    ),
                ));
            }
        }

        Ok(())
    }

    /// Refuses a translation that drops a slot, or uses one twice.
    ///
    /// The call site supplies one wrapper per slot, so a translation that
    /// leaves the link out fails the build rather than shipping a sentence
    /// nobody can click, and one that opens it twice would need a second
    /// wrapper to be supplied.
    fn check_every_slot_is_wrapped_around_something(
        &self,
        templates: &[Template],
    ) -> syn::Result<()> {
        let slots = self
            .parameters
            .iter()
            .filter(|parameter| matches!(parameter.kind, Kind::Slot));

        for parameter in slots {
            for (arm, template) in self.arms.iter().zip(templates) {
                let wrapped = template
                    .wrapped()
                    .iter()
                    .filter(|name| **name == &parameter.name)
                    .count();

                let complaint = match wrapped {
                    1 => continue,
                    0 => format!(
                        "this translation says nothing between `{{{name}}}` and `{{/{name}}}`, \
                         and a slot is what keeps a sentence whole, so every language wraps \
                         something in it",
                        name = parameter.name
                    ),
                    used => format!(
                        "`{}` is opened {used} times here, and the call site supplies one \
                         wrapper, so a slot wraps one thing",
                        parameter.name
                    ),
                };

                return Err(syn::Error::new(arm.text.span(), complaint));
            }
        }

        Ok(())
    }

    /// Refuses a parameter nothing reads.
    ///
    /// Not a translation that leaves one out, which is ordinary: German says
    /// nothing about who a file is assigned to and English does. This is a
    /// parameter no arm branches on and no translation puts in, which is a
    /// call site handing over a value that cannot reach the text.
    fn check_every_parameter_is_read(
        &self,
        templates: &[Template],
        branched: &[bool],
    ) -> syn::Result<()> {
        for (index, parameter) in self.parameters.iter().enumerate() {
            let read = templates.iter().any(|template| {
                template.interpolated().contains(&&parameter.name)
                    || template.wrapped().contains(&&parameter.name)
            });

            if !branched[index] && !read {
                return Err(syn::Error::new(
                    parameter.name.span(),
                    format!(
                        "nothing reads `{}`: no arm branches on it and no translation puts it in \
                         or wraps anything in it",
                        parameter.name
                    ),
                ));
            }
        }

        Ok(())
    }

    /// One outer arm per locale, in the order the locales first appear.
    ///
    /// Arms of one locale are gathered into a single arm holding a `match` of
    /// their own, which is what lets a message be written as a flat list while
    /// each language decides its own categories.
    fn locales(
        &self,
        locale: &Path,
        aliases: &Path,
        templates: &[Template],
        branched: &[bool],
        markup: bool,
    ) -> syn::Result<Vec<TokenStream>> {
        let text = |template: &Template| {
            if markup {
                template.markup()
            } else {
                template.string()
            }
        };

        let mut grouped: Vec<(&Ident, Vec<usize>)> = Vec::new();

        for (index, arm) in self.arms.iter().enumerate() {
            match grouped
                .iter_mut()
                .find(|(variant, _)| *variant == &arm.locale)
            {
                Some((_, arms)) => arms.push(index),
                None => grouped.push((&arm.locale, vec![index])),
            }
        }

        let mut emitted = Vec::with_capacity(grouped.len());

        for (variant, arms) in grouped {
            let alone = arms.len() == 1 && self.arms[arms[0]].patterns.is_none();
            let subjects = self.subjects(variant, aliases, branched);

            if subjects.is_empty() || alone {
                if let Some(second) = arms.get(1) {
                    return Err(syn::Error::new(
                        self.arms[*second].locale.span(),
                        format!(
                            "`{variant}` is already answered by an earlier arm, and nothing here \
                             tells the two apart"
                        ),
                    ));
                }

                let text = text(&templates[arms[0]]);
                emitted.push(quote! { #locale::#variant => #text, });

                continue;
            }

            let subject = tuple(&subjects, variant.span());

            let inner = arms
                .iter()
                .map(|index| {
                    let patterns = self.patterns(*index, variant, aliases, branched)?;
                    let pattern = tuple(&patterns, variant.span());
                    let text = text(&templates[*index]);

                    Ok(quote_spanned! { variant.span() => #pattern => #text, })
                })
                .collect::<syn::Result<Vec<_>>>()?;

            // Spanned at the locale, so that a category this language has and
            // this message left out is reported where that language is.
            emitted.push(quote_spanned! { variant.span() =>
                #locale::#variant => match #subject {
                    #(#inner)*
                },
            });
        }

        Ok(emitted)
    }

    /// What one locale's inner `match` looks at, one term per branched
    /// parameter.
    fn subjects(&self, variant: &Ident, aliases: &Path, branched: &[bool]) -> Vec<TokenStream> {
        let aliases = at(aliases, variant.span());

        self.parameters
            .iter()
            .zip(branched)
            .filter(|(_, branched)| **branched)
            .map(|(parameter, _)| {
                let name = &parameter.name;

                match parameter.kind {
                    // The category this language puts the count in, which is
                    // that language's own type. Spanned at the locale, since
                    // what this is matched against is what a category missing
                    // from this language is reported against.
                    Kind::Plural => quote_spanned! { variant.span() =>
                        #aliases::#variant::category(::exos::Count::magnitude(#name))
                    },
                    // By reference, so that a parameter which is matched can
                    // still be interpolated, whatever it is. A slot never
                    // reaches this, having been refused as something to
                    // branch on above.
                    Kind::Slot | Kind::Value(_) => quote_spanned! { variant.span() => &#name },
                }
            })
            .collect()
    }

    /// One arm's patterns, in the same order as [`Message::subjects`].
    fn patterns(
        &self,
        index: usize,
        variant: &Ident,
        aliases: &Path,
        branched: &[bool],
    ) -> syn::Result<Vec<TokenStream>> {
        let arm = &self.arms[index];

        self.parameters
            .iter()
            .enumerate()
            .filter(|(index, _)| branched[*index])
            .map(|(index, parameter)| {
                let Some(patterns) = &arm.patterns else {
                    return Ok(quote! { _ });
                };

                let domain = match &parameter.kind {
                    Kind::Plural => quote! { #aliases::#variant::Plural },
                    Kind::Value(declared) => declared.to_token_stream(),
                    // A slot has no values to name, which is refused above, so
                    // there is nothing here to qualify a name with.
                    Kind::Slot => TokenStream::new(),
                };

                qualify(&patterns[index], &domain)
            })
            .collect()
    }

    /// That every type an arm tells values of apart has values to tell apart.
    ///
    /// A bound rather than a check inside this macro, so that getting it wrong
    /// is the ordinary trait error, at the type the message declared.
    fn assertions(&self, branched: &[bool]) -> Vec<TokenStream> {
        self.parameters
            .iter()
            .zip(branched)
            .filter_map(|(parameter, branched)| match &parameter.kind {
                Kind::Value(declared) if *branched => Some(quote_spanned! { declared.span() =>
                    const _: fn() = || {
                        fn enumerable<T: ::exos::Enumerable>() {}
                        enumerable::<#declared>();
                    };
                }),
                _ => None,
            })
            .collect()
    }

    /// What the message says, in every language it was written in.
    ///
    /// Whatever documentation the message carried comes first, so that a note
    /// about where a sentence is used stays the summary, and the translations
    /// follow it as a list.
    fn documentation(&self) -> Vec<TokenStream> {
        let mut docs: Vec<TokenStream> = Vec::new();

        if self.docs.is_empty() {
            let summary = format!("The `{}` message.", self.name);
            docs.push(quote! { #[doc = #summary] });
        } else {
            docs.extend(self.docs.iter().map(ToTokens::to_token_stream));
        }

        docs.push(quote! { #[doc = ""] });

        for arm in &self.arms {
            let patterns = arm.patterns.as_ref().map(|patterns| {
                let written = patterns
                    .iter()
                    .map(|pattern| pattern.to_token_stream().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                format!(" {{ {written} }}")
            });

            let line = format!(
                "- `{}{}`: `{}`",
                arm.locale,
                patterns.unwrap_or_default(),
                arm.text.value()
            );

            docs.push(quote! { #[doc = #line] });
        }

        docs
    }

    /// The parameter list, as a phrase an error message can end with.
    fn declared(&self) -> String {
        if self.parameters.is_empty() {
            return String::from("no parameters");
        }

        let names: Vec<String> = self
            .parameters
            .iter()
            .map(|parameter| format!("`{}`", parameter.name))
            .collect();

        names.join(", ")
    }
}

impl Parameter {
    /// How the parameter arrives at the generated function.
    fn input(&self) -> TokenStream {
        let name = &self.name;

        match &self.kind {
            Kind::Plural => quote! { #name: impl ::exos::Count },
            // The wrapper, which is handed the words the translation put
            // inside it and answers with them wrapped. Once, because that is
            // how often a slot is written.
            Kind::Slot => quote! {
                #name: impl ::core::ops::FnOnce(::exos::Markup) -> ::exos::Markup
            },
            Kind::Value(declared) => quote! { #name: #declared },
        }
    }
}

/// Where the aliases for `locale`'s languages are.
///
/// Beside the locale set itself, since that is where `locales!` expanded.
fn aliases(locale: &Path) -> Path {
    let mut path = locale.clone();

    path.segments.pop();
    path.segments.push(format_ident!("{ALIASES}").into());

    path
}

/// One term, or a tuple of them.
///
/// Carrying `span`, because what a `match` is reported at is the span from the
/// first token of what it looks at to the last, and a tuple built anywhere
/// else would report the whole block.
fn tuple(terms: &[TokenStream], span: Span) -> TokenStream {
    match terms {
        [only] => quote! { #only },
        terms => quote_spanned! { span => (#(#terms),*) },
    }
}

/// The same path, reported wherever `span` is.
///
/// `crate::Locale` is a convention rather than something anybody wrote, so the
/// path to a language's categories starts life with no span of its own, and a
/// `match` looking at one would be reported at the invocation instead of at
/// the locale.
fn at(path: &Path, span: Span) -> Path {
    let mut path = path.clone();

    for segment in &mut path.segments {
        segment.ident.set_span(span);
    }

    path
}

/// Reads a pattern as a value of the domain the parameter was declared with.
///
/// A bare name in an arm is a value rather than a binding, which is what makes
/// `De { One }` mean the category and not "call whatever this is `One`". The
/// wildcard stays a wildcard, an alternative qualifies each of its sides, and
/// anything already written out is left alone.
fn qualify(pattern: &Pat, domain: &TokenStream) -> syn::Result<TokenStream> {
    match pattern {
        Pat::Wild(_) => Ok(quote! { _ }),
        Pat::Or(alternatives) => {
            let cases = alternatives
                .cases
                .iter()
                .map(|case| qualify(case, domain))
                .collect::<syn::Result<Vec<_>>>()?;

            Ok(quote! { #(#cases)|* })
        }
        Pat::Ident(name) if name.subpat.is_none() && name.by_ref.is_none() => {
            let value = &name.ident;

            Ok(quote_spanned! { name.span() => #domain::#value })
        }
        pattern => Ok(pattern.to_token_stream()),
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    fn expand_ok(input: &str) -> String {
        expand(input.parse().expect("valid input")).to_string()
    }

    #[test]
    fn a_message_becomes_a_function_that_reads_the_locale() {
        let expanded = expand_ok(r#"clear { De = "Aufheben", En = "Clear", }"#);

        assert!(expanded.contains("pub mod t"));
        assert!(expanded.contains("pub fn clear ()"));
        assert!(expanded.contains(":: exos :: locale :: < crate :: Locale > ()"));
        assert!(expanded.contains(
            r#"crate :: Locale :: De => :: std :: string :: String :: from ("Aufheben")"#
        ));
    }

    /// The inner match is over the language's own categories, reached through
    /// the variant the arm named rather than through a tag this macro cannot
    /// see.
    #[test]
    fn a_count_branches_on_the_category_of_the_language_being_rendered() {
        let expanded = expand_ok(
            r#"items(count: Plural) {
                De { One } = "{count} Element",
                De { _ } = "{count} Elemente",
            }"#,
        );

        assert!(
            expanded.contains(
                "crate :: __locales :: De :: category (:: exos :: Count :: magnitude (count))"
            ),
            "{expanded}"
        );
        assert!(expanded.contains("crate :: __locales :: De :: Plural :: One =>"));
    }

    /// The whole of what a call site says about a count is that it is a whole
    /// number, so `len()` and a literal both go in without a cast.
    #[test]
    fn a_count_arrives_as_any_whole_number() {
        let expanded = expand_ok(r#"items(count: Plural) { En { _ } = "{count}", }"#);

        assert!(expanded.contains("count : impl :: exos :: Count"));
    }

    #[test]
    fn a_parameter_that_is_only_interpolated_stays_out_of_the_match() {
        let expanded = expand_ok(
            r#"greeting(name: &str, count: Plural) {
                En { _, One } = "{name}, one message",
                En { _, _ } = "{name}, {count} messages",
            }"#,
        );

        assert!(expanded.contains("name : & str"));
        assert!(!expanded.contains("& name"), "{expanded}");
    }

    /// Both parameters are told apart, so both are looked at, and an arm that
    /// says nothing about one says `_` for it.
    #[test]
    fn several_branched_parameters_are_matched_as_a_tuple() {
        let expanded = expand_ok(
            r#"assigned(to: Assignee, count: Plural) {
                En { Me, One } = "one file for you",
                En { _, _ } = "files",
            }"#,
        );

        assert!(expanded.contains("match (& to , crate :: __locales :: En :: category"));
        assert!(
            expanded.contains("(Assignee :: Me , crate :: __locales :: En :: Plural :: One) =>")
        );
    }

    /// Enumerating a domain is what stage three needs to project one, and
    /// carrying it as a bound makes a domain that cannot be enumerated an
    /// ordinary trait error rather than a rule inside this macro.
    #[test]
    fn a_type_that_is_branched_on_has_to_be_enumerable() {
        let expanded =
            expand_ok(r#"assigned(to: Assignee) { En { Me } = "you", En { _ } = "x", }"#);

        assert!(expanded.contains("enumerable :: < Assignee > ()"));
    }

    #[test]
    fn a_type_that_is_only_interpolated_is_asked_for_nothing() {
        let expanded = expand_ok(r#"hello(name: &str) { En = "Hello {name}", }"#);

        assert!(!expanded.contains("enumerable"));
    }

    /// An arm with nothing to decide is the text, without a match around it.
    #[test]
    fn a_locale_that_says_one_thing_says_it_without_looking() {
        let expanded = expand_ok(
            r#"assigned(count: Plural) {
                De { .. } = "…",
                En { One } = "one",
                En { _ } = "many",
            }"#,
        );

        assert!(
            expanded
                .contains(r#"crate :: Locale :: De => :: std :: string :: String :: from ("…")"#)
        );
    }

    #[test]
    fn a_message_can_be_written_against_a_locale_set_somewhere_else() {
        let expanded = expand_ok(r#"in shop::Locale { clear { En = "Clear", } }"#);

        assert!(expanded.contains(":: exos :: locale :: < shop :: Locale > ()"));
        assert!(expanded.contains("shop :: Locale :: En"));
    }

    #[test]
    fn the_translations_are_the_documentation() {
        let expanded = expand_ok(r#"clear { De = "Aufheben", En = "Clear", }"#);

        assert!(
            expanded.contains(r#"doc = "- `De`: `Aufheben`""#),
            "{expanded}"
        );
    }

    #[test]
    fn a_placeholder_naming_no_parameter_is_refused() {
        let expanded = expand_ok(r#"items(count: Plural) { En { _ } = "{total} items", }"#);

        assert!(expanded.contains("compile_error"));
        assert!(expanded.contains("names no parameter"));
    }

    #[test]
    fn an_arm_that_does_not_name_every_parameter_is_refused() {
        let expanded =
            expand_ok(r#"assigned(to: Assignee, count: Plural) { En { One } = "{count}", }"#);

        assert!(expanded.contains("compile_error"));
    }

    /// A value a call site is asked for and no language can reach is a
    /// parameter somebody meant to write into a sentence.
    #[test]
    fn a_parameter_nothing_reads_is_refused() {
        let expanded =
            expand_ok(r#"items(count: Plural, total: u32) { En { _, _ } = "{count}", }"#);

        assert!(expanded.contains("compile_error"));
        assert!(expanded.contains("nothing reads `total`"));
    }

    #[test]
    fn a_second_arm_for_a_locale_with_nothing_to_decide_is_refused() {
        let expanded = expand_ok(r#"clear { En = "Clear", En = "Again", }"#);

        assert!(expanded.contains("compile_error"));
        assert!(expanded.contains("already answered"));
    }

    // ---- slots --------------------------------------------------------------

    /// A wrapper in and markup out, which is what lets the href, the classes
    /// and the routing stay in Rust while the words stay in the sentence.
    #[test]
    fn a_slot_arrives_as_a_wrapper_and_the_message_answers_with_markup() {
        let expanded = expand_ok(
            r#"accept(terms: Slot) {
                En = "Please accept the {terms}terms{/terms}.",
            }"#,
        );

        assert!(
            expanded.contains(
                "terms : impl :: core :: ops :: FnOnce (:: exos :: Markup) -> :: exos :: Markup"
            ),
            "{expanded}"
        );
        assert!(expanded.contains("-> :: exos :: Markup"), "{expanded}");
    }

    /// One return type per function, so a language that needs no emphasis
    /// answers with markup all the same.
    #[test]
    fn a_slot_in_one_language_makes_every_language_markup() {
        let expanded = expand_ok(r#"greeting { De = "Hallo", En = "{b}Hello{/b}", }"#);

        assert!(expanded.contains("-> :: exos :: Markup"));
        assert!(
            expanded.contains(
                r#"crate :: Locale :: De => :: exos :: Markup (:: std :: string :: String :: from ("Hallo"))"#
            ),
            "{expanded}"
        );
    }

    #[test]
    fn a_message_without_a_slot_is_still_a_string() {
        let expanded = expand_ok(r#"clear { En = "Clear", }"#);

        assert!(expanded.contains("-> :: std :: string :: String"));
        assert!(!expanded.contains("Markup"));
    }

    /// A translation that drops the link ships a sentence nobody can click, so
    /// it fails the build instead.
    #[test]
    fn a_translation_that_leaves_a_slot_out_is_refused() {
        let expanded = expand_ok(
            r#"accept(terms: Slot) {
                De = "Bitte akzeptieren.",
                En = "Please accept the {terms}terms{/terms}.",
            }"#,
        );

        assert!(expanded.contains("compile_error"));
        assert!(expanded.contains("says nothing between"), "{expanded}");
    }

    #[test]
    fn a_translation_that_opens_a_slot_twice_is_refused() {
        let expanded = expand_ok(
            r#"accept(terms: Slot) {
                En = "{terms}these{/terms} and {terms}those{/terms}",
            }"#,
        );

        assert!(expanded.contains("compile_error"));
        assert!(expanded.contains("opened 2 times"), "{expanded}");
    }

    #[test]
    fn nothing_branches_on_a_slot() {
        let expanded = expand_ok(
            r#"accept(terms: Slot) {
                En { Something } = "{terms}terms{/terms}",
            }"#,
        );

        assert!(expanded.contains("compile_error"));
        assert!(
            expanded.contains("wrapper rather than a value"),
            "{expanded}"
        );
    }

    /// Emphasis is available in every message, so a parameter cannot take the
    /// name away from one.
    #[test]
    fn a_parameter_named_after_a_built_in_slot_is_refused() {
        let expanded = expand_ok(r#"row(b: u32) { En = "{b}", }"#);

        assert!(expanded.contains("compile_error"));
        assert!(expanded.contains("built-in slot"), "{expanded}");
    }
}
