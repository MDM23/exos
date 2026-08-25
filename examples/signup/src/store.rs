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
