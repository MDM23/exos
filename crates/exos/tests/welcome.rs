//! What a dev build answers with before there is anything to answer.
//!
//! Its own test binary, because the subject is a binary that links no routes at
//! all and a single `#[exos::get]` anywhere in this file would take it away.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt as _;

async fn get(uri: &str) -> (StatusCode, String) {
    let response = exos::app()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers");

    let status = response.status();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    (
        status,
        String::from_utf8(bytes.to_vec()).expect("it is UTF-8"),
    )
}

#[cfg(debug_assertions)]
mod dev {
    use super::{StatusCode, get};

    #[tokio::test]
    async fn a_binary_with_no_routes_says_so_and_points_somewhere() {
        let (status, body) = get("/").await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("exos is running"), "{body}");
        assert!(body.contains("docs/guide.md"), "it links the guide");
    }

    /// A fallback rather than a route on `/`, because a first run knocks on
    /// whichever path the developer had in mind, and a 404 at that path is the
    /// one answer that explains nothing.
    #[tokio::test]
    async fn it_answers_wherever_the_first_run_happens_to_knock() {
        assert_eq!(get("/admin/reports").await.0, StatusCode::OK);
    }

    /// Which is what makes it disappear on its own: the runtime holds a stream
    /// open, a watcher restarts the server, and the tab reloads into whatever
    /// the first route now serves.
    #[tokio::test]
    async fn it_loads_the_runtime_so_it_replaces_itself() {
        let (_, body) = get("/").await;

        assert!(body.contains(&exos::runtime()), "{body}");
    }

    /// The assets are still mounted under a fallback, and a fallback that ate
    /// them would answer a missing file with a page that says everything is
    /// fine.
    #[tokio::test]
    async fn it_does_not_stand_in_front_of_the_assets() {
        assert_eq!(get(&exos::runtime()).await.0, StatusCode::OK);
        assert_eq!(get("/_exos/nothing.js").await.0, StatusCode::NOT_FOUND);
    }
}

/// An application that lost its routes in production should fail like one.
#[cfg(not(debug_assertions))]
#[tokio::test]
async fn a_release_build_offers_nothing_of_the_sort() {
    assert_eq!(get("/").await.0, StatusCode::NOT_FOUND);
}
