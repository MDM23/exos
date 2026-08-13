//! The document, and the three routes that serve it.

use exos::{Page, view};

use crate::{board::board, compose::composer, filter::Filter};

#[exos::get("/")]
async fn all() -> Page {
    document(Filter::All)
}

#[exos::get("/active")]
async fn active() -> Page {
    document(Filter::Active)
}

#[exos::get("/completed")]
async fn completed() -> Page {
    document(Filter::Completed)
}

/// The whole document, showing one view of the list.
///
/// A filter is three routes rather than a signal, so a view can be bookmarked
/// and shared. Moving between them is a navigation the runtime morphs, which
/// is why the live connection survives it.
fn document(filter: Filter) -> Page {
    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"todos"</title>
                <link rel="stylesheet" href={ exos::asset!("css/app.css") }>
                <script defer src={ exos::runtime() }></script>
            </head>
            <body>
                <section class="todoapp">
                    { composer() }
                    { board(filter) }
                </section>

                <footer class="info">
                    <p>"Double-click a todo to edit it"</p>
                    <p>"Every change is pushed to every open tab. Open a second one."</p>
                </footer>
            </body>
        </html>
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::get;

    #[tokio::test]
    async fn the_document_ships_one_stylesheet_and_one_script() {
        let html = get("/").await;

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert_eq!(html.matches("rel=\"stylesheet\"").count(), 1);
        assert_eq!(html.matches("<script").count(), 1);
    }

    /// The shell holds no state of its own. Each model is declared by the
    /// markup it belongs to, and lands on the document from there.
    #[tokio::test]
    async fn the_document_element_declares_nothing() {
        let html = get("/").await;

        assert!(
            html.starts_with("<!DOCTYPE html><html lang=\"en\">"),
            "{html:.120}"
        );
        assert_eq!(html.matches("data-signals-root=").count(), 2, "{html:.900}");
    }

    #[tokio::test]
    async fn each_view_is_served_at_its_own_route() {
        for filter in Filter::ALL {
            let html = get(filter.path()).await;

            assert!(html.contains(&format!(
                "<a href=\"{}\" aria-current=\"page\">{}</a>",
                filter.path(),
                filter.label()
            )));
        }
    }
}
