//! Resolution through the whole stack, in an application with three languages.
//!
//! What the unit tests cover one rule at a time, this covers as a request: a
//! browser asks, a handler renders, and the response says which of the two
//! decided. The locale set is the real one `locales!` generates, so what is
//! exercised here is also the code that macro writes.

use axum::{
    body::Body,
    http::{Request, Response, header},
};
use exos::{Page, view};
use tower::ServiceExt as _;

exos::locales! {
    /// Right to left, which is the case the document has to carry.
    Ar = "ar",
    De = "de",
    #[fallback]
    En = "en",
}

/// What the request resolved to, which is what every message will read.
#[exos::get("/locale/tag")]
async fn tag() -> String {
    exos::locale::<Locale>().tag().to_owned()
}

/// An application that knows who is reading says so, and that is step one.
///
/// The real thing resolves a session name to a viewer and drops their profile's
/// language in, which is a database call and belongs in a handler.
#[exos::get("/locale/mine")]
async fn mine() -> String {
    exos::scope().set(Locale::Ar);

    exos::locale::<Locale>().tag().to_owned()
}

#[exos::get("/locale/document")]
async fn document() -> Page {
    let locale: Locale = exos::locale();

    Page(view! {
        <!DOCTYPE html>
        <html { exos::lang(locale) }>
            <body>{ locale.tag() }</body>
        </html>
    })
}

/// A route with no words in it, which is most of them: an asset, a health
/// check, a fragment.
#[exos::get("/locale/nothing")]
async fn nothing() -> String {
    String::from("nothing to translate")
}

/// One request, with the languages a browser holding these preferences sends.
async fn request(uri: &str, accepted: Option<&str>) -> Response<Body> {
    let mut builder = Request::builder().method("GET").uri(uri);

    if let Some(accepted) = accepted {
        builder = builder.header(header::ACCEPT_LANGUAGE, accepted);
    }

    exos::app()
        .oneshot(builder.body(Body::empty()).expect("a valid request"))
        .await
        .expect("the router answers")
}

async fn body(response: Response<Body>) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
}

/// What the request answered with, and what it said that answer varies by.
async fn resolved(uri: &str, accepted: Option<&str>) -> (String, Vec<String>) {
    let response = request(uri, accepted).await;

    let vary = response
        .headers()
        .get_all(header::VARY)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
        .collect();

    (body(response).await, vary)
}

#[tokio::test]
async fn a_browser_is_answered_in_the_language_it_asked_for() {
    let (tag, _) = resolved("/locale/tag", Some("de")).await;

    assert_eq!(tag, "de");
}

/// The reason lookup rather than an exact match: a browser asks for the region
/// it is in, and an application declares the language.
#[tokio::test]
async fn a_region_is_answered_by_the_language_it_belongs_to() {
    let (tag, _) = resolved("/locale/tag", Some("de-CH,de;q=0.9,en;q=0.8")).await;

    assert_eq!(tag, "de");
}

#[tokio::test]
async fn a_language_this_application_does_not_have_gets_the_fallback() {
    let (tag, _) = resolved("/locale/tag", Some("fr-CA, fr;q=0.9")).await;
    assert_eq!(tag, "en");

    let (tag, _) = resolved("/locale/tag", None).await;
    assert_eq!(tag, "en");
}

/// The whole reason the header is a step rather than the answer: what an
/// application knows about the reader beats what their browser is set to.
#[tokio::test]
async fn what_the_application_says_wins() {
    let (tag, _) = resolved("/locale/mine", Some("de")).await;

    assert_eq!(tag, "ar");
}

#[tokio::test]
async fn a_response_that_read_the_header_says_it_varies_by_it() {
    let (_, vary) = resolved("/locale/tag", Some("de")).await;

    assert_eq!(vary, ["Accept-Language"]);
}

/// Including one that read it and found nothing it could use. A cache handed
/// this response would otherwise serve English to the next reader whatever
/// they asked for.
#[tokio::test]
async fn a_request_that_sent_no_header_varies_by_it_all_the_same() {
    let (_, vary) = resolved("/locale/tag", None).await;

    assert_eq!(vary, ["Accept-Language"]);
}

/// It did not vary by the header, so it does not claim to. What such a
/// response does need is a caching policy of its own, which is the
/// application's to decide because the reason is theirs.
#[tokio::test]
async fn a_response_the_application_decided_does_not() {
    let (_, vary) = resolved("/locale/mine", Some("de")).await;

    assert!(vary.is_empty());
}

#[tokio::test]
async fn a_response_with_no_language_in_it_says_nothing_about_one() {
    let (body, vary) = resolved("/locale/nothing", Some("de")).await;

    assert_eq!(body, "nothing to translate");
    assert!(vary.is_empty());
}

#[tokio::test]
async fn the_document_carries_the_language_it_was_rendered_in() {
    let (html, _) = resolved("/locale/document", Some("de")).await;

    assert!(
        html.starts_with("<!DOCTYPE html><html lang=\"de\"><body>de</body>"),
        "{html}"
    );
}

/// `dir` where the script needs it, which is the other half of what the
/// document has to carry and the half that is easy to forget.
#[tokio::test]
async fn a_right_to_left_document_says_which_way_it_runs() {
    let (html, _) = resolved("/locale/document", Some("ar")).await;

    assert!(
        html.starts_with("<!DOCTYPE html><html lang=\"ar\" dir=\"rtl\"><body>ar</body>"),
        "{html}"
    );
}

/// The generated enum answers through the trait exos reads it by, as well as
/// through the inherent items an application uses.
#[test]
fn a_locale_set_answers_the_framework_and_the_application_alike() {
    fn declared<L: exos::LocaleSet>() -> Vec<&'static str> {
        L::ALL.iter().map(|locale| locale.tag()).collect()
    }

    assert_eq!(declared::<Locale>(), ["ar", "de", "en"]);
    assert_eq!(<Locale as exos::LocaleSet>::FALLBACK, Locale::En);
    assert_eq!(exos::LocaleSet::tag(Locale::De), Locale::De.tag());
    assert_eq!(
        exos::LocaleSet::direction(Locale::Ar),
        Locale::Ar.direction()
    );
}
