//! Being told when the streams change, which a gauge of who is connected is
//! published from.
//!
//! A test binary of its own, because the count it reads is process-global and
//! another test opening a stream would move it.

use core::time::Duration;

use axum::{Router, body::Body, http::Request};
use tokio::sync::watch;
use tower::ServiceExt as _;

fn app() -> Router {
    exos::app().into()
}

async fn told(changes: &mut watch::Receiver<()>) -> bool {
    tokio::time::timeout(Duration::from_secs(1), changes.changed())
        .await
        .is_ok_and(|changed| changed.is_ok())
}

/// A stream opening is news, and so is it going away, and the count read
/// when told already says so.
#[tokio::test]
async fn a_stream_opening_and_closing_is_told() {
    let mut changes = exos::live::connection_changes();

    let stream = app()
        .oneshot(
            Request::get("/_exos/live")
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers");

    assert!(told(&mut changes).await, "told when it opened");
    assert_eq!(exos::live::connection_count(), 1);

    drop(stream);

    assert!(told(&mut changes).await, "and when it went away");
    assert_eq!(exos::live::connection_count(), 0);
}
