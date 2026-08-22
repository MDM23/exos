//! A registration form, and what a form costs.
//!
//! The other three examples are about state that changes under you. This one
//! is about a form. It was written by hand first, against what exos had before
//! any of the forms roadmap, so that the stages could be designed against a
//! cost that was visible rather than argued about; each one that has since
//! landed took something back out of it.
//!
//! Five things it does:
//!
//! * **A shape rule** is declared on the model in [`form`], and that one
//!   declaration answers on both sides: the extractor refuses a body that
//!   breaks it, and the control asks it again while it is being typed.
//! * **A message** goes into one record per model, keyed by field, whichever
//!   side decided what is in it.
//! * **A conditional section** is shown by one signal and gated on that same
//!   signal, which is the one pair nothing holds together.
//! * **A searchable multi-select** is in [`workshops`], and needs no
//!   client-side loop: every option is rendered once and decides for itself
//!   whether it is on screen.
//! * **Repeating rows** are in [`attendees`] and are part of the one
//!   submission, each with its own rules and its own place to say what is
//!   wrong. Adding and removing one is the browser's alone: the server holds
//!   nothing about them and there is no route for a row.
//!
//! Run it with `cargo run -p signup`.

use exos::Violation;

use crate::store::{Programme, Registrations};

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

/// Seeds the programme and says how this form words a refusal.
///
/// Separate from `main` so tests can call it.
fn boot() {
    exos::provide(Programme::seed());
    exos::provide(Registrations::default());

    // exos ships no text, because an application's languages are its own. A
    // violation is a value and this is the one function that turns one into a
    // sentence; in an application with more than one language every arm here
    // would be a `messages!` call instead of a literal.
    exos::complaints(|field, violation| match (field, violation) {
        ("attendees", Violation::Required) => String::from("Add at least one attendee."),
        ("company", Violation::Required) => String::from("An invoice needs a company."),
        ("email", Violation::Malformed) => String::from("That is not an email address."),
        ("vat", Violation::Required) => String::from("An invoice needs a VAT id."),
        ("workshops", Violation::Required) => String::from("Pick at least one workshop."),
        (_, Violation::Required) => String::from("This is needed."),
        (_, Violation::TooShort { least }) => format!("At least {least} characters."),
        (_, Violation::TooLong { most }) => format!("At most {most} characters."),
        (_, _) => String::from("That does not look right."),
    });
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
