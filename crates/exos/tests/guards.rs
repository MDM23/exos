//! What a second guard is answered with.
//!
//! Its own test binary, because the subject is a binary that links two of them
//! and every `exos::app` in it therefore panics.

use axum::{extract::Request, middleware::Next, response::Response};

#[exos::guard]
async fn first(request: Request, next: Next) -> Response {
    next.run(request).await
}

#[exos::guard]
async fn second(request: Request, next: Next) -> Response {
    next.run(request).await
}

/// At startup, where it is one stack trace, rather than as whichever of the two
/// the linker happened to hand over first.
#[test]
#[should_panic(expected = "two guards")]
fn two_guards_are_refused_rather_than_ordered() {
    let _ = exos::app();
}
