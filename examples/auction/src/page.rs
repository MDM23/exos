//! The document, and the two routes that serve it.
//!
//! There are two pages so that one of them can show no lots at all. Being
//! outbid still reaches you on the catalogue, because a directed effect is
//! addressed to a person rather than to a region of a screen, and that is the
//! thing this example exists to demonstrate.

use exos::{Markup, Page, view};

use crate::{bidder, room, store::Role, toast};

#[exos::get("/")]
async fn sale_room() -> Page {
    let role = bidder::role().await;

    document(
        "The sale room",
        "/",
        view! {
            <h1>"The sale room"</h1>

            <p class="lede">
                "Bid on a lot and the price moves in every tab watching it, because
                 the price is state and state is published. Outbid somebody and only
                 they are told, because that is an event and events are addressed to
                 a person. Reload afterwards: the price is still there and the
                 message is not."
            </p>

            <p class="lede">
                "Nothing closes on a timer. A lot stands until the auctioneer brings
                 the hammer down, so when one goes it is because somebody did it. The
                 winner is told all the same, and they asked for nothing."
            </p>

            { rostrum(role) }
            { room::lots(role) }
        },
    )
    .await
}

/// What the auctioneer is told about the room, and nobody else needs.
///
/// Not rendered rather than rendered hidden. Native control flow, at render
/// time, on the server: markup a reader has no use for is markup they have no
/// reason to receive, and `hidden` is a suggestion to a browser rather than a
/// decision.
fn rostrum(role: Role) -> Markup {
    if role == Role::Bidder {
        return Markup::default();
    }

    view! {
        <section class="hammer">
            <h2>"The rostrum"</h2>
            <p>
                "You do not bid in your own sale, so your copy of each lot carries
                 the hammer instead. That is a different fragment from the one
                 everybody else is watching, addressed by a topic that has your
                 role in it, which is how a lot can say two things without ever
                 saying them to the wrong person. Closing one tells the winner and
                 every auctioneer, and neither of them asked."
            </p>
        </section>
    }
}

#[exos::get("/catalogue")]
async fn catalogue() -> Page {
    document(
        "The catalogue",
        "/catalogue",
        view! {
            <h1>"The catalogue"</h1>

            <p class="lede">
                "A different document, with no lots on it. Getting here morphed the
                 body instead of loading it, so the live connection was never
                 dropped and this tab is still whoever it was."
            </p>

            <p>
                "Have somebody outbid you from another window. The message arrives
                 here, on a page that renders no fragment for that lot and is
                 subscribed to nothing. That is the difference between a patch and a
                 directed effect: one goes to a screen, the other goes to you."
            </p>

            <p><a href="/">"Back to the sale room"</a></p>
        },
    )
    .await
}

/// The document every page sits in.
///
/// Async because naming the browser is, in an application, a database call
/// away. The name has to be settled here rather than at the first bid: the
/// stream is opened by this document and carries whatever cookie this response
/// sets, so a visitor named later would hold a connection that is nobody.
async fn document(title: &str, path: &str, body: Markup) -> Page {
    let who = bidder::bar().await;

    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>{ title }</title>
                <link rel="stylesheet" href={ exos::asset!("css/app.css") }>
                <script defer src={ exos::runtime() }></script>
            </head>
            <body>
                <nav class="site-nav">
                    <div class="shell">
                        <a href="/" aria-current={ current(path, "/") }>"Sale room"</a>
                        <a href="/catalogue" aria-current={ current(path, "/catalogue") }>
                            "Catalogue"
                        </a>
                    </div>
                </nav>

                <div class="shell">{ who }</div>

                <main class="shell">{ body }</main>

                { toast::banner() }

                <footer class="shell info">
                    <p>
                        "Open this in two tabs to see one message land in both, and in
                         a private window to be somebody else."
                    </p>
                </footer>
            </body>
        </html>
    })
}

/// `aria-current` is what a screen reader announces, so the styling keys off
/// the same attribute rather than a parallel class that could drift from it.
fn current(path: &str, href: &str) -> Option<&'static str> {
    (path == href).then_some("page")
}

#[cfg(test)]
mod tests {
    use crate::tests::get;

    #[tokio::test]
    async fn the_document_ships_one_stylesheet_and_one_script() {
        let html = get("/").await;

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert_eq!(html.matches("rel=\"stylesheet\"").count(), 1);
        assert_eq!(html.matches("<script").count(), 1);
    }

    /// The one that matters: the message slot is on every page, and the
    /// catalogue subscribes to nothing at all.
    #[tokio::test]
    async fn the_catalogue_carries_the_slot_and_no_fragments() {
        let html = get("/catalogue").await;

        assert!(html.contains("class=\"toast\""));
        assert!(!html.contains("<exos-live"), "and watches nothing");
    }

    #[tokio::test]
    async fn the_sale_room_watches_one_fragment_per_lot() {
        let html = get("/").await;

        assert_eq!(html.matches("<exos-live").count(), 4);
    }

    #[tokio::test]
    async fn the_current_page_is_the_only_one_marked() {
        let html = get("/catalogue").await;

        assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
        assert!(html.contains("<a href=\"/catalogue\" aria-current=\"page\">"));
    }
}
