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

use axum::Router;

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
    let listener = tokio::net::TcpListener::bind(ADDRESS).await?;
    println!("listening on http://{ADDRESS}");

    axum::serve(listener, app()).await
}

/// The application: every route, and the list it holds.
///
/// The seed is what it starts with rather than what every call to this puts
/// back, so the tests build one per request and the list they change persists
/// between them.
fn app() -> Router {
    exos::app().provide(Todos::seed()).into()
}

/// What the modules' own tests are written against.
///
/// The application data is global, so these tests only ever read it. Every
/// operation that changes the list is a free function over a `Vec` and is
/// tested in [`store`] against a local one, which keeps those tests
/// independent of each other and of the order they run in.
#[cfg(test)]
pub(crate) mod tests {
    use axum::http::StatusCode;
    use exos::ModelFields;
    use exos_test::{Answer, Browser};
    use serde::Serialize;

    pub(crate) use super::app;

    /// The application, for a test that reads the list rather than serving it.
    pub(crate) fn seed() {
        drop(app());
    }

    /// The document served at `uri`.
    pub(crate) async fn get(uri: &str) -> String {
        let answer = Browser::new(app()).get(uri).await;

        assert_eq!(answer.status(), StatusCode::OK);
        answer.body().to_owned()
    }

    /// What an action answered, for one that needs no tab of its own.
    pub(crate) async fn post<T: ModelFields + Serialize>(uri: &str, model: &T) -> Answer {
        let answer = Browser::new(app()).post(uri, model).await;

        assert_eq!(answer.status(), StatusCode::OK);
        assert!(answer.is_effect(), "an action answers with an effect");

        answer
    }
}
