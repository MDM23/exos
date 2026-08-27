//! Nesting an application under a prefix, with nothing configured.
//!
//! `Router::nest` already routes exos wherever it is told. The only thing that
//! does not follow from that is the URL written into a page, because a URL in
//! HTML is absolute and absolute needs the prefix. exos works it out from the
//! first request rather than being told, so this file is about a mount point
//! nobody mentions anywhere.
//!
//! The prefix is process-global and settles once, so this is a test binary of
//! its own. Its neighbour [`base.rs`](base.rs) covers being told instead.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use exos::{Effect, Page, on_click, view};
use tower::ServiceExt as _;

const BASE: &str = "/admin";

#[exos::post("/files/{id}/favorite")]
async fn favorite(axum::extract::Path(_id): axum::extract::Path<u32>) -> Effect {
    Effect::none()
}

#[exos::get("/page")]
async fn page() -> Page {
    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <script defer src={ exos::runtime() }></script>
            </head>
            <body>
                <h1>"Nested"</h1>
                <button {on_click(|_| favorite::post(3))}>"Favorite"</button>
                <a {page::link()}>"This page"</a>
                <a id="written" href={ exos::url("/files") }>"Files"</a>
            </body>
        </html>
    })
}

/// The application as a downstream crate would compose it, and the whole of
/// what it says about where exos lives.
fn mounted() -> Router {
    Router::new().nest(BASE, exos::app().into())
}

async fn served(uri: &str) -> axum::response::Response {
    mounted()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers")
}

async fn status(uri: &str) -> StatusCode {
    served(uri).await.status()
}

/// The rendered page, which is where every URL a browser would use comes from.
/// Asking `exos::runtime()` directly would be asking a question no browser
/// asks.
async fn rendered() -> String {
    let bytes = axum::body::to_bytes(served("/admin/page").await.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    String::from_utf8(bytes.to_vec()).expect("a page is text")
}

/// The value of the first attribute spelled `name="..."` in the page.
async fn attribute(name: &str) -> String {
    let html = rendered().await;
    let opening = format!("{name}=\"");

    let start = html.find(&opening).expect("the attribute is there") + opening.len();
    let end = start + html[start..].find('"').expect("an attribute ends");

    html[start..end].to_owned()
}

async fn script_url() -> String {
    attribute("src").await
}

/// The whole property, end to end and in the order a browser does it: ask for a
/// page, read the URL it carries, and fetch that. Either half alone would pass
/// while the other served 404s.
#[tokio::test]
async fn the_page_carries_a_url_the_router_answers_on() {
    let src = script_url().await;

    assert!(src.starts_with(&format!("{BASE}/_exos/exos-")), "got {src}");
    assert_eq!(status(&src).await, StatusCode::OK);
}

/// The prefix is learned rather than configured, so a request has to have
/// happened before there is anything to have learned it from.
#[tokio::test]
async fn the_prefix_is_the_one_nesting_put_there() {
    // Any request settles it, and asking for a page is the honest one.
    assert!(!script_url().await.is_empty());

    assert_eq!(exos::base_path(), BASE);
}

#[tokio::test]
async fn the_live_endpoints_are_reachable_under_it() {
    assert_eq!(status(&format!("{BASE}/_exos/live")).await, StatusCode::OK);
}

/// Nesting is what moved them, so nothing is left behind where they used to be.
#[tokio::test]
async fn nothing_answers_at_the_root() {
    assert_eq!(status("/_exos/live").await, StatusCode::NOT_FOUND);
    assert_eq!(status("/page").await, StatusCode::NOT_FOUND);
}

/// A route path is written out in the attribute that declares it, and nesting
/// puts it under the prefix along with everything else. Nothing rewrites it
/// twice.
#[tokio::test]
async fn a_route_sits_where_nesting_put_it() {
    assert_eq!(status("/admin/page").await, StatusCode::OK);
    assert_eq!(status("/admin/admin/page").await, StatusCode::NOT_FOUND);
}

/// The rule the whole thing rests on: a route attribute says the path the
/// *server* sees, and a caller has to ask for the one the *browser* is served
/// at. Nesting puts those two apart, so a caller that emitted the attribute's
/// own path would post into a 404 from every button on the page.
#[tokio::test]
async fn a_typed_caller_posts_to_where_the_browser_can_reach_it() {
    let handler = attribute("data-on-click").await;

    assert!(handler.contains("/admin/files/3/favorite"), "got {handler}");

    assert_eq!(
        status("/admin/files/3/favorite").await,
        StatusCode::METHOD_NOT_ALLOWED,
        "and that URL is routed, since a GET of a POST route is not a 404"
    );
}

/// A link built from the route rather than from a string, which is what makes
/// renaming the route a compile error at every place that links to it. It is
/// also the page being served, and knowing that means adding the prefix the
/// request arrived under back to the path the router was left with.
#[tokio::test]
async fn a_route_knows_its_own_url_and_that_it_is_the_page_being_read() {
    let html = rendered().await;

    assert!(
        html.contains("href=\"/admin/page\" aria-current=\"page\""),
        "{html}"
    );
    assert_eq!(status("/admin/page").await, StatusCode::OK);
}

/// The same prefix, for a path that is not a route's, which is the case the
/// route attribute cannot help with.
#[tokio::test]
async fn a_path_written_by_hand_gets_the_prefix_too() {
    let html = rendered().await;

    assert!(
        html.contains("id=\"written\" href=\"/admin/files\""),
        "{html}"
    );
}
