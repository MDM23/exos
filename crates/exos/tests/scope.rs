//! The request scope as a handler and a fragment see it.

use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use exos::view;
use tower::ServiceExt as _;

/// A type of this test binary's own, since the scope keys by type just as
/// [`exos::data`] does.
#[derive(Debug)]
struct Visited;

#[exos::get("/scope")]
async fn visited() -> String {
    let scope = exos::scope();
    let seen_before = scope.get::<Visited>().is_some();
    scope.set(Visited);

    format!("{seen_before}")
}

async fn body(uri: &str) -> String {
    let response = exos::app()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers");

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is read");

    String::from_utf8(bytes.to_vec()).expect("the body is text")
}

/// The layer is what puts it there, so a handler never asks for it and never
/// arranges it.
#[tokio::test]
async fn a_handler_is_served_inside_a_request_scope() {
    assert_eq!(body("/scope").await, "false");
}

/// The interesting half: the second request must not see the first one's
/// value, which is what separates this from `provide`.
#[tokio::test]
async fn each_request_gets_a_scope_of_its_own() {
    assert_eq!(body("/scope").await, "false");
    assert_eq!(body("/scope").await, "false");
}

#[exos::live]
fn reads_the_scope() -> exos::Markup {
    let _ = exos::scope();
    view! { <span>"never rendered"</span> }
}

/// The rule the macro enforces, checked through the macro rather than through
/// the mask it expands to: a fragment renders again from whatever publishes it,
/// where there is no request to read.
///
/// Asking for the markup is what runs the body, since a fragment carries its
/// render rather than the result of one, so the mask bites where the reading
/// would happen rather than where the fragment was named.
#[test]
#[should_panic(expected = "a live fragment cannot read the request scope")]
fn a_live_fragment_cannot_read_the_request_scope() {
    exos::with_scope(|| drop(reads_the_scope().markup()));
}
