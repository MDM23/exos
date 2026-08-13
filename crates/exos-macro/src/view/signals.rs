//! Inferring signal declarations from the expressions that use them.
//!
//! A signal reference lives inside a JavaScript string, so nothing about it
//! reaches Rust's type system. The string is a *literal*, though, and the
//! macro holds the whole subtree at expansion time, so the names can be read
//! out and declared automatically.
//!
//! Every inferred name starts as `null`, which is all a name in a raw string
//! can ask for: there is no Rust value to take a starting value from.
//!
//! This is the path for names something other than Rust owns, the sortable
//! plugin's `_order` being the one in the tree. Handles bypass it entirely.
//! They are Rust values, so the compiler already checks them, their names are
//! generated rather than written, and there is no string to read.

use proc_macro2::{TokenStream, TokenTree};
use rstml::node::{Node, NodeAttribute, NodeElement};

// -----------------------------------------------------------------------------
//                                  COLLECTION
// -----------------------------------------------------------------------------

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
            // `data-sortable="post('/reorder', { order: $._order })"`
            Some(syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(text),
                ..
            })) => scan(&text.value(), found),

            // `data-on-click={ format!("$._order = null; {url}") }`. The
            // literal part of a format string is still a literal, and this is
            // how a hand-written handler tends to be built.
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

        // `a.$.b` reads a property named `$` off something else, so `$` has to
        // start the path for this to be a signal read. A preceding dot matters
        // as much as a preceding word character.
        let preceded = index.checked_sub(1).is_some_and(|before| {
            let character = characters[before];
            character.is_alphanumeric() || character == '_' || character == '.'
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

// -----------------------------------------------------------------------------
//                                   EMISSION
// -----------------------------------------------------------------------------

/// `{"a":null,"b":null}`, escaped for an attribute.
pub(super) fn static_declaration(names: &[String]) -> String {
    let body = names
        .iter()
        .map(|name| format!("&quot;{name}&quot;:null"))
        .collect::<Vec<_>>()
        .join(",");

    format!(" {DECLARATION}=\"{{{body}}}\"")
}

/// Rejects a value on `data-signals`.
///
/// The attribute is a bare marker: it says this element is a scope, which an
/// element without an `id` cannot otherwise say. Starting values come from
/// handles, so a value here is a mistake worth naming rather than ignoring.
pub(super) fn reject_value(value: Option<&syn::Expr>, out: &mut TokenStream) {
    let Some(expression) = value else {
        return;
    };

    out.extend(
        syn::Error::new_spanned(
            expression,
            "data-signals takes no value; declare a signal by putting its \
             handle in an attribute block, as in `{&gone}`",
        )
        .to_compile_error(),
    );
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

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
        assert_eq!(names("!$._order"), vec!["_order"]);
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
}
