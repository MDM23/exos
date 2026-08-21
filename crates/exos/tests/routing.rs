//! Route discovery, exercised only through the public API.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use exos::{Markup, view};
use tower::ServiceExt as _;

#[exos::get("/files")]
async fn files() -> Markup {
    view! { <h1>"Files"</h1> }
}

#[exos::get("/files/{id}")]
async fn show(axum::extract::Path(id): axum::extract::Path<u32>) -> Markup {
    view! { <h1>{ id }</h1> }
}

#[exos::post("/files")]
async fn create() -> StatusCode {
    StatusCode::CREATED
}

/// A route declared where the whole crate has been glob-imported.
///
/// The generated module does `use super::*`, so `exos::url` lands in the same
/// scope as the `url` the attribute generates. An item beats a glob import, so
/// the caller reaches its own, and this is here because the day that stops
/// being true it should be a build failure rather than a link to the wrong
/// place.
mod globbed {
    use exos::*;

    #[exos::get("/globbed/{id}")]
    async fn show(axum::extract::Path(id): axum::extract::Path<u32>) -> Markup {
        view! { <h1>{ id }</h1> }
    }

    pub(super) fn link() -> String {
        show::url(7)
    }
}

async fn status(uri: &str, method: &str) -> StatusCode {
    exos::app()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers")
        .status()
}

#[tokio::test]
async fn an_attribute_is_the_whole_registration() {
    assert_eq!(status("/files", "GET").await, StatusCode::OK);
    assert_eq!(status("/files/7", "GET").await, StatusCode::OK);
}

#[tokio::test]
async fn two_methods_can_share_one_path() {
    assert_eq!(status("/files", "GET").await, StatusCode::OK);
    assert_eq!(status("/files", "POST").await, StatusCode::CREATED);
}

#[tokio::test]
async fn an_unregistered_path_is_not_found() {
    assert_eq!(status("/nope", "GET").await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_runtime_is_served_and_cached_forever() {
    let response = exos::app()
        .oneshot(
            Request::builder()
                .uri(exos::runtime())
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .expect("assets carry a caching policy"),
        "public, max-age=31536000, immutable"
    );
}

#[tokio::test]
async fn a_route_url_survives_a_glob_import_of_the_crate() {
    assert_eq!(globbed::link(), "/globbed/7");
    assert_eq!(status("/globbed/7", "GET").await, StatusCode::OK);
}

/// The URL is derived from the content, so two call sites cannot disagree
/// about it, and the second one embeds nothing.
#[tokio::test]
async fn one_file_referenced_twice_gives_one_url() {
    let once = exos::asset!("js/exos.js");
    let twice = exos::asset!("js/exos.js");

    assert_eq!(once, twice);
    assert_eq!(status(&once.url(), "GET").await, StatusCode::OK);
}

/// Editing a plugin has to change the runtime's URL, or a browser holding a
/// year-long cache entry would never see the change.
///
/// The path is what carries the hash. A dev build hangs `?dev` off it, which is
/// how the client tells which build it is running under, so the assertion is
/// about the file rather than about the whole string.
#[tokio::test]
async fn the_runtime_url_carries_a_content_hash() {
    let url = exos::runtime();
    let (path, _) = url.split_once('?').unwrap_or((&url, ""));

    assert!(path.starts_with("/_exos/exos-"), "{url}");
    assert!(path.ends_with(".js"), "{url}");
    assert_ne!(path, "/_exos/exos.js");
}

/// The query says which build this is, and it has to be gone from the one that
/// ships: a release tab that opened a stream on every page would hold a
/// connection open for nothing.
#[tokio::test]
async fn only_a_dev_build_marks_the_runtime_as_one() {
    assert_eq!(
        exos::runtime().contains("?dev"),
        cfg!(debug_assertions),
        "{}",
        exos::runtime()
    );
}
