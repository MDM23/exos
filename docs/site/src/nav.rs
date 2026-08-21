//! The sidebar, which is a markdown file.
//!
//! `content/navigation.md` is an ordinary document: every `##` is a group and
//! every link under it is a page. So adding a page is editing a list rather
//! than a table in Rust, and the file still reads as documentation on its own.
//!
//! It is parsed per request rather than cached. It is two kilobytes, and a
//! cache would be the one thing in a debug build that did not pick up an edit.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::content;

/// One heading in the sidebar and the pages under it.
#[derive(Debug)]
pub(crate) struct Group {
    /// What the heading says.
    pub(crate) title: String,
    /// Its pages, in reading order.
    pub(crate) pages: Vec<Entry>,
}

/// One page in the sidebar.
#[derive(Debug)]
pub(crate) struct Entry {
    /// The page it points at.
    pub(crate) slug: String,
    /// What the link says, which need not be the page's own title.
    pub(crate) title: String,
}

/// The sidebar.
pub(crate) fn groups() -> Vec<Group> {
    let Some(source) = content::page(content::NAVIGATION) else {
        return Vec::new();
    };

    let mut groups: Vec<Group> = Vec::new();
    let mut link: Option<(String, String)> = None;

    for event in Parser::new(&source) {
        match event {
            Event::Start(Tag::Heading { .. }) => groups.push(Group {
                title: String::new(),
                pages: Vec::new(),
            }),

            Event::Start(Tag::Link { dest_url, .. }) => {
                link = Some((dest_url.into_string(), String::new()));
            }

            Event::End(TagEnd::Link) => {
                let (Some((slug, title)), Some(group)) = (link.take(), groups.last_mut()) else {
                    continue;
                };

                group.pages.push(Entry { slug, title });
            }

            Event::Code(text) | Event::Text(text) => match (&mut link, groups.last_mut()) {
                (Some((_, title)), _) => title.push_str(&text),
                (None, Some(group)) if group.pages.is_empty() => group.title.push_str(&text),
                _ => {}
            },

            _ => {}
        }
    }

    // The `#` at the top of the file opened a group of its own, and the prose
    // under it explaining what the file is opened nothing. Neither is a group.
    groups.retain(|group| !group.pages.is_empty());
    groups
}

/// Every page the sidebar reaches, in reading order.
pub(crate) fn order() -> Vec<String> {
    groups()
        .into_iter()
        .flat_map(|group| group.pages)
        .map(|entry| entry.slug)
        .collect()
}

/// What comes before and after `slug`, for the links at the foot of a page.
pub(crate) fn neighbours(slug: &str) -> (Option<Entry>, Option<Entry>) {
    let mut pages: Vec<Entry> = groups().into_iter().flat_map(|group| group.pages).collect();
    let Some(at) = pages.iter().position(|entry| entry.slug == slug) else {
        return (None, None);
    };

    // Taken in this order because removing what follows leaves what precedes
    // where it was.
    let next = (at + 1 < pages.len()).then(|| pages.remove(at + 1));
    let previous = (at > 0).then(|| pages.remove(at - 1));

    (previous, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sidebar_is_grouped_and_the_prose_at_the_top_is_not_a_group() {
        let groups = groups();

        assert_eq!(
            groups.first().map(|group| group.title.as_str()),
            Some("Prologue")
        );
        assert!(groups.iter().all(|group| !group.pages.is_empty()));
        assert!(groups.len() >= 6, "{groups:#?}");
    }

    /// A page nobody linked to is a page nobody can reach, and a link to a
    /// page that is not there is a 404 in the sidebar. Both are build-time
    /// mistakes that only a test can catch, since the sidebar is content.
    #[test]
    fn the_sidebar_and_the_content_directory_hold_the_same_pages() {
        let mut linked = order();
        linked.sort();

        let mut present: Vec<String> = content::slugs()
            .filter(|slug| slug != content::NAVIGATION)
            .collect();
        present.sort();

        assert_eq!(linked, present);
    }

    #[test]
    fn a_page_knows_what_is_either_side_of_it() {
        let order = order();
        let (previous, next) = neighbours(&order[1]);

        assert_eq!(previous.map(|entry| entry.slug), Some(order[0].clone()));
        assert_eq!(next.map(|entry| entry.slug), Some(order[2].clone()));

        let (first, _) = neighbours(&order[0]);
        assert!(first.is_none());

        let (_, past_the_end) = neighbours(order.last().expect("a page"));
        assert!(past_the_end.is_none());
    }
}
