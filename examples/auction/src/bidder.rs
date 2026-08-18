//! Who anybody is, and how the room is told.
//!
//! Three audiences, and each is here for a different reason. [`Viewer`] is one
//! signed-in person. [`Role`] is everybody doing a job,
//! which is one audience reaching several people. [`Guest`] is somebody the
//! room has never been introduced to, addressable as the name their cookie
//! carries and nothing more.
//!
//! An audience is any `Hash` type, so none of these is a string to spell and a
//! typo in one is a compile error.

use axum::extract::Path;
use exos::{Audience, Audiences, Effect, Id, Markup, on_click, view};

use crate::{
    room,
    store::{self, Account, Accounts, Bidder, Guests, Lots, Role},
};

// -----------------------------------------------------------------------------
//                                  AUDIENCES
// -----------------------------------------------------------------------------

/// One signed-in person, wherever they are and however many tabs they have.
#[derive(Hash)]
pub(crate) struct Viewer(pub(crate) u32);

impl Audience for Viewer {
    const NAME: &'static str = "viewer";
}

/// One visitor with no account, addressed by the name in their cookie.
#[derive(Hash)]
pub(crate) struct Guest(pub(crate) Id);

impl Audience for Guest {
    const NAME: &'static str = "guest";
}

/// Everybody here to do the same job.
impl Audience for Role {
    const NAME: &'static str = "role";
}

/// Teaches exos what a session name stands for.
///
/// This runs once per connection, on the stream's `GET`, which carries the
/// cookie because an `EventSource` is opened with an ordinary request. Both
/// arms earn their place: the first is one person and their job at once, and
/// the second is why the name arrives as an `Option` rather than exos deciding
/// on the application's behalf that an anonymous visitor is nobody.
pub(crate) fn identify() {
    exos::identify(async |name: Option<Id>| {
        // No name at all, which is a browser whose very first request is the
        // stream. There is nothing to key an audience on, and no cookie can be
        // set from here to make one.
        let Some(name) = name else {
            return Ok(Audiences::none());
        };

        Ok(match exos::data::<Accounts>().of(&name).await {
            Some(account) => Audiences::of(&Viewer(account.id)).and(&account.role),
            // A name with nobody behind it is still a name. That is the whole
            // reason a guest can be outbid and hear about it.
            None => Audiences::of(&Guest(name)),
        })
    });
}

// -----------------------------------------------------------------------------
//                              WHO IS ASKING
// -----------------------------------------------------------------------------

/// Who is making this request.
///
/// `start` rather than `id`, because bidding is the point at which the room
/// has to be able to call you something. In practice the page already named
/// this browser, so this hands back the name the cookie carried.
pub(crate) async fn current() -> Bidder {
    let name = exos::session().start();

    match exos::data::<Accounts>().of(&name).await {
        Some(account) => Bidder::Viewer(account.id),
        None => Bidder::Guest(name),
    }
}

/// What whoever is asking is here to do.
///
/// A visitor with no account is here to bid, which is what lets a guest use
/// the room without the example having a third kind of person in it.
pub(crate) async fn role() -> Role {
    let Some(name) = exos::session().id() else {
        return Role::Bidder;
    };

    exos::data::<Accounts>()
        .of(&name)
        .await
        .map_or(Role::Bidder, |account| account.role)
}

/// Whether whoever is asking runs the sale.
///
/// exos has no permission model and this is what that means in practice: a
/// check at the call site, in ordinary Rust, where it can be read.
pub(crate) async fn is_staff() -> bool {
    role().await == Role::Staff
}

/// What the room calls somebody.
///
/// Safe to call while rendering a fragment, because it reads application data
/// and never the request: a fragment renders again from whatever publishes it,
/// where there is no request to read.
pub(crate) fn describe(bidder: &Bidder) -> String {
    match bidder {
        Bidder::Viewer(id) => exos::data::<Accounts>()
            .account(*id)
            .map_or_else(|| String::from("somebody"), |account| account.name),
        Bidder::Guest(name) => exos::data::<Guests>().number(name).map_or_else(
            || String::from("a guest"),
            |number| format!("Guest {number}"),
        ),
    }
}

// -----------------------------------------------------------------------------
//                                THE NAME BAR
// -----------------------------------------------------------------------------

/// Who you are, and how to be somebody else.
///
/// Naming the browser happens here rather than at the first bid, and the order
/// is the reason. The stream is opened by the document this markup is in, so
/// it carries whatever cookie this response sets. A visitor named after that
/// would hold a connection that is nobody until they reloaded, and every
/// directed effect aimed at them would miss.
pub(crate) async fn bar() -> Markup {
    let name = exos::session().start();

    match exos::data::<Accounts>().of(&name).await {
        Some(account) => known(&account),
        None => unknown(exos::data::<Guests>().issue(&name)),
    }
}

/// The bar for somebody with an account.
fn known(account: &Account) -> Markup {
    view! {
        <div class="who">
            <span class="who-name">
                "Bidding as "<strong>{ &account.name }</strong>
                { badge(account.role) }
            </span>

            <button class="link" type="button" {on_click(|_| leave::post())}>
                "Leave the room"
            </button>
        </div>
    }
}

/// The bar for somebody the room has not been introduced to.
fn unknown(number: u32) -> Markup {
    let people = exos::data::<Accounts>();

    view! {
        <div class="who">
            <span class="who-name">
                "Bidding as "<strong>{ format!("Guest {number}") }</strong>
                <span class="hint">
                    ", who can bid and will be told when they are outbid"
                </span>
            </span>

            <span class="claim">
                "Login as: "
                {
                    people
                        .everybody()
                        .iter()
                        .map(|account| {
                            let id = account.id;

                            view! {
                                <button
                                    class="link"
                                    type="button"
                                    {on_click(move |_| claim::post(id))}
                                >{ &account.name }</button>
                            }
                        })
                        .collect::<Vec<_>>()
                }
            </span>
        </div>
    }
}

/// The badge next to a name, for the job that has one.
fn badge(role: Role) -> Markup {
    match role {
        Role::Bidder => Markup::default(),
        Role::Staff => view! { <span class="badge">"staff"</span> },
    }
}

// -----------------------------------------------------------------------------
//                              BECOMING SOMEBODY
// -----------------------------------------------------------------------------

/// Signs in, which is a rotation and then whatever the application keeps.
#[exos::post("/bidders/{id}/claim")]
async fn claim(Path(id): Path<u32>) -> Effect {
    let accounts = exos::data::<Accounts>();

    let Some(account) = accounts.account(id) else {
        return Effect::none();
    };

    let session = exos::session();

    // Read before rotating, because rotating replaces it. Whatever this
    // browser bid as a guest is recorded under the old name, and deciding what
    // becomes of it is the application's business rather than exos's.
    let previous = session.id();
    let name = session.rotate();

    accounts.claim(&name, id).await;

    if let Some(previous) = previous {
        // Bids follow the person to their account, unless the account is one
        // that does not bid. Carrying them into the rostrum would be a way
        // around the refusal in `bid`, so taking the rostrum gives them up.
        let touched = exos::data::<Lots>().update(|lots| match account.role {
            Role::Staff => store::withdraw(lots, &previous),
            Role::Bidder => store::claim(lots, &previous, id),
        });

        for lot in touched {
            room::publish_lot(lot);
        }

        accounts.forget(&previous).await;
    }

    // This tab, and only this tab. See the note below on the ones it cannot
    // reach.
    Effect::reload()
}

// Signing in changes the cookie for the whole browser, and only the tab that
// asked can be answered. The others go on showing the old name, and their
// streams go on carrying the old audience, because a stream resolves its
// identity when it opens and never again.
//
// Pushing them a reload looks like the answer and is not. It would go out
// while the response carrying the new cookie is still being written, so a tab
// that acted on it at once would reload with the *old* cookie, come back as
// the guest it used to be, and never be prompted again. That is a coin flip,
// and a coin flip is worse than a known limitation.
//
// Nor would a `reconnect` step help, though the roadmap rightly wants one for
// other reasons: a reopened stream carries whatever cookie the jar holds at
// that instant, which is the same race in a different coat.
//
// What would fix it is server-side and needs no client involvement at all. At
// a rotation exos knows both names, so it could end every connection opened
// under the old one and let the browser reopen them. See the roadmap.

/// Signs out. Taking the cookie back is exos's half; forgetting what was kept
/// under the name is this application's, and it has to happen, because a name
/// the browser stopped sending is not a name nobody else has.
#[exos::post("/session/end")]
async fn leave() -> Effect {
    let session = exos::session();

    if let Some(name) = session.id() {
        exos::data::<Accounts>().forget(&name).await;
    }

    session.end();

    Effect::reload()
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
    use axum::http::StatusCode;

    use super::Lots;
    use crate::tests::{as_guest, body, claimed, request, set_cookie};

    #[tokio::test]
    async fn a_visit_is_named_before_the_stream_could_open() {
        let response = request("GET", "/", None).await;

        // The cookie rides out on the document, so the `EventSource` the
        // runtime opens a moment later already carries it.
        assert!(
            set_cookie(&response)
                .expect("the page names the browser")
                .starts_with("exos=")
        );

        // Which number is whichever these tests have handed out so far, since
        // the room is global and they run in parallel. That there is one is
        // the thing being claimed.
        assert!(body(response).await.contains("Bidding as <strong>Guest "));
    }

    #[tokio::test]
    async fn a_name_the_room_knows_is_shown_by_its_own_name() {
        let session = claimed(1).await;
        let html = body(request("GET", "/", Some(&session)).await).await;

        assert!(
            html.contains("Bidding as <strong>Ada</strong>"),
            "{html:.600}"
        );
        assert!(!html.contains("Login as"));
    }

    /// The fixation defence, and the reason signing in answers with a reload:
    /// the name that opened the stream is not the name that leaves this
    /// handler, so the connection behind it has to go.
    #[tokio::test]
    async fn claiming_an_account_replaces_the_name_and_reloads() {
        let guest = as_guest().await;

        let response = request("POST", "/bidders/1/claim", Some(&guest)).await;
        assert_eq!(response.status(), StatusCode::OK);

        let cookie = set_cookie(&response).expect("a new name");
        assert!(!cookie.contains(&guest), "the guest's name is gone");

        assert!(body(response).await.contains("event: reload"));
    }

    #[tokio::test]
    async fn leaving_takes_the_cookie_back() {
        let session = claimed(1).await;

        let response = request("POST", "/session/end", Some(&session)).await;
        let cookie = set_cookie(&response).expect("the cookie comes back");

        assert!(cookie.contains("Max-Age=0"));

        let after = body(request("GET", "/", Some(&session)).await).await;
        assert!(after.contains("Login as"), "and it means nobody");
    }

    /// The rule the bid handler enforces, closed at the other end. Carrying a
    /// guest's bids into the rostrum would let the auctioneer win a lot they
    /// were never allowed to bid on, which is exactly what used to happen.
    #[tokio::test]
    async fn taking_the_rostrum_gives_up_what_you_bid_on_the_way_in() {
        const LOT: u32 = 3;

        let guest = as_guest().await;
        let before = exos::data::<Lots>().one(LOT).expect("the lot").price();

        request("POST", &format!("/lots/{LOT}/bid"), Some(&guest)).await;
        assert_ne!(
            exos::data::<Lots>().one(LOT).expect("the lot").price(),
            before,
            "the guest really was leading it"
        );

        request("POST", "/bidders/3/claim", Some(&guest)).await;

        let after = exos::data::<Lots>().one(LOT).expect("the lot");

        assert_eq!(after.price(), before);
        assert_eq!(after.leader(), None, "and the auctioneer wins nothing");
    }

    /// Two people, one room, two different pages: the role reaches the markup
    /// through the topic, so each is served the lot they can act on and never
    /// the other one.
    #[tokio::test]
    async fn the_auctioneer_gets_a_different_room_from_everybody_else() {
        let staff = claimed(3).await;
        let html = body(request("GET", "/", Some(&staff)).await).await;

        assert!(html.contains("staff"), "the badge on the name");
        assert!(html.contains("The rostrum"));
        assert!(html.contains("Bring the hammer down"));
        assert!(!html.contains("Bid £"), "and no way to bid at all");

        let bidder = claimed(1).await;
        let html = body(request("GET", "/", Some(&bidder)).await).await;

        assert!(html.contains("Bid £"));
        assert!(!html.contains("The rostrum"));
        assert!(!html.contains("Bring the hammer down"));
    }
}
