//! What one arm's string is made of.

use core::mem;

use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{Ident, LitStr};

use crate::escape::escape_attribute;

/// One arm's text, split at its placeholders and its slots.
pub(crate) struct Template {
    parts: Vec<Part>,
    /// The literal this was written as, so that what a parameter has to be for
    /// the sentence to hold is reported at the sentence.
    span: Span,
}

/// A run of text as it was written, a parameter to put in, or words wrapped in
/// something the call site supplies.
enum Part {
    Text(String),
    Value(Ident),
    Slot(Slot),
}

/// `{terms}…{/terms}`, and what it holds.
struct Slot {
    name: Ident,
    parts: Vec<Part>,
}

/// The slots every message has, whatever it declares.
///
/// Emphasis falls on different words in different languages and there is
/// nothing for a call site to decide about it, so these two are the mechanism
/// with the wrapper already written. The list is short on purpose: every entry
/// is a decision about semantics made on a translator's behalf.
pub(crate) fn builtin(name: &Ident) -> Option<(&'static str, &'static str)> {
    match name.to_string().as_str() {
        "b" => Some(("<strong>", "</strong>")),
        "i" => Some(("<em>", "</em>")),
        _ => None,
    }
}

impl Template {
    /// Splits a message string into text, the parameters it interpolates and
    /// the slots it wraps things in.
    ///
    /// `{count}` names a parameter and `{terms}…{/terms}` wraps words in one,
    /// which of the two a name means being decided by whether it was declared
    /// as a `Slot`. `{{` and `}}` are the braces themselves. Nothing else is
    /// syntax: the rest is text, and it is escaped rather than parsed.
    pub(crate) fn parse(literal: &LitStr, slots: &[&Ident]) -> syn::Result<Self> {
        let source = literal.value();
        let span = literal.span();

        let mut characters = source.chars().peekable();
        let mut open: Vec<(Ident, Vec<Part>)> = Vec::new();
        let mut parts: Vec<Part> = Vec::new();
        let mut text = String::new();

        while let Some(character) = characters.next() {
            match character {
                '{' | '}' if characters.peek() == Some(&character) => {
                    characters.next();
                    text.push(character);
                }
                '}' => {
                    return Err(syn::Error::new(
                        span,
                        "a `}` here closes a placeholder that was never opened; write `}}` for \
                         the brace itself",
                    ));
                }
                '{' => {
                    let mut name = String::new();

                    loop {
                        match characters.next() {
                            Some('}') => break,
                            Some(character) => name.push(character),
                            None => {
                                return Err(syn::Error::new(
                                    span,
                                    "a `{` here opens a placeholder that is never closed; write \
                                     `{{` for the brace itself",
                                ));
                            }
                        }
                    }

                    if !text.is_empty() {
                        parts.push(Part::Text(mem::take(&mut text)));
                    }

                    if let Some(closing) = name.strip_prefix('/') {
                        let closing = named(closing, span)?;

                        let Some((opened, outer)) = open.pop() else {
                            return Err(syn::Error::new(
                                span,
                                format!(
                                    "`{{/{closing}}}` closes a slot that nothing opened; only a \
                                     parameter declared as `Slot`, and the built-in `{{b}}` and \
                                     `{{i}}`, open one"
                                ),
                            ));
                        };

                        if opened != closing {
                            return Err(syn::Error::new(
                                span,
                                format!(
                                    "`{{/{closing}}}` closes `{opened}`, which is the slot open \
                                     here; slots nest rather than overlap"
                                ),
                            ));
                        }

                        let inner = mem::replace(&mut parts, outer);
                        parts.push(Part::Slot(Slot {
                            name: opened,
                            parts: inner,
                        }));

                        continue;
                    }

                    let name = named(&name, span)?;

                    if slots.contains(&&name) || builtin(&name).is_some() {
                        open.push((name, mem::take(&mut parts)));
                        continue;
                    }

                    parts.push(Part::Value(name));
                }
                _ => text.push(character),
            }
        }

        if !text.is_empty() {
            parts.push(Part::Text(text));
        }

        if let Some((opened, _)) = open.last() {
            return Err(syn::Error::new(
                span,
                format!(
                    "`{{{opened}}}` opens a slot that is never closed; write `{{/{opened}}}` \
                     where the words it wraps end"
                ),
            ));
        }

        Ok(Self { parts, span })
    }

    /// The parameters this text puts in, in the order they first appear.
    ///
    /// A parameter may appear anywhere in a translation, as often as it likes
    /// or not at all, since word order is the translation's business.
    pub(crate) fn interpolated(&self) -> Vec<&Ident> {
        let mut used: Vec<&Ident> = Vec::new();

        walk(&self.parts, &mut |part| {
            if let Part::Value(name) = part
                && !used.contains(&name)
            {
                used.push(name);
            }
        });

        used
    }

    /// Every slot this text opens, in order, built-in ones included.
    ///
    /// One per occurrence rather than one per name, because using a slot twice
    /// is a thing to be refused rather than a thing to be counted once.
    pub(crate) fn wrapped(&self) -> Vec<&Ident> {
        let mut used: Vec<&Ident> = Vec::new();

        walk(&self.parts, &mut |part| {
            if let Part::Slot(slot) = part {
                used.push(&slot.name);
            }
        });

        used
    }

    /// The one expression that builds this text as a `String`.
    ///
    /// A placeholder becomes the parameter of that name, which the generated
    /// function has in scope, so what `format!` is handed reads the way the
    /// message was written. It carries the literal's span, so that a parameter
    /// which cannot be written into a sentence is reported at the sentence.
    pub(crate) fn string(&self) -> TokenStream {
        if self.interpolated().is_empty() {
            let text = written(&self.parts);

            return quote_spanned! { self.span => ::std::string::String::from(#text) };
        }

        let mut format = String::new();

        flatten(&self.parts, &mut |part| match part {
            Part::Text(text) => {
                for character in text.chars() {
                    match character {
                        '{' => format.push_str("{{"),
                        '}' => format.push_str("}}"),
                        _ => format.push(character),
                    }
                }
            }
            Part::Value(name) => {
                format.push('{');
                format.push_str(&name.to_string());
                format.push('}');
            }
            Part::Slot(_) => {}
        });

        quote_spanned! { self.span => ::std::format!(#format) }
    }

    /// The one expression that builds this text as `Markup`.
    ///
    /// What a message with a slot in it answers with. The words are escaped
    /// here, while this crate compiles, and an interpolated value is escaped
    /// where it is written, so the only structure the markup can carry is a
    /// slot that was declared in Rust.
    pub(crate) fn markup(&self) -> TokenStream {
        markup(&self.parts, self.span)
    }
}

/// Hands every part to `visit`, slots included, outermost first.
fn walk<'parts>(parts: &'parts [Part], visit: &mut impl FnMut(&'parts Part)) {
    for part in parts {
        visit(part);

        if let Part::Slot(slot) = part {
            walk(&slot.parts, visit);
        }
    }
}

/// Hands every part that carries text or a value to `visit`, in reading order.
fn flatten<'parts>(parts: &'parts [Part], visit: &mut impl FnMut(&'parts Part)) {
    for part in parts {
        match part {
            Part::Slot(slot) => flatten(&slot.parts, visit),
            part => visit(part),
        }
    }
}

/// The words of a sentence that has nothing to put in, as they were written.
///
/// Unescaped, because a message without a slot answers with a `String` and is
/// escaped by whatever renders it, exactly as any other string is. Escaping
/// here as well would show a reader `&amp;amp;`.
fn written(parts: &[Part]) -> String {
    let mut text = String::new();

    flatten(parts, &mut |part| {
        if let Part::Text(written) = part {
            text.push_str(written);
        }
    });

    text
}

/// Builds these parts into a `Markup`.
fn markup(parts: &[Part], span: Span) -> TokenStream {
    // Nothing to put in, so nothing to build: the sentence is its own escaped
    // text, and a `String` nothing pushes to would only warn about being `mut`.
    if parts.iter().all(|part| matches!(part, Part::Text(_))) {
        let text = escape_attribute(&written(parts));

        return quote_spanned! { span => ::exos::Markup(::std::string::String::from(#text)) };
    }

    let written = writes(parts, span);

    quote_spanned! { span =>
        {
            let mut __text = ::std::string::String::new();
            #(#written)*
            ::exos::Markup(__text)
        }
    }
}

/// What one level of a sentence writes into the string being built.
fn writes(parts: &[Part], span: Span) -> Vec<TokenStream> {
    parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => {
                let escaped = escape_attribute(text);

                quote! { __text.push_str(#escaped); }
            }
            // Escaped as it is written, and asked for nothing but a `Display`,
            // so that adding emphasis to a sentence cannot change what its
            // parameters have to be.
            Part::Value(name) => quote! { ::exos::escape_display_into(&#name, &mut __text); },
            Part::Slot(slot) => {
                let inner = writes(&slot.parts, span);

                match builtin(&slot.name) {
                    // A tag this macro wrote, so there is nothing to call and
                    // nothing to escape.
                    Some((open, close)) => quote! {
                        __text.push_str(#open);
                        #(#inner)*
                        __text.push_str(#close);
                    },
                    // The call site's wrapper, handed the words already
                    // escaped and trusted with what it hands back, which is
                    // the same trust `Markup` is everywhere else.
                    None => {
                        let name = &slot.name;
                        let inner = markup(&slot.parts, span);

                        quote_spanned! { span =>
                            ::exos::Render::render_to(&#name(#inner), &mut __text);
                        }
                    }
                }
            }
        })
        .collect()
}

/// Reads a placeholder's name, which is all a placeholder can hold.
fn named(name: &str, span: Span) -> syn::Result<Ident> {
    let parsed = syn::parse_str::<Ident>(name).map_err(|_| {
        syn::Error::new(
            span,
            format!(
                "`{{{name}}}` is not a parameter name, and a message takes no formatting of its \
                 own"
            ),
        )
    })?;

    Ok(Ident::new(&parsed.to_string(), span))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;
    use quote::format_ident;

    fn template(text: &str) -> syn::Result<Template> {
        with(text, &[])
    }

    fn with(text: &str, slots: &[&str]) -> syn::Result<Template> {
        let declared: Vec<Ident> = slots.iter().map(|name| format_ident!("{name}")).collect();

        Template::parse(
            &LitStr::new(text, Span::call_site()),
            &declared.iter().collect::<Vec<_>>(),
        )
    }

    fn emitted(text: &str) -> String {
        template(text)
            .expect("a message this test wrote")
            .string()
            .to_string()
    }

    fn rendered(text: &str, slots: &[&str]) -> String {
        with(text, slots)
            .expect("a message this test wrote")
            .markup()
            .to_string()
    }

    /// What this text was refused with, and the empty string where it was not
    /// refused at all, which no assertion below is looking for.
    fn refusal(text: &str, slots: &[&str]) -> String {
        match with(text, slots) {
            Ok(_) => String::new(),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn a_string_with_nothing_in_it_is_the_string() {
        assert!(emitted("Clear selection").contains(r#"String :: from ("Clear selection")"#));
    }

    /// A message with no slot in it is a `String`, which is escaped by
    /// whatever renders it. Escaping here too would show a reader the escape.
    #[test]
    fn a_string_is_left_as_it_was_written() {
        assert!(emitted("Save & close").contains(r#"String :: from ("Save & close")"#));
        assert!(emitted("{count} < 10").contains(r#"format ! ("{count} < 10")"#));
    }

    #[test]
    fn a_placeholder_becomes_the_parameter_of_that_name() {
        assert!(emitted("{count} items").contains(r#"format ! ("{count} items")"#));
    }

    /// Word order is the translation's business, so a parameter appears where
    /// the language puts it, as often as it likes.
    #[test]
    fn a_parameter_is_counted_once_however_often_it_appears() {
        let template = template("{count} of {count}").expect("a message this test wrote");
        let used = template.interpolated();

        assert_eq!(used.len(), 1);
        assert_eq!(used[0], "count");
    }

    #[test]
    fn a_doubled_brace_is_the_brace_itself() {
        assert!(emitted("{{literal}}").contains(r#"String :: from ("{literal}")"#));
        assert!(emitted("{{{count}}}").contains(r#"format ! ("{{{count}}}")"#));
    }

    #[test]
    fn a_brace_that_opens_nothing_or_closes_nothing_is_refused() {
        assert!(template("{count").is_err());
        assert!(template("count}").is_err());
    }

    /// A message is text rather than a format string, so the one thing a
    /// placeholder can hold is a name.
    #[test]
    fn a_placeholder_that_is_not_a_name_is_refused() {
        assert!(template("{}").is_err());
        assert!(template("{count:>4}").is_err());
        assert!(template("{count.len()}").is_err());
    }

    // ---- slots --------------------------------------------------------------

    /// The words stay in the sentence and the wrapper is called with them,
    /// already escaped.
    #[test]
    fn a_slot_hands_its_words_to_the_call_sites_wrapper() {
        let expanded = rendered("Please accept the {terms}terms{/terms}.", &["terms"]);

        assert!(
            expanded.contains(r#"push_str ("Please accept the ")"#),
            "{expanded}"
        );
        assert!(
            expanded.contains(
                r#"terms (:: exos :: Markup (:: std :: string :: String :: from ("terms")))"#
            ),
            "{expanded}"
        );
    }

    /// Emphasis is the same mechanism with the wrapper already written, so
    /// nothing is declared and nothing is called.
    #[test]
    fn a_built_in_slot_writes_its_own_tags() {
        let expanded = rendered("You have {b}{count} unread{/b}", &[]);

        assert!(expanded.contains(r#"push_str ("<strong>")"#), "{expanded}");
        assert!(expanded.contains("escape_display_into (& count , & mut __text)"));
        assert!(expanded.contains(r#"push_str ("</strong>")"#));
    }

    /// The words of a message are escaped while this crate compiles, so a
    /// translation cannot introduce an element by being edited.
    #[test]
    fn the_words_around_a_slot_are_escaped_here() {
        let expanded = rendered("a < b {b}&{/b}", &[]);

        assert!(expanded.contains(r#"push_str ("a &lt; b ")"#), "{expanded}");
        assert!(expanded.contains(r#"push_str ("&amp;")"#), "{expanded}");
    }

    /// Which of the two a name means is decided by the declaration, so a
    /// parameter that is not a slot is put in rather than wrapped.
    #[test]
    fn a_name_that_was_not_declared_a_slot_is_a_placeholder() {
        let template = with("{count} items", &["terms"]).expect("a message this test wrote");

        assert!(template.wrapped().is_empty());
        assert_eq!(template.interpolated().len(), 1);
    }

    #[test]
    fn a_slot_that_is_never_closed_is_refused() {
        assert!(refusal("Please accept the {terms}terms", &["terms"]).contains("never closed"));
    }

    #[test]
    fn a_slot_that_closes_nothing_is_refused() {
        assert!(refusal("terms{/terms}", &[]).contains("nothing opened"));
    }

    #[test]
    fn slots_that_overlap_rather_than_nest_are_refused() {
        assert!(with("{terms}a{b}b{/terms}c{/b}", &["terms"]).is_err());
    }

    #[test]
    fn a_slot_holds_a_slot() {
        let template =
            with("{terms}the {b}terms{/b}{/terms}", &["terms"]).expect("a message this test wrote");

        assert_eq!(template.wrapped().len(), 2);
    }
}
