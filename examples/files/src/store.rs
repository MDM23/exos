//! The example's data, and the operations on it.
//!
//! The operations are free functions over a slice rather than methods that
//! reach for global state. That is what lets them be tested with a local
//! `Vec`, with no shared state between tests and no reliance on the order they
//! run in.

use std::sync::Mutex;

/// One row in the directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Entry {
    /// The identifier the DOM and the client use.
    pub(crate) id: u32,
    /// The file's name.
    pub(crate) name: String,
    /// Whether the viewer starred it.
    pub(crate) favorite: bool,
    /// Which user owns it, for the presence dot.
    pub(crate) owner: u32,
}

/// The directory, as application data.
#[derive(Debug, Default)]
pub(crate) struct Files(Mutex<Vec<Entry>>);

impl Files {
    /// A directory with something in it.
    #[must_use]
    pub(crate) fn seed() -> Self {
        let names = [
            "annual-report.pdf",
            "budget.xlsx",
            "vacation-photos",
            "meeting-notes.md",
            "invoice-2026.pdf",
        ];

        let entries = names
            .iter()
            .enumerate()
            .map(|(index, name)| Entry {
                id: u32::try_from(index).unwrap_or(0) + 1,
                name: (*name).to_owned(),
                favorite: index % 3 == 0,
                owner: u32::try_from(index % 3).unwrap_or(0) + 1,
            })
            .collect();

        Self(Mutex::new(entries))
    }

    /// A copy of the rows, for rendering.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned by a panic in another thread while held.
    #[must_use]
    pub(crate) fn snapshot(&self) -> Vec<Entry> {
        self.0
            .lock()
            .expect("the store lock is never held across a panic")
            .clone()
    }

    /// Applies `change` to the rows.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned; see [`snapshot`](Self::snapshot).
    pub(crate) fn update(&self, change: impl FnOnce(&mut Vec<Entry>)) {
        let mut entries = self
            .0
            .lock()
            .expect("the store lock is never held across a panic");

        change(&mut entries);
    }
}

/// Who is online, as application data.
#[derive(Debug, Default)]
pub(crate) struct Presence(Mutex<Vec<bool>>);

impl Presence {
    /// Three users, two of them online.
    #[must_use]
    pub(crate) fn seed() -> Self {
        Self(Mutex::new(vec![true, false, true]))
    }

    /// Whether `user` is online.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned by a panic in another thread while held.
    #[must_use]
    pub(crate) fn online(&self, user: u32) -> bool {
        let index = user.saturating_sub(1) as usize;

        self.0
            .lock()
            .expect("the presence lock is never held across a panic")
            .get(index)
            .copied()
            .unwrap_or(false)
    }

    /// Flips a user's presence.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned; see [`online`](Self::online).
    pub(crate) fn toggle(&self, user: u32) {
        let index = user.saturating_sub(1) as usize;

        let mut state = self
            .0
            .lock()
            .expect("the presence lock is never held across a panic");

        if let Some(flag) = state.get_mut(index) {
            *flag = !*flag;
        }
    }
}

/// Flips one row's star, and reports whether the row existed.
pub(crate) fn toggle_favorite(entries: &mut [Entry], id: u32) -> bool {
    match entries.iter_mut().find(|entry| entry.id == id) {
        Some(entry) => {
            entry.favorite = !entry.favorite;
            true
        }
        None => false,
    }
}

/// Drops one row.
pub(crate) fn delete(entries: &mut Vec<Entry>, id: u32) {
    entries.retain(|entry| entry.id != id);
}

/// Drops every row in `picked`.
pub(crate) fn archive(entries: &mut Vec<Entry>, picked: &[u32]) {
    entries.retain(|entry| !picked.contains(&entry.id));
}

/// Reorders the rows to match `order`.
///
/// An id the client invented matches nothing and sorts last: the payload
/// selects among rows, it does not define them.
pub(crate) fn reorder(entries: &mut [Entry], order: &[String]) {
    entries.sort_by_key(|entry| {
        order
            .iter()
            .position(|id| id.parse::<u32>() == Ok(entry.id))
            .unwrap_or(usize::MAX)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, local directory. Nothing here touches global state, so these
    /// tests are independent of each other and of their running order.
    fn rows() -> Vec<Entry> {
        (1..=4)
            .map(|id| Entry {
                id,
                name: format!("file-{id}"),
                favorite: false,
                owner: 1,
            })
            .collect()
    }

    fn ids(entries: &[Entry]) -> Vec<u32> {
        entries.iter().map(|entry| entry.id).collect()
    }

    #[test]
    fn favouriting_flips_one_row_and_reports_that_it_existed() {
        let mut entries = rows();

        assert!(toggle_favorite(&mut entries, 2));
        assert!(entries[1].favorite);

        assert!(toggle_favorite(&mut entries, 2));
        assert!(!entries[1].favorite);
    }

    #[test]
    fn favouriting_a_missing_row_reports_failure() {
        assert!(!toggle_favorite(&mut rows(), 99));
    }

    #[test]
    fn deleting_removes_only_the_named_row() {
        let mut entries = rows();
        delete(&mut entries, 3);

        assert_eq!(ids(&entries), vec![1, 2, 4]);
    }

    #[test]
    fn archiving_removes_the_whole_selection() {
        let mut entries = rows();
        archive(&mut entries, &[2, 4]);

        assert_eq!(ids(&entries), vec![1, 3]);
    }

    #[test]
    fn archiving_ignores_ids_that_are_not_there() {
        let mut entries = rows();
        archive(&mut entries, &[99]);

        assert_eq!(ids(&entries), vec![1, 2, 3, 4]);
    }

    #[test]
    fn reordering_follows_the_order_it_is_given() {
        let mut entries = rows();
        reorder(&mut entries, &[String::from("3"), String::from("1")]);

        assert_eq!(&ids(&entries)[..2], &[3, 1]);
    }

    #[test]
    fn an_invented_id_neither_creates_nor_duplicates_a_row() {
        let mut entries = rows();
        reorder(&mut entries, &[String::from("77"), String::from("2")]);

        let mut sorted = ids(&entries);
        sorted.sort_unstable();

        assert_eq!(sorted, vec![1, 2, 3, 4]);
        assert_eq!(ids(&entries)[0], 2, "the real id moved to the front");
    }
}
