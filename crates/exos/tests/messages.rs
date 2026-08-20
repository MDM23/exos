//! Messages, in an application with three languages.
//!
//! The macro's own tests check the shape of what it writes. This compiles it: a
//! real locale set, a real enum to branch on, and a request whose language
//! decides which string comes out.

use axum::{
    body::Body,
    http::{Request, Response, header},
};
use exos::{Page, view};
use tower::ServiceExt as _;

exos::locales! {
    /// Six categories, which is the most any language has.
    Ar = "ar",
    De = "de",
    #[fallback]
    En = "en",
}

/// Something to branch on that is neither a count nor a flag.
#[derive(Clone, Copy, exos::Enumerable)]
enum Assignee {
    Me,
    Somebody,
}

exos::messages! {
    clear_selection {
        Ar = "إلغاء التحديد",
        De = "Auswahl aufheben",
        En = "Clear selection",
    }

    /// Arabic answers every count with one arm, by putting the number where
    /// nothing has to agree with it. That is what `..` is for: a translation
    /// decides how much of a distinction it needs.
    items_selected(count: Plural) {
        Ar { .. } = "العناصر المحددة: {count}",
        De { One } = "{count} Element ausgewählt",
        De { _ }   = "{count} Elemente ausgewählt",
        En { One } = "{count} item selected",
        En { _ }   = "{count} items selected",
    }

    /// Not a translation but a fixture: the categories themselves, so that
    /// what this asserts is which one a count reaches rather than anybody's
    /// Arabic.
    counted(count: Plural) {
        Ar { Zero } = "zero",
        Ar { One }  = "one",
        Ar { Two }  = "two",
        Ar { Few }  = "few",
        Ar { Many } = "many",
        Ar { _ }    = "other",
        De { One }  = "one",
        De { _ }    = "other",
        En { One }  = "one",
        En { _ }    = "other",
    }

    /// Two things to tell apart, and two languages that do not bother: the
    /// count survives the crossing into a language that says nothing about it,
    /// because a parameter is interpolated where the translation puts it.
    assigned(to: Assignee, count: Plural) {
        Ar { .. } = "الملفات: {count}",
        De { .. } = "{count} Dateien zugewiesen",
        En { Me, One }      = "{count} file assigned to you",
        En { Me, _ }        = "{count} files assigned to you",
        En { Somebody, _ }  = "{count} files assigned to somebody",
    }

    /// A flag is a domain like any other, and needs nothing declared.
    sound(on: bool) {
        Ar { true } = "الصوت مفعل",
        Ar { _ }    = "الصوت متوقف",
        De { true } = "Ton an",
        De { _ }    = "Ton aus",
        En { true } = "Sound on",
        En { _ }    = "Sound off",
    }

    /// Nothing to decide, so nothing but the name to put in. The ampersand is
    /// there to be escaped by whatever renders it.
    greeting(name: &str) {
        Ar = "مرحبا {name}",
        De = "Hallo {name} & willkommen",
        En = "Hello {name} & welcome",
    }

    /// A sentence with a link in it, which cannot be composed from two
    /// messages: the link lands in a different place in each language, and
    /// German puts a slot inside a slot to make the words bold as well.
    accept_terms(terms: Slot) {
        Ar = "يرجى قبول {terms}شروط الخدمة{/terms}",
        De = "Bitte die {terms}{b}Nutzungsbedingungen{/b}{/terms} lesen & annehmen.",
        En = "Please accept the {terms}terms of service{/terms}.",
    }

    /// Emphasis falls on different words in different languages, so there is
    /// nothing here for a call site to decide and nothing to declare.
    unread(count: Plural) {
        Ar { .. } = "الرسائل غير المقروءة: {b}{count}{/b}",
        De { One } = "Sie haben {b}{count} ungelesene{/b} Nachricht",
        De { _ }   = "Sie haben {b}{count} ungelesene{/b} Nachrichten",
        En { One } = "You have {b}{count} unread{/b} message",
        En { _ }   = "You have {b}{count} unread{/b} messages",
    }

    /// The words a translation puts in a slot are escaped like all the others,
    /// so a translator cannot introduce an element by editing one.
    dismiss(link: Slot) {
        Ar = "{link}<إغلاق>{/link}",
        De = "{link}<schließen>{/link}",
        En = "{link}<close>{/link}",
    }

    /// No slot, so a string, and the ampersand is escaped by whatever renders
    /// it rather than by the macro.
    save_and_close {
        Ar = "حفظ وإغلاق",
        De = "Speichern & schließen",
        En = "Save & close",
    }
}

/// The macro is usable more than once, so messages live next to the feature
/// that says them rather than in one file every branch touches. The locale set
/// is named here rather than found by convention, which is what a crate that is
/// not the application does.
mod inbox {
    exos::messages! {
        in crate::Locale {
            heading {
                Ar = "البريد",
                De = "Posteingang",
                En = "Inbox",
            }
        }
    }
}

#[exos::get("/messages/page")]
async fn page() -> Page {
    let locale: Locale = exos::locale();

    Page(view! {
        <!DOCTYPE html>
        <html { exos::lang(locale) }>
            <body>
                <button>{ t::clear_selection() }</button>
                <button>{ t::save_and_close() }</button>
                <p>{ t::items_selected(3) }</p>
                <p>{ t::greeting("Ada") }</p>
                <p>{ t::accept_terms(|inner| view! { <a href="/terms">{ inner }</a> }) }</p>
            </body>
        </html>
    })
}

async fn body(uri: &str, accepted: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::ACCEPT_LANGUAGE, accepted)
        .body(Body::empty())
        .expect("a valid request");

    let response: Response<Body> = exos::app()
        .oneshot(request)
        .await
        .expect("the router answers");

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");

    String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
}

/// Renders `message` in `locale`, the way a handler that resolved its reader
/// would.
fn spoken<T>(locale: Locale, message: impl FnOnce() -> T) -> T {
    exos::with_scope(|| {
        exos::scope().set(locale);
        message()
    })
}

#[test]
fn a_message_is_the_string_of_whatever_language_the_request_is_in() {
    assert_eq!(spoken(Locale::De, t::clear_selection), "Auswahl aufheben");
    assert_eq!(spoken(Locale::En, t::clear_selection), "Clear selection");
    assert_eq!(spoken(Locale::Ar, t::clear_selection), "إلغاء التحديد");
}

#[test]
fn a_count_picks_the_arm_and_is_written_into_it() {
    assert_eq!(
        spoken(Locale::De, || t::items_selected(1)),
        "1 Element ausgewählt"
    );
    assert_eq!(
        spoken(Locale::De, || t::items_selected(7)),
        "7 Elemente ausgewählt"
    );
    assert_eq!(
        spoken(Locale::En, || t::items_selected(0)),
        "0 items selected"
    );
}

/// A `usize` out of `len()` and a literal are both counts, and neither needs a
/// cast to be one.
#[test]
fn a_count_is_whatever_whole_number_the_call_site_already_had() {
    let picked: Vec<u32> = vec![4, 9];

    assert_eq!(
        spoken(Locale::En, || t::items_selected(picked.len())),
        "2 items selected"
    );
    assert_eq!(
        spoken(Locale::En, || t::items_selected(1_u8)),
        "1 item selected"
    );
}

/// A plural rule asks about the absolute value, and the text is handed the
/// count as it arrived.
#[test]
fn a_negative_count_is_singular_where_its_magnitude_is() {
    assert_eq!(
        spoken(Locale::En, || t::items_selected(-1_i32)),
        "-1 item selected"
    );
}

/// Six categories, reached through a message rather than through the
/// evaluator, which is what proves the two matches line up.
#[test]
fn a_language_reaches_every_category_it_has() {
    let counted = |count: u64| spoken(Locale::Ar, move || t::counted(count));

    assert_eq!(counted(0), "zero");
    assert_eq!(counted(1), "one");
    assert_eq!(counted(2), "two");
    assert_eq!(counted(3), "few");
    assert_eq!(counted(11), "many");
    assert_eq!(counted(100), "other");
}

#[test]
fn a_language_with_two_categories_reaches_both_of_its_own() {
    assert_eq!(spoken(Locale::De, || t::counted(1)), "one");
    assert_eq!(spoken(Locale::De, || t::counted(2)), "other");
}

#[test]
fn several_domains_are_told_apart_at_once() {
    let assigned = |to, count: u32| spoken(Locale::En, move || t::assigned(to, count));

    assert_eq!(assigned(Assignee::Me, 1), "1 file assigned to you");
    assert_eq!(assigned(Assignee::Me, 4), "4 files assigned to you");
    assert_eq!(
        assigned(Assignee::Somebody, 4),
        "4 files assigned to somebody"
    );
}

/// German tells none of them apart and still writes the count out, because an
/// arm that decides nothing is still a translation.
#[test]
fn a_language_that_makes_no_distinction_says_so_once() {
    assert_eq!(
        spoken(Locale::De, || t::assigned(Assignee::Me, 4)),
        "4 Dateien zugewiesen"
    );
}

#[test]
fn a_flag_is_a_domain_without_anything_being_declared() {
    assert_eq!(spoken(Locale::En, || t::sound(true)), "Sound on");
    assert_eq!(spoken(Locale::De, || t::sound(false)), "Ton aus");
}

#[test]
fn a_parameter_that_is_only_interpolated_needs_no_arm_of_its_own() {
    assert_eq!(
        spoken(Locale::En, || t::greeting("Ada")),
        "Hello Ada & welcome"
    );
}

#[test]
fn a_block_beside_a_feature_says_which_locale_set_it_is_written_against() {
    assert_eq!(spoken(Locale::De, inbox::t::heading), "Posteingang");
    assert_eq!(spoken(Locale::En, inbox::t::heading), "Inbox");
}

/// The categories a language has, in the order its rules are tried, which is
/// what a projection will walk.
#[test]
fn a_languages_categories_are_a_domain_like_any_other() {
    use exos::Enumerable as _;

    assert_eq!(de::Plural::ALL.len(), 2);
    assert_eq!(ar::Plural::ALL.len(), 6);
}

/// The href, the classes and the routing stay in Rust; the words, including
/// the ones inside the link, stay in the sentence.
#[test]
fn a_slot_keeps_the_sentence_whole_and_the_wrapper_at_the_call_site() {
    let linked = || t::accept_terms(|inner| view! { <a href="/terms">{ inner }</a> });

    assert_eq!(
        spoken(Locale::En, linked).as_str(),
        "Please accept the <a href=\"/terms\">terms of service</a>."
    );
}

/// German wraps bold words in the link and puts the whole thing somewhere
/// else, which is the reason a sentence is one message rather than three.
#[test]
fn a_slot_holds_whatever_the_language_puts_in_it() {
    let linked = || t::accept_terms(|inner| view! { <a href="/terms">{ inner }</a> });

    assert_eq!(
        spoken(Locale::De, linked).as_str(),
        "Bitte die <a href=\"/terms\"><strong>Nutzungsbedingungen</strong></a> lesen \
         &amp; annehmen."
    );
}

#[test]
fn emphasis_is_a_slot_with_nothing_to_declare() {
    assert_eq!(
        spoken(Locale::En, || t::unread(1)).as_str(),
        "You have <strong>1 unread</strong> message"
    );
    assert_eq!(
        spoken(Locale::En, || t::unread(4)).as_str(),
        "You have <strong>4 unread</strong> messages"
    );
}

/// No part of a message string is ever parsed as HTML, so the only structure a
/// translation can carry is a slot that was declared in Rust.
#[test]
fn the_words_inside_a_slot_are_escaped_like_all_the_others() {
    let wrapped = || t::dismiss(|inner| view! { <b>{ inner }</b> });

    assert_eq!(spoken(Locale::En, wrapped).as_str(), "<b>&lt;close&gt;</b>");
}

/// A message without a slot is text, and text is escaped once, by whatever
/// renders it.
#[test]
fn a_message_without_a_slot_carries_its_words_as_they_were_written() {
    assert_eq!(spoken(Locale::En, t::save_and_close), "Save & close");
}

#[tokio::test]
async fn a_request_is_answered_in_the_language_it_asked_for() {
    let html = body("/messages/page", "de-CH, en;q=0.8").await;

    assert!(html.contains("<button>Auswahl aufheben</button>"), "{html}");
    assert!(html.contains("<p>3 Elemente ausgewählt</p>"), "{html}");
}

/// A message is a string, so it goes into a template through the same escaping
/// as any other string and cannot bring markup with it.
#[tokio::test]
async fn a_message_in_a_template_is_escaped_like_any_other_text() {
    let html = body("/messages/page", "en").await;

    assert!(html.contains("<p>Hello Ada &amp; welcome</p>"), "{html}");
    assert!(html.contains("<button>Save &amp; close</button>"), "{html}");
}

/// One with a slot is markup, and reaches the document as the elements the
/// call site wrapped its words in.
#[tokio::test]
async fn a_message_with_a_slot_reaches_the_document_as_markup() {
    let html = body("/messages/page", "en").await;

    assert!(
        html.contains("<p>Please accept the <a href=\"/terms\">terms of service</a>.</p>"),
        "{html}"
    );
}
