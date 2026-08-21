//! The form: what it holds, what it says when a field is wrong, and the action
//! that accepts it.
//!
//! Every rule here is written twice on purpose, once as a Rust condition over
//! the value that arrived and once as an expression the browser answers while
//! somebody types. That is what the framework should be writing, and doing it
//! by hand is what this example is for.

use exos::{Bound, Effect, Markup, Model, Refusal, bind, data, on_submit, show, text, view};
use serde::{Deserialize, Serialize};

use crate::{
    attendees::roster,
    store::{self, Programme, Registration, Registrations, Roster},
    workshops::picker,
};

/// Everything the form holds.
///
/// The rules a value can be judged on alone are declared here and checked by
/// the extractor, so nothing below calls a validator. What is left in the
/// handler is the rules that need something this struct does not hold.
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Signup {
    /// Who is registering.
    #[valid(required, length = 2..=40)]
    pub(crate) name: String,
    /// Where the confirmation goes.
    #[valid(required, email)]
    pub(crate) email: String,
    /// Whether the billing section applies at all.
    pub(crate) invoice: bool,
    /// Who the invoice is made out to, needed only when one is asked for.
    #[valid(required_with = invoice)]
    pub(crate) company: String,
    /// The tax id it needs, on the same terms.
    #[valid(required_with = invoice)]
    pub(crate) vat: String,
    /// A code only the server can rule on.
    pub(crate) code: String,
    /// Which workshops were picked.
    #[valid(required)]
    pub(crate) workshops: Vec<u32>,
    /// Never filled in, and here so that a message has a field to hang on.
    ///
    /// The rows live on the server, so this model does not hold them, and the
    /// record is keyed by field. A message about something that is not a field
    /// has no home, and inventing one is the cheapest way to give it one.
    pub(crate) attendees: String,
}

/// The form.
pub(crate) fn registration() -> Markup {
    let form = Signup::signals();

    view! {
        <div id="signup">
            <form {&form} {on_submit(|_| register::post(&form))}>
                <h1>"Register"</h1>

                { field("name", "Your name", "text", &form.name) }
                { field("email", "Email", "email", &form.email) }

                <div class="field">
                    <span class="label">"Workshops"</span>
                    { picker(&form.workshops) }
                </div>

                <div class="field">
                    <span class="label">"Who is coming"</span>
                    { roster() }

                    // One message for the whole group, because an error still
                    // has nowhere row-shaped to go.
                    <p
                        class="error"
                        {show(form.attendees.invalid())}
                        {text(form.attendees.error())}
                    ></p>
                </div>

                <label class="check">
                    <input type="checkbox" {bind(&form.invoice)}>
                    "I need an invoice"
                </label>

                <fieldset class="billing" {show(form.invoice.get())}>
                    <legend>"Billing"</legend>

                    { field("company", "Company", "text", &form.company) }
                    { field("vat", "VAT id", "text", &form.vat) }
                </fieldset>

                { field("code", "Discount code", "text", &form.code) }

                <button type="submit">"Register"</button>
            </form>
        </div>
    }
}

/// One labelled field, its control, and whatever is wrong with it.
///
/// Nothing here knows which rules the field has or where they were checked. A
/// message is a message, whether a declared rule produced it or the handler
/// did.
fn field(
    id: &'static str,
    label: &'static str,
    kind: &'static str,
    value: &Bound<String>,
) -> Markup {
    view! {
        <div class="field">
            <label for={ id }>{ label }</label>
            <input id={ id } type={ kind } {bind(value)}>
            <p class="error" {show(value.invalid())} {text(value.error())}></p>
        </div>
    }
}

/// Accepts a registration, or says what is wrong with it.
///
/// Every rule about a single value was checked by the extractor, so a body
/// that broke one never reached this line. What is left is the two kinds a
/// rule on a field cannot express: one that needs a fact this model does not
/// hold, and one about data that is not in it at all.
#[exos::post("/register")]
async fn register(Model(form): Model<Signup>) -> Result<Effect, Refusal<Signup>> {
    let mut refusal = Refusal::new();

    // Ids arrive from checkboxes, and markup is not a promise. Not a message
    // anybody should see: a well-behaved page cannot produce it.
    if !data::<Programme>().holds(&form.workshops) {
        refusal.add(Signup::WORKSHOPS, "That is not on the programme.");
    }

    // The rows are not part of the submission, so this reads the store and the
    // message goes on the field invented to hold it.
    if let Some(message) = store::roster_fault(&data::<Roster>().snapshot()) {
        refusal.add(Signup::ATTENDEES, message);
    }

    // The round trip this form exists to have.
    if !form.code.trim().is_empty() && !store::accepts(&form.code) {
        refusal.add(Signup::CODE, "That code is not one of ours.");
    }

    if !refusal.is_empty() {
        return Err(refusal);
    }

    // Read rather than taken, so this example stays one an ordinary test can
    // run twice. An application could not: the rows are the server's, so a
    // form that succeeded has to be emptied here or it stays a resource with
    // somebody's half-typed guest list in it, and nothing on the client can do
    // that for it. That is the other end of the same problem.
    let attending: Vec<String> = data::<Roster>()
        .snapshot()
        .iter()
        .map(|row| row.name.clone())
        .collect();

    let taken = data::<Registrations>().add(Registration {
        name: form.name.trim().to_owned(),
        email: form.email.trim().to_owned(),
        workshops: form.workshops.clone(),
    });

    Ok(Effect::patch(confirmation(&form, &attending, taken)))
}

/// What replaces the form once it is accepted.
fn confirmation(form: &Signup, attending: &[String], taken: usize) -> Markup {
    let programme = data::<Programme>();

    let picked: Vec<&str> = programme
        .all()
        .iter()
        .filter(|workshop| form.workshops.contains(&workshop.id))
        .map(|workshop| workshop.title.as_str())
        .collect();

    view! {
        <div id="signup" class="done">
            <h1>"You are registered"</h1>

            <p>
                "A confirmation is on its way to "
                <strong>{ form.email.trim() }</strong>
                "."
            </p>

            <ul>
                {
                    attending
                        .iter()
                        .map(|name| view! { <li>{ name }</li> })
                        .collect::<Vec<_>>()
                }
            </ul>

            <ul>
                { picked.iter().map(|title| view! { <li>{ *title }</li> }).collect::<Vec<_>>() }
            </ul>

            <p class="count">{ taken }" registered so far."</p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{get, post};

    fn draft() -> Signup {
        Signup {
            name: String::from("Ada"),
            email: String::from("ada@example.com"),
            workshops: vec![1],
            ..Signup::default()
        }
    }

    /// The control carries its own rules and the record it writes them into.
    /// The message is baked in at render time, because that is when the locale
    /// is known and where the application's own wording lives.
    #[tokio::test]
    async fn a_control_carries_the_rules_it_answers_for_itself() {
        let html = get("/").await;
        let state = <Signup as exos::Validate>::STATE;

        assert!(html.contains("data-bind-rules="), "{html:.2000}");
        assert!(html.contains("At least 2 characters."));
        assert!(html.contains(&format!("data-bind-state=\"{state}\"")));
    }

    /// And whatever reads the message reads one slot, whichever side decided
    /// what is in it. No template knows there were ever two.
    #[tokio::test]
    async fn a_message_is_read_from_one_place() {
        let html = get("/").await;
        let signals = Signup::signals();
        let state = <Signup as exos::Validate>::STATE;

        assert!(html.contains(&format!(
            "data-text=\"($.{state}[&quot;{}&quot;] ?? &quot;&quot;)\"",
            signals.name.name()
        )));

        // The record is declared with the fields, so the index cannot land on
        // an undefined before anything has been written.
        assert!(html.contains(&format!("&quot;{state}&quot;:{{}}")));
    }

    /// A plain signal has no record, because nothing off the page can say
    /// anything about one. The dropdown's search box is the case in this page.
    #[tokio::test]
    async fn a_signal_that_is_not_a_field_carries_none() {
        let html = get("/").await;

        let search = html
            .split_once("type=\"search\"")
            .and_then(|(_, rest)| rest.split_once('>'))
            .map(|(tag, _)| tag)
            .expect("the search box is on the page");

        assert!(search.contains("data-bind="), "{search}");
        assert!(!search.contains("data-bind-state="), "{search}");
    }

    /// The billing section is shown by one signal and gated on the same one,
    /// spelled once per place. Nothing holds the two together, which is the
    /// coupling stage 4 decided against buying.
    #[tokio::test]
    async fn the_billing_section_is_shown_by_the_signal_its_rules_read() {
        let html = get("/").await;
        let invoice = Signup::signals().invoice;

        assert!(html.contains(&format!("data-show=\"$.{}\"", invoice.name())));
    }

    /// A gated field is silent until its sibling arms it, on the server.
    #[tokio::test]
    async fn a_gate_is_shut_until_the_field_that_arms_it_is_filled_in() {
        let stream = post("/register", &exos::to_wire(&draft())).await;
        assert!(stream.contains("You are registered"), "{stream}");

        let stream = post(
            "/register",
            &exos::to_wire(&Signup {
                invoice: true,
                ..draft()
            }),
        )
        .await;

        assert!(stream.contains("An invoice needs a company."), "{stream}");
        assert!(stream.contains("An invoice needs a VAT id."), "{stream}");
    }

    /// And in the browser, where the gate is the sibling's own presence in
    /// front of the rule rather than a second thing to keep in step.
    #[tokio::test]
    async fn the_same_gate_is_carried_to_the_control() {
        let html = get("/").await;
        let signals = Signup::signals();

        let company = html
            .split_once(r#"id="company""#)
            .and_then(|(_, rest)| rest.split_once('>'))
            .map(|(tag, _)| tag)
            .expect("the company field is on the page");

        assert!(
            company.contains(&format!("$.{}", signals.invoice.name())),
            "{company}"
        );
        assert!(company.contains("An invoice needs a company."), "{company}");
    }

    /// Nothing calls the validator, so a body that breaks a declared rule
    /// never reaches the handler and comes back as the record anyway.
    #[tokio::test]
    async fn a_declared_rule_is_checked_before_the_handler_runs() {
        let stream = post("/register", &exos::to_wire(&Signup::default())).await;
        let signals = Signup::signals();

        assert!(stream.contains("This is needed."), "{stream}");
        assert!(
            stream.contains(&format!("\"{}\"", signals.name.name())),
            "{stream}"
        );

        // The handler's own rules did not run: the roster is fine and the
        // extractor refused before anything could ask about it.
        assert!(!stream.contains("attendee"), "{stream}");
    }

    /// A field that now passes is cleared by not being in the record, which is
    /// what removes the writing-every-message-every-time of the old version.
    #[tokio::test]
    async fn a_record_says_only_what_is_wrong() {
        let signals = Signup::signals();

        let stream = post(
            "/register",
            &exos::to_wire(&Signup {
                email: String::from("not-an-address"),
                ..draft()
            }),
        )
        .await;

        assert!(stream.contains("That is not an email address."), "{stream}");
        assert!(
            !stream.contains(&format!("\"{}\"", signals.name.name())),
            "{stream}"
        );
    }

    /// A length is counted in UTF-16 code units on both sides, so a value the
    /// browser called too long is too long here too.
    #[tokio::test]
    async fn a_length_is_refused_the_way_the_browser_counts_it() {
        let stream = post(
            "/register",
            &exos::to_wire(&Signup {
                name: String::from("A"),
                ..draft()
            }),
        )
        .await;

        assert!(stream.contains("At least 2 characters."), "{stream}");
    }

    /// The rule that has to be a round trip.
    #[tokio::test]
    async fn a_code_is_ruled_on_by_the_server_alone() {
        let stream = post(
            "/register",
            &exos::to_wire(&Signup {
                code: String::from("nope"),
                ..draft()
            }),
        )
        .await;

        assert!(stream.contains("That code is not one of ours."), "{stream}");

        let stream = post(
            "/register",
            &exos::to_wire(&Signup {
                code: String::from("earlybird"),
                ..draft()
            }),
        )
        .await;

        assert!(stream.contains("You are registered"), "{stream}");
    }

    /// A checkbox carries whatever the markup said, so what arrives is checked
    /// like anything else on the wire.
    #[tokio::test]
    async fn a_workshop_nobody_offers_is_refused() {
        let stream = post(
            "/register",
            &exos::to_wire(&Signup {
                workshops: vec![99],
                ..draft()
            }),
        )
        .await;

        assert!(stream.contains("That is not on the programme."), "{stream}");
    }
}
