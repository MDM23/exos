//! The document, and the one route that serves it.

use exos::{Page, view};

use crate::form::registration;

#[exos::get("/")]
async fn index() -> Page {
    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Register"</title>
                <link rel="stylesheet" href={ exos::asset!("css/app.css") }>
                <script defer src={ exos::runtime() }></script>
            </head>
            <body>
                { registration() }

                <footer class="info">
                    <p>"Try EARLYBIRD or SPEAKER as a discount code."</p>
                    <p>"Every rule on this page is written twice, by hand."</p>
                </footer>
            </body>
        </html>
    })
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

    /// The form is one element the reply can replace, which is what lets the
    /// confirmation arrive as a patch rather than as a second document.
    #[tokio::test]
    async fn the_form_is_addressable_as_one_element() {
        let html = get("/").await;

        assert_eq!(html.matches("id=\"signup\"").count(), 1);
    }
}
