//! The exos documentation site, written in exos.
//!
//! The pages are markdown in [`content`], the sidebar is one of them, and
//! [`markdown`] turns either into markup. There is no generator step and no
//! output directory: a page is rendered when it is asked for, out of a file
//! that a release build has already embedded in the binary.
//!
//! Run it with `cargo run -p exos-docs` and open <http://localhost:3000>.

mod content;
mod markdown;
mod nav;
mod page;
mod search;

/// The address the site listens on.
const ADDRESS: &str = "127.0.0.1:3000";

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(ADDRESS).await?;
    println!("listening on http://{ADDRESS}");

    axum::serve(listener, exos::app()).await
}

/// What the modules' own tests are written against.
#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
pub(crate) mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        response::Response,
    };
    use tower::ServiceExt as _;

    /// The response served at `uri`.
    async fn respond(uri: &str) -> Response {
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

    /// The document served at `uri`, which has to be one.
    pub(crate) async fn get(uri: &str) -> String {
        let response = respond(uri).await;
        assert_eq!(response.status(), StatusCode::OK, "{uri}");

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body is readable");

        String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
    }

    /// What `uri` answered with.
    pub(crate) async fn status(uri: &str) -> StatusCode {
        respond(uri).await.status()
    }
}
