//! Search, on the server.
//!
//! No index is shipped to the browser and nothing is fetched from anybody
//! else's service: the pages are already in the process, and reading twenty of
//! them takes less time than the round trip that carried the question.
//!
//! It is a page rather than a patch, which is worth a sentence because a
//! search box that answers as you type is the obvious thing to build here. Two
//! reasons it is not that. A result set is worth linking to, and a page has a
//! URL while a patch has nothing. And a patch per keystroke is a race: the
//! answer to `rou` can land after the answer to `routes`, and the last patch
//! to arrive is the one left on the screen. A navigation is still a fetch the
//! runtime morphs, so this costs the feel of it and nothing else.

use axum::extract::Query;
use exos::{Markup, Page, view};
use serde::Deserialize;

use crate::{content, markdown, nav, page};

/// What was asked.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Terms {
    /// The words, as the box in the masthead names them.
    #[serde(default)]
    q: String,
}

/// One page that matched.
#[derive(Debug)]
struct Hit {
    /// The page.
    slug: String,
    /// Its title.
    title: String,
    /// Where it matched, with the match marked.
    excerpt: Markup,
}

#[exos::get("/docs/search")]
async fn search(Query(terms): Query<Terms>) -> Page {
    let query = terms.q.trim();

    page::shell("", "Search", &[], results(query, &hits(query)))
}

/// The box in the masthead.
///
/// A plain form, so it works on a page whose JavaScript never arrived, and the
/// runtime morphs the document it navigates to like any other link.
pub(crate) fn field() -> Markup {
    view! {
        <form class="search" role="search" action={ exos::url("/docs/search") } method="get">
            <input
                type="search"
                name="q"
                aria-label="Search the documentation"
                placeholder="Search"
                autocomplete="off"
            >
        </form>
    }
}

/// What the search page shows.
fn results(query: &str, hits: &[Hit]) -> Markup {
    if query.is_empty() {
        return view! {
            <h1>"Search"</h1>
            <p>"Type something into the box above."</p>
        };
    }

    view! {
        <h1>{ format!("Search: {query}") }</h1>

        <p class="count">
            {
                match hits.len() {
                    0 => String::from("Nothing matched."),
                    1 => String::from("One page matched."),
                    count => format!("{count} pages matched."),
                }
            }
        </p>

        <ol class="hits">
            {
                hits.iter()
                    .map(|hit| view! {
                        <li>
                            <a href={ exos::url(format!("/docs/{}", hit.slug)) }>
                                { hit.title.as_str() }
                            </a>
                            <p>{ &hit.excerpt }</p>
                        </li>
                    })
                    .collect::<Vec<_>>()
            }
        </ol>
    }
}

/// Every page holding `query`, titles first and otherwise in reading order.
///
/// Reading order is the ranking. The guide was written to be read front to
/// back, so the earlier of two pages saying the same thing is the one that
/// says it first.
fn hits(query: &str) -> Vec<Hit> {
    if query.is_empty() {
        return Vec::new();
    }

    let needle = query.to_ascii_lowercase();
    let mut hits = Vec::new();

    for slug in nav::order() {
        let Some(source) = content::page(&slug) else {
            continue;
        };

        let document = markdown::render(&source);
        let text = markdown::plain_text(&source);

        let Some(at) = text.to_ascii_lowercase().find(&needle) else {
            continue;
        };

        let titled = document.title.to_ascii_lowercase().contains(&needle);

        hits.push((
            titled,
            Hit {
                slug,
                title: document.title,
                excerpt: excerpt(&text, at, needle.len()),
            },
        ));
    }

    hits.sort_by_key(|(titled, _)| !titled);
    hits.into_iter().map(|(_, hit)| hit).collect()
}

/// How much of a page is shown either side of a hit.
const CONTEXT: usize = 90;

/// The words either side of a hit, with the hit marked.
///
/// `at` and `length` are byte offsets into `text` rather than into its
/// lowercased copy, which is why the needle is lowercased with the ASCII
/// version: it is the one that cannot change a string's length.
fn excerpt(text: &str, at: usize, length: usize) -> Markup {
    let start = boundary(text, at.saturating_sub(CONTEXT), false);
    let end = boundary(text, (at + length + CONTEXT).min(text.len()), true);

    let mut out = String::new();

    if start > 0 {
        out.push('…');
    }

    exos::escape_into(text[start..at].trim_start(), &mut out);
    out.push_str("<mark>");
    exos::escape_into(&text[at..at + length], &mut out);
    out.push_str("</mark>");
    exos::escape_into(text[at + length..end].trim_end(), &mut out);

    if end < text.len() {
        out.push('…');
    }

    Markup(out)
}

/// The nearest index either side of `at` that a string can be cut at.
fn boundary(text: &str, mut at: usize, forwards: bool) -> usize {
    while !text.is_char_boundary(at) {
        if forwards {
            at += 1;
        } else {
            at -= 1;
        }
    }

    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::get;

    #[tokio::test]
    async fn a_query_finds_the_page_it_names_and_marks_what_matched() {
        let html = get("/docs/search?q=live+fragment").await;

        assert!(html.contains("href=\"/docs/live-fragments\""));
        assert!(html.contains("<mark>live fragment</mark>"), "{html:.400}");
    }

    #[tokio::test]
    async fn a_query_nothing_holds_says_so_rather_than_failing() {
        let html = get("/docs/search?q=kubernetes").await;

        assert!(html.contains("Nothing matched."));
    }

    #[tokio::test]
    async fn the_box_is_answered_without_a_query_too() {
        let html = get("/docs/search").await;

        assert!(html.contains("Type something into the box above."));
    }

    /// A title is what somebody is usually looking for, so it outranks a page
    /// that merely mentions the word.
    #[test]
    fn a_page_named_after_the_query_comes_first() {
        let hits = hits("effects");

        assert_eq!(hits.first().map(|hit| hit.slug.as_str()), Some("effects"));
        assert!(hits.len() > 1, "{hits:#?}");
    }

    #[test]
    fn an_excerpt_is_cut_on_character_boundaries() {
        let text = "Elemente ausgewählt, und zwar sofort und ohne Umstände zu machen";
        let at = text.find("sofort").expect("the needle");

        let excerpt = excerpt(text, at, "sofort".len());

        assert!(excerpt.as_str().contains("<mark>sofort</mark>"));
    }
}
