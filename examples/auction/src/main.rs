//! A live auction room: what a directed effect is for.
//!
//! The other two examples are about one person and their screen. This one is
//! about who anybody is, and it exists because two things that look alike are
//! not:
//!
//! * **A price is state.** The server holds it, a [live
//!   fragment](room::lot) renders it, and a publish replaces it in every tab
//!   watching that lot. Reload and it is still there.
//! * **Being outbid is an event.** It happened once, to one person, and no
//!   fragment's re-render produces it. It is [sent](toast::tell) to them
//!   wherever they are, including a page showing no lots at all. Reload and it
//!   is gone, because it never was state.
//!
//! Who anybody is comes from the session name their cookie carries, resolved
//! once when their stream opens; [`bidder`] is the whole of that. Somebody who
//! has not claimed an account is still addressable, as the name itself, which
//! is why a guest can be outbid and hear about it.
//!
//! Run it with `cargo run -p auction`, then:
//!
//! 1. Open <http://localhost:3000> in **two ordinary tabs**. Both are the same
//!    browser and therefore the same guest, and a message lands in both.
//! 2. Open it again in a **private window**. That is a second cookie jar and
//!    therefore somebody else.
//! 3. Bid from the private window on a lot the first guest leads. The price
//!    moves everywhere; the message arrives only in the first two tabs.
//! 4. Navigate one of them to the catalogue and do it again. The message still
//!    finds it, on a page subscribed to nothing.
//! 5. Reload. The price stayed, the message did not.
//!
//! Nothing closes on a timer. A lot stands until whoever is signed in as the
//! auctioneer brings the hammer down, which is what makes the moment legible:
//! it happened because somebody did it. The winner still asked for nothing,
//! which is what keeps their message a directed effect rather than a reply.

mod bidder;
mod page;
mod room;
mod store;
mod toast;

use axum::Router;

use crate::store::{Accounts, Guests, Lots};

/// The address the example listens on.
const ADDRESS: &str = "127.0.0.1:3000";

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(ADDRESS).await?;
    println!("listening on http://{ADDRESS}");

    axum::serve(listener, app()).await
}

/// The application: the room it holds, and who anybody in it is.
///
/// The seeds are what it starts with rather than what every call to this puts
/// back, so the tests build one per request and the room they change persists
/// between them.
fn app() -> Router {
    exos::app()
        .provide(Lots::seed())
        .provide(Accounts::seed())
        .provide(Guests::default())
        .identify(bidder::audiences)
        .into()
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, header},
        response::Response,
    };
    use exos::Id;
    use tower::ServiceExt as _;

    use super::*;

    /// The application, for a test that reads the room rather than serving it.
    ///
    /// The data is global, so these tests only ever read the lots through the
    /// router. Every operation that changes one is a free function over a slice
    /// and is tested in [`store`] against a local `Vec`, which keeps those
    /// tests independent of each other and of their order.
    pub(crate) fn seeded() {
        drop(app());
    }

    /// One request, with the cookie a browser holding `session` would send.
    pub(crate) async fn request(method: &str, uri: &str, session: Option<&str>) -> Response {
        let mut builder = Request::builder().method(method).uri(uri);

        if let Some(session) = session {
            builder = builder.header(header::COOKIE, format!("exos={session}"));
        }

        app()
            .oneshot(builder.body(Body::empty()).expect("a valid request"))
            .await
            .expect("the router answers")
    }

    pub(crate) async fn body(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body is readable");

        String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
    }

    /// What the response tells the browser to keep, if anything.
    pub(crate) fn set_cookie(response: &Response) -> Option<&str> {
        response
            .headers()
            .get(header::SET_COOKIE)?
            .to_str()
            .ok()
            .filter(|cookie| cookie.starts_with("exos="))
    }

    /// The name out of a `Set-Cookie`, which is what a browser would send back
    /// and therefore what the next request here carries.
    fn named(response: &Response) -> String {
        set_cookie(response)
            .expect("the response names a session")
            .trim_start_matches("exos=")
            .split(';')
            .next()
            .expect("a cookie has a value")
            .to_owned()
    }

    /// A browser that has been in the room and has no account.
    pub(crate) async fn as_guest() -> String {
        named(&request("GET", "/", None).await)
    }

    /// A browser the room already knows as `account`.
    pub(crate) async fn claimed(account: u32) -> String {
        seeded();

        let name = Id::random();
        exos::data::<Accounts>().claim(&name, account).await;

        name.to_string()
    }
}
