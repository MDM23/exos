//! Values that live for one request.
//!
//! [`data`](crate::data) is keyed by type and lives for the process, which is
//! what lets a view three levels deep reach the database handle without every
//! caller above it accepting and forwarding one. The same argument applies to
//! whatever belongs to the request being served, and the same answer works,
//! except that the value changes per request rather than per process.
//!
//! ```
//! # #[derive(Debug, PartialEq)]
//! struct Principal(u32);
//!
//! exos::with_scope(|| {
//!     exos::scope().set(Principal(7));
//!     assert_eq!(exos::scope().get::<Principal>().as_deref(), Some(&Principal(7)));
//! });
//! ```
//!
//! An axum extractor would be the obvious alternative and does not fit, for one
//! specific reason: a [`view!`](crate::view) fragment is a plain function, not a
//! handler. It cannot extract anything, and making it able to would mean
//! threading a parameter through every template in the application, which is
//! exactly the cost [`data`](crate::data) exists to avoid.
//!
//! # Two rules, built in rather than documented
//!
//! **Outside a request there is no scope, and asking is a panic.** Not `None`.
//! This mirrors what [`data`](crate::data) does for an unprovided type, for the
//! same reason: a background job reading the request is a programming mistake
//! made once, not a condition every caller should handle. `None` would quietly
//! render the logged-out view of something and then publish it.
//!
//! **A live fragment never sees it.** The body of [`live`](crate::live) renders
//! twice, inline during a request and again from whatever publishes it, so a
//! fragment reading the request would produce different HTML in the two places
//! and break the topic invariant. The macro renders through [`detached`], which
//! makes [`scope`] panic inside a fragment always, during a request as much as
//! outside one. A fragment's arguments are its whole input, and this is what
//! says so.

use core::any::{Any, TypeId};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use axum::{extract::Request, middleware::Next, response::Response};

type Store = RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>;

/// What the task-local holds, which is not always a store.
///
/// The masked case is why this is an enum rather than an `Option<Arc<Store>>`:
/// a fragment rendering inside a request has to be told apart from a background
/// job, so that the two panics can say different things.
#[derive(Clone)]
enum State {
    Request(Arc<Store>),
    Masked,
}

tokio::task_local! {
    static STATE: State;
}

/// The values belonging to the request being served.
///
/// A handle, not a guard. Cloning it is an [`Arc`] bump and every clone reads
/// and writes the same store, so it can be passed around freely within the
/// request it came from.
#[derive(Clone)]
pub struct Scope(Arc<Store>);

impl Scope {
    /// The value of type `T`, if one was [`set`](Self::set) during this request.
    ///
    /// Unlike [`data`](crate::data) this answers with an `Option` rather than
    /// panicking. A missing type here is not a wiring mistake: it is the
    /// ordinary way to say that nothing has written one yet, which is what
    /// "anonymous" will look like once a session is what lives in here.
    ///
    /// # Panics
    ///
    /// If the store lock was poisoned by a panic in another thread while it was
    /// held. Nothing here can panic while holding it.
    #[must_use]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.0
            .read()
            .expect("the store lock is never held across a panic")
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|any| any.downcast::<T>().ok())
    }

    /// Stores `value` for the rest of this request, replacing any previous
    /// value of the same type.
    ///
    /// Returns what was displaced, so an accidental overwrite is at least
    /// observable.
    ///
    /// # Panics
    ///
    /// If the store lock was poisoned; see [`get`](Self::get).
    pub fn set<T: Send + Sync + 'static>(&self, value: T) -> Option<Arc<T>> {
        let previous = self
            .0
            .write()
            .expect("the store lock is never held across a panic")
            .insert(TypeId::of::<T>(), Arc::new(value));

        previous.and_then(|any| any.downcast::<T>().ok())
    }
}

impl core::fmt::Debug for Scope {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The values are `dyn Any` and cannot describe themselves, so how many
        // there are is the only honest thing to print.
        let count = self.0.read().map(|store| store.len()).unwrap_or_default();

        formatter
            .debug_struct("Scope")
            .field("values", &count)
            .finish()
    }
}

/// The scope of the request being served.
///
/// # Panics
///
/// If there is no request, or if the caller is inside a live fragment. Both are
/// programming mistakes rather than conditions to handle, and the module docs
/// say why.
#[must_use]
pub fn scope() -> Scope {
    match STATE.try_with(Clone::clone) {
        Ok(State::Request(store)) => Scope(store),
        Ok(State::Masked) => panic!(
            "a live fragment cannot read the request scope; its arguments are \
             its whole input, because it renders again from whatever publishes \
             it"
        ),
        Err(_) => panic!(
            "no request scope; exos::scope is reachable only while serving a \
             request, and a background job is not one"
        ),
    }
}

/// Runs `body` in a request scope of its own.
///
/// This is what the layer does for a request, exposed because a test that
/// renders a view needs the same thing without a server in front of it. Being
/// per task rather than process-global is what lets two tests hold different
/// scopes at once, which [`provide`](crate::provide) cannot do.
///
/// ```
/// # struct Principal(u32);
/// exos::with_scope(|| {
///     exos::scope().set(Principal(7));
/// });
/// ```
pub fn with_scope<R>(body: impl FnOnce() -> R) -> R {
    STATE.sync_scope(State::Request(Arc::default()), body)
}

/// Renders with no request scope, whatever the caller had.
///
/// This is the fragment mask. It is called by `#[exos::live]` and there is no
/// reason to call it yourself.
#[doc(hidden)]
pub fn detached<R>(render: impl FnOnce() -> R) -> R {
    STATE.sync_scope(State::Masked, render)
}

/// Gives every request a scope of its own.
///
/// Mounted by [`app`](crate::app) around everything, so the stream and the
/// asset routes are inside it too.
pub(crate) async fn layer(request: Request, next: Next) -> Response {
    // The outermost thing exos runs, and therefore where the one question that
    // needs a request but does not belong to one gets answered: where this
    // application is mounted. It settles on the first request and is read while
    // rendering, which happens later in this same one.
    crate::base::observe(&request);

    // One small allocation per request, empty until something writes to it: a
    // `HashMap` does not allocate for its entries until the first insert.
    STATE
        .scope(State::Request(Arc::default()), next.run(request))
        .await
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Principal(u32);

    #[derive(Debug, PartialEq)]
    struct Displaced(u8);

    #[test]
    fn a_value_comes_back_under_its_own_type_within_the_request() {
        with_scope(|| {
            scope().set(Principal(7));
            assert_eq!(
                *scope().get::<Principal>().expect("it was set"),
                Principal(7)
            );
        });
    }

    #[test]
    fn a_type_nothing_wrote_is_none_rather_than_a_panic() {
        with_scope(|| assert!(scope().get::<Principal>().is_none()));
    }

    #[test]
    fn writing_twice_hands_back_the_displaced_value() {
        with_scope(|| {
            assert!(scope().set(Displaced(1)).is_none());

            let previous = scope()
                .set(Displaced(2))
                .expect("the first value comes back");
            assert_eq!(*previous, Displaced(1));
        });
    }

    /// Every handle names the same store, which is what makes reaching for the
    /// scope again three frames down cheap rather than wrong.
    #[test]
    fn two_handles_in_one_request_see_one_store() {
        with_scope(|| {
            let first = scope();
            scope().set(Principal(7));

            assert_eq!(*first.get::<Principal>().expect("it was set"), Principal(7));
        });
    }

    /// The whole point of a task-local over a global: two requests in flight
    /// hold different values of one type, which `provide` cannot do.
    #[tokio::test]
    async fn one_request_cannot_see_another_request_s_values() {
        let first = tokio::spawn(async {
            with_scope(|| {
                scope().set(Principal(1));
                scope().get::<Principal>().expect("its own value").0
            })
        });

        let second = tokio::spawn(async {
            with_scope(|| {
                scope().set(Principal(2));
                scope().get::<Principal>().expect("its own value").0
            })
        });

        assert_eq!(first.await.expect("the task finishes"), 1);
        assert_eq!(second.await.expect("the task finishes"), 2);
    }

    /// A scope does not leak into whatever the request spawned, because a
    /// task-local belongs to the task that set it.
    #[tokio::test]
    async fn a_spawned_task_is_not_inside_the_request_that_spawned_it() {
        with_scope(|| scope().set(Principal(7)));

        let escaped = tokio::spawn(async { STATE.try_with(|_| ()).is_ok() })
            .await
            .expect("the task finishes");

        assert!(!escaped);
    }

    #[test]
    #[should_panic(expected = "no request scope")]
    fn asking_outside_a_request_is_a_panic_rather_than_an_empty_scope() {
        let _ = scope();
    }

    #[test]
    #[should_panic(expected = "a live fragment cannot read the request scope")]
    fn a_fragment_is_masked_even_though_a_request_is_being_served() {
        with_scope(|| detached(scope));
    }

    #[test]
    #[should_panic(expected = "a live fragment cannot read the request scope")]
    fn a_fragment_outside_a_request_says_the_same_thing() {
        detached(scope);
    }

    /// Masking is only as wide as the fragment. The handler that rendered one
    /// keeps its scope on the next line.
    #[test]
    fn the_mask_lifts_when_the_fragment_is_done() {
        with_scope(|| {
            scope().set(Principal(7));
            detached(|| ());

            assert_eq!(
                *scope().get::<Principal>().expect("it was set"),
                Principal(7)
            );
        });
    }
}
