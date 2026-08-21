//! Markdown into markup.
//!
//! Three things happen here that plain `CommonMark` does not do. Headings get an
//! id derived from their text, so the outline beside a page and every
//! cross-reference in the guide have something to point at. Code is
//! highlighted on the server, which is why this site ships no highlighter and
//! keeps the promise the guide makes about bundlers. And a link to another
//! page is rewritten through [`exos::url`], so the site survives being mounted
//! under a prefix like anything else exos writes.

use std::sync::LazyLock;

use exos::Markup;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};
use syntect::{
    html::{ClassStyle, ClassedHTMLGenerator},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

/// One heading, for the outline beside a page.
#[derive(Debug)]
pub(crate) struct Heading {
    /// What a link to it points at.
    pub(crate) id: String,
    /// What it says.
    pub(crate) text: String,
}

/// A rendered page.
#[derive(Debug)]
pub(crate) struct Document {
    /// The `#` heading, which is also what the tab says.
    pub(crate) title: String,
    /// Everything, including that heading.
    pub(crate) body: Markup,
    /// Every `##` in it, in the order they are read.
    pub(crate) outline: Vec<Heading>,
}

/// Renders one page.
pub(crate) fn render(source: &str) -> Document {
    let mut events = Vec::new();
    let mut outline = Vec::new();
    let mut title = String::new();

    // What is being collected rather than emitted: a heading, whose text is
    // not known until it ends and is needed to build its own id, or a code
    // block, which is replaced wholesale by the highlighted version.
    let mut heading: Option<(HeadingLevel, Vec<Event<'_>>)> = None;
    let mut code: Option<(String, String)> = None;

    for event in Parser::new_ext(source, options()) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => heading = Some((level, Vec::new())),

            Event::End(TagEnd::Heading(_)) => {
                let Some((level, inner)) = heading.take() else {
                    continue;
                };

                let text = plain(&inner);
                let id = slug(&text);

                match level {
                    HeadingLevel::H1 => title = text,
                    HeadingLevel::H2 => outline.push(Heading {
                        id: id.clone(),
                        text,
                    }),
                    _ => {}
                }

                events.push(Event::Start(Tag::Heading {
                    level,
                    id: Some(id.clone().into()),
                    classes: Vec::new(),
                    attrs: Vec::new(),
                }));
                events.extend(inner);
                events.push(Event::Html(anchor(&id).into()));
                events.push(Event::End(TagEnd::Heading(level)));
            }

            Event::Start(Tag::CodeBlock(kind)) => code = Some((language(&kind), String::new())),

            Event::End(TagEnd::CodeBlock) => {
                let Some((language, source)) = code.take() else {
                    continue;
                };

                events.push(Event::Html(block(&language, &source).into()));
            }

            Event::Text(text) if code.is_some() => {
                if let Some((_, collected)) = code.as_mut() {
                    collected.push_str(&text);
                }
            }

            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest_url = internal(&dest_url).map_or(dest_url, Into::into);

                push(
                    &mut heading,
                    &mut events,
                    Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }),
                );
            }

            other => push(&mut heading, &mut events, other),
        }
    }

    let mut body = String::new();
    html::push_html(&mut body, events.into_iter());

    Document {
        title,
        body: Markup(body),
        outline,
    }
}

/// Everything a page says, with the markup taken back out.
///
/// What search reads. Block-level events become a space so that a heading and
/// the paragraph under it do not run into one word.
pub(crate) fn plain_text(source: &str) -> String {
    let mut out = String::new();

    for event in Parser::new_ext(source, options()) {
        match event {
            Event::Code(text) | Event::Text(text) => out.push_str(&text),
            Event::End(_) | Event::HardBreak | Event::SoftBreak => out.push(' '),
            _ => {}
        }
    }

    out
}

/// Which extensions the content is written against.
fn options() -> Options {
    Options::ENABLE_FOOTNOTES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES
}

/// Sends an event to whichever buffer is collecting.
fn push<'a>(
    heading: &mut Option<(HeadingLevel, Vec<Event<'a>>)>,
    events: &mut Vec<Event<'a>>,
    event: Event<'a>,
) {
    match heading {
        Some((_, inner)) => inner.push(event),
        None => events.push(event),
    }
}

/// The text of a heading, for its id and for the outline.
fn plain(events: &[Event<'_>]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Code(text) | Event::Text(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect()
}

/// What a heading is linked to by.
///
/// The same rule GitHub applies, because the guide's cross-references were
/// written against a document GitHub was rendering and have to keep working.
fn slug(text: &str) -> String {
    let mut out = String::new();

    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if (character.is_whitespace() || character == '-' || character == '_')
            && !out.ends_with('-')
        {
            out.push('-');
        }
    }

    out.trim_matches('-').to_owned()
}

/// The permalink that follows a heading.
fn anchor(id: &str) -> String {
    format!("<a class=\"anchor\" href=\"#{id}\" aria-label=\"Permalink\">#</a>")
}

/// Which language a fence declared, if it declared one.
fn language(kind: &CodeBlockKind<'_>) -> String {
    match kind {
        CodeBlockKind::Fenced(info) => info.split(',').next().unwrap_or_default().trim().to_owned(),
        CodeBlockKind::Indented => String::new(),
    }
}

/// One code block, highlighted where the language is one syntect knows.
fn block(language: &str, source: &str) -> String {
    let mut out = String::from("<pre class=\"code\" data-language=\"");
    exos::escape_into(language, &mut out);
    out.push_str("\"><code>");

    match highlight(language, source) {
        Some(highlighted) => out.push_str(&highlighted),
        None => exos::escape_into(source, &mut out),
    }

    out.push_str("</code></pre>");
    out
}

/// The class prefix highlighted tokens carry.
///
/// syntect writes one class per component of a scope, so `keyword.operator`
/// arrives as `t-keyword t-operator` and the stylesheet can colour the general
/// case without naming every specific one.
const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "t-" };

/// The syntaxes, parsed once.
static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Highlighted source, or `None` for a language nothing here can parse.
///
/// Classes rather than inline styles, so the two themes are ten rules in the
/// stylesheet rather than two generated sheets on every page.
fn highlight(language: &str, source: &str) -> Option<String> {
    let syntax = SYNTAXES.find_syntax_by_token(language)?;
    let mut generator = ClassedHTMLGenerator::new_with_class_style(syntax, &SYNTAXES, CLASS_STYLE);

    for line in LinesWithEndings::from(source) {
        generator
            .parse_html_for_line_which_includes_newline(line)
            .ok()?;
    }

    Some(generator.finalize())
}

/// The page a link points at, or `None` where it points somewhere else.
///
/// Anything carrying a scheme or a leading `#` is already pointing where it
/// means to, and so is anything with a path in it: a page is named by a slug,
/// and a slug has no slash. That last one is the rule worth stating, because
/// `../examples/todos` is a link the guide could reasonably hold and is not a
/// page here however hard it is squinted at.
pub(crate) fn target(dest: &str) -> Option<&str> {
    let slug = dest.split('#').next().unwrap_or_default();

    if slug.is_empty() || dest.contains(':') || dest.contains('/') {
        return None;
    }

    Some(slug)
}

/// The same link as a URL this application writes, and therefore prefixed.
fn internal(dest: &str) -> Option<String> {
    target(dest).map(|_| exos::url(format!("/docs/{dest}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_heading_is_the_title_and_is_still_rendered() {
        let document = render("# Routes\n\nSomething.\n");

        assert_eq!(document.title, "Routes");
        assert!(document.body.as_str().contains("<h1 id=\"routes\">Routes"));
    }

    #[test]
    fn the_outline_is_the_second_level_only() {
        let document = render("# A\n\n## One\n\n### Deeper\n\n## Two\n");

        let headings: Vec<&str> = document
            .outline
            .iter()
            .map(|heading| heading.text.as_str())
            .collect();

        assert_eq!(headings, ["One", "Two"]);
    }

    /// The guide's cross-references were written against GitHub's slugs, so
    /// these are the shapes that have to keep resolving.
    #[test]
    fn a_heading_is_linked_to_the_way_github_links_to_it() {
        assert_eq!(
            slug("Why `Page` is still its own type"),
            "why-page-is-still-its-own-type"
        );
        assert_eq!(
            slug("Slots: a sentence with a link in it"),
            "slots-a-sentence-with-a-link-in-it"
        );
        assert_eq!(slug("Hello, exos"), "hello-exos");
    }

    #[test]
    fn a_link_to_a_page_becomes_a_url_and_everything_else_is_left_alone() {
        assert_eq!(internal("models"), Some(String::from("/docs/models")));
        assert_eq!(
            internal("models#rules-on-a-model"),
            Some(String::from("/docs/models#rules-on-a-model"))
        );

        assert_eq!(internal("#per-request"), None);
        assert_eq!(internal("https://example.com"), None);
        assert_eq!(internal("/docs/models"), None);
        assert_eq!(internal("../examples/todos"), None);
    }

    #[test]
    fn rust_is_highlighted_and_an_unknown_language_is_escaped() {
        let rust = render("```rust\nlet x = 1;\n```\n");
        assert!(rust.body.as_str().contains("t-keyword"), "{:?}", rust.body);

        let other = render("```wat\n<script>\n```\n");
        assert!(other.body.as_str().contains("&lt;script&gt;"));
    }

    #[test]
    fn plain_text_keeps_the_words_and_drops_the_markup() {
        let text = plain_text("# Routes\n\nA [link](routes) and `code`.\n");

        assert!(text.contains("Routes"));
        assert!(text.contains("link"));
        assert!(!text.contains('['));
    }
}
