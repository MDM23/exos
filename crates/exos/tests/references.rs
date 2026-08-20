//! A file a stylesheet names with `url()`, from the macro to the response.
//!
//! The stylesheet is embedded with the image's hashed name written into it, and
//! the image has to be embedded and routed too or the rule points at a 404.
//! What makes that work is that the rewritten URL is relative: everything is
//! served from one directory, so the browser resolves it against the
//! stylesheet's own URL and never needs to know where the application is
//! mounted.

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use tower::ServiceExt as _;

async fn served(uri: &str) -> Response {
    exos::app()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers")
}

async fn text(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    String::from_utf8(bytes.to_vec()).expect("text")
}

/// The URL the `url()` was rewritten to, resolved the way a browser resolves it:
/// against the stylesheet it was written in.
async fn referenced() -> String {
    let stylesheet = exos::asset!("tests/fixtures/app.css");
    let css = text(served(&stylesheet).await).await;

    let relative = css
        .split_once("url(\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(url, _)| url.to_owned())
        .unwrap_or_else(|| panic!("no url() left in {css}"));

    let directory = stylesheet
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .expect("an asset URL has a directory");

    assert!(
        relative.starts_with("./"),
        "an absolute URL would not survive a base: {relative}"
    );

    format!("{directory}/{}", relative.trim_start_matches("./"))
}

#[tokio::test]
async fn the_file_a_stylesheet_names_is_served_beside_it() {
    let response = served(&referenced().await).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|value| value.to_str().expect("a readable header")),
        Some("image/svg+xml"),
        "the extension decides the type, as it does for any other asset"
    );

    assert!(text(response).await.starts_with("<svg"));
}
