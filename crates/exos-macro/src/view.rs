//! Expansion of the `view!` macro.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;
use rstml::{
    Parser, ParserConfig,
    node::{Node, NodeAttribute, NodeElement, NodeName},
};

mod escape;
mod signals;

use self::escape::{collapse_whitespace, escape_attribute, escape_text};

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
        .raw_text_elements(RAW_TEXT.iter().copied().collect::<HashSet<_>>());

    let nodes = match Parser::new(config).parse_simple(input) {
        Ok(nodes) => nodes,
        Err(error) => return error.to_compile_error(),
    };

    let mut body = TokenStream::new();
    for node in &nodes {
        emit_node(node, &mut body, false);
    }

    quote! {{
        let mut __out = ::std::string::String::new();
        #body
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

        // Unquoted text between tags. Same treatment, but indentation in the
        // source should not become bytes on the wire.
        Node::RawText(text) => {
            let text = text.to_string_best();

            if raw {
                push_literal(&text, out);
                return;
            }

            let collapsed = collapse_whitespace(&text);
            if !collapsed.is_empty() {
                push_literal(&escape_text(&collapsed), out);
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

    let mut declared = false;

    for attribute in element.attributes() {
        if let NodeAttribute::Attribute(attribute) = attribute {
            if attribute.key.to_string() == signals::DECLARATION {
                declared = true;
                signals::emit_declaration(attribute.value(), &inferred, out);
            } else {
                emit_attribute(&attribute.key, attribute.value(), out);
            }
        }
    }

    // Nothing was declared by hand but the subtree uses signals, so the whole
    // declaration is static and goes straight into the markup.
    if !declared && !inferred.is_empty() {
        push_literal(&signals::static_declaration(&inferred), out);
    }

    emit_attribute_blocks(element, out);
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

/// `<li {gone} {show(..)} {class("busy", ..)}>`.
///
/// Blocks are collected and merged rather than pushed one at a time: the style
/// this encourages is to repeat them, so two `class` blocks must produce one
/// `class` attribute. Emitting duplicates would be silently wrong, because
/// browsers keep the first and drop the rest.
fn emit_attribute_blocks<C: rstml::node::CustomNode>(
    element: &NodeElement<C>,
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
        return;
    }

    out.extend(quote! {
        {
            let mut __attributes = ::exos::Attributes::new();
            #(::exos::IntoAttributes::write(#blocks, &mut __attributes);)*
            __out.push_str(&__attributes.render());
        }
    });
}

fn emit_attribute(key: &NodeName, value: Option<&syn::Expr>, out: &mut TokenStream) {
    let name = key.to_string();

    match value {
        // `class="files"`, written straight into the markup.
        Some(syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(text),
            ..
        })) => {
            let escaped = escape_attribute(&text.value());
            push_literal(&format!(" {name}=\"{escaped}\""), out);
        }

        // `class={expr}`. An `Option` that is `None` drops the attribute,
        // which is what `aria-current` and friends need.
        Some(expression) => {
            let open = format!(" {name}=\"");

            out.extend(quote! {
                if let Some(__value) = ::exos::AttributeValue::attribute_value(&(#expression)) {
                    __out.push_str(#open);
                    ::exos::Render::render_to(&__value, &mut __out);
                    __out.push('"');
                }
            });
        }

        // `<input disabled>`.
        None => push_literal(&format!(" {name}"), out),
    }
}
