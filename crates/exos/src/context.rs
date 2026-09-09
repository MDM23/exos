//! Application data, reachable from anywhere.
//!
//! Values are keyed by type and live for the process. The point is that a view
//! three levels deep can ask for the database handle without every caller
//! above it having to accept and forward one.
//!
//! ```
//! # #[derive(Debug, PartialEq)]
//! struct Greeting(&'static str);
//!
//! exos::provide(Greeting("hello"));
//! assert_eq!(*exos::data::<Greeting>(), Greeting("hello"));
//! ```
//!
//! # The trade
//!
//! This is global state, and it buys ergonomics with two costs worth knowing.
//!
//! Tests share it. Cargo runs a crate's tests in one process and in parallel,
//! so tests that provide different values of the same type will interfere.
//! Give each test its own type, or seed once and have tests touch disjoint
//! data.
//!
//! The type is the key. Two `String`s cannot both be stored, so wrap distinct
//! things in distinct newtypes, which is the sort of thing worth doing anyway.

use core::panic::Location;
use std::{
    any::{Any, TypeId, type_name},
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock, RwLock},
};

type Registry = RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::default)
}

/// Stores `value`, replacing any previous value of the same type.
///
/// Returns what was displaced, so a test can put things back and an accidental
/// overwrite is at least observable.
///
/// # Panics
///
/// If the registry lock was poisoned by a panic in another thread while it was
/// held. Nothing here can panic while holding it, so this cannot happen from
/// within the crate.
pub fn provide<T: Send + Sync + 'static>(value: T) -> Option<Arc<T>> {
    let previous = registry()
        .write()
        .expect("the registry lock is never held across a panic")
        .insert(TypeId::of::<T>(), Arc::new(value));

    previous.and_then(|any| any.downcast::<T>().ok())
}

/// The same, for [`App::provide`](crate::App::provide), which says where.
///
/// A line that has declared a type has declared it for the process, whatever
/// becomes of the value afterwards. That is what lets an application be built
/// more than once: the first one seeds the store, and the next leaves alone
/// whatever has changed it since, which is the difference between what an
/// application starts with and what it is doing now.
pub(crate) fn declare<T: Send + Sync + 'static>(value: T, at: &'static Location<'static>) {
    static DECLARED: OnceLock<RwLock<HashSet<(TypeId, &'static Location<'static>)>>> =
        OnceLock::new();

    // The guard is held across the provide rather than dropped after the
    // insert, so whoever is told the type is already declared is told it by
    // somebody who has finished declaring it. Between the two there is a
    // window where an application has been built, is serving a request, and
    // reads a type nothing has stored yet, which surfaces as a panic in a
    // handler rather than as anything the caller could have seen coming.
    // Two threads building an application at once is exactly what a test
    // module does.
    let mut declared = DECLARED
        .get_or_init(RwLock::default)
        .write()
        .expect("the declaration lock is never held across a panic");

    if declared.insert((TypeId::of::<T>(), at)) {
        drop(provide(value));
    }
}

/// The value of type `T`, if one was provided.
///
/// # Panics
///
/// If the registry lock was poisoned; see [`provide`].
pub fn try_data<T: Send + Sync + 'static>() -> Option<Arc<T>> {
    let registry = registry()
        .read()
        .expect("the registry lock is never held across a panic");

    registry
        .get(&TypeId::of::<T>())
        .cloned()
        .and_then(|any| any.downcast::<T>().ok())
}

/// The value of type `T`.
///
/// This is the one to reach for in handlers and views. A missing value is a
/// wiring mistake made once at startup rather than a condition to handle on
/// every request, so failing loudly and naming the type is more useful than an
/// `Option` every caller has to unwrap.
///
/// # Panics
///
/// If no value of type `T` has been [`provide`]d.
pub fn data<T: Send + Sync + 'static>() -> Arc<T> {
    try_data::<T>().unwrap_or_else(|| {
        panic!(
            "no application data of type `{}`; provide it before serving",
            type_name::<T>()
        )
    })
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use std::{sync::Barrier, thread};

    use super::*;

    // Distinct types per test, which is the discipline the module docs ask of
    // callers and the only way these stay independent under a shared registry.
    #[derive(Debug, PartialEq)]
    struct RoundTrip(u32);

    #[derive(Debug, PartialEq)]
    struct Displaced(u8);

    #[test]
    fn a_value_comes_back_under_its_own_type() {
        provide(RoundTrip(7));
        assert_eq!(*data::<RoundTrip>(), RoundTrip(7));
    }

    #[test]
    fn a_missing_type_is_none_rather_than_a_panic() {
        struct NeverProvided;
        assert!(try_data::<NeverProvided>().is_none());
    }

    /// Two threads building an application at once, which is what a module of
    /// tests does. Whoever is told the type is already declared has to be told
    /// so by somebody who has finished declaring it, or it goes on to serve a
    /// request out of a registry that holds nothing yet.
    ///
    /// The window is a few instructions wide, so this races over a good many
    /// types rather than one: a declaration is per type, so one type is one
    /// chance at it for the life of the process.
    #[test]
    fn a_declaration_is_finished_before_a_second_caller_is_told_of_it() {
        macro_rules! race {
            ($($name:ident)*) => {$(
                {
                    struct $name;

                    // One call site, so both threads declare the same type at
                    // the same place and exactly one of them is the first.
                    fn seed() {
                        declare($name, Location::caller());
                    }

                    let ready = Arc::new(Barrier::new(2));
                    let racing: Vec<_> = (0..2)
                        .map(|_| {
                            let ready = Arc::clone(&ready);

                            thread::spawn(move || {
                                ready.wait();
                                seed();
                                try_data::<$name>().is_some()
                            })
                        })
                        .collect();

                    for racer in racing {
                        assert!(
                            racer.join().expect("neither thread panics"),
                            "a declaration was announced before it had landed"
                        );
                    }
                }
            )*};
        }

        race!(A B C D E F G H I J K L M N O P Q R S T);
        race!(U V W X Y Z);
    }

    #[test]
    fn providing_twice_hands_back_the_displaced_value() {
        assert!(provide(Displaced(1)).is_none());

        let previous = provide(Displaced(2)).expect("the first value comes back");
        assert_eq!(*previous, Displaced(1));
        assert_eq!(*data::<Displaced>(), Displaced(2));
    }
}
