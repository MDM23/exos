//! The form: what it holds, what it says when a field is wrong, and the action
//! that accepts it.
//!
//! Every rule here is written twice on purpose, once as a Rust condition over
//! the value that arrived and once as an expression the browser answers while
//! somebody types. That is what the framework should be writing, and doing it
//! by hand is what this example is for.

use axum::http::StatusCode;
use exos::{
    Effect, Js, Markup, Model, Signal, bind, class, data, on_focusout, on_submit, show, signal,
    text, view,
};
use serde::{Deserialize, Serialize};

use crate::{
    store::{self, Programme, Registration, Registrations},
    workshops::picker,
};

/// Everything the form holds, and everything it can be told.
///
/// The six `_error` fields are the shape of what is missing: a message has to
/// live on the document for a handler to be able to write it, and there is
/// nowhere else to put one, so each field carries its own by hand.
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Signup {
    /// Who is registering.
    pub(crate) name: String,
    /// Where the confirmation goes.
    pub(crate) email: String,
    /// Whether the billing section applies at all.
    pub(crate) invoice: bool,
    /// Who the invoice is made out to.
    pub(crate) company: String,
    /// The tax id it needs.
    pub(crate) vat: String,
    /// A code only the server can rule on.
    pub(crate) code: String,
    /// Which workshops were picked.
    pub(crate) workshops: Vec<u32>,

    /// What is wrong with [`name`](Self::name).
    pub(crate) name_error: String,
    /// What is wrong with [`email`](Self::email).
    pub(crate) email_error: String,
    /// What is wrong with [`company`](Self::company).
    pub(crate) company_error: String,
    /// What is wrong with [`vat`](Self::vat).
    pub(crate) vat_error: String,
    /// What is wrong with [`code`](Self::code).
    pub(crate) code_error: String,
    /// What is wrong with [`workshops`](Self::workshops).
    pub(crate) workshops_error: String,
}

/// The form, with every rule it can answer without asking.
pub(crate) fn registration() -> Markup {
    let form = Signup::signals();

    view! {
        <div id="signup">
            <form {&form} {on_submit(|_| register::post(&form))}>
                <h1>"Register"</h1>

                {
                    field(
                        "name",
                        "Your name",
                        "text",
                        &form.name,
                        &form.name_error,
                        Some((form.name.get().trim().is_empty(), "A name is needed.")),
                    )
                }

                {
                    field(
                        "email",
                        "Email",
                        "email",
                        &form.email,
                        &form.email_error,
                        Some((
                            !form.email.get().contains("@"),
                            "That is not an email address.",
                        )),
                    )
                }

                <div class="field">
                    <span class="label">"Workshops"</span>
                    { picker(&form.workshops, &form.workshops_error) }
                </div>

                <label class="check">
                    <input type="checkbox" {bind(&form.invoice)}>
                    "I need an invoice"
                </label>

                // The section and the rules under it are gated on one signal,
                // spelled here and again in each rule below. Nothing checks
                // that the three agree.
                <fieldset class="billing" {show(form.invoice.get())}>
                    <legend>"Billing"</legend>

                    {
                        field(
                            "company",
                            "Company",
                            "text",
                            &form.company,
                            &form.company_error,
                            Some((
                                form.invoice.get().and(form.company.get().trim().is_empty()),
                                "An invoice needs a company.",
                            )),
                        )
                    }

                    {
                        field(
                            "vat",
                            "VAT id",
                            "text",
                            &form.vat,
                            &form.vat_error,
                            Some((
                                form.invoice.get().and(form.vat.get().trim().is_empty()),
                                "An invoice needs a VAT id.",
                            )),
                        )
                    }
                </fieldset>

                // No client rule at all: whether a code exists is a fact only
                // the server holds, so this field can be wrong in exactly one
                // way and only after a round trip.
                { field("code", "Discount code", "text", &form.code, &form.code_error, None) }

                <button type="submit">"Register"</button>
            </form>
        </div>
    }
}

/// One labelled field, its control, and both of the ways it complains.
///
/// `broken` is the browser's copy of the rule, and it is optional because a
/// rule the server alone can answer has no browser copy to give.
fn field(
    id: &'static str,
    label: &'static str,
    kind: &'static str,
    value: &Signal<String>,
    error: &Signal<String>,
    broken: Option<(Js<bool>, &'static str)>,
) -> Markup {
    // Gating the client's complaint on having left the field once is what
    // keeps a form from being red before it is read. A plain signal is enough,
    // because nothing off the page ever writes it, and it is declared on the
    // wrapper so every field gets its own.
    let touched = signal(false);

    // The server's message wins where there is one, which is the precedence a
    // framework would apply rather than each field arranging it.
    let said = !error.get().is_empty();

    let wrong = match &broken {
        Some((rule, _)) => rule.clone().and(touched.get()).or(said.clone()),
        None => said.clone(),
    };

    view! {
        <div class="field" {&touched}>
            <label for={ id }>{ label }</label>

            <input
                id={ id }
                type={ kind }
                {bind(value)}
                {class("invalid", wrong)}
                {on_focusout(|_| touched.set(true))}
            >

            <p class="error" {show(said.clone())} {text(error.get())}></p>

            {
                match broken {
                    Some((rule, complaint)) => view! {
                        <p class="error" {show(rule.and(touched.get()).and(!said))}>
                            { complaint }
                        </p>
                    },
                    None => Markup::default(),
                }
            }
        </div>
    }
}

/// Something wrong with the form.
struct Fault {
    /// The signal the message is written to.
    error: Signal<String>,
    /// Where the caret should go. Spelled again here because nothing connects
    /// a field to the id its input was rendered with.
    id: &'static str,
    /// What to say about it.
    message: &'static str,
}

/// Every shape rule, asked a second time against what arrived.
///
/// These are the same questions the template asks in the browser, and keeping
/// the two in step is nobody's job but the author's.
fn faults(form: &Signup) -> Vec<Fault> {
    let signals = Signup::signals();
    let mut faults = Vec::new();

    if form.name.trim().is_empty() {
        faults.push(Fault {
            error: signals.name_error,
            id: "name",
            message: "A name is needed.",
        });
    }

    if !form.email.contains('@') {
        faults.push(Fault {
            error: signals.email_error,
            id: "email",
            message: "That is not an email address.",
        });
    }

    if form.workshops.is_empty() {
        faults.push(Fault {
            error: signals.workshops_error,
            id: "workshops",
            message: "Pick at least one workshop.",
        });
    }

    if form.invoice && form.company.trim().is_empty() {
        faults.push(Fault {
            error: signals.company_error,
            id: "company",
            message: "An invoice needs a company.",
        });
    }

    if form.invoice && form.vat.trim().is_empty() {
        faults.push(Fault {
            error: signals.vat_error,
            id: "vat",
            message: "An invoice needs a VAT id.",
        });
    }

    faults
}

/// Writes every error signal, the ones that now pass included.
///
/// Clearing what passes is not optional and is the easiest thing here to
/// forget: a message left behind describes a value that is no longer there.
fn report(faults: &[Fault]) -> Effect {
    let signals = Signup::signals();
    let mut effect = Effect::none();

    for error in [
        signals.code_error,
        signals.company_error,
        signals.email_error,
        signals.name_error,
        signals.vat_error,
        signals.workshops_error,
    ] {
        let message = faults
            .iter()
            .find(|fault| fault.error.name() == error.name())
            .map_or_else(String::new, |fault| fault.message.to_owned());

        effect = effect.and_set(&error, message);
    }

    effect
}

/// Accepts a registration, or says what is wrong with it.
#[exos::post("/register")]
async fn register(Model(form): Model<Signup>) -> Result<Effect, (StatusCode, Effect)> {
    let mut faults = faults(&form);

    // Ids arrive from checkboxes, and markup is not a promise. This is a rule
    // no browser copy could ever stand in for, and it is not a message anybody
    // should see: a well-behaved page cannot produce it.
    if !data::<Programme>().holds(&form.workshops) {
        faults.push(Fault {
            error: Signup::signals().workshops_error,
            id: "workshops",
            message: "That is not on the programme.",
        });
    }

    // The round trip this form exists to have. Asked only once the shape
    // holds, so a form with an empty name does not also argue about a code.
    if faults.is_empty() && !form.code.trim().is_empty() && !store::accepts(&form.code) {
        faults.push(Fault {
            error: Signup::signals().code_error,
            id: "code",
            message: "That code is not one of ours.",
        });
    }

    if let Some(first) = faults.first() {
        let caret = format!("#{}", first.id);

        // A refusal says what the outcome was and what to do about it, and the
        // client applies the second whatever the first says.
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            report(&faults).focus(caret),
        ));
    }

    let taken = data::<Registrations>().add(Registration {
        name: form.name.trim().to_owned(),
        email: form.email.trim().to_owned(),
        workshops: form.workshops.clone(),
    });

    Ok(Effect::patch(confirmation(&form, taken)))
}

/// What replaces the form once it is accepted.
fn confirmation(form: &Signup, taken: usize) -> Markup {
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

    /// Both copies of one rule are on the page: the client's is an expression
    /// over the signal, the server's arrives as a message in another.
    #[tokio::test]
    async fn a_field_carries_both_of_its_complaints() {
        let html = get("/").await;
        let signals = Signup::signals();

        assert!(html.contains(&format!("($.{}.trim()).length === 0", signals.name.name())));
        assert!(html.contains(&format!("data-text=\"$.{}\"", signals.name_error.name())));
    }

    /// The billing rules and the section that shows them are gated on the same
    /// signal, spelled once per place. Nothing checks that they agree.
    #[tokio::test]
    async fn the_billing_section_is_shown_by_the_same_signal_its_rules_read() {
        let html = get("/").await;
        let invoice = Signup::signals().invoice;

        assert!(html.contains(&format!("data-show=\"$.{}\"", invoice.name())));
        assert!(html.contains(&format!("$.{} &amp;&amp; ", invoice.name())));
    }

    #[tokio::test]
    async fn an_empty_form_is_refused_with_a_message_per_field() {
        let stream = post("/register", &exos::to_wire(&Signup::default())).await;
        let signals = Signup::signals();

        assert!(stream.contains("A name is needed."), "{stream}");
        assert!(stream.contains("Pick at least one workshop."), "{stream}");
        assert!(stream.contains(&format!("\"{}\"", signals.name_error.name())));
    }

    /// The caret goes to the first thing wrong, which is what makes a refusal
    /// something to act on rather than something to read.
    #[tokio::test]
    async fn a_refusal_moves_the_caret_to_the_first_fault() {
        let stream = post("/register", &exos::to_wire(&Signup::default())).await;

        assert!(stream.contains("event: focus"), "{stream}");
        assert!(stream.contains("#name"), "{stream}");
    }

    /// Every error signal is written, including the ones that now pass, or a
    /// message about a value somebody has since fixed stays on screen.
    #[tokio::test]
    async fn a_report_clears_what_no_longer_applies() {
        let signals = Signup::signals();
        let stream = post("/register", &exos::to_wire(&draft())).await;

        assert!(!stream.contains("A name is needed."), "{stream}");

        let stream = post(
            "/register",
            &exos::to_wire(&Signup {
                name: String::new(),
                ..draft()
            }),
        )
        .await;

        assert!(
            stream.contains(&format!("\"{}\":\"\"", signals.email_error.name())),
            "{stream}"
        );
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
