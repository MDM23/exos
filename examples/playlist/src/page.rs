//! The document, and the two routes that serve it.

use exos::{Markup, Page, view};

use crate::{
    room::room,
    selection::{Selection, bar},
};

#[exos::get("/")]
async fn listening_room() -> Page {
    // Declared here because this is the markup it belongs to. Being a model it
    // lands on the document, so the handlers reach it with `Effect::set` from
    // wherever they sit, and the rows inside the fragment bind to the same
    // signal this bar reads.
    let selection = Selection::signals();

    document(
        "The listening room",
        "/",
        view! {
            <h1>"The listening room"</h1>

            <p class="lede">
                "A queue everybody shares. Drag to change the order, heart
                 anything, tick a few and remove them. When a track ends the
                 mark moves down the list and nothing else does, and every tab
                 follows, so open a second one."
            </p>

            <section {&selection}>
                { bar(&selection) }
                { room() }
            </section>
        },
    )
}

#[exos::get("/about")]
async fn about() -> Page {
    document(
        "About the room",
        "/about",
        view! {
            <h1>"About the room"</h1>

            <p class="lede">
                "A different document. Getting here morphed the body instead of
                 loading it, so the live connection was never dropped and the
                 music did not stop."
            </p>

            <p>
                "Try removing the track that is playing. The row goes at once,
                 because the click paints before the server has answered, and
                 then comes back exactly where it was, because the room will not
                 remove what it is playing. Nothing here simulates a failure:
                 that is a rule the room has, and the correction is what an
                 optimistic update looks like when it turns out to be wrong."
            </p>

            <p><a href="/">"Back to the room"</a></p>
        },
    )
}

/// The document every page sits in.
fn document(title: &str, path: &str, body: Markup) -> Page {
    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>{ title }</title>
                <link rel="stylesheet" href={ exos::asset!("css/app.css") }>
                <script defer src={ exos::runtime() }></script>
                <script defer src={ exos::asset!("js/sleeve.js") }></script>
            </head>
            <body>
                <nav class="site-nav">
                    <div class="shell">
                        <a href="/" aria-current={ current(path, "/") }>"Room"</a>
                        <a href="/about" aria-current={ current(path, "/about") }>"About"</a>
                    </div>
                </nav>

                <main class="shell">{ body }</main>
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

    /// Both scripts are deferred, so document order is execution order, and
    /// the plugin is written against a runtime that has already run.
    #[tokio::test]
    async fn the_document_ships_one_stylesheet_and_the_runtime_before_the_plugin() {
        let html = get("/").await;

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert_eq!(html.matches("rel=\"stylesheet\"").count(), 1);
        assert_eq!(html.matches("<script").count(), 2);

        let runtime = html.find("/_exos/exos-").expect("the runtime is linked");
        let plugin = html.find("/_exos/sleeve-").expect("the plugin is linked");

        assert!(runtime < plugin, "{html:.600}");
    }

    /// The shell holds no state of its own. The selection is declared by the
    /// section it belongs to, and lands on the document from there.
    #[tokio::test]
    async fn the_selection_is_declared_where_it_belongs() {
        let html = get("/").await;

        assert!(
            html.starts_with("<!DOCTYPE html><html lang=\"en\">"),
            "{html:.120}"
        );
        assert_eq!(html.matches("data-signals-root=").count(), 1);
    }

    #[tokio::test]
    async fn the_other_page_watches_nothing() {
        let html = get("/about").await;

        assert!(!html.contains("<exos-live"));
        assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
    }
}
