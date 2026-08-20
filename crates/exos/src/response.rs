//! What a handler hands back.

use axum::{
    body::Body,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};

use crate::Markup;

/// A complete HTML document.
///
/// A `GET` answers with one of these. It has to: a cold browser, a bookmark or
/// a crawler gets no JavaScript, so the document is the only representation
/// that always works.
///
/// The caching policy is `no-cache, private` rather than `no-store`. A
/// no-store document is ineligible for the back/forward cache, and keeping
/// history restorable is worth more than refusing to store a page that must be
/// revalidated anyway.
///
/// A route that sometimes answers with something else says so in its return
/// type. `Result<Page, `[`Redirect`](axum::response::Redirect)`>` is a page
/// that may send the browser elsewhere instead.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Page(pub Markup);

impl From<Markup> for Page {
    fn from(markup: Markup) -> Self {
        Self(markup)
    }
}

impl IntoResponse for Page {
    fn into_response(self) -> Response {
        let mut response = html(self.0);

        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, private"),
        );

        response
    }
}

/// Rendering markup directly is the shorthand for a fragment, and for the
/// handlers simple enough not to need a [`Page`].
impl IntoResponse for Markup {
    fn into_response(self) -> Response {
        html(self)
    }
}

fn html(markup: Markup) -> Response {
    let mut response = Response::new(Body::from(markup.into_string()));

    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );

    response
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_html_that_caches_but_revalidates() {
        let response = Page(Markup(String::from("<p></p>"))).into_response();
        let headers = response.headers();

        assert_eq!(
            headers.get(header::CONTENT_TYPE).expect("a content type"),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            headers.get(header::CACHE_CONTROL).expect("a policy"),
            "no-cache, private"
        );
    }

    #[test]
    fn bare_markup_carries_no_caching_policy_of_its_own() {
        let response = Markup(String::from("<li></li>")).into_response();

        assert!(response.headers().get(header::CACHE_CONTROL).is_none());
    }
}
