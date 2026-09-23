//! A registration form, and what a form costs.
//!
//! The other three examples are about state that changes under you. This one
//! is about a form. It was written by hand first, against what exos had before
//! any of the forms roadmap, so that the stages could be designed against a
//! cost that was visible rather than argued about; each one that has since
//! landed took something back out of it.
//!
//! Six things it does:
//!
//! * **A shape rule** is declared on the model in [`form`], and that one
//!   declaration answers on both sides: the extractor refuses a body that
//!   breaks it, and the control asks it again while it is being typed.
//! * **A message** goes into one record per model, keyed by field, whichever
//!   side decided what is in it.
//! * **The submit button** reads that record whole, so it answers for the rows
//!   and for what the server said as much as for a rule a control checked
//!   itself.
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

use axum::Router;
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
    let listener = tokio::net::TcpListener::bind(ADDRESS).await?;
    println!("listening on http://{ADDRESS}");

    axum::serve(listener, app()).await
}

/// The application: the programme it holds, and how it words a refusal.
///
/// One for the process, and the tests serve this same one.
fn app() -> Router {
    exos::app()
        .provide(Programme::seed())
        .provide(Registrations::default())
        // exos ships no text, because an application's languages are its own.
        // A violation is a value and this is the one function that turns one
        // into a sentence; in an application with more than one language every
        // arm here would be a `messages!` call instead of a literal.
        .complaints(|field, violation| match (field, violation) {
            ("attendees", Violation::Required) => String::from("Add at least one attendee."),
            ("company", Violation::Required) => String::from("An invoice needs a company."),
            ("email", Violation::Malformed) => String::from("That is not an email address."),
            ("vat", Violation::Required) => String::from("An invoice needs a VAT id."),
            // The pattern's name is what makes this sayable. Without it every
            // pattern on the form shares one sentence about the format.
            (_, Violation::Unmatched { pattern: "VAT" }) => {
                String::from("A VAT id is a country code and up to twelve more characters.")
            }
            ("workshops", Violation::Required) => String::from("Pick at least one workshop."),
            (_, Violation::Required) => String::from("This is needed."),
            (_, Violation::TooShort { least }) => format!("At least {least} characters."),
            (_, Violation::TooLong { most }) => format!("At most {most} characters."),
            (_, _) => String::from("That does not look right."),
        })
        .into()
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
    };
    use exos::ModelFields;
    use exos_test::Browser;
    use serde::Serialize;

    pub(crate) use super::app;

    /// The application, for a test that reads the programme rather than
    /// serving it.
    pub(crate) fn seed() {
        drop(app());
    }

    /// The document served at `uri`.
    pub(crate) async fn get(uri: &str) -> String {
        let answer = Browser::new(app()).get(uri).await;

        assert_eq!(answer.status(), StatusCode::OK);
        answer.body().to_owned()
    }

    /// What a control is told about one value while it is being edited.
    ///
    /// The pair in the URL is what the binding carries: the model that answers
    /// for the field, and the field. A message or nothing, as text, because a
    /// check is about one value rather than about the form.
    ///
    /// Sent rather than posted: the body is one value, which is what the
    /// binding puts on the wire, rather than a model.
    pub(crate) async fn check(model: &str, field: &str, value: &str) -> String {
        let answer = Browser::new(app())
            .send(
                Request::builder()
                    .method("POST")
                    .uri(format!("/_exos/check/{model}/{field}"))
                    .header("content-type", "application/json")
                    .body(Body::from(format!("\"{value}\"")))
                    .expect("a valid request"),
            )
            .await;

        assert_eq!(answer.status(), StatusCode::OK);
        answer.body().to_owned()
    }

    /// What an action answered, refusal or not, still framed.
    ///
    /// This example's tests are about the messages in a refusal rather than
    /// about which steps carry them, so they read the frames as text. The
    /// status is not asserted on the way past: a refusal is half of what this
    /// example is about, and it arrives with the status it deserves.
    pub(crate) async fn post<T: ModelFields + Serialize>(uri: &str, model: &T) -> String {
        let answer = Browser::new(app()).post(uri, model).await;

        assert!(answer.is_effect(), "an action answers with an effect");
        answer.body().to_owned()
    }
}
