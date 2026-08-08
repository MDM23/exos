//! Inferring signal declarations from the expressions that use them.
//!
//! A signal reference lives inside a JavaScript string, so nothing about it
//! reaches Rust's type system. The string is a *literal*, though, and the
//! macro holds the whole subtree at expansion time, so the names can be read
//! out and declared automatically.
//!
//! Only signals that need no starting value are inferred, with `null` as the
//! default: `!$._gone` on an undeclared signal is already `true`. A signal
//! carrying a value from the server still says so, because that value has to
//! come from somewhere.
//!
//! Typed handles bypass this entirely. They are Rust values, so the compiler
//! already checks them and there is no string to read.

use proc_macro2::{TokenStream, TokenTree};
use quote::quote;
use rstml::node::{Node, NodeAttribute, NodeElement};

/// The attribute an element declares its signals with.
pub(super) const DECLARATION: &str = "data-signals";

/// Attributes whose value the client evaluates as an expression.
const EXPRESSION_ATTRIBUTES: &[&str] = &[
    "data-attr",
    "data-class",
    "data-prop",
    "data-show",
    "data-sortable",
    "data-text",
];

fn is_expression_attribute(name: &str) -> bool {
    EXPRESSION_ATTRIBUTES.contains(&name) || name.starts_with("data-on-")
}

/// An element owns a scope when it can be named, which is the same condition
/// the runtime uses: scoping keys on `id`.
fn opens_scope<C: rstml::node::CustomNode>(element: &NodeElement<C>) -> bool {
    element.attributes().iter().any(|attribute| {
        matches!(
            attribute,
            NodeAttribute::Attribute(attribute)
                if matches!(attribute.key.to_string().as_str(), "id" | DECLARATION)
        )
    })
}

/// Signal names referenced in this element's subtree that it does not declare.
///
/// Stops at nested scopes: a name used inside `<li id="row">` belongs to that
/// row, not to the list around it.
pub(super) fn inferred<C: rstml::node::CustomNode>(element: &NodeElement<C>) -> Vec<String> {
    if !opens_scope(element) {
        return Vec::new();
    }

    let mut found = Vec::new();
    collect_from_attributes(element, &mut found);

    for child in &element.children {
        collect(child, &mut found);
    }

    let declared = declared(element);
    found.retain(|name| !declared.contains(name));
    found
}

fn collect<C: rstml::node::CustomNode>(node: &Node<C>, found: &mut Vec<String>) {
    let Node::Element(element) = node else {
        return;
    };

    // A nested scope collects its own. Stopping here is what keeps a hundred
    // rows from hoisting their signals onto the list.
    if opens_scope(element) {
        return;
    }

    collect_from_attributes(element, found);

    for child in &element.children {
        collect(child, found);
    }
}

fn collect_from_attributes<C: rstml::node::CustomNode>(
    element: &NodeElement<C>,
    found: &mut Vec<String>,
) {
    for attribute in element.attributes() {
        let NodeAttribute::Attribute(attribute) = attribute else {
            continue;
        };

        if !is_expression_attribute(&attribute.key.to_string()) {
            continue;
        }

        match attribute.value() {
            // `data-show="!$._gone"`
            Some(syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(text),
                ..
            })) => scan(&text.value(), found),

            // `data-on-click={ format!("$._gone = true; {url}") }`. The
            // literal part of a format string is still a literal, and this is
            // how a per-row handler tends to be written.
            Some(syn::Expr::Macro(call)) => {
                if let Some(literal) = format_literal(call) {
                    scan(&literal, found);
                }
            }

            _ => {}
        }
    }
}

/// The leading string literal of `format!` or `concat!`, if there is one.
fn format_literal(call: &syn::ExprMacro) -> Option<String> {
    let name = call.mac.path.segments.last()?.ident.to_string();
    if !matches!(name.as_str(), "concat" | "format") {
        return None;
    }

    // Re-implementing `format_args!` is not the job; the leading literal is
    // all that is needed and it is the first token tree.
    let leading: TokenStream = call
        .mac
        .tokens
        .clone()
        .into_iter()
        .take_while(|token| !matches!(token, TokenTree::Punct(punct) if punct.as_char() == ','))
        .collect();

    syn::parse2::<syn::LitStr>(leading)
        .ok()
        .map(|literal| literal.value())
}

/// Pulls `name` out of every `$.name` in a JavaScript expression.
fn scan(source: &str, found: &mut Vec<String>) {
    let characters: Vec<char> = source.chars().collect();
    let mut index = 0;

    while index + 1 < characters.len() {
        if characters[index] != '$' || characters[index + 1] != '.' {
            index += 1;
            continue;
        }

        // `a.$.b` is not a signal read: `$` has to start the path.
        let preceded = index.checked_sub(1).is_some_and(|before| {
            characters[before].is_alphanumeric() || characters[before] == '_'
        });

        if preceded {
            index += 1;
            continue;
        }

        let start = index + 2;
        let mut end = start;
        while end < characters.len()
            && (characters[end].is_alphanumeric() || characters[end] == '_')
        {
            end += 1;
        }

        if end > start {
            let name: String = characters[start..end].iter().collect();
            if !found.contains(&name) {
                found.push(name);
            }
        }

        index = end.max(index + 1);
    }
}

/// Names the element declares itself, which are never inferred over.
fn declared<C: rstml::node::CustomNode>(element: &NodeElement<C>) -> Vec<String> {
    for attribute in element.attributes() {
        let NodeAttribute::Attribute(attribute) = attribute else {
            continue;
        };

        if attribute.key.to_string() != DECLARATION {
            continue;
        }

        // `signals! { fav: x, gone: false }`: read the keys back out.
        if let Some(syn::Expr::Macro(call)) = attribute.value()
            && call
                .mac
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "signals")
        {
            return keys(&call.mac.tokens);
        }
    }

    Vec::new()
}

/// The identifiers that appear in key position of a `signals!` invocation.
fn keys(tokens: &TokenStream) -> Vec<String> {
    let mut names = Vec::new();
    let mut expecting_name = true;

    for token in tokens.clone() {
        match &token {
            TokenTree::Ident(ident) if expecting_name => {
                names.push(ident.to_string());
                expecting_name = false;
            }
            TokenTree::Punct(punct) if punct.as_char() == ',' => expecting_name = true,
            _ => {}
        }
    }

    names
}

/// `{"a":null,"b":null}`, escaped for an attribute.
pub(super) fn static_declaration(names: &[String]) -> String {
    let body = names
        .iter()
        .map(|name| format!("&quot;{name}&quot;:null"))
        .collect::<Vec<_>>()
        .join(",");

    format!(" {DECLARATION}=\"{{{body}}}\"")
}

/// Emits `data-signals`, folding the inferred names in as defaults.
pub(super) fn emit_declaration(
    value: Option<&syn::Expr>,
    inferred: &[String],
    out: &mut TokenStream,
) {
    let Some(expression) = value else {
        // `<div data-signals>` is meaningless on its own, but it does mark a
        // scope, so honour the inferred set and nothing else.
        if !inferred.is_empty() {
            let literal = static_declaration(inferred);
            out.extend(quote! { __out.push_str(#literal); });
        }
        return;
    };

    out.extend(quote! {
        {
            let mut __signals = #expression;
            // Inferred names are defaults: an explicit starting value in the
            // declaration is left alone.
            __signals.default_null(&[#(#inferred),*]);
            __out.push_str(" data-signals=\"");
            ::exos::Render::render_to(&__signals, &mut __out);
            __out.push('"');
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<String> {
        let mut found = Vec::new();
        scan(source, &mut found);
        found
    }

    #[test]
    fn reads_signal_names_out_of_an_expression() {
        assert_eq!(names("!$._gone"), vec!["_gone"]);
        assert_eq!(names("$.a + $.b"), vec!["a", "b"]);
    }

    #[test]
    fn ignores_a_dollar_that_does_not_start_the_path() {
        assert!(names("other.$.nope").is_empty());
    }

    #[test]
    fn reports_each_name_once() {
        assert_eq!(names("$.x = !$.x"), vec!["x"]);
    }

    #[test]
    fn reads_the_keys_of_a_signals_invocation() {
        let tokens: TokenStream = "fav: entry.favorite, gone: false".parse().expect("valid");
        assert_eq!(keys(&tokens), vec!["fav", "gone"]);
    }
}
