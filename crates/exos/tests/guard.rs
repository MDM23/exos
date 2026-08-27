//! Middleware an application puts on [`exos::app`], and where that leaves it.
//!
//! Ordinary axum middleware. What matters is that it runs inside exos's own
//! layers rather than above them: it asks [`exos::session`] like anything else
//! being served, instead of parsing a cookie of its own.

use axum::{
    body::Body,
    extract::Request,
    http::{StatusCode, header},
    middleware::{Next, from_fn},
    response::{IntoResponse as _, Redirect, Response},
};
use exos::{Markup, view};
use tower::ServiceExt as _;

/// Whoever the guard resolved the session name to. A handler could be handed
/// this as an extension; a view could not, which is why it goes in the scope.
#[derive(Debug)]
struct Viewer(String);

/// The application's whole session guard, which is what one is meant to look
/// like: resolve who this is, refuse if it is nobody.
async fn guard(request: Request, next: Next) -> Response {
    if request.uri().path() == "/login" {
        return next.run(request).await;
    }

    let Some(id) = exos::session().id() else {
        return Redirect::to("/login").into_response();
    };

    exos::scope().set(Viewer(id.to_string()));

    next.run(request).await
}

#[exos::get("/behind")]
async fn behind() -> Markup {
    view! { <p>{ named() }</p> }
}

/// Not a handler, and therefore unable to extract anything. Reading the guard's
/// work out of the scope is the whole reason the guard runs inside it.
fn named() -> String {
    exos::scope()
        .get::<Viewer>()
        .map_or_else(|| String::from("anonymous"), |viewer| viewer.0.clone())
}

#[exos::get("/login")]
async fn login() -> Markup {
    view! { <h1>"Sign in"</h1> }
}

/// A name shaped the way exos mints them, since the guard rejects anything
/// else before the application ever sees it.
const NAME: &str = "0123456789abcdef0123456789abcdef";

async fn request(uri: &str, session: Option<&str>) -> Response {
    let mut builder = Request::builder().uri(uri);

    if let Some(session) = session {
        builder = builder.header(header::COOKIE, format!("exos={session}"));
    }

    // `route_layer` rather than `layer`: a guard answers early, and one that
    // ran for a request matching no route would report every mistyped URL as
    // somewhere to sign in.
    exos::app()
        .route_layer(from_fn(guard))
        .oneshot(builder.body(Body::empty()).expect("a valid request"))
        .await
        .expect("the router answers")
}

async fn body(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    String::from_utf8(bytes.to_vec()).expect("the markup is text")
}

#[tokio::test]
async fn a_guard_reads_the_session_the_request_carried() {
    let response = request("/behind", Some(NAME)).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(body(response).await.contains(NAME));
}

#[tokio::test]
async fn a_request_the_guard_refuses_never_reaches_the_handler() {
    let response = request("/behind", None).await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .expect("a redirect names where to"),
        "/login"
    );
}

/// The guard decides, so a page it lets through anonymously is served
/// anonymously.
#[tokio::test]
async fn what_the_guard_lets_past_is_served() {
    let response = request("/login", None).await;

    assert_eq!(response.status(), StatusCode::OK);
}

/// The runtime is fetched by a `<script>` that cannot follow a redirect into
/// an HTML page, so a guard reaching exos's own routes would break every page
/// it was protecting.
#[tokio::test]
async fn exos_serves_its_own_routes_from_outside_the_guard() {
    let response = request(&exos::runtime(), None).await;

    assert_eq!(response.status(), StatusCode::OK);
}

/// A guard answers early, and one that ran for a request matching no route at
/// all would report every mistyped URL as somewhere to sign in.
#[tokio::test]
async fn a_path_no_route_claims_is_not_found_rather_than_refused() {
    let response = request("/nope", None).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
