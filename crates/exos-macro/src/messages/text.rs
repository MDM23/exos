//! What one arm's string is made of.

use core::mem;

use proc_macro2::{Span, TokenStream};
use quote::quote_spanned;
use syn::{Ident, LitStr};

/// One arm's text, split at its placeholders.
pub(crate) struct Template {
    parts: Vec<Part>,
    /// The literal this was written as, so that what a parameter has to be for
    /// the sentence to hold is reported at the sentence.
    span: Span,
}

/// A run of text as it was written, or a parameter to put in.
enum Part {
    Text(String),
    Value(Ident),
}

impl Template {
    /// Splits a message string into text and the parameters it interpolates.
    ///
    /// `{count}` names a parameter, and `{{` and `}}` are the braces
    /// themselves. Nothing else is syntax: the string is text, and it is
    /// written out escaped wherever it lands.
    pub(crate) fn parse(literal: &LitStr) -> syn::Result<Self> {
        let source = literal.value();
        let span = literal.span();

        let mut characters = source.chars().peekable();
        let mut parts = Vec::new();
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

                    let name = syn::parse_str::<Ident>(&name).map_err(|_| {
                        syn::Error::new(
                            span,
                            format!(
                                "`{{{name}}}` is not a parameter name, and a message takes no \
                                     formatting of its own"
                            ),
                        )
                    })?;

                    if !text.is_empty() {
                        parts.push(Part::Text(mem::take(&mut text)));
                    }

                    parts.push(Part::Value(Ident::new(&name.to_string(), span)));
                }
                _ => text.push(character),
            }
        }

        if !text.is_empty() {
            parts.push(Part::Text(text));
        }

        Ok(Self { parts, span })
    }

    /// The parameters this text puts in, in the order they first appear.
    ///
    /// A parameter may appear anywhere in a translation, as often as it likes
    /// or not at all, since word order is the translation's business.
    pub(crate) fn interpolated(&self) -> Vec<&Ident> {
        let mut used: Vec<&Ident> = Vec::new();

        for part in &self.parts {
            if let Part::Value(name) = part
                && !used.contains(&name)
            {
                used.push(name);
            }
        }

        used
    }

    /// The one expression that builds this text.
    ///
    /// A placeholder becomes the parameter of that name, which the generated
    /// function has in scope, so what `format!` is handed reads the way the
    /// message was written. It carries the literal's span, so that a parameter
    /// which cannot be written into a sentence is reported at the sentence.
    pub(crate) fn emit(&self) -> TokenStream {
        if self.interpolated().is_empty() {
            let text: String = self
                .parts
                .iter()
                .map(|part| match part {
                    Part::Text(text) => text.as_str(),
                    Part::Value(_) => "",
                })
                .collect();

            return quote_spanned! { self.span => ::std::string::String::from(#text) };
        }

        let mut format = String::new();

        for part in &self.parts {
            match part {
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
            }
        }

        quote_spanned! { self.span => ::std::format!(#format) }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    fn template(text: &str) -> syn::Result<Template> {
        Template::parse(&LitStr::new(text, proc_macro2::Span::call_site()))
    }

    fn emitted(text: &str) -> String {
        template(text)
            .expect("a message this test wrote")
            .emit()
            .to_string()
    }

    #[test]
    fn a_string_with_nothing_in_it_is_the_string() {
        assert!(emitted("Clear selection").contains(r#"String :: from ("Clear selection")"#));
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
}
