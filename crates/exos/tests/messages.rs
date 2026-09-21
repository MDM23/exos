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

    /// A time is not a fact this side has, so the message answers with an
    /// expression whichever side it was called from, and `Date` is which of
    /// the three ways the browser writes one.
    due(when: Date) {
        Ar = "موعد التسليم {when}",
        De = "Fällig am {when}",
        En = "Due on {when}",
    }

    /// A server dimension beside it is still resolved here. What crosses is
    /// the one sentence `to` already chose, with a hole where the date goes.
    posted(to: Assignee, when: Ago) {
        Ar { .. }          = "نشر {when}",
        De { .. }          = "Veröffentlicht {when}",
        En { Me, _ }       = "You posted this {when}",
        En { Somebody, _ } = "Posted {when}",
    }

    /// The declaration says which of the three reads it; the call site says
    /// which zone it belongs to and how long a form to write. A kickoff is at
    /// the venue, so this one is called with both.
    kickoff_at(when: Time) {
        Ar = "انطلاق المباراة {when}",
        De = "Anstoß um {when}",
        En = "Kickoff at {when}",
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
/// not the application does, and the module is this file's rather than the
/// macro's: a block expands into plain functions, so whatever a call site says
/// before `heading` was chosen here.
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
                <button>{ clear_selection() }</button>
                <button>{ save_and_close() }</button>
                <p>{ items_selected(3) }</p>
                <p>{ greeting("Ada") }</p>
                <p>{ accept_terms(|inner| view! { <a href="/terms">{ inner }</a> }) }</p>
            </body>
        </html>
    })
}

/// The same three messages, over a count only the browser has.
///
/// Nothing about the call sites says which side they are on: the argument is
/// an expression rather than a number, and that is the whole of the
/// difference.
#[exos::get("/messages/projected")]
async fn projected() -> Page {
    let locale: Locale = exos::locale();
    let picked = exos::signal(Vec::<u32>::new());

    Page(view! {
        <!DOCTYPE html>
        <html { exos::lang(locale) }>
            <body {&picked}>
                <p id="selected" {exos::text(items_selected(picked.get().len()))}></p>
                <p id="assigned" {exos::text(assigned(Assignee::Me, picked.get().len()))}></p>
                <p id="here">{ items_selected(3) }</p>
            </body>
        </html>
    })
}

/// The same again over a time, which no argument decides: the zone is the
/// browser's whoever called this.
#[exos::get("/messages/dated")]
async fn dated() -> Page {
    let locale: Locale = exos::locale();
    let when = exos::Instant::from_millis(1_787_130_000_000);

    Page(view! {
        <!DOCTYPE html>
        <html { exos::lang(locale) }>
            <body>
                <p id="due" {exos::text(due(when))}></p>
                <p id="posted" {exos::text(posted(Assignee::Me, when))}></p>
                <p id="plain">{ exos::When::ago(when) }</p>
                <p id="kickoff" {exos::text(kickoff_at(
                    exos::When::of(when).long().zone("Europe/Berlin"),
                ))}></p>
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
    assert_eq!(spoken(Locale::De, clear_selection), "Auswahl aufheben");
    assert_eq!(spoken(Locale::En, clear_selection), "Clear selection");
    assert_eq!(spoken(Locale::Ar, clear_selection), "إلغاء التحديد");
}

#[test]
fn a_count_picks_the_arm_and_is_written_into_it() {
    assert_eq!(
        spoken(Locale::De, || items_selected(1)),
        "1 Element ausgewählt"
    );
    assert_eq!(
        spoken(Locale::De, || items_selected(7)),
        "7 Elemente ausgewählt"
    );
    assert_eq!(spoken(Locale::En, || items_selected(0)), "0 items selected");
}

/// A `usize` out of `len()` and a literal are both counts, and neither needs a
/// cast to be one.
#[test]
fn a_count_is_whatever_whole_number_the_call_site_already_had() {
    let picked: Vec<u32> = vec![4, 9];

    assert_eq!(
        spoken(Locale::En, || items_selected(picked.len())),
        "2 items selected"
    );
    assert_eq!(
        spoken(Locale::En, || items_selected(1_u8)),
        "1 item selected"
    );
}

/// A plural rule asks about the absolute value, and the text is handed the
/// count as it arrived.
#[test]
fn a_negative_count_is_singular_where_its_magnitude_is() {
    assert_eq!(
        spoken(Locale::En, || items_selected(-1_i32)),
        "-1 item selected"
    );
}

/// A count is the one number a message knows is a number, so it goes into the
/// sentence the way the language writes one rather than the way Rust does.
#[test]
fn a_count_is_written_the_way_the_language_writes_a_number() {
    assert_eq!(
        spoken(Locale::De, || items_selected(1_234_567)),
        "1.234.567 Elemente ausgewählt"
    );
    assert_eq!(
        spoken(Locale::En, || items_selected(1_234_567)),
        "1,234,567 items selected"
    );
}

/// Including one inside a slot, which is the other path the same value takes.
#[test]
fn a_count_inside_a_slot_is_written_the_same_way() {
    assert_eq!(
        spoken(Locale::De, || unread(12_345)).as_str(),
        "Sie haben <strong>12.345 ungelesene</strong> Nachrichten"
    );
}

/// Six categories, reached through a message rather than through the
/// evaluator, which is what proves the two matches line up.
#[test]
fn a_language_reaches_every_category_it_has() {
    let counted = |count: u64| spoken(Locale::Ar, move || counted(count));

    assert_eq!(counted(0), "zero");
    assert_eq!(counted(1), "one");
    assert_eq!(counted(2), "two");
    assert_eq!(counted(3), "few");
    assert_eq!(counted(11), "many");
    assert_eq!(counted(100), "other");
}

#[test]
fn a_language_with_two_categories_reaches_both_of_its_own() {
    assert_eq!(spoken(Locale::De, || counted(1)), "one");
    assert_eq!(spoken(Locale::De, || counted(2)), "other");
}

#[test]
fn several_domains_are_told_apart_at_once() {
    let assigned = |to, count: u32| spoken(Locale::En, move || assigned(to, count));

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
        spoken(Locale::De, || assigned(Assignee::Me, 4)),
        "4 Dateien zugewiesen"
    );
}

#[test]
fn a_flag_is_a_domain_without_anything_being_declared() {
    assert_eq!(spoken(Locale::En, || sound(true)), "Sound on");
    assert_eq!(spoken(Locale::De, || sound(false)), "Ton aus");
}

#[test]
fn a_parameter_that_is_only_interpolated_needs_no_arm_of_its_own() {
    assert_eq!(
        spoken(Locale::En, || greeting("Ada")),
        "Hello Ada & welcome"
    );
}

#[test]
fn a_block_beside_a_feature_says_which_locale_set_it_is_written_against() {
    assert_eq!(spoken(Locale::De, inbox::heading), "Posteingang");
    assert_eq!(spoken(Locale::En, inbox::heading), "Inbox");
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
    let linked = || accept_terms(|inner| view! { <a href="/terms">{ inner }</a> });

    assert_eq!(
        spoken(Locale::En, linked).as_str(),
        "Please accept the <a href=\"/terms\">terms of service</a>."
    );
}

/// German wraps bold words in the link and puts the whole thing somewhere
/// else, which is the reason a sentence is one message rather than three.
#[test]
fn a_slot_holds_whatever_the_language_puts_in_it() {
    let linked = || accept_terms(|inner| view! { <a href="/terms">{ inner }</a> });

    assert_eq!(
        spoken(Locale::De, linked).as_str(),
        "Bitte die <a href=\"/terms\"><strong>Nutzungsbedingungen</strong></a> lesen \
         &amp; annehmen."
    );
}

#[test]
fn emphasis_is_a_slot_with_nothing_to_declare() {
    assert_eq!(
        spoken(Locale::En, || unread(1)).as_str(),
        "You have <strong>1 unread</strong> message"
    );
    assert_eq!(
        spoken(Locale::En, || unread(4)).as_str(),
        "You have <strong>4 unread</strong> messages"
    );
}

/// No part of a message string is ever parsed as HTML, so the only structure a
/// translation can carry is a slot that was declared in Rust.
#[test]
fn the_words_inside_a_slot_are_escaped_like_all_the_others() {
    let wrapped = || dismiss(|inner| view! { <b>{ inner }</b> });

    assert_eq!(spoken(Locale::En, wrapped).as_str(), "<b>&lt;close&gt;</b>");
}

/// A message without a slot is text, and text is escaped once, by whatever
/// renders it.
#[test]
fn a_message_without_a_slot_carries_its_words_as_they_were_written() {
    assert_eq!(spoken(Locale::En, save_and_close), "Save & close");
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

/// A count the browser holds cannot be resolved here, so the message answers
/// with an expression and its variants ride out with the page.
#[tokio::test]
async fn a_projected_message_is_read_out_of_a_table_the_document_carries() {
    let html = body("/messages/projected", "en").await;

    assert!(html.contains("data-text=\"msg(&quot;m"), "{html}");
    assert!(html.contains("data-messages="), "{html}");

    // The same message resolved here is still the sentence, in the same
    // document, from a call site that differs only in its argument.
    assert!(
        html.contains("<p id=\"here\">3 items selected</p>"),
        "{html}"
    );
}

/// What crosses is one message in one language: the variants of the count and
/// nothing else, with the sentence split where the number goes.
#[tokio::test]
async fn what_crosses_is_the_variants_of_the_language_the_page_is_in() {
    let html = body("/messages/projected", "de").await;

    assert!(html.contains("&quot;lang&quot;:&quot;de&quot;"), "{html}");
    assert!(
        html.contains("&quot;one&quot;:[&quot;&quot;,&quot; Element"),
        "{html}"
    );
    assert!(!html.contains("item selected"), "{html}");
}

/// A dimension the server knows is resolved on the way out. `assigned` branches
/// on who as well as on how many, and only the count is enumerated.
#[tokio::test]
async fn a_server_side_dimension_does_not_cross_with_it() {
    let html = body("/messages/projected", "en").await;

    assert!(html.contains("assigned to you"), "{html}");
    assert!(!html.contains("assigned to somebody"), "{html}");
}

/// A time crosses for the reason a count does, and the message says so in its
/// return type: there is no argument to decide, because there is no reading of
/// it the server could finish.
#[tokio::test]
async fn a_message_carrying_a_time_is_written_in_the_browser() {
    let html = body("/messages/dated", "de").await;

    assert!(
        html.contains("date(&quot;2026-08-19T09:00:00Z&quot;)"),
        "{html}"
    );
    assert!(html.contains("&quot;Fällig am &quot;"), "{html}");
    assert!(!html.contains("<p id=\"due\">Fällig"), "{html}");
}

/// And the sentence around it is chosen here, where the server's own
/// dimensions are known. Only the date is left to the browser.
#[tokio::test]
async fn a_dimension_the_server_knows_still_does_not_cross_with_a_time() {
    let html = body("/messages/dated", "en").await;

    assert!(html.contains("You posted this "), "{html}");
    assert!(!html.contains("Posted &quot;"), "{html}");
    assert!(
        html.contains("ago(&quot;2026-08-19T09:00:00Z&quot;)"),
        "{html}"
    );
}

/// The element's own text is the instant, so a crawler and a reader without
/// the runtime see something true rather than something wrong.
#[tokio::test]
async fn an_element_holding_a_time_says_it_before_the_runtime_writes_it() {
    let html = body("/messages/dated", "en").await;

    assert!(html.contains("datetime=\"2026-08-19T09:00:00Z\""), "{html}");
    assert!(html.contains(">2026-08-19T09:00:00Z</time>"), "{html}");
}

/// The declaration decides which of the three helpers reads the time, and the
/// call site decides the rest. A kickoff is at the venue, so the message that
/// says so carries the venue's zone rather than the reader's.
#[tokio::test]
async fn a_message_carries_the_zone_and_the_form_its_call_site_asked_for() {
    let html = body("/messages/dated", "de").await;

    assert!(
        html.contains(
            "time(&quot;2026-08-19T09:00:00Z&quot;, &quot;long&quot;, &quot;Europe/Berlin&quot;)"
        ),
        "{html}"
    );
    assert!(html.contains("&quot;Anstoß um &quot;"), "{html}");
}
