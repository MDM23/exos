//! A registration form, with every rule written by hand.
//!
//! The other three examples are about state that changes under you. This one
//! is about a form, and it exists to be the thing the forms roadmap is
//! designed against: it is written entirely with what exos has today, so what
//! it costs is visible rather than argued about.
//!
//! Four things it does, and each of them twice or by hand:
//!
//! * **A shape rule** is a Rust condition in [`form`] and the same question as
//!   an expression in the template. Nothing holds the two together.
//! * **A message** lives in a model field per field, because a handler can
//!   only write what sits on the document.
//! * **A conditional section** is shown by one signal and its rules read that
//!   same signal, spelled once in each place.
//! * **A searchable multi-select** is in [`workshops`], and needs no
//!   client-side loop: every option is rendered once and decides for itself
//!   whether it is on screen.
//!
//! Run it with `cargo run -p signup`.

use crate::store::{Programme, Registrations, Roster};

mod attendees;
mod form;
mod page;
mod store;
mod workshops;

/// The address the example listens on.
const ADDRESS: &str = "127.0.0.1:3000";

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    boot();

    let listener = tokio::net::TcpListener::bind(ADDRESS).await?;
    println!("listening on http://{ADDRESS}");

    axum::serve(listener, exos::app()).await
}

/// Seeds the programme. Separate from `main` so tests can call it.
fn boot() {
    exos::provide(Programme::seed());
    exos::provide(Registrations::default());
    exos::provide(Roster::seed());
}

/// What the modules' own tests are written against.
#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
pub(crate) mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use std::sync::Once;
    use tower::ServiceExt as _;

    /// Seeds the programme once, however many tests ask.
    pub(crate) fn seed() {
        static BOOT: Once = Once::new();
        BOOT.call_once(super::boot);
    }

    /// The router, with every discovered route on it.
    pub(crate) fn app() -> Router {
        seed();
        exos::app()
    }

    /// The body of a response, whatever it answered.
    ///
    /// Unlike the other examples this one does not assert `200` on the way
    /// past: a refusal is half of what this example is about, and it arrives
    /// with the status it deserves.
    async fn body(response: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body is readable");

        String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
    }

    /// The document served at `uri`.
    pub(crate) async fn get(uri: &str) -> String {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
        body(response).await
    }

    /// The event stream an action answers with, refusal or not.
    pub(crate) async fn post(uri: &str, payload: &str) -> String {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_owned()))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(
            response
                .headers()
                .get("content-type")
                .expect("a content type"),
            "text/event-stream"
        );

        body(response).await
    }
}
