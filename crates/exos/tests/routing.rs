//! Route discovery, exercised only through the public API.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use exos::{Markup, view};
use tower::{Service as _, ServiceExt as _};

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

/// A name rather than a number, which is where encoding starts to matter.
#[exos::get("/files/named/{name}")]
async fn named(axum::extract::Path(name): axum::extract::Path<String>) -> Markup {
    view! { <h1>{ name }</h1> }
}

#[exos::get("/files/under/{*rest}")]
async fn under(axum::extract::Path(rest): axum::extract::Path<String>) -> Markup {
    view! { <h1>{ rest }</h1> }
}

async fn status(uri: &str, method: &str) -> StatusCode {
    exos::app()
        .oneshot(
            Request::builder()
                .method(method)
                // What the runtime sends and what an unsafe request is refused
                // without; see the `csrf` module.
                .header("x-exos", "true")
                .uri(uri)
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers")
        .status()
}

/// What a GET of `uri` answered with, which is how a URL a caller built is
/// checked against the route it was built from.
async fn read(uri: &str) -> String {
    let response = exos::app()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers");

    assert_eq!(response.status(), StatusCode::OK, "{uri}");

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
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

/// An application puts exos's layers and endpoints up the first time it answers
/// anything, and the request after that is served by that same router.
///
/// Sealing it a second time would be waste; sealing it out of routes that had
/// been consumed would be a second request answered by an empty application.
#[tokio::test]
async fn one_application_answers_more_than_one_request() {
    let mut app = exos::app();

    for uri in ["/files", "/files/7", &exos::runtime()] {
        let response = app
            .call(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK, "{uri}");
    }
}

/// The typed caller proves a parameter is of the right type. Encoding is what
/// makes it prove the parameter *names* what it was given: a `/` written in
/// raw is another segment, so the URL lands on another route or on none, and
/// the one thing the caller exists to rule out is a link that is wrong.
#[tokio::test]
async fn a_parameter_names_itself_rather_than_reshaping_the_url() {
    let url = named::url(String::from("annual report/2026?draft"));

    assert_eq!(url, "/files/named/annual%20report%2F2026%3Fdraft");
    assert_eq!(read(&url).await, "<h1>annual report/2026?draft</h1>");
}

/// A wildcard is a path rather than one segment, so the separators it was
/// given are separators and everything between them is still a name.
#[tokio::test]
async fn a_wildcard_keeps_the_path_it_was_given() {
    let url = under::url(String::from("notes/a b/c?d"));

    assert_eq!(url, "/files/under/notes/a%20b/c%3Fd");
    assert_eq!(read(&url).await, "<h1>notes/a b/c?d</h1>");
}
