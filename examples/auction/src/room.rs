//! The lots, the bidding, and the hammer.
//!
//! One live fragment per lot and role, so a bid on one lot patches one lot,
//! and the auctioneer's copy of it carries a different button from everybody
//! else's.
//!
//! The role is *in the topic* rather than read from the session, which is the
//! escape hatch the guide names for content that depends on who is looking. A
//! topic still completely determines its content: `lot(1, Staff)` means one
//! thing, `lot(1, Bidder)` means another, and nobody is ever served both.
//! Reading the session inside the fragment would break that, and the framework
//! makes it impossible anyway, because a fragment renders detached.
//!
//! What does *not* go in the topic is the viewer. "You are winning" would be a
//! fragment per person, and it is a directed effect instead: two readers would
//! otherwise share a topic and receive each other's content, and the room's
//! standing line says who leads by name so that everybody can read the same
//! words.

use axum::extract::Path;
use exos::{Effect, Flag, Markup, on_click, publish, send, view};

use crate::{
    bidder,
    store::{self, Lot, Lots, Role},
    toast::{self, Note},
};

// -----------------------------------------------------------------------------
//                                   THE LOTS
// -----------------------------------------------------------------------------

/// One lot, as one kind of person sees it, following the server on its own.
#[exos::live]
pub(crate) fn lot(id: u32, role: Role) -> Markup {
    let Some(lot) = exos::data::<Lots>().one(id) else {
        // A topic that names nothing renders nothing, rather than the example
        // deciding that a missing lot is a crash.
        return Markup::default();
    };

    view! {
        <article class="lot" data-closed={ lot.closed }>
            <h2 class="lot-title">{ &lot.title }</h2>

            <p class="price">{ format!("£{}", lot.price()) }</p>
            <p class="standing">{ standing(&lot) }</p>

            { control(&lot, role) }
        </article>
    }
}

/// The one thing this reader can do about the lot.
///
/// The auctioneer is not offered a bid, because they do not bid. This decides
/// what is on the page; the handler decides what is allowed, and it checks
/// again. A button that is not rendered is a courtesy, not a permission.
fn control(lot: &Lot, role: Role) -> Markup {
    let id = lot.id;
    let shut = Flag(lot.closed);

    match role {
        Role::Staff => view! {
            <button
                class="bid"
                type="button"
                disabled={ shut }
                {on_click(move |_| close::post(id))}
            >"Bring the hammer down"</button>
        },
        Role::Bidder => {
            let next = lot.price() + store::INCREMENT;

            view! {
                <button
                    class="bid"
                    type="button"
                    disabled={ shut }
                    {on_click(move |_| bid::post(id))}
                >{ format!("Bid £{next}") }</button>
            }
        }
    }
}

/// Every lot, each its own fragment, as `role` sees them.
pub(crate) fn lots(role: Role) -> Markup {
    let lots = exos::data::<Lots>().snapshot();

    view! {
        <section class="lots">
            { lots.iter().map(|entry| lot(entry.id, role)).collect::<Vec<_>>() }
        </section>
    }
}

/// Pushes one lot to every tab watching it, whichever way they are seeing it.
///
/// Every action ends here. Publishing the copy nobody is displaying is free
/// and silent, so this needs to know nothing about who is in the room.
pub(crate) fn publish_lot(id: u32) {
    for role in [Role::Bidder, Role::Staff] {
        publish(&lot(id, role));
    }
}

/// Where a lot stands, said the same way to everybody.
fn standing(lot: &Lot) -> String {
    match (lot.leader(), lot.closed) {
        (Some(leader), true) => format!("Sold to {}", bidder::describe(leader)),
        (Some(leader), false) => format!("{} leads", bidder::describe(leader)),
        (None, true) => String::from("Unsold"),
        (None, false) => String::from("No bids yet"),
    }
}

// -----------------------------------------------------------------------------
//                                  THE BIDDING
// -----------------------------------------------------------------------------

/// Raises a lot, and says three different things to three different people.
///
/// This handler is the example in miniature. The publish is **state**, and it
/// reaches every tab watching this lot. What it hands back is a **reply**, and
/// it reaches exactly the tab that asked, with no identity involved at all.
/// The tell is an **event**, and it reaches one person on every tab they have
/// open, including one showing the catalogue and no lots whatever. Only the
/// third needs to know who anybody is, which is the whole argument for
/// directed effects being a separate thing from publishing.
#[exos::post("/lots/{id}/bid")]
async fn bid(Path(id): Path<u32>) -> Effect {
    // The auctioneer runs the sale and does not bid in it. The check is here
    // rather than on the button, and it has to be: the button lives inside a
    // live fragment, and a fragment says the same thing to everybody, so it
    // cannot know who is reading it. That is the topic invariant doing its
    // job rather than getting in the way.
    if bidder::is_staff().await {
        return toast::note(Note::Warn, "The auctioneer does not bid in their own sale.");
    }

    let me = bidder::current().await;

    let Some(accepted) = exos::data::<Lots>().update(|lots| store::bid(lots, id, &me)) else {
        return toast::note(Note::Warn, "That lot is closed.");
    };

    publish_lot(id);

    if let Some(outbid) = &accepted.outbid {
        toast::tell(
            outbid,
            Note::Warn,
            format!(
                "Outbid on {}. It stands at £{}.",
                accepted.title, accepted.price
            ),
        );
    }

    // Only this tab, because only this tab asked. The bidder's other tabs see
    // the price move from the publish above, which is the right amount of
    // news for a screen that did not do anything.
    toast::note(
        Note::Good,
        format!("Your bid is in, at £{}.", accepted.price),
    )
}

/// Brings the hammer down on one lot, if you are the one holding it.
#[exos::post("/lots/{id}/close")]
async fn close(Path(id): Path<u32>) -> Effect {
    // exos has no permission model, and this is what that means: a check at
    // the call site, in ordinary Rust, where a reader can see it. The button
    // is only rendered for staff, and that is not why this is safe. Directed
    // effects are the same story, which is why the guide says authorizing one
    // is the sender's job.
    if !bidder::is_staff().await {
        return toast::note(Note::Warn, "Only the auctioneer closes a lot.");
    }

    hammer(id);

    // Nothing to reply with. Whatever happened comes back on the stream, as
    // the result every auctioneer is told.
    Effect::none()
}

/// Closes a lot and tells everybody who needs to know.
///
/// There is a request behind this, but it is somebody else's: the auctioneer
/// asked, and the winner did not. That is the case directed effects exist for.
/// A reply can only reach whoever made the request, so the one person who most
/// wants to hear is the one person it cannot be told.
pub(crate) fn hammer(id: u32) {
    let Some(closed) = exos::data::<Lots>().update(|lots| store::close(lots, id)) else {
        return;
    };

    // State first, to whoever is watching this lot.
    publish_lot(id);

    if let Some(winner) = closed.leader() {
        let won = format!("You won {} at £{}.", closed.title, closed.price());

        // Persist first, push second. There is no ledger in an example this
        // size, so the branch that cannot reach anybody prints instead of
        // emailing, but the shape is the one that matters: `connected` is a
        // hint, the record is elsewhere, and the push is the accelerator.
        if toast::reaches(winner) {
            toast::tell(winner, Note::Good, won);
        } else {
            println!(
                "auction: {} has no tab open; would email: {won}",
                bidder::describe(winner)
            );
        }
    }

    // One audience, however many people are doing that job. Every auctioneer
    // gets the result, on every tab, and nobody else does.
    send(&Role::Staff, &toast::note(Note::Good, result(&closed)));
}

/// What the sale room hears when the hammer comes down.
fn result(lot: &Lot) -> String {
    lot.leader().map_or_else(
        || format!("{} went unsold.", lot.title),
        |winner| {
            format!(
                "{} sold to {} at £{}.",
                lot.title,
                bidder::describe(winner),
                lot.price()
            )
        },
    )
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
    use axum::body::BodyDataStream;
    use tokio_stream::StreamExt as _;

    use super::*;
    use crate::{
        store::Bidder,
        tests::{body, claimed, request, seeded},
    };

    fn markup(id: u32) -> String {
        seeded();
        lot(id, Role::Bidder).to_markup().into_string()
    }

    /// One open tab, past its greeting and subscribed to nothing.
    async fn listening(session: &str) -> BodyDataStream {
        let mut events = request("GET", "/_exos/live", Some(session))
            .await
            .into_body()
            .into_data_stream();

        let greeting = next(&mut events).await;
        assert!(greeting.contains("event: connection"), "{greeting}");

        events
    }

    /// The next event, as the browser would read it off the wire.
    ///
    /// The ceiling is a backstop so a stream that says nothing fails the test
    /// rather than hanging it, and never a wait: everything asserted above has
    /// already been sent.
    async fn next(events: &mut BodyDataStream) -> String {
        let chunk = tokio::time::timeout(core::time::Duration::from_secs(5), events.next())
            .await
            .expect("the stream says something rather than nothing at all")
            .expect("the stream is still open")
            .expect("the body does not fail");

        String::from_utf8(chunk.to_vec()).expect("an event is text")
    }

    async fn bid_as(session: &str, id: u32) {
        let response = request("POST", &format!("/lots/{id}/bid"), Some(session)).await;

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    /// Being able to subscribe is the authorization, so the wrapper carries a
    /// token the client could not have produced.
    #[test]
    fn a_lot_is_a_live_fragment() {
        let html = markup(1);

        assert!(html.starts_with("<exos-live style=\"display:contents\" id=\"live-lot-"));
        assert!(html.contains("data-token=\""));
    }

    /// Two lots must not share a fragment, or a bid on one would patch both.
    /// Nor may two roles, or the auctioneer's hammer would land in a bidder's
    /// tab and a bidder would be watching a fragment they were never served.
    #[test]
    fn every_lot_and_role_is_a_topic_of_its_own() {
        seeded();

        let mut topics: Vec<String> = exos::data::<Lots>()
            .snapshot()
            .iter()
            .flat_map(|entry| {
                [Role::Bidder, Role::Staff]
                    .map(|role| lot(entry.id, role).topic().as_str().to_owned())
            })
            .collect();

        let total = topics.len();
        topics.sort();
        topics.dedup();

        assert_eq!(topics.len(), total);
    }

    /// The escape hatch working: one lot, two topics, and the difference
    /// between them is what each reader is offered rather than what the lot
    /// is worth.
    #[test]
    fn the_auctioneer_is_offered_the_hammer_and_nobody_else_is() {
        seeded();

        let theirs = lot(1, Role::Staff).to_markup().into_string();
        let everybody = markup(1);

        assert!(theirs.contains("Bring the hammer down"));
        assert!(!theirs.contains("Bid £"), "and never a bid");

        assert!(everybody.contains("Bid £"));
        assert!(!everybody.contains("hammer"), "nor the other way round");

        // The same lot and the same price, said to both.
        for html in [&theirs, &everybody] {
            assert!(html.contains("No bids yet"), "{html}");
        }
    }

    /// A topic has to completely determine its content, so what a lot says
    /// cannot depend on who is reading it, or on anything else that could
    /// differ between two renders. Everything viewer-shaped in this example
    /// travels as a directed effect instead.
    #[test]
    fn a_lot_says_the_same_thing_to_everybody() {
        assert_eq!(markup(1), markup(1));
    }

    #[test]
    fn a_lot_with_no_bids_says_so() {
        let html = markup(1);

        assert!(html.contains("No bids yet"));
        assert!(html.contains("Bid £130"), "{html}");
    }

    #[test]
    fn a_topic_naming_nothing_renders_nothing() {
        let html = markup(999);

        assert!(html.ends_with("\"></exos-live>"), "{html}");
        assert!(!html.contains("<article"));
    }

    /// The example's premise, over two real streams: one bid, two people, and
    /// only one of them is told.
    ///
    /// Lot 4 belongs to this test, because the room is global and these run in
    /// parallel; everything above only reads. Neither stream subscribes to
    /// anything, so the only events either of them can carry are the directed
    /// ones, and the prices in them say which is which.
    ///
    /// Nothing waits on a timer. That Grace was not told is asserted by
    /// telling her something else afterwards and checking that arrives first,
    /// which is a fact about ordering rather than about how long a test is
    /// willing to wait.
    #[tokio::test]
    async fn being_outbid_reaches_the_person_and_nobody_else() {
        const LOT: u32 = 4;

        let ada = claimed(1).await;
        let grace = claimed(2).await;

        let mut hers = listening(&ada).await;
        let mut theirs = listening(&grace).await;

        // 250, and each bid adds ten.
        bid_as(&ada, LOT).await; // 260, pushing nobody out
        bid_as(&grace, LOT).await; // 270, and Ada hears about it
        bid_as(&ada, LOT).await; // 280, and Grace hears about it

        let told_ada = next(&mut hers).await;
        assert!(told_ada.starts_with("event: signals"), "{told_ada}");
        assert!(told_ada.contains("270"), "{told_ada}");

        // Had the first message reached Grace it would be sitting in front of
        // this one, and it names a different price.
        let told_grace = next(&mut theirs).await;
        assert!(told_grace.starts_with("event: signals"), "{told_grace}");
        assert!(told_grace.contains("280"), "{told_grace}");
    }

    /// The auctioneer runs the sale and does not bid in it.
    ///
    /// Refused by the handler rather than by the button, and it has to be: the
    /// button is inside a fragment, a fragment says the same thing to
    /// everybody, and so it cannot know who is reading it. Lot 2 is this
    /// test's, and the point is that its price does not move.
    #[tokio::test]
    async fn the_auctioneer_does_not_bid_in_their_own_sale() {
        const LOT: u32 = 2;

        let staff = claimed(3).await;
        let before = exos::data::<Lots>().one(LOT).expect("the lot").price();

        let refused = body(request("POST", &format!("/lots/{LOT}/bid"), Some(&staff)).await).await;

        assert!(refused.contains("does not bid"), "{refused}");
        assert!(refused.contains("warn"), "and it reads as a refusal");
        assert_eq!(
            exos::data::<Lots>().one(LOT).expect("the lot").price(),
            before
        );
    }

    #[test]
    fn what_a_lot_says_follows_what_happened_to_it() {
        seeded();

        let mut lots = vec![Lot {
            id: 1,
            title: String::from("A clock"),
            reserve: 100,
            bids: Vec::new(),
            closed: false,
        }];

        assert_eq!(standing(&lots[0]), "No bids yet");

        drop(store::bid(&mut lots, 1, &Bidder::Viewer(1)));
        assert_eq!(standing(&lots[0]), "Ada leads");

        drop(store::close(&mut lots, 1));
        assert_eq!(standing(&lots[0]), "Sold to Ada");
        assert_eq!(result(&lots[0]), "A clock sold to Ada at £110.");
    }
}
