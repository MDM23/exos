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
mod tests {
    use exos::Id;
    use exos_test::Browser;

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

    /// A browser that has never been here.
    pub(crate) fn visitor() -> Browser {
        Browser::new(app())
    }

    /// The document served at `uri`, for a test with no browser of its own.
    pub(crate) async fn get(uri: &str) -> String {
        visitor().get(uri).await.body().to_owned()
    }

    /// A browser that has been in the room and has no account.
    ///
    /// It has a name because the room named it, which is what a visit does
    /// before the stream it will open could carry one.
    pub(crate) async fn as_guest() -> Browser {
        let mut guest = visitor();
        drop(guest.get("/").await);

        guest
    }

    /// A browser the room already knows as `account`.
    ///
    /// The name is bound here rather than by signing in, so that a test about
    /// anything else starts on the far side of that. Which is also what a
    /// browser arriving with a session an earlier visit left it looks like.
    pub(crate) async fn claimed(account: u32) -> Browser {
        seeded();

        let name = Id::random();
        exos::data::<Accounts>().claim(&name, account).await;

        visitor().with_cookie("exos", &name.to_string())
    }
}
