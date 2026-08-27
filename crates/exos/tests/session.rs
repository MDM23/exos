//! Sessions through the whole stack, and the division of labour they are for.
//!
//! exos names the browser and carries the name in a cookie. Everything below
//! called `Sessions` is the application's: a table it owns, keyed by the name,
//! holding whatever it decided a session means. That is the whole arrangement,
//! and this file is the worked example of it as much as it is the test.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use exos::{Id, data};
use tower::ServiceExt as _;

/// What an application keeps under a session name. A `HashMap` here, a table
/// with an index and an expiry job in anything real.
#[derive(Default)]
struct Sessions(Mutex<HashMap<Id, u32>>);

impl Sessions {
    fn bind(&self, id: &Id, user: u32) {
        self.lock().insert(id.clone(), user);
    }

    fn viewer(&self, id: &Id) -> Option<u32> {
        self.lock().get(id).copied()
    }

    fn forget(&self, id: &Id) {
        self.lock().remove(id);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Id, u32>> {
        self.0.lock().expect("the lock is not poisoned")
    }
}

/// Resolving the name is the application's step, and it happens in a handler,
/// where awaiting a database would be legal.
fn viewer() -> Option<u32> {
    let id = exos::session().id()?;

    data::<Sessions>().viewer(&id)
}

#[exos::get("/session/who")]
async fn who() -> String {
    viewer().map_or_else(|| String::from("anonymous"), |user| user.to_string())
}

#[exos::post("/session/sign-in")]
async fn sign_in() -> StatusCode {
    let session = exos::session();
    let sessions = data::<Sessions>();

    // Read before the rotation, because it replaces the name an anonymous
    // visit may have left something under.
    let previous = session.id();
    let id = session.rotate();

    sessions.bind(&id, 7);

    if let Some(previous) = previous {
        sessions.forget(&previous);
    }

    StatusCode::NO_CONTENT
}

#[exos::post("/session/sign-out")]
async fn sign_out() -> StatusCode {
    let session = exos::session();

    if let Some(id) = session.id() {
        data::<Sessions>().forget(&id);
    }

    session.end();

    StatusCode::NO_CONTENT
}

/// A session with nobody behind it, which is what `start` is for.
#[exos::post("/cart/add")]
async fn add_to_cart() -> String {
    exos::session().start().to_string()
}

/// One request, with the cookie a browser holding `session` would send.
async fn request(method: &str, uri: &str, session: Option<&str>) -> Response {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(session) = session {
        builder = builder.header(header::COOKIE, format!("theme=dark; exos={session}"));
    }

    exos::app()
        .provide(Sessions::default())
        .oneshot(builder.body(Body::empty()).expect("a valid request"))
        .await
        .expect("the router answers")
}

/// What the response tells the browser to keep, if anything.
fn set_cookie(response: &Response) -> Option<&str> {
    response
        .headers()
        .get(header::SET_COOKIE)?
        .to_str()
        .ok()
        .filter(|cookie| cookie.starts_with("exos="))
}

/// The name out of a `Set-Cookie`, which is what a browser would send back and
/// therefore what the next request here carries.
fn named(response: &Response) -> String {
    set_cookie(response)
        .expect("the response names a session")
        .trim_start_matches("exos=")
        .split(';')
        .next()
        .expect("a cookie has a value")
        .to_owned()
}

async fn body(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
}

/// Nothing asked for a name, so nothing is written and a crawler is charged for
/// none of it.
#[tokio::test]
async fn a_visit_that_asks_for_nothing_is_told_nothing() {
    let response = request("GET", "/session/who", None).await;

    assert!(set_cookie(&response).is_none());
    assert_eq!(body(response).await, "anonymous");
}

/// The point of the whole arrangement: exos carries the name, the application
/// carries the meaning, and the two meet again on the next request.
#[tokio::test]
async fn a_name_carries_what_the_application_put_under_it() {
    let signed_in = request("POST", "/session/sign-in", None).await;
    assert_eq!(signed_in.status(), StatusCode::NO_CONTENT);

    let session = named(&signed_in);
    let response = request("GET", "/session/who", Some(&session)).await;

    assert_eq!(body(response).await, "7");
}

#[tokio::test]
async fn a_name_the_browser_already_has_is_not_sent_again() {
    let session = named(&request("POST", "/session/sign-in", None).await);
    let response = request("GET", "/session/who", Some(&session)).await;

    assert!(set_cookie(&response).is_none());
}

/// The fixation defence, end to end: whatever named the session before the
/// privilege change does not name it after, and is not a way back in.
#[tokio::test]
async fn signing_in_replaces_the_name_the_session_had() {
    let planted = named(&request("POST", "/session/sign-in", None).await);
    let signed_in = request("POST", "/session/sign-in", Some(&planted)).await;

    assert_ne!(named(&signed_in), planted);

    let stale = request("GET", "/session/who", Some(&planted)).await;
    assert_eq!(body(stale).await, "anonymous");
}

#[tokio::test]
async fn signing_out_takes_the_cookie_back() {
    let session = named(&request("POST", "/session/sign-in", None).await);

    let signed_out = request("POST", "/session/sign-out", Some(&session)).await;
    assert!(
        set_cookie(&signed_out)
            .expect("the response takes the cookie back")
            .contains("Max-Age=0")
    );

    let after = request("GET", "/session/who", Some(&session)).await;
    assert_eq!(body(after).await, "anonymous");
}

/// An anonymous session is a name with nothing behind it, which is a shopping
/// cart before anybody has signed in.
#[tokio::test]
async fn a_session_can_be_started_without_a_viewer() {
    let started = request("POST", "/cart/add", None).await;
    let name = named(&started);

    assert_eq!(
        body(started).await,
        name,
        "the handler is given the name that was minted"
    );

    let response = request("GET", "/session/who", Some(&name)).await;
    assert!(
        set_cookie(&response).is_none(),
        "the browser already has it"
    );

    assert_eq!(body(response).await, "anonymous", "and it means nobody yet");
}

/// exos never expires a session, because it has no idea when the application's
/// record does. A cookie running out first would sign somebody out early.
#[tokio::test]
async fn the_cookie_is_locked_down_and_outlasts_any_record() {
    let response = request("POST", "/session/sign-in", None).await;
    let cookie = set_cookie(&response).expect("the response names a session");

    for attribute in ["HttpOnly", "Path=/", "SameSite=Lax", "Secure"] {
        assert!(cookie.contains(attribute), "{cookie}");
    }

    assert!(cookie.contains("Max-Age=34560000"), "{cookie}");
}

/// A name the application has never heard of is anonymous, and costs it one
/// lookup. exos does not know the difference and has nothing to say about it.
#[tokio::test]
async fn a_name_nothing_is_stored_under_is_simply_anonymous() {
    let response = request("GET", "/session/who", Some(&"a".repeat(32))).await;

    assert_eq!(body(response).await, "anonymous");
}

#[tokio::test]
async fn a_cookie_that_is_not_shaped_like_a_name_is_not_one() {
    let response = request("GET", "/session/who", Some("../../etc/passwd")).await;

    assert_eq!(body(response).await, "anonymous");
}
