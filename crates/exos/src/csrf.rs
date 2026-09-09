//! Why a request that changes something came from this application.
//!
//! A browser attaches a viewer's cookies to a request whichever page caused
//! it, so a `POST` arriving with a session proves that the browser has one and
//! nothing at all about who asked for it. Three things stand between somebody
//! else's page and a state change here, and only the last of them is a rule
//! rather than a circumstance:
//!
//! * **`SameSite=Lax`** on the [session](crate::session) cookie, so a
//!   cross-site `POST` carries no session to begin with. It says nothing about
//!   a sibling origin on the same site, which is same-site and gets the cookie.
//! * **JSON bodies.** A cross-origin form can send three content types and
//!   `Json<T>` refuses all three. It says nothing about a handler that takes a
//!   form-encoded body, or one that takes no body at all.
//! * **This header**, which is what closes both of those gaps. A page from
//!   another origin cannot set a header on a request it makes to this one
//!   without a preflight, and a preflight needs CORS this application never
//!   turned on.
//!
//! So the runtime sends `X-Exos` on every call it makes, and every request
//! with an unsafe method has to carry it. That is one rule with no
//! configuration and nothing to derive: no token to render into a form, no
//! secret to rotate, and no per-route decision to forget.
//!
//! # What it costs
//!
//! An unsafe request that did not come from the runtime is refused, which is
//! every `curl -X POST` at an application's own routes and every `<form
//! method="post">` submitted without JavaScript. Neither is a shape exos
//! serves: an action is `name::post(..)`, recorded in Rust and sent by the
//! runtime, and a page that arrives at a cold browser is a `GET`.
//!
//! Something that genuinely has to be reachable by another client, a webhook
//! or an API for somebody else's program, is mounted beside the application
//! rather than inside it, where it is also outside the session and the scope:
//!
//! ```no_run
//! # async fn hook() {}
//! let served = axum::Router::new()
//!     .merge(axum::Router::from(exos::app()))
//!     .route("/hooks/payments", axum::routing::post(hook));
//! ```
//!
//! # Turning CORS on undoes it
//!
//! A permissive `Access-Control-Allow-Headers` is an application telling
//! browsers that another origin may send this header, which is the whole of
//! what makes it proof. Allow the origins that need it and nothing wider.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

/// What the runtime sends and what an unsafe request has to carry.
///
/// The value is never read. What cannot be forged is the header being there at
/// all, so anything in it would be a second thing to keep in step for no gain.
const HEADER: &str = "x-exos";

/// What a refusal says, since whoever sees one is holding a terminal or
/// reading a log rather than a browser: it names the header, because that is
/// the whole of what is missing.
const REFUSED: &str = "a request that changes something must carry the X-Exos header, which the \
                       exos runtime sends on every call it makes";

/// Refuses an unsafe request that does not carry [`HEADER`].
///
/// Outermost, so a refusal costs the request nothing else: no session parsed,
/// no scope opened, and none of the middleware the application mounted inside.
///
/// Safe methods pass, which is every page, every asset and the stream itself.
/// A `GET` that changes something is a defect this cannot see and neither can
/// anything else; see [RFC 9110][safe].
///
/// [safe]: https://www.rfc-editor.org/rfc/rfc9110#name-safe-methods
pub(crate) async fn layer(request: Request, next: Next) -> Response {
    if request.method().is_safe() || request.headers().contains_key(HEADER) {
        return next.run(request).await;
    }

    (StatusCode::FORBIDDEN, REFUSED).into_response()
}
