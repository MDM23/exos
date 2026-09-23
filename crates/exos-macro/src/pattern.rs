//! Expansion of the `pattern!` macro.
//!
//! A pattern is the one shape rule whose two halves are written in different
//! languages, so this is where the subset that makes them agree is enforced.
//! What is written is JavaScript's dialect, because that is the one the
//! browser cannot be talked out of, and [`lower`] writes the Rust spelling of
//! the same thing while refusing everything the two would read differently.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, Ident, LitStr, Token, Visibility,
    parse::{Parse, ParseStream},
};

/// What JavaScript's `.` means, spelled for Rust.
///
/// Rust's excludes `\n` alone, so a pattern written against the browser's
/// would quietly accept a carriage return on the server. Lowered rather than
/// refused, because a dot is what somebody writes without thinking about
/// either engine.
const DOT: &str = "[^\\n\\r\\x{2028}\\x{2029}]";

/// One `pattern!` declaration.
struct Declaration {
    attrs: Vec<Attribute>,
    visibility: Visibility,
    name: Ident,
    source: LitStr,
}

impl Parse for Declaration {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let visibility = input.parse()?;
        let name = input.parse()?;
        input.parse::<Token![=]>()?;
        let source = input.parse()?;

        Ok(Self {
            attrs,
            visibility,
            name,
            source,
        })
    }
}

/// Expands a declaration into the constant both halves read.
pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let Declaration {
        attrs,
        visibility,
        name,
        source,
    } = match syn::parse2(input) {
        Ok(declaration) => declaration,
        Err(error) => return error.to_compile_error(),
    };

    let written = source.value();

    let rust = match lower(&written) {
        Ok(rust) => anchored(&rust),
        Err(complaint) => return syn::Error::new_spanned(&source, complaint).to_compile_error(),
    };

    // Compiled here so that a pattern nothing understands is an error at the
    // line somebody wrote it on, and so the copy the server keeps can never
    // fail to compile once it is running.
    if let Err(error) = regex_lite::Regex::new(&rust) {
        return syn::Error::new_spanned(&source, error).to_compile_error();
    }

    let js = anchored(&written);
    let label = name.to_string();

    let docs = attrs.is_empty().then(|| {
        let summary = format!("The `{label}` pattern.");
        quote! { #[doc = #summary] }
    });

    quote! {
        #docs
        #(#attrs)*
        #visibility static #name: ::exos::Pattern =
            ::exos::Pattern::new(#label, #rust, #js);
    }
}

/// A validating pattern always means the whole value.
///
/// Both engines search rather than match, and HTML's own `pattern` attribute
/// settled this the same way, so the anchors are written here rather than
/// remembered at every declaration.
fn anchored(source: &str) -> String {
    format!("^(?:{source})$")
}

/// The Rust spelling of what the browser reads as written, or what the two
/// would have disagreed about.
///
/// Each refusal here is legal in one dialect and either illegal or something
/// else in the other, which is the one failure a pattern can have that no
/// compiler and no test of ours would find: a value one side accepted and the
/// other did not.
fn lower(source: &str) -> Result<String, String> {
    let mut lowered = String::with_capacity(source.len());
    let mut rest = source.chars().peekable();
    let mut class = false;

    while let Some(character) = rest.next() {
        match character {
            '\\' => match rest.next() {
                Some('A' | 'z' | 'Z') => {
                    return Err(String::from(
                        "a pattern is anchored at both ends already, so `\\A` and `\\z` say \
                         nothing here and mean nothing in the browser",
                    ));
                }
                Some(escaped) => {
                    lowered.push('\\');
                    lowered.push(escaped);
                }
                None => return Err(String::from("a pattern cannot end in a backslash")),
            },

            // A nested class is Rust's, and `[:alpha:]` is a POSIX class the
            // browser reads as the characters it is spelled with.
            '[' if class => {
                return Err(String::from(
                    "a character class cannot hold another one; write out the characters",
                ));
            }
            '[' => {
                class = true;
                lowered.push('[');
            }
            ']' if class => {
                class = false;
                lowered.push(']');
            }
            ']' => {
                return Err(String::from(
                    "a `]` that closes nothing is a literal to one engine and an error to the \
                     other; write `\\]`",
                ));
            }

            '.' if !class => lowered.push_str(DOT),

            // Lookaround, backreferences, named groups and inline flags all
            // arrive through this door, and each is a divergence rather than a
            // feature: a flag is ASCII on one side and Unicode on the other,
            // and the rest the browser spells its own way or not at all.
            '(' if !class && rest.peek() == Some(&'?') => {
                rest.next();

                if rest.next() != Some(':') {
                    return Err(String::from(
                        "a group is `(` or `(?:`; lookaround, flags and named groups are not part \
                         of the subset both engines agree on",
                    ));
                }

                lowered.push_str("(?:");
            }

            _ => lowered.push(character),
        }
    }

    Ok(lowered)
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole of the lowering: everything else is written once and read by
    /// both.
    #[test]
    fn a_dot_is_written_out_as_the_class_the_browser_reads_it_as() {
        assert_eq!(lower("a.b").expect("lowers"), format!("a{DOT}b"));
        assert_eq!(lower(r"a\.b").expect("lowers"), r"a\.b");
        assert_eq!(lower("[.]").expect("lowers"), "[.]");
        assert_eq!(lower(r"[\]].").expect("lowers"), format!(r"[\]]{DOT}"));
        assert_eq!(lower(r"(?:\d{3})?").expect("lowers"), r"(?:\d{3})?");
    }

    #[test]
    fn the_subset_is_what_the_two_engines_read_the_same_way() {
        assert!(lower("[0-9]{5}").is_ok());
        assert!(lower(r"[A-Z]{2}\d{2}[A-Z0-9]{1,30}").is_ok());

        assert!(lower("(?=x)").is_err());
        assert!(lower("(?i)abc").is_err());
        assert!(lower("(?<name>x)").is_err());
        assert!(lower("[[:alpha:]]").is_err());
        assert!(lower(r"\Aabc\z").is_err());
        assert!(lower("a]").is_err());
        assert!(lower(r"abc\").is_err());
    }

    /// A pattern names the whole value, which is what HTML's own attribute
    /// means and what nobody remembers to write the anchors for.
    #[test]
    fn a_pattern_is_anchored_at_both_ends() {
        assert_eq!(anchored("[0-9]{5}"), "^(?:[0-9]{5})$");
    }
}
