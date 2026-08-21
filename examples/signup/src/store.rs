//! What the form is filled in against.
//!
//! The programme never changes while the example runs, so it is handed out by
//! reference. Registrations do, so they sit behind a lock, and the operation
//! on them is a free function over a `Vec` for the reason
//! [todos](../../todos/src/store.rs) gives: a test can have its own list.

use std::sync::Mutex;

/// The codes that exist. The only rule in this example a browser cannot check.
const CODES: [&str; 2] = ["EARLYBIRD", "SPEAKER"];

/// One workshop on the programme.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Workshop {
    /// What a checkbox carries and what the model collects.
    pub(crate) id: u32,
    /// What the option says.
    pub(crate) title: String,
    /// Which day it runs on, so the list is worth searching.
    pub(crate) track: String,
}

/// Everything on offer.
#[derive(Debug, Default)]
pub(crate) struct Programme(Vec<Workshop>);

impl Programme {
    /// A programme long enough that filtering it is worth doing.
    #[must_use]
    pub(crate) fn seed() -> Self {
        let seeds = [
            ("Async Rust from the bottom up", "Monday"),
            ("Borrowing without fighting", "Monday"),
            ("Error handling that survives review", "Monday"),
            ("Macros: when and when not", "Tuesday"),
            ("Property testing in anger", "Tuesday"),
            ("Server-rendered hypermedia", "Tuesday"),
            ("Tracing a request end to end", "Wednesday"),
            ("Type-driven API design", "Wednesday"),
            ("Unsafe, and how to avoid it", "Wednesday"),
        ];

        let workshops = seeds
            .iter()
            .enumerate()
            .map(|(index, (title, track))| Workshop {
                id: u32::try_from(index).unwrap_or(0) + 1,
                title: (*title).to_owned(),
                track: (*track).to_owned(),
            })
            .collect();

        Self(workshops)
    }

    /// The whole programme, in the order it is shown.
    pub(crate) fn all(&self) -> &[Workshop] {
        &self.0
    }

    /// Whether every id was one this programme offers.
    ///
    /// A checkbox carries whatever the markup said, and markup is not a
    /// promise: the ids that arrive are checked like anything else on the wire.
    pub(crate) fn holds(&self, picked: &[u32]) -> bool {
        picked
            .iter()
            .all(|id| self.0.iter().any(|workshop| workshop.id == *id))
    }
}

/// One person on the registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Attendee {
    /// What the row is keyed by, in the DOM and in the routes.
    pub(crate) id: u32,
    /// Who they are, as far as it has been typed.
    pub(crate) name: String,
}

/// The rows of the form that are not fields of it.
///
/// This is the price of a repeating group today. The rows cannot ride along in
/// the submission, so they are held here, which makes a half-filled form a
/// resource on the server rather than state in a browser. One list for the
/// whole process is what an example can afford; an application would key it by
/// the viewer and would then have to decide when an abandoned one expires.
#[derive(Debug, Default)]
pub(crate) struct Roster(Mutex<Vec<Attendee>>);

impl Roster {
    /// A roster with two rows, so the form opens with something to edit.
    #[must_use]
    pub(crate) fn seed() -> Self {
        let rows = ["Ada Lovelace", "Alan Turing"]
            .iter()
            .enumerate()
            .map(|(index, name)| Attendee {
                id: u32::try_from(index).unwrap_or(0) + 1,
                name: (*name).to_owned(),
            })
            .collect();

        Self(Mutex::new(rows))
    }

    /// A copy of the rows, for rendering.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned by a panic in another thread while held.
    pub(crate) fn snapshot(&self) -> Vec<Attendee> {
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
    pub(crate) fn update<T>(&self, change: impl FnOnce(&mut Vec<Attendee>) -> T) -> T {
        let mut rows = self
            .0
            .lock()
            .expect("the store lock is never held across a panic");

        change(&mut rows)
    }
}

/// Appends an empty row, and hands back the id it was given.
pub(crate) fn add_attendee(rows: &mut Vec<Attendee>) -> u32 {
    let id = rows.iter().map(|row| row.id).max().unwrap_or(0) + 1;

    rows.push(Attendee {
        id,
        name: String::new(),
    });

    id
}

/// Writes one row's name.
pub(crate) fn rename_attendee(rows: &mut [Attendee], id: u32, name: &str) {
    if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
        row.name = name.trim().to_owned();
    }
}

/// Drops one row.
pub(crate) fn remove_attendee(rows: &mut Vec<Attendee>, id: u32) {
    rows.retain(|row| row.id != id);
}

/// What is wrong with the rows, if anything.
///
/// A free function over a slice, like every other rule that has to be tested
/// without a server in front of it. It can only answer for the group, because
/// the message it produces has only one place to go.
pub(crate) fn roster_fault(rows: &[Attendee]) -> Option<&'static str> {
    if rows.is_empty() {
        return Some("Add at least one attendee.");
    }

    rows.iter()
        .any(|row| row.name.trim().is_empty())
        .then_some("Every attendee needs a name.")
}

/// One accepted registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Registration {
    /// Who is coming.
    pub(crate) name: String,
    /// Where the confirmation goes.
    pub(crate) email: String,
    /// Which workshops they picked.
    pub(crate) workshops: Vec<u32>,
}

/// The registrations taken so far.
#[derive(Debug, Default)]
pub(crate) struct Registrations(Mutex<Vec<Registration>>);

impl Registrations {
    /// Records one, and hands back how many there are now.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned by a panic in another thread while held.
    pub(crate) fn add(&self, registration: Registration) -> usize {
        let mut taken = self
            .0
            .lock()
            .expect("the store lock is never held across a panic");

        taken.push(registration);
        taken.len()
    }
}

/// Whether a discount code is one this conference issued.
///
/// The rule that has to be a round trip. Nothing about the codes reaches the
/// browser, which is the whole reason it cannot be checked there.
pub(crate) fn accepts(code: &str) -> bool {
    CODES.contains(&code.trim().to_uppercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_programme_knows_what_it_offers() {
        let programme = Programme::seed();

        assert!(programme.holds(&[1, 2]));
        assert!(!programme.holds(&[1, 99]));
        assert!(programme.holds(&[]));
    }

    #[test]
    fn a_code_is_accepted_whatever_case_it_arrives_in() {
        assert!(accepts("earlybird"));
        assert!(accepts("  SPEAKER "));
        assert!(!accepts("FRIEND"));
    }

    /// A fresh, local list, so nothing here depends on what another test did.
    fn rows() -> Vec<Attendee> {
        vec![
            Attendee {
                id: 1,
                name: String::from("Ada"),
            },
            Attendee {
                id: 2,
                name: String::from("Alan"),
            },
        ]
    }

    #[test]
    fn a_new_row_gets_an_id_of_its_own_and_no_name() {
        let mut rows = rows();

        assert_eq!(add_attendee(&mut rows), 3);
        assert_eq!(rows[2].name, "");
    }

    #[test]
    fn renaming_writes_the_trimmed_name() {
        let mut rows = rows();
        rename_attendee(&mut rows, 2, "  Grace  ");

        assert_eq!(rows[1].name, "Grace");
    }

    #[test]
    fn removing_drops_one_row() {
        let mut rows = rows();
        remove_attendee(&mut rows, 1);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, 2);
    }

    /// The rule can only answer for the group, because the message it produces
    /// has one place to go and that place is not a row.
    #[test]
    fn a_roster_is_faulted_as_a_whole() {
        assert_eq!(roster_fault(&[]), Some("Add at least one attendee."));
        assert_eq!(roster_fault(&rows()), None);

        let mut half = rows();
        half[1].name = String::from("  ");

        assert_eq!(roster_fault(&half), Some("Every attendee needs a name."));
    }

    #[test]
    fn registrations_count_up() {
        let taken = Registrations::default();

        let one = Registration {
            name: String::from("Ada"),
            email: String::from("ada@example.com"),
            workshops: vec![1],
        };

        assert_eq!(taken.add(one.clone()), 1);
        assert_eq!(taken.add(one), 2);
    }
}
