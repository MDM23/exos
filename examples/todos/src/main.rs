//! `TodoMVC`, written in exos.
//!
//! The classic list: type to add, click to complete, double-click to edit,
//! filter, clear. Everything the browser does is written in Rust and checked
//! by the compiler, and every change is published, so two tabs show the same
//! list without either of them polling.
//!
//! The modules follow the features rather than the layers: [`compose`] is the
//! field a todo is typed into and the action that accepts it, [`item`] is one
//! row and everything a click on it can do, [`board`] is the list around them.
//!
//! Run it with `cargo run -p todos` and open <http://localhost:3000> twice.

use crate::store::Todos;

mod board;
mod compose;
mod filter;
mod item;
mod page;
mod store;

/// The address the example listens on.
const ADDRESS: &str = "127.0.0.1:3000";

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    boot();

    let listener = tokio::net::TcpListener::bind(ADDRESS).await?;
    println!("listening on http://{ADDRESS}");

    axum::serve(listener, exos::app()).await
}

/// Seeds the list. Separate from `main` so tests can call it.
fn boot() {
    exos::provide(Todos::seed());
}

/// What the modules' own tests are written against.
///
/// The application data is global, so these tests only ever read it. Every
/// operation that changes the list is a free function over a `Vec` and is
/// tested in [`store`] against a local one, which keeps those tests
/// independent of each other and of the order they run in.
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

    /// Seeds the list once, however many tests ask.
    pub(crate) fn seed() {
        static BOOT: Once = Once::new();
        BOOT.call_once(super::boot);
    }

    /// The router, with every discovered route on it.
    pub(crate) fn app() -> Router {
        seed();
        exos::app()
    }

    /// The body of a response that answered `200`.
    async fn body(response: axum::response::Response) -> String {
        assert_eq!(response.status(), StatusCode::OK);

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

        body(response).await
    }

    /// The event stream an action answers with.
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
