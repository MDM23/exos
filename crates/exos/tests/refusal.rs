//! Answering no, and still saying what to do about it.
//!
//! The client applies an effect whatever status carries it, which is what lets
//! a handler be honest about a refusal and be heard at the same time. This is
//! the server's half: that the shape the guide recommends compiles, and that
//! what goes on the wire is the same event stream a success would send.
//!
//! Its other half is in
//! [`js/tests/runtime.test.js`](../js/tests/runtime.test.js), which is where
//! whether the browser acts on it can be checked at all.

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use exos::{Effect, Model, view};
use serde::{Deserialize, Serialize};
use tower::ServiceExt as _;

#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Draft {
    error: String,
    title: String,
}

/// The shape the guide recommends, and the reason this file exists: a refusal
/// answers with the status it deserves *and* with what the page should do.
#[exos::post("/drafts")]
async fn save(Model(draft): Model<Draft>) -> Result<Effect, (StatusCode, Effect)> {
    if draft.title.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Effect::set(&Draft::signals().error, String::from("A title is needed."))
                .focus("#title"),
        ));
    }

    Ok(Effect::patch(
        view! { <li id="drafts">{ &draft.title }</li> },
    ))
}

async fn posted(draft: &Draft) -> (StatusCode, String) {
    let response = exos::app()
        .oneshot(
            Request::builder()
                .method("POST")
                .header("x-exos", "true")
                .uri("/drafts")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(exos::to_wire(draft)))
                .expect("a valid request"),
        )
        .await
        .expect("the router answers");

    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    assert_eq!(
        content_type, "text/event-stream",
        "an effect is an effect whichever way the handler answered"
    );

    (
        status,
        String::from_utf8(bytes.to_vec()).expect("an event is text"),
    )
}

/// A refusal that had to answer `200` in order to be heard would be a lie told
/// to every log, proxy and test in front of it.
#[tokio::test]
async fn a_refusal_carries_both_the_status_and_the_effect() {
    let (status, body) = posted(&Draft::default()).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("event: signals"), "{body}");
    assert!(body.contains("A title is needed."), "{body}");
    assert!(body.ends_with("event: focus\ndata: #title\n\n"), "{body}");
}

#[tokio::test]
async fn a_success_answers_the_same_way_it_always_did() {
    let (status, body) = posted(&Draft {
        error: String::new(),
        title: String::from("A clock"),
    })
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with("event: patch"), "{body}");
    assert!(body.contains("A clock"), "{body}");
}
