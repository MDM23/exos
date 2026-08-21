//! Where the pages come from.
//!
//! A release build embeds every markdown file in the binary, so what gets
//! deployed is one file with nothing beside it. A debug build reads them off
//! disk instead, which makes editing a page and reloading the browser the
//! whole cycle. That split is `rust-embed`'s default and is the same bargain
//! [`asset!`](exos::asset) strikes with minification.

use rust_embed::Embed;

/// The markdown, however this build holds it.
#[derive(Embed)]
#[folder = "content/"]
#[include = "*.md"]
struct Pages;

/// The file the sidebar is written in.
pub(crate) const NAVIGATION: &str = "navigation";

/// The page a visit to the site starts on.
pub(crate) const ENTRY: &str = "installation";

/// The markdown of one page.
///
/// `None` covers both a slug nothing is filed under and a slug that could not
/// name a file in the first place. A debug build resolves these against the
/// filesystem, so a slug that is not checked is a directory traversal.
pub(crate) fn page(slug: &str) -> Option<String> {
    if !is_slug(slug) {
        return None;
    }

    let file = Pages::get(&format!("{slug}.md"))?;
    String::from_utf8(file.data.into_owned()).ok()
}

/// Every page, unordered, including the navigation.
///
/// Nothing serving a request needs this: a page is found by the slug that was
/// asked for. It is here so that a test can hold the directory and the sidebar
/// against each other.
#[cfg(test)]
pub(crate) fn slugs() -> impl Iterator<Item = String> {
    Pages::iter().filter_map(|file| file.strip_suffix(".md").map(str::to_owned))
}

/// Whether a slug could name a page at all.
fn is_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_found_by_its_slug() {
        assert!(page("routes").is_some_and(|source| source.starts_with("# Routes")));
    }

    #[test]
    fn nothing_outside_the_content_directory_is_reachable() {
        for slug in ["../../../etc/passwd", "..", "routes/../../Cargo", "Routes"] {
            assert!(page(slug).is_none(), "{slug}");
        }
    }

    #[test]
    fn the_entry_page_and_the_navigation_both_exist() {
        assert!(page(ENTRY).is_some());
        assert!(page(NAVIGATION).is_some());
    }

    /// The guide was one document, so every cross-reference in it was an
    /// anchor. Splitting it turned each one into a link to another page, and a
    /// link to a page that is not there is a 404 somebody has to click to
    /// find.
    #[test]
    fn every_link_between_pages_points_at_a_page_that_is_there() {
        for slug in slugs() {
            let source = page(&slug).expect("the page that was just listed");

            for event in pulldown_cmark::Parser::new(&source) {
                let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) =
                    event
                else {
                    continue;
                };

                let Some(target) = crate::markdown::target(&dest_url) else {
                    continue;
                };

                assert!(page(target).is_some(), "{slug}.md links to {dest_url}");
            }
        }
    }
}
