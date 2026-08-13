//! Which todos a page shows.
//!
//! A filter is a route rather than client state. The three views are three
//! URLs, so one can be bookmarked, shared and reached with a link, and moving
//! between them is a navigation the runtime morphs rather than a signal the
//! browser holds.

use crate::store::Todo;

/// The three views of the list, in the order the footer lists them.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Filter {
    /// Everything.
    All,
    /// What is left to do.
    Active,
    /// What is done.
    Completed,
}

impl Filter {
    /// Every filter.
    ///
    /// A published list is one fragment per filter, so anything that changes
    /// the list walks this.
    pub(crate) const ALL: [Self; 3] = [Self::All, Self::Active, Self::Completed];

    /// The route this view is served at.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::All => "/",
            Self::Active => "/active",
            Self::Completed => "/completed",
        }
    }

    /// What the footer link says.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Active => "Active",
            Self::Completed => "Completed",
        }
    }

    /// Whether `todo` belongs in this view.
    pub(crate) const fn keeps(self, todo: &Todo) -> bool {
        match self {
            Self::All => true,
            Self::Active => !todo.done,
            Self::Completed => todo.done,
        }
    }

    /// `aria-current` for a link to `self` from the page showing `showing`.
    ///
    /// What a screen reader announces, so the styling keys off the same
    /// attribute rather than a parallel class that could drift from it.
    pub(crate) fn current(self, showing: Self) -> Option<&'static str> {
        (self == showing).then_some("page")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(done: bool) -> Todo {
        Todo {
            id: 1,
            title: String::from("read it"),
            done,
        }
    }

    #[test]
    fn every_filter_has_a_route_of_its_own() {
        let mut paths: Vec<&str> = Filter::ALL.iter().map(|filter| filter.path()).collect();
        paths.sort_unstable();
        paths.dedup();

        assert_eq!(paths.len(), Filter::ALL.len());
    }

    #[test]
    fn each_view_keeps_what_it_is_named_after() {
        assert!(Filter::All.keeps(&todo(true)));
        assert!(Filter::All.keeps(&todo(false)));

        assert!(Filter::Active.keeps(&todo(false)));
        assert!(!Filter::Active.keeps(&todo(true)));

        assert!(Filter::Completed.keeps(&todo(true)));
        assert!(!Filter::Completed.keeps(&todo(false)));
    }

    #[test]
    fn only_the_view_being_shown_is_current() {
        assert_eq!(Filter::Active.current(Filter::Active), Some("page"));
        assert_eq!(Filter::Active.current(Filter::All), None);
    }
}
