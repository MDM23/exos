//! Being told where the application is mounted, rather than working it out.
//!
//! [`mounted.rs`](mounted.rs) covers the ordinary case, where nesting is
//! visible from inside and exos finds the prefix on its own. This is the case
//! it cannot see: a reverse proxy serving the application at `/admin` while
//! forwarding `/` to it. The server genuinely never receives that prefix, so no
//! amount of looking will find it and it has to be said.
//!
//! Everything a browser is given still has to carry it, which is the whole
//! test. The prefix is process-global and settles once, so this is a test
//! binary of its own.

use std::sync::Once;

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

/// Declared at the path the *server* is asked for, which is what the proxy
/// forwards. The prefix is what the browser adds in front, and exos puts it
/// back on everything it writes.
#[exos::get("/page")]
async fn page() -> Page {
    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <script defer src={ exos::runtime() }></script>
            </head>
            <body>
                <h1>"Behind a proxy"</h1>
                <button {on_click(|_| favorite::post(3))}>"Favorite"</button>
            </body>
        </html>
    })
}

/// Every test starts here, including the ones that only read a URL, because a
/// request would otherwise settle the prefix at the root first and this call
/// would then be the second one.
fn setup() {
    static SET: Once = Once::new();
    SET.call_once(|| exos::base(BASE));
}

/// No nesting anywhere: the proxy already took the prefix off, so the router
/// answers exactly the paths it is declared with.
fn behind_a_proxy() -> Router {
    setup();
    exos::app()
}

async fn served(uri: &str) -> axum::response::Response {
    behind_a_proxy()
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

/// The value of the first attribute spelled `name="..."` in the page.
async fn attribute(name: &str) -> String {
    let bytes = axum::body::to_bytes(served("/page").await.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    let html = String::from_utf8(bytes.to_vec()).expect("a page is text");
    let opening = format!("{name}=\"");

    let start = html.find(&opening).expect("the attribute is there") + opening.len();
    let end = start + html[start..].find('"').expect("an attribute ends");

    html[start..end].to_owned()
}

#[tokio::test]
async fn what_the_browser_is_given_carries_the_prefix() {
    setup();

    assert!(
        attribute("src")
            .await
            .starts_with(&format!("{BASE}/_exos/")),
        "the runtime is loaded through the proxy"
    );
    assert!(
        attribute("data-on-click")
            .await
            .contains("/admin/files/3/favorite"),
        "and so is every action"
    );
}

/// The other half, and the one that makes this different from nesting: the
/// server is asked for the stripped path, because that is what the proxy
/// forwards.
#[tokio::test]
async fn the_router_answers_the_paths_it_is_declared_with() {
    setup();

    assert_eq!(status("/page").await, StatusCode::OK);
    assert_eq!(status("/_exos/live").await, StatusCode::OK);
    assert_eq!(
        status("/admin/page").await,
        StatusCode::NOT_FOUND,
        "nothing here mounted anything under the prefix"
    );
}

/// Being told is what an application does instead of being found out, so a
/// request afterwards must not change the answer.
#[tokio::test]
async fn a_request_does_not_overwrite_what_was_said() {
    setup();

    assert_eq!(status("/page").await, StatusCode::OK);
    assert_eq!(exos::base_path(), BASE);
}

/// Two parts of a program disagreeing about where it lives, or one of them
/// arriving after a request had already answered the question. Both are worth
/// being loud about.
#[tokio::test]
#[should_panic(expected = "a base is already in place")]
async fn a_second_base_is_refused() {
    setup();
    exos::base("/elsewhere");
}
