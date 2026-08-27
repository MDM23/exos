//! Building the application more than once.
//!
//! A test serves a request against an application it built, so a binary with
//! twenty tests builds twenty of them, and everything an application says once
//! has to survive that. The rule is that saying the same thing again says
//! nothing new: the same line is one decision however often it runs, and two
//! places that disagree are the panic they always were.
//!
//! Its own test binary, because what it is about is process-wide.

use axum::Router;
use exos::{Audiences, Id, Keys};

/// What this binary starts with, and the one place that says any of it.
#[derive(Debug, PartialEq)]
struct Seed(u32);

/// The application, exactly as an application is written: built where it is
/// served rather than held in a static somewhere.
fn app() -> Router {
    exos::app()
        .provide(Seed(1))
        .keys(Keys::from_secret("what this binary signs with"))
        .identify(async |_name: Option<Id>| Ok(Audiences::none()))
        .into()
}

#[test]
fn building_it_again_says_nothing_new() {
    drop(app());
    drop(app());
}

/// The difference between what an application starts with and what it is doing
/// now. A seed put back on the next request would undo every change since,
/// which is the bug this rule exists to make impossible.
#[test]
fn what_it_starts_with_is_not_put_back_by_the_next_one() {
    drop(app());

    exos::provide(Seed(2));
    drop(app());

    assert_eq!(*exos::data::<Seed>(), Seed(2));
}

/// Two applications disagreeing about who a connection is, which is what the
/// panic was always for.
#[test]
#[should_panic(expected = "a resolver is already in place")]
fn a_resolver_from_somewhere_else_is_refused() {
    drop(app());
    drop(exos::app().identify(async |_name: Option<Id>| Ok(Audiences::none())));
}

/// The same for the key everything is signed under, where the two applications
/// disagreeing show up later as tokens that intermittently fail to verify.
#[test]
#[should_panic(expected = "the signing key is already in place")]
fn a_different_key_is_refused() {
    drop(app());
    drop(exos::app().keys(Keys::from_secret("something else entirely")));
}
