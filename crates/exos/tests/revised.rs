//! A form rendered again for what the reader has picked so far.
//!
//! The server's half: the page and the revision route ask one render, the
//! route judges nothing, and it answers with the markup and only the fields the
//! render changed. Whether a control asks, and what it keeps, is in
//! [`js/tests/runtime.test.js`](../js/tests/runtime.test.js).

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    middleware::{Next, from_fn},
    response::{IntoResponse as _, Response},
};
use exos::{Form, FormKey, Markup, Validate as _, bind, view};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower::ServiceExt as _;

#[exos::form]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Team {
    #[revises]
    tenant: String,

    #[valid(required)]
    sport: String,

    name: String,
}

/// The one tenant with a single sport has it chosen for the reader.
impl Form for Team {
    async fn render(&mut self, key: FormKey) -> Markup {
        if self.tenant == "solo" {
            self.sport = String::from("football");
        }

        let form = self.signals(key);

        view! {
            <form {&form}>
                <select {bind(&form.tenant)}></select>
                <input {bind(&form.sport)}>
                <input {bind(&form.name)}>
            </form>
        }
    }
}

async fn revise(app: exos::App, form: &str, body: String) -> (StatusCode, String) {
    let request = Request::builder()
        .method("POST")
        .header("x-exos", "true")
        .uri(format!("/_exos/revise/{form}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("a valid request");

    let response = app.oneshot(request).await.expect("the router answers");

    let status = response.status();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");

    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn solo() -> Team {
    Team {
        tenant: String::from("solo"),
        ..Team::default()
    }
}

/// The form names its element and says what a revision sends, and the one
/// control others depend on says which form it revises.
#[tokio::test]
async fn a_form_names_its_element_and_its_revising_controls() {
    let html = Team::default().markup().await;
    let state = <Team as exos::Validate>::STATE;

    assert!(html.as_str().contains(&format!("id=\"{state}\"")), "{html}");
    assert!(html.as_str().contains("data-revise="), "{html}");
    assert_eq!(
        html.as_str()
            .matches(&format!("data-bind-revise=\"{state}\""))
            .count(),
        1,
        "{html}"
    );
}

/// What the render decided is declared on the first render too, where no
/// step exists to correct it afterwards.
#[tokio::test]
async fn a_value_chosen_for_the_reader_is_declared_on_first_render() {
    let html = solo().markup().await;

    assert!(html.as_str().contains("football"), "{html}");
}

/// Only what the render changed goes back, so a field the reader edited while
/// this was out is not overwritten with what they sent.
#[tokio::test]
async fn a_revision_answers_with_the_markup_and_what_changed() {
    let state = <Team as exos::Validate>::STATE;
    let (status, body) = revise(exos::app(), state, exos::to_wire(&solo())).await;

    assert_eq!(status, StatusCode::OK, "{body}");

    let answer: Value = serde_json::from_str(&body).expect("json");
    let signals = answer["signals"].as_object().expect("signals");

    assert_eq!(signals.len(), 1, "{signals:?}");
    assert_eq!(signals.values().next(), Some(&Value::from("football")));
    assert!(answer["patch"].as_str().expect("a patch").contains(state));
}

/// A form being revised is half filled in, which is not a reason to refuse it.
#[tokio::test]
async fn a_revision_refuses_nothing() {
    let state = <Team as exos::Validate>::STATE;
    let (status, body) = revise(exos::app(), state, exos::to_wire(&Team::default())).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        !Team::default().validate().is_empty(),
        "the draft breaks a rule"
    );
}

#[tokio::test]
async fn a_form_nobody_declared_is_not_found() {
    let (status, _) = revise(exos::app(), "nothing", String::from("{}")).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A revision runs the application's render, so the application's guard
/// answers it the way it answers an action.
#[tokio::test]
async fn a_guard_refuses_a_revision() {
    async fn guard(_: Request<Body>, _: Next) -> Response {
        StatusCode::FORBIDDEN.into_response()
    }

    let app = exos::app().route_layer(from_fn(guard));
    let state = <Team as exos::Validate>::STATE;
    let (status, _) = revise(app, state, exos::to_wire(&solo())).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}
