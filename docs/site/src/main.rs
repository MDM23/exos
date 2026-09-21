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
pub(crate) mod tests {
    use axum::http::StatusCode;
    use exos_test::{Answer, Browser};

    /// What `uri` answered, to whoever asked for it.
    async fn respond(uri: &str) -> Answer {
        Browser::new(exos::app().into()).get(uri).await
    }

    /// The document served at `uri`, which has to be one.
    pub(crate) async fn get(uri: &str) -> String {
        let answer = respond(uri).await;

        assert_eq!(answer.status(), StatusCode::OK, "{uri}");
        answer.body().to_owned()
    }

    /// What `uri` answered with.
    pub(crate) async fn status(uri: &str) -> StatusCode {
        respond(uri).await.status()
    }
}
