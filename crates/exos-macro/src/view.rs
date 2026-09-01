//! Expansion of the `view!` macro.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;
use rstml::{
    Parser, ParserConfig,
    node::{KVAttributeValue, KeyedAttribute, Node, NodeAttribute, NodeElement},
};

mod signals;

use crate::escape::{escape_attribute, escape_text};

/// Elements that must not be given a closing tag.
///
/// HTML's own void list. Teaching the parser about it is what lets the input
/// be actual HTML rather than XML with HTML-looking tags.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// Elements whose content is character data, not markup.
const RAW_TEXT: &[&str] = &["script", "style"];

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let config = ParserConfig::new()
        .always_self_closed_elements(VOID.iter().copied().collect::<HashSet<_>>())
        .raw_text_elements(RAW_TEXT.iter().copied().collect::<HashSet<_>>())
        .recover_block(true);

    // A template being typed is a template that does not parse, which is most
    // of the time an editor asks what this expands to. So the errors are
    // collected beside the tree rather than instead of it, and every block
    // that did arrive is emitted with the spans it was written at.
    //
    // That is what rust-analyzer completes in. It expands the macro twice,
    // once as written and once with a marker spliced in at the caret, then
    // reads the offset the marker landed at in the second expansion back into
    // the first. So the two have to agree character for character up to that
    // point, which is why the errors go last: a `compile_error!` ahead of the
    // body shifts it, and the caret is read against whatever the shift lands
    // on rather than against what is being typed.
    let (nodes, diagnostics) = Parser::new(config).parse_recoverable(input).split_vec();

    let mut body = TokenStream::new();
    for node in &nodes {
        emit_node(node, &mut body, false);
    }

    let errors = diagnostics.into_iter().map(|error| {
        // Named through the value rather than the type: rstml does not
        // re-export it, and a dependency for one call is not worth it.
        error.emit_as_expr_tokens()
    });

    quote! {{
        let mut __out = ::std::string::String::new();
        #body
        #(#errors)*
        ::exos::Markup(__out)
    }}
}

/// Appends a string literal to the output buffer.
fn push_literal(text: &str, out: &mut TokenStream) {
    if text.is_empty() {
        return;
    }

    out.extend(quote! { __out.push_str(#text); });
}

/// `raw` is set inside `<script>` and `<style>`, whose content is character
/// data. Escaping there would corrupt it: `a && b` in JavaScript must not
/// become `a &amp;&amp; b`.
fn emit_node<C: rstml::node::CustomNode>(node: &Node<C>, out: &mut TokenStream, raw: bool) {
    match node {
        Node::Element(element) => emit_element(element, out),

        // A quoted string in the template is author-written text. It is still
        // escaped: the author wrote text, so a `<` in it is a less-than sign
        // and not the start of a tag.
        Node::Text(text) => {
            let value = text.value_string();
            push_literal(&if raw { value } else { escape_text(&value) }, out);
        }

        // Unquoted text between tags. Whitespace here is the template's own
        // indentation and never reaches the wire. Anything else is refused:
        // outside a raw-text element it has already been through Rust's
        // tokenizer, which comes back with the spacing rearranged, so
        // `50% off` would render as `50 % off` and `don't` would not compile
        // at all.
        Node::RawText(text) => {
            if raw {
                push_literal(&text.to_string_best(), out);
            } else if !text.is_empty() {
                out.extend(
                    syn::Error::new_spanned(
                        text,
                        "text in a view is written as a string literal: `<p>\"Hello\"</p>`",
                    )
                    .to_compile_error(),
                );
            }
        }

        Node::Block(block) => {
            let tokens = block
                .try_block()
                .map_or_else(|| quote! { #block }, |inner| quote! { #inner });

            out.extend(quote! { ::exos::Render::render_to(&(#tokens), &mut __out); });
        }

        Node::Doctype(_) => push_literal("<!DOCTYPE html>", out),

        Node::Fragment(fragment) => {
            for child in &fragment.children {
                emit_node(child, out, false);
            }
        }

        // Comments are notes to the author, not output. Custom nodes are never
        // produced, because none are configured on the parser.
        Node::Comment(_) | Node::Custom(_) => {}
    }
}

fn emit_element<C: rstml::node::CustomNode>(element: &NodeElement<C>, out: &mut TokenStream) {
    let name = element.open_tag.name.to_string();
    let inferred = signals::inferred(element);

    push_literal(&format!("<{name}"), out);

    for attribute in element.attributes() {
        if let NodeAttribute::Attribute(attribute) = attribute {
            if attribute.key.to_string() == signals::DECLARATION {
                // Consumed rather than written out. Bare, it only marks a
                // scope, and the declaration it implies is emitted below with
                // whatever the handles on this element declare.
                signals::reject_value(attribute.value(), out);
            } else {
                emit_attribute(attribute, out);
            }
        }
    }

    emit_attribute_blocks(element, &inferred, out);
    push_literal(">", out);

    if VOID.contains(&name.as_str()) {
        return;
    }

    let raw = RAW_TEXT.contains(&name.as_str());
    for child in &element.children {
        emit_node(child, out, raw);
    }

    push_literal(&format!("</{name}>"), out);
}

/// `<li {gone} {show(..)} {class("busy", ..)}>`, and the signals inferred
/// alongside them.
///
/// Blocks are collected and merged rather than pushed one at a time: the style
/// this encourages is to repeat them, so two `class` blocks must produce one
/// `class` attribute. Emitting duplicates would be silently wrong, because
/// browsers keep the first and drop the rest. Inferred names go through the
/// same merge for that reason: an element that both declares a handle and
/// mentions a name in a raw expression has one `data-signals` between them.
fn emit_attribute_blocks<C: rstml::node::CustomNode>(
    element: &NodeElement<C>,
    inferred: &[String],
    out: &mut TokenStream,
) {
    let blocks: Vec<TokenStream> = element
        .attributes()
        .iter()
        .filter_map(|attribute| match attribute {
            // Unwrapping the single expression out of the block keeps
            // `unused_braces` quiet in the calling crate.
            NodeAttribute::Block(block) => Some(match block.try_block() {
                Some(inner) => match inner.stmts.as_slice() {
                    [syn::Stmt::Expr(expression, None)] => quote! { #expression },
                    _ => quote! { #inner },
                },
                None => quote! { #block },
            }),
            NodeAttribute::Attribute(_) => None,
        })
        .collect();

    if blocks.is_empty() {
        // Nothing on this element is computed, so its declaration is a
        // constant and goes straight into the markup.
        if !inferred.is_empty() {
            push_literal(&signals::static_declaration(inferred), out);
        }

        return;
    }

    out.extend(quote! {
        {
            let mut __attributes = ::exos::Attributes::new();
            // Seeded first, so a handle declaring the same name wins. An
            // inferred name is a default; a handle is a statement.
            #(__attributes.signal(#inferred, ::exos::serde_json::Value::Null);)*
            #(::exos::IntoAttributes::write(#blocks, &mut __attributes);)*
            __out.push_str(&__attributes.render());
        }
    });
}

fn emit_attribute(attribute: &KeyedAttribute, out: &mut TokenStream) {
    let name = attribute.key.to_string();

    match attribute
        .possible_value
        .to_value()
        .map(|value| &value.value)
    {
        // `class="files"`, written straight into the markup.
        Some(KVAttributeValue::Expr(syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(text),
            ..
        }))) => {
            let escaped = escape_attribute(&text.value());
            push_literal(&format!(" {name}=\"{escaped}\""), out);
        }

        // `class={expr}`. An `Option` that is `None` drops the attribute,
        // which is what `aria-current` and friends need. A block that did not
        // parse is emitted as it was written, for the reason `expand` gives.
        Some(KVAttributeValue::Expr(expression)) => emit_attribute_value(&name, expression, out),
        Some(KVAttributeValue::InvalidBraced(block)) => emit_attribute_value(&name, block, out),

        // `<input disabled>`.
        None => push_literal(&format!(" {name}"), out),
    }
}

fn emit_attribute_value(name: &str, value: &impl quote::ToTokens, out: &mut TokenStream) {
    let open = format!(" {name}=\"");

    out.extend(quote! {
        if let Some(__value) = ::exos::AttributeValue::attribute_value(&(#value)) {
            __out.push_str(#open);
            ::exos::Render::render_to(&__value, &mut __out);
            __out.push('"');
        }
    });
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    fn expand_ok(template: &str) -> String {
        expand(template.parse().expect("valid template")).to_string()
    }

    /// The block is what an editor completes in, so it stays in the expansion
    /// even when it is half written and the macro is reporting an error about
    /// it.
    #[test]
    fn a_block_that_does_not_parse_keeps_its_tokens() {
        let expanded = expand_ok("<a href={ thing. }>{ locale:: }</a>");

        assert!(expanded.contains("compile_error"), "{expanded}");
        assert!(expanded.contains("thing ."), "{expanded}");
        assert!(expanded.contains("locale ::"), "{expanded}");
    }

    /// Nothing goes in front of the body, because an editor reads the caret's
    /// offset in one expansion of this against another and anything ahead of
    /// the body moves it. Completion opens on whatever the shift lands on
    /// rather than on what is being typed, which looks like it works.
    #[test]
    fn an_error_comes_after_the_body_it_is_about() {
        let expanded = expand_ok("<a>{ locale:: }</a>");
        let body = expanded.find("push_str").expect("a body");

        assert!(body < expanded.find("compile_error").expect("an error"));
    }

    #[test]
    fn bare_text_is_refused() {
        assert!(expand_ok("<p>Hello</p>").contains("compile_error"));
    }

    #[test]
    fn indentation_around_tags_is_not_text() {
        let expanded = expand_ok("<ul>\n    <li>\"a\"</li>\n</ul>");
        assert!(!expanded.contains("compile_error"), "{expanded}");
    }

    /// Content of a raw-text element is character data, so the rule does not
    /// reach into it: `<script>` has no string literals to write.
    #[test]
    fn script_content_stays_bare() {
        let expanded = expand_ok("<script>a && b</script>");
        assert!(!expanded.contains("compile_error"), "{expanded}");
    }
}
