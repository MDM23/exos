//! A rule only the server can answer, asked twice from one declaration.
//!
//! `checked_by` names a function, and the same function answers the route a
//! control asks while somebody is typing and the extractor that runs before a
//! handler does. This is the server's half of both: that one declaration mounts
//! the route, that what it says is the application's own message, and that
//! neither half asks about a value there is nothing to ask about.
//!
//! Its other half is in
//! [`js/tests/runtime.test.js`](../js/tests/runtime.test.js), which is where
//! whether the control asks at all can be checked.

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use exos::{Effect, Model, Validate as _, view};
use serde::{Deserialize, Serialize};
use tower::ServiceExt as _;

#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Order {
    #[valid(required, checked_by = coupon)]
    code: String,

    /// Checked by a function no test below gives a value it would reach.
    ///
    /// What it is here to prove is the two guards: a value the shape rules have
    /// already refused is not asked about, and neither is one that is absent.
    #[valid(length = 2..=4, checked_by = never)]
    size: String,
}

/// The rule that has to be a round trip: nothing about the codes reaches the
/// browser, so the message is the application's rather than a [`Violation`].
async fn coupon(code: String) -> Result<(), String> {
    match code.trim() == "EARLYBIRD" {
        true => Ok(()),
        false => Err(String::from("That code is not one of ours.")),
    }
}

async fn never(size: String) -> Result<(), String> {
    unreachable!("nothing asks about `{size}`")
}

#[exos::post("/orders")]
async fn place(Model(order): Model<Order>) -> Effect {
    Effect::patch(view! { <li id="orders">{ &order.code }</li> })
}

/// What a control asks while the field is being edited.
async fn check(model: &str, field: &str, value: &str) -> (StatusCode, String) {
    answered(
        Request::builder()
            .method("POST")
            .header("x-exos", "true")
            .uri(format!("/_exos/check/{model}/{field}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!("\"{value}\"")))
            .expect("a valid request"),
    )
    .await
}

/// And what the submission carrying it answers.
async fn place_order(order: &Order) -> (StatusCode, String) {
    answered(
        Request::builder()
            .method("POST")
            .header("x-exos", "true")
            .uri("/orders")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(exos::to_wire(order)))
            .expect("a valid request"),
    )
    .await
}

async fn answered(request: Request<Body>) -> (StatusCode, String) {
    let response = exos::app()
        .oneshot(request)
        .await
        .expect("the router answers");

    let status = response.status();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    (status, String::from_utf8(bytes.to_vec()).expect("UTF-8"))
}

/// The pair the binding carries: the model that answers, and the field.
fn addressed() -> (&'static str, String) {
    (Order::STATE, Order::signals().code.name().to_owned())
}

#[tokio::test]
async fn a_checked_field_answers_with_the_message_its_function_wrote() {
    let (model, field) = addressed();

    assert_eq!(
        check(model, &field, "nope").await,
        (
            StatusCode::OK,
            String::from("That code is not one of ours.")
        )
    );
}

/// Nothing rather than a message, because the control writes its own slot and
/// an empty answer is what leaves it alone.
#[tokio::test]
async fn a_value_the_function_accepts_says_nothing() {
    let (model, field) = addressed();

    assert_eq!(
        check(model, &field, "EARLYBIRD").await,
        (StatusCode::OK, String::new())
    );
}

/// The guard the extractor applies as well: nothing asks the application
/// whether an empty string is one of its codes.
#[tokio::test]
async fn an_absent_value_is_not_asked_about() {
    let (model, _) = addressed();
    let size = Order::signals().size.name().to_owned();

    assert_eq!(
        check(model, &size, "   ").await,
        (StatusCode::OK, String::new())
    );
}

/// One route for every checked field, so a field that declares no rule is a
/// miss rather than a handler somebody can reach.
#[tokio::test]
async fn a_field_nothing_declares_a_rule_for_is_not_a_check() {
    let (model, _) = addressed();

    assert_eq!(check(model, "s0", "nope").await.0, StatusCode::NOT_FOUND);
    assert_eq!(check("s0", "s0", "nope").await.0, StatusCode::NOT_FOUND);
}

/// The client's copy is feedback. What decides is the same function, run again
/// where no handler can decline to.
#[tokio::test]
async fn the_same_function_refuses_the_submission() {
    let (status, body) = place_order(&Order {
        code: String::from("nope"),
        size: String::new(),
    })
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("That code is not one of ours."), "{body}");
}

#[tokio::test]
async fn a_value_it_accepts_reaches_the_handler() {
    let (status, body) = place_order(&Order {
        code: String::from("EARLYBIRD"),
        size: String::new(),
    })
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("EARLYBIRD"), "{body}");
}

/// And a field the shape rules have already refused is not asked about, which
/// `never` is what proves: the length is wrong, so the round trip that would
/// panic is one nothing makes.
#[tokio::test]
async fn a_value_the_shape_rules_refused_is_not_asked_about() {
    let (status, body) = place_order(&Order {
        code: String::from("EARLYBIRD"),
        size: String::from("XXXXL"),
    })
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("At most 4 characters."), "{body}");
}
