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
//! # A rule the server alone can answer
//!
//! `checked_by` names a function, and it is the one rule with no browser half:
//! whether this code exists is a question about the application's data rather
//! than about the value's shape.
//!
//! ```ignore
//! #[valid(required, checked_by = coupon)]
//! code: String,
//!
//! async fn coupon(code: String) -> Result<(), String> { /* ... */ }
//! ```
//!
//! It is asked twice from that one declaration: while the field is being
//! edited, over a route exos mounts for every checked field, and again in the
//! extractor before a handler runs, so a submission is judged by the same
//! function whatever the browser was told. Neither asks about a value that is
//! absent or that broke a shape rule first.
//!
//! # exos ships no text
//!
//! A [`Violation`] is a value, not a sentence, because an application's
//! languages are its own and [`messages!`](crate::messages) is where its text
//! lives. [`complaints`] is the one function that turns one into the other, and
//! the default is English so that `cargo run` says something sensible.
//!
//! A message from `checked_by` is the application's outright, the way
//! [`Refusal::add`]'s is: a rule exos does not know cannot have a [`Violation`]
//! exos does.
//!
//! # Two evaluators, one impl
//!
//! [`Presence`] and [`Length`] each answer their question twice, once against a
//! Rust value and once as an expression for the browser, and the two halves sit
//! in one impl block a few lines apart. That is what keeps a rule from meaning
//! two things: the copies cannot drift, because neither is written by hand at a
//! call site.

use core::{future::Future, marker::PhantomData, pin::Pin};
use std::{collections::BTreeMap, sync::OnceLock};

use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Serialize, Serializer, de::DeserializeOwned};
use serde_json::Value;

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

/// Where a message about the model itself is kept in its record.
///
/// The empty string, which is not a field's wire name and not a row's key
/// either, so a refusal about the whole submission lands in the record every
/// other message lands in rather than in a second place a template has to read.
const MODEL: &str = "";

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
        self.refuse(key, || complain(field, violation));
    }

    /// Records a message somebody else wrote against a field.
    ///
    /// What a [`Violation`] cannot carry: a rule exos does not know, answered
    /// by a handler through [`Refusal`] or by a `checked_by` function. The
    /// first message on a field still wins, and the message is built only if
    /// there is a slot for it.
    fn refuse(&mut self, key: impl Into<String>, message: impl FnOnce() -> String) {
        self.0.entry(key.into()).or_insert_with(message);
    }

    /// The same, for the `#[model]` expansion, which has a message in hand.
    #[doc(hidden)]
    pub fn said(&mut self, key: impl Into<String>, message: String) {
        self.refuse(key, || message);
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

/// Whether a model's record holds nothing, as the browser reads it.
///
/// The aggregation of every rule of every field, and it folds nothing: the
/// record is already the one place a verdict lands, whichever side reached it
/// and whichever row it was about. Generated onto the model's handle, where it
/// is `form.valid()`.
#[doc(hidden)]
#[must_use]
pub fn all_valid(state: &str) -> Js<bool> {
    Js::raw(format!("Object.keys($.{state} ?? {{}}).length === 0"))
}

/// What the handler said about the model itself, as the browser reads it.
///
/// The same read a field's message is, off the key no field can spell.
/// Generated onto the model's handle, where it is `form.refusal()`.
#[doc(hidden)]
#[must_use]
pub fn model_refusal(state: &str) -> Js<String> {
    Js::raw(format!(
        "($.{state}[{key}] ?? \"\")",
        key = crate::quote_js(MODEL)
    ))
}

/// Whether anything writing into that record has been edited.
///
/// One flag the runtime sets beside the per-field one every binding already
/// sets, so this is a read rather than a fold, and a row counts like any other
/// control. Generated onto the handle as `form.dirty()`.
#[doc(hidden)]
#[must_use]
pub fn any_dirty(state: &str) -> Js<bool> {
    Js::raw(format!("dirty({})", crate::quote_js(state)))
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

    /// What only the server can say about this value, under `prefix`.
    ///
    /// The `checked_by` half, awaited by [`Model`](crate::Model) once the
    /// shape rules have run: a field they already refused is not asked about,
    /// and neither is one that is absent. A model declaring none answers
    /// immediately, which is why this has a default.
    #[doc(hidden)]
    fn check_into(&self, prefix: &str, errors: &mut Errors) -> impl Future<Output = ()> + Send {
        let _ = (prefix, errors);
        async {}
    }

    /// What is wrong with this value.
    ///
    /// The shape rules alone. What a round trip answers is
    /// [`check_into`](Validate::check_into), which the extractor awaits
    /// afterwards.
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
///         refusal.add(Signup::CODE, no_such_code());
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
        let found = M::FIELDS.iter().find(|(_, name)| *name == field.name());

        // A token is generated with the model, so a miss is one built by hand
        // and there is nothing to say it: a refusal that lands nowhere answers
        // exactly like one that worked, and the page it leaves behind is a
        // form that did nothing on submit.
        debug_assert!(
            found.is_some(),
            "{} declares no field named `{}`",
            core::any::type_name::<M>(),
            field.name(),
        );

        let Some((key, _)) = found else {
            return;
        };

        self.errors.refuse(*key, || message.into());
    }

    /// Says what is wrong with the submission rather than with a field of it.
    ///
    /// Whether these two are a login is a question about the pair, and hanging
    /// its answer on the password says something the server does not know. It
    /// lands in the same record under a key no field has, which is
    /// `form.refusal()` in a template, and any edit into the model retires it
    /// the way editing a field retires what was said about that field.
    ///
    /// ```ignore
    /// #[exos::post("/login")]
    /// async fn login(Model(form): Model<Login>) -> Result<Effect, Refusal<Login>> {
    ///     let Some(user) = accounts().authenticate(&form).await else {
    ///         let mut refusal = Refusal::new();
    ///         refusal.say(wrong_credentials());
    ///         return Err(refusal);
    ///     };
    ///
    ///     /* ... */
    /// }
    /// ```
    pub fn say(&mut self, message: impl Into<String>) {
        self.errors.refuse(MODEL, || message.into());
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
//                                THE ROUND TRIP
// -----------------------------------------------------------------------------

/// The route every checked field is asked through.
///
/// One route rather than one per field: the pair in the path resolves to an
/// entry, and a URL space that grew with a struct would buy a map lookup
/// either way.
const CHECK: &str = "/_exos/check/{model}/{field}";

/// A field whose rule only the server can answer.
///
/// Submitted by the `#[model]` expansion, once per `checked_by`. There is no
/// reason to name this type yourself.
#[derive(Clone, Copy)]
pub struct CheckEntry {
    model: &'static str,
    field: &'static str,
    ask: Ask,
}

/// The shim `#[model]` monomorphised: the field's own type, deserialized, and
/// the application's function awaited.
///
/// Boxed because the entries are collected into one list and every function
/// has a future of its own. The value is owned for the same reason: it outlives
/// the call that made it.
type Ask = fn(Value) -> Pin<Box<dyn Future<Output = Option<String>> + Send>>;

impl CheckEntry {
    /// Describes a checked field for the route to resolve.
    #[must_use]
    pub const fn new(model: &'static str, field: &'static str, ask: Ask) -> Self {
        Self { model, field, ask }
    }
}

impl core::fmt::Debug for CheckEntry {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("CheckEntry")
            .field("model", &self.model)
            .field("field", &self.field)
            .finish_non_exhaustive()
    }
}

inventory::collect!(CheckEntry);

/// Mounts the route a field is checked through.
///
/// This is the first route exos mounts that reads an application's own data,
/// and what holds it is what holds every other action: a `POST` with a JSON
/// body carrying the session cookie, which a cross-origin page cannot make.
/// The answer is the application's own function, so a rate limit or a refusal
/// to answer belongs there.
pub(crate) fn routes() -> Router {
    Router::new().route(CHECK, post(check))
}

/// Asks one field's rule about one value.
///
/// The answer is the message or nothing, as text: the control writes its own
/// slot with it, where a record write would clear every other message on the
/// form.
async fn check(Path((model, field)): Path<(String, String)>, Json(value): Json<Value>) -> Response {
    let found = inventory::iter::<CheckEntry>
        .into_iter()
        .find(|entry| entry.model == model && entry.field == field);

    let Some(entry) = found else {
        return StatusCode::NOT_FOUND.into_response();
    };

    (entry.ask)(value).await.unwrap_or_default().into_response()
}

/// Runs one field's rule, for the shim the `#[model]` expansion writes.
///
/// The guard is [`Presence`], the same question the extractor asks before it
/// awaits the same function: nothing asks the application whether an empty
/// string is taken. A value that does not deserialize says nothing either, and
/// the submission is where that is refused.
#[doc(hidden)]
pub async fn asked<T, F>(value: Value, ask: impl FnOnce(T) -> F) -> Option<String>
where
    T: DeserializeOwned + Presence,
    F: Future<Output = Result<(), String>>,
{
    let value: T = serde_json::from_value(value).ok()?;

    match value.is_present() {
        true => ask(value).await.err(),
        false => None,
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

    /// The parts of a `#[model]` expansion a refusal reads, written out so
    /// that one can be built here without the macro. Its values are nobody's
    /// business but the handler's, which is why there are none.
    #[derive(Debug)]
    struct Login;

    impl ModelFields for Login {
        const FIELDS: &'static [(&'static str, &'static str)] =
            &[("s1", "email"), ("s2", "password")];
    }

    impl Validate for Login {
        const STATE: &'static str = "s0";

        fn validate_into(&self, _prefix: &str, _errors: &mut Errors) {}
    }

    /// A handler names a field by the name it declares, and the browser reads
    /// the wire key, so the token is translated on the way out.
    #[test]
    fn a_refusal_about_a_field_lands_under_that_field_key() {
        let mut refusal = Refusal::<Login>::new();
        refusal.add(Field::new("password"), "Wrong credentials given");

        assert_eq!(refusal.errors.get("s2"), Some("Wrong credentials given"));
    }

    /// A refusal about the submission has no field to be keyed by, and the
    /// empty key is the one nothing else can produce.
    #[test]
    fn a_refusal_about_the_model_is_kept_under_the_empty_key() {
        let mut refusal = Refusal::<Login>::new();
        refusal.say("Wrong email or password.");
        refusal.say("Something else.");

        assert!(!refusal.is_empty());
        assert_eq!(refusal.errors.get(MODEL), Some("Wrong email or password."));
    }

    /// A model's two questions are one read each. The aggregation is not the
    /// rules folded together: a fold could only see the fields a document
    /// declares, which leaves out every row and everything the server alone
    /// decided.
    #[test]
    fn a_form_asks_the_record_rather_than_the_rules() {
        assert_eq!(
            all_valid("s1").source(),
            "Object.keys($.s1 ?? {}).length === 0"
        );
        assert_eq!(any_dirty("s1").source(), "dirty(\"s1\")");
    }

    /// And what it was refused with, which is the same read a field's message
    /// is, off the key no field can spell.
    #[test]
    fn a_form_reads_what_it_was_refused_with_off_the_empty_key() {
        assert_eq!(model_refusal("s1").source(), "($.s1[\"\"] ?? \"\")");
    }

    #[test]
    fn an_empty_record_serializes_to_an_empty_object() {
        let errors = Errors::default();

        assert!(errors.is_empty());
        assert_eq!(serde_json::to_string(&errors).expect("serializes"), "{}");
    }
}
