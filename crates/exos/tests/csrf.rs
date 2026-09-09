//! What stands between somebody else's page and a state change here.
//!
//! A browser attaches this application's cookies to a request whichever page
//! caused it, so the question a handler cannot answer for itself is whether
//! the request was asked for. exos answers it once, for every route it serves:
//! an unsafe method has to carry the header the runtime sends, which a page
//! from another origin cannot set without a preflight nobody granted.

use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderValue, Request as HttpRequest, StatusCode, header},
    middleware::{Next, from_fn},
    response::Response,
};
use exos::{Effect, Markup, view};
use tower::ServiceExt as _;

#[exos::get("/orders")]
async fn list() -> Markup {
    view! { <h1>"Orders"</h1> }
}

#[exos::post("/orders")]
async fn place() -> Effect {
    Effect::none()
}

/// Whether a request reached the application at all, said on the response so
/// that two tests running at once cannot read each other's answer.
async fn reached(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    response
        .headers_mut()
        .insert("x-reached", HeaderValue::from_static("true"));

    response
}

fn app() -> Router {
    exos::app().layer(from_fn(reached)).into()
}

/// One request, as a client that is or is not the runtime would send it.
async fn sent(method: &str, uri: &str, runtime: bool) -> Response {
    let mut builder = HttpRequest::builder().method(method).uri(uri);

    if runtime {
        builder = builder.header("x-exos", "true");
    }

    app()
        .oneshot(builder.body(Body::empty()).expect("a valid request"))
        .await
        .expect("the router answers")
}

/// The ordinary case, which is every action an application declares: the
/// runtime says the header on every call it makes.
#[tokio::test]
async fn a_call_carrying_the_header_is_served() {
    assert_eq!(sent("POST", "/orders", true).await.status(), StatusCode::OK);
}

/// The whole of the defence. A form on another origin can post here and the
/// browser will attach the cookie, and this is where that stops: setting a
/// header of its own would need a preflight, and a preflight needs CORS this
/// application never turned on.
#[tokio::test]
async fn a_state_change_without_the_header_is_refused() {
    let response = sent("POST", "/orders", false).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        response.headers().get("x-reached").is_none(),
        "and nothing the application mounted ran for it"
    );
}

/// Safe methods are how a page, an asset and the stream itself arrive, and a
/// browser sends them from anywhere whatever exos does. Refusing them would
/// break every link into the application and defend nothing.
#[tokio::test]
async fn a_page_is_served_to_a_browser_that_sends_nothing() {
    assert_eq!(sent("GET", "/orders", false).await.status(), StatusCode::OK);
}

/// exos's own endpoints are requests like any other. The subscription endpoint
/// is the one that changes something, and the runtime sends the header there
/// as well.
#[tokio::test]
async fn the_endpoints_exos_mounts_are_guarded_too() {
    assert_eq!(
        sent("POST", "/_exos/subscribe", false).await.status(),
        StatusCode::FORBIDDEN
    );
}

/// Whoever hits this is holding a terminal or reading a log, so the refusal
/// says which header is missing rather than only that something is.
#[tokio::test]
async fn a_refusal_says_what_was_missing() {
    let response = sent("DELETE", "/orders", false).await;
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    let said = String::from_utf8(bytes.to_vec()).expect("the body is text");

    assert!(said.contains("X-Exos"), "{said}");
}

/// The header is proof by being there at all, so what is in it is nothing
/// anybody has to agree on: a value the runtime changed would be a second
/// thing to keep in step for no gain.
#[tokio::test]
async fn what_the_header_says_is_never_read() {
    let response = app()
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/orders")
                .header("x-exos", "")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers");

    assert_eq!(response.status(), StatusCode::OK);
}
