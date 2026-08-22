//! Rules declared on a model, checked on the way in.
//!
//! A rule is written once, on the field it is about, and
//! [`Model`](crate::Model) checks it before a handler body runs. Nothing calls
//! a validator, because there is no call site to forget:
//!
//! ```ignore
//! #[exos::model]
//! #[derive(Debug, Default, Deserialize, Serialize)]
//! struct Signup {
//!     #[valid(required, length = 2..=40)]
//!     name: String,
//!
//!     #[valid(required, email)]
//!     email: String,
//!
//!     invoice: bool,
//!
//!     #[valid(required_with = invoice)]
//!     vat: String,
//! }
//! ```
//!
//! What arrives broken never reaches the handler. It comes back as a `422`
//! carrying an [`Effect`](crate::Effect) that writes the messages into the
//! model's own record, which a template reads through the handle.
//!
//! # A rule under a condition
//!
//! `required_with` is the ordinary `required` rule with the named field's
//! [`Presence`] in front of it, on both sides, so a section a checkbox reveals
//! is checked while it is showing and silent while it is not. It is not a rule
//! of its own: a rule sees one value, and letting one read a sibling would put
//! a model type parameter on every rule that never uses one.
//!
//! Nothing holds a gate and whatever `show`s the section together. A section
//! revealed on more than the gate names is validated while hidden, and the
//! submit then fails with a message nobody can see. That is the application's
//! to watch, and it is what buys the gate out of being a second concept only
//! forms would have.
//!
//! # exos ships no text
//!
//! A [`Violation`] is a value, not a sentence, because an application's
//! languages are its own and [`messages!`](crate::messages) is where its text
//! lives. [`complaints`] is the one function that turns one into the other, and
//! the default is English so that `cargo run` says something sensible.
//!
//! # Two evaluators, one impl
//!
//! [`Presence`] and [`Length`] each answer their question twice, once against a
//! Rust value and once as an expression for the browser, and the two halves sit
//! in one impl block a few lines apart. That is what keeps a rule from meaning
//! two things: the copies cannot drift, because neither is written by hand at a
//! call site.

use core::marker::PhantomData;
use std::{collections::BTreeMap, sync::OnceLock};

use axum::response::{IntoResponse, Response};
use serde::{Serialize, Serializer};

use crate::{Field, Js, ModelFields};

/// What is wrong with one value.
///
/// Carried instead of a sentence, so the sentence stays in the application's
/// [`messages!`](crate::messages) where the compiler holds it to every locale.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Violation {
    /// Nothing was filled in.
    Required,
    /// Shorter than `least`, counted the way [`Length`] counts.
    TooShort {
        /// The shortest this may be.
        least: usize,
    },
    /// Longer than `most`.
    TooLong {
        /// The longest this may be.
        most: usize,
    },
    /// The right length and the wrong shape.
    Malformed,
}

/// What is wrong with a model, keyed by the field's wire name.
///
/// Empty is valid. The whole record is written at once, so a field that now
/// passes is cleared by not being in it and nothing has to remember to.
///
/// A key is a `String` rather than a `&'static str` because a field of a row
/// has no static name: it is the [`Rows`](crate::Rows) field, the row's id and
/// the field, and the id is only known once there is a row.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Errors(BTreeMap<String, String>);

impl Errors {
    /// Records a violation against a field.
    ///
    /// Called by the `#[model]` expansion. The first violation on a field wins,
    /// because a field that is empty is not also too short and saying both is
    /// how a form ends up shouting.
    #[doc(hidden)]
    pub fn add(&mut self, key: impl Into<String>, field: &'static str, violation: Violation) {
        self.0
            .entry(key.into())
            .or_insert_with(|| complain(field, violation));
    }

    /// Whether anything is wrong.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// What is wrong with one field, by its wire name.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
}

/// Serialized as the object the client reads, so the record is one signal
/// write rather than one per field.
impl Serialize for Errors {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// A model that knows its own rules.
///
/// Implemented by `#[model]` for every model, whether or not it declares any,
/// so that [`Model`](crate::Model) can check without knowing which did.
pub trait Validate: ModelFields {
    /// The name of the signal holding this model's [`Errors`].
    const STATE: &'static str;

    /// What is wrong with this value, under `prefix`.
    ///
    /// The prefix is empty for a model somebody submits and names the row for
    /// a model that is one, so that a message lands on the same key the row's
    /// control reads. Nothing outside the expansion has a reason to call this;
    /// [`validate`](Validate::validate) is the whole question.
    #[doc(hidden)]
    fn validate_into(&self, prefix: &str, errors: &mut Errors);

    /// What is wrong with this value.
    fn validate(&self) -> Errors {
        let mut errors = Errors::default();
        self.validate_into("", &mut errors);
        errors
    }
}

/// A handler's own refusal, in the shape a declared rule already produces.
///
/// Rules on the model cover what a value can be judged on alone. Everything
/// else is ordinary Rust in the handler, where the data is: whether this code
/// exists, whether that name is taken, whether these two fields agree. This is
/// how such a rule answers, so a viewer cannot tell the two apart:
///
/// ```ignore
/// #[exos::post("/signup")]
/// async fn signup(Model(form): Model<Signup>) -> Result<Effect, Refusal<Signup>> {
///     let mut refusal = Refusal::new();
///
///     if !data::<Codes>().accepts(&form.code) {
///         refusal.add(Signup::CODE, t::no_such_code());
///     }
///
///     if !refusal.is_empty() {
///         return Err(refusal);
///     }
///
///     /* ... */
/// }
/// ```
///
/// The message is the application's, because a rule exos does not know cannot
/// have a [`Violation`] exos does.
#[derive(Debug)]
pub struct Refusal<M> {
    errors: Errors,
    marker: PhantomData<fn() -> M>,
}

impl<M: Validate> Refusal<M> {
    /// A refusal with nothing wrong yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            errors: Errors::default(),
            marker: PhantomData,
        }
    }

    /// Says what is wrong with one field.
    ///
    /// The field is a token rather than a name, so renaming it breaks this
    /// line rather than quietly addressing nothing.
    pub fn add(&mut self, field: Field<M>, message: impl Into<String>) {
        let Some((key, _)) = M::FIELDS.iter().find(|(_, name)| *name == field.name()) else {
            return;
        };

        self.errors
            .0
            .entry((*key).to_owned())
            .or_insert_with(|| message.into());
    }

    /// Whether anything is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

impl<M: Validate> Default for Refusal<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// The same `422` and the same record write a declared rule answers with.
impl<M: Validate> IntoResponse for Refusal<M> {
    fn into_response(self) -> Response {
        crate::ModelRejection::Refused {
            state: M::STATE,
            errors: self.errors,
        }
        .into_response()
    }
}

// -----------------------------------------------------------------------------
//                                  THE TEXT
// -----------------------------------------------------------------------------

/// Turns a violation into something a person reads.
type Complaints = Box<dyn Fn(&str, Violation) -> String + Send + Sync>;

static COMPLAINTS: OnceLock<Complaints> = OnceLock::new();

/// Says how this application words a refusal.
///
/// The field arrives under the name it is declared with, which never leaves the
/// server, so an application can answer per field where the general sentence is
/// not good enough:
///
/// ```
/// # use exos::{Violation, complaints};
/// complaints(|field, violation| match (field, violation) {
///     ("vat", Violation::Required) => String::from("An invoice needs a VAT id."),
///     (_, Violation::Required) => String::from("This is needed."),
///     _ => String::from("That does not look right."),
/// });
/// ```
///
/// Called once, at startup, like [`keys`](crate::keys). A second call is
/// ignored rather than racing the first.
pub fn complaints(say: impl Fn(&str, Violation) -> String + Send + Sync + 'static) {
    drop(COMPLAINTS.set(Box::new(say)));
}

/// What to say about one violation, for the `#[model]` expansion.
///
/// The browser's copy of a message is baked in at render time, which is when
/// the locale is known and where the application's own wording lives.
#[doc(hidden)]
#[must_use]
pub fn complaint(field: &str, violation: Violation) -> String {
    complain(field, violation)
}

/// Folds a field's rules into one expression yielding its message.
///
/// A chain of ternaries rather than combinators, because there is no
/// conditional in the vocabulary and this is generated rather than written.
/// First match wins, which is the rule [`Errors::add`] follows on the server.
#[doc(hidden)]
#[must_use]
pub fn chain(rules: Vec<(Js<bool>, String)>) -> Option<Js<String>> {
    if rules.is_empty() {
        return None;
    }

    let mut chain = String::from("\"\"");

    for (broken, message) in rules.into_iter().rev() {
        chain = format!(
            "{} ? {} : {chain}",
            broken.source(),
            crate::quote_js(&message)
        );
    }

    Some(Js::raw(chain))
}

/// What to say about one violation.
fn complain(field: &str, violation: Violation) -> String {
    match COMPLAINTS.get() {
        Some(say) => say(field, violation),
        None => default_complaint(violation),
    }
}

/// English, for an application that has not said otherwise.
///
/// Deliberately not a warning on stderr the way an unconfigured key is. A key
/// left unset is a security hole; text left unset is a page in one language,
/// which is exactly right until it is not.
fn default_complaint(violation: Violation) -> String {
    match violation {
        Violation::Required => String::from("This is needed."),
        Violation::TooShort { least } => format!("At least {least} characters."),
        Violation::TooLong { most } => format!("At most {most} characters."),
        Violation::Malformed => String::from("That does not look right."),
    }
}

// -----------------------------------------------------------------------------
//                                 THE QUESTIONS
// -----------------------------------------------------------------------------

/// A type `required` can be asked about.
///
/// Both halves live here so they cannot drift: whatever the server calls
/// filled in is what the browser calls filled in, because one impl says both.
pub trait Presence: Sized {
    /// Whether this value counts as filled in.
    fn is_present(&self) -> bool;

    /// The same question, for the browser to answer as it is typed.
    fn present(value: Js<Self>) -> Js<bool>;
}

impl Presence for String {
    fn is_present(&self) -> bool {
        !self.trim().is_empty()
    }

    fn present(value: Js<Self>) -> Js<bool> {
        !value.trim().is_empty()
    }
}

/// A ticked box is present and an unticked one is not, which is what makes
/// `required` on a checkbox mean "accept the terms".
impl Presence for bool {
    fn is_present(&self) -> bool {
        *self
    }

    fn present(value: Js<Self>) -> Js<bool> {
        value
    }
}

impl<T> Presence for Vec<T> {
    fn is_present(&self) -> bool {
        !self.is_empty()
    }

    fn present(value: Js<Self>) -> Js<bool> {
        value.any()
    }
}

/// `None` is absent and `Some` is present, whatever is inside it.
///
/// This is how a number takes part. A bare number is always present, so
/// `required` on one says nothing; wrapping it says what a blank field means.
impl<T> Presence for Option<T> {
    fn is_present(&self) -> bool {
        self.is_some()
    }

    fn present(value: Js<Self>) -> Js<bool> {
        // Loose inequality on purpose: it is the one comparison that catches
        // `null` and `undefined` together, and a signal a patch has not yet
        // declared reads as the second.
        Js::raw(format!("{} != null", value.source()))
    }
}

/// A type a length can be asked of.
pub trait Length: Sized {
    /// How long this is.
    fn measure(&self) -> usize;

    /// The same measurement, in the browser.
    fn length(value: Js<Self>) -> Js<u32>;
}

/// Counted in UTF-16 code units rather than in `char`s, because that is what
/// JavaScript counts and a limit has to mean one thing on both sides. The two
/// disagree from the first emoji onwards, and a form that accepts what it
/// showed as too long is worse than either rule alone.
impl Length for String {
    fn measure(&self) -> usize {
        self.encode_utf16().count()
    }

    fn length(value: Js<Self>) -> Js<u32> {
        value.len()
    }
}

impl<T> Length for Vec<T> {
    fn measure(&self) -> usize {
        self.len()
    }

    fn length(value: Js<Self>) -> Js<u32> {
        value.len()
    }
}

/// Whether a string looks like an address.
///
/// Deliberately the shallowest check that is not wrong: something, an `@`,
/// something with a dot in it. Every stricter rule rejects an address that
/// works, and the only real test of an address is sending to it.
#[doc(hidden)]
#[must_use]
pub fn is_email(value: &str) -> bool {
    let mut parts = value.split('@');

    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };

    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

/// The same question, as an expression.
///
/// Written with [`Js::raw`] because the combinators cannot say it yet, and
/// kept next to [`is_email`] so the two are read together.
#[doc(hidden)]
#[must_use]
pub fn email_js(value: &Js<String>) -> Js<bool> {
    Js::raw(format!(
        "/^[^@]+@[^@.]+(\\.[^@.]+)+$/.test({})",
        value.source()
    ))
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_is_present_when_it_holds_more_than_whitespace() {
        assert!(String::from("Ada").is_present());
        assert!(!String::from("   ").is_present());
        assert!(!String::new().is_present());
    }

    /// The browser's copy of the same question, which is the half that could
    /// have said something else.
    #[test]
    fn presence_says_the_same_thing_in_both_languages() {
        let value = Js::<String>::raw("$.name");
        assert_eq!(
            String::present(value).source(),
            "!(($.name.trim()).length === 0)"
        );

        let value = Js::<Vec<u32>>::raw("$.picked");
        assert_eq!(<Vec<u32>>::present(value).source(), "$.picked.length > 0");

        let value = Js::<bool>::raw("$.terms");
        assert_eq!(<bool>::present(value).source(), "$.terms");
    }

    /// A bare number is always present, so `Option` is how one opts in.
    #[test]
    fn an_option_is_present_when_it_is_some() {
        assert!(Some(0_u32).is_present());
        assert!(!Option::<u32>::None.is_present());
    }

    /// UTF-16 code units, because that is what the browser counts. Counting
    /// `char`s here would accept a value the field had already called too long.
    #[test]
    fn a_length_is_counted_the_way_javascript_counts_it() {
        assert_eq!(String::from("abc").measure(), 3);
        assert_eq!(String::from("🎈").measure(), 2);
        assert_eq!(String::from("🎈").chars().count(), 1);
    }

    #[test]
    fn an_address_needs_a_local_part_an_at_and_a_dotted_domain() {
        assert!(is_email("ada@example.com"));
        assert!(is_email("a+b@mail.example.co.uk"));

        assert!(!is_email("ada@example"));
        assert!(!is_email("@example.com"));
        assert!(!is_email("ada@@example.com"));
        assert!(!is_email("ada@.com"));
        assert!(!is_email("ada"));
    }

    #[test]
    fn the_first_violation_on_a_field_is_the_one_it_keeps() {
        let mut errors = Errors::default();

        errors.add("s1", "name", Violation::Required);
        errors.add("s1", "name", Violation::TooShort { least: 3 });

        assert_eq!(errors.get("s1"), Some("This is needed."));
        assert!(!errors.is_empty());
    }

    #[test]
    fn an_empty_record_serializes_to_an_empty_object() {
        let errors = Errors::default();

        assert!(errors.is_empty());
        assert_eq!(serde_json::to_string(&errors).expect("serializes"), "{}");
    }
}
