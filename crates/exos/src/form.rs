//! A form: a model that renders itself.
//!
//! The page asks for a form's markup with [`Form::markup`], and so does the
//! revision route, with whatever the reader has picked so far. The rules
//! deciding what is offered, what is kept and what is chosen for the reader
//! therefore exist once, on the server, next to the queries that already know
//! who is asking.

use core::{future::Future, pin::Pin};

use axum::{
    Json, Router,
    body::Bytes,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};

use crate::{
    Markup, Validate,
    model::{read, wire},
};

/// A model that renders itself.
///
/// Declared with `#[exos::form]` on the struct and this trait beside it:
///
/// ```ignore
/// #[exos::form]
/// #[derive(Default, Deserialize, Serialize)]
/// struct TeamForm {
///     #[revises]
///     tenant: String,
///     sport: String,
/// }
///
/// impl exos::Form for TeamForm {
///     type Error = Error;
///
///     async fn render(&mut self, key: FormKey) -> Result<Markup, Error> {
///         let sports = Sport::of(&self.tenant).await?;
///         self.sport = keep_or_preselect(&self.sport, &sports);
///
///         let form = self.signals(key);
///
///         Ok(view! { <form {&form}> ... </form> })
///     }
/// }
/// ```
///
/// A control bound to a `#[revises]` field posts the form when it changes, and
/// the answer is this same render: the markup is morphed in and the fields
/// `render` changed are written back, except one the reader edited while the
/// request was out.
pub trait Form: Validate + DeserializeOwned + Serialize + Send + Sized + 'static {
    /// What a render fails with, answered the way a handler's failure is.
    ///
    /// The page gets it back from [`markup`](Self::markup) and hands it on with
    /// `?`. A revision answers with its response, so whatever the application
    /// layers on around its handlers says it here too. A form that reads
    /// nothing names [`Infallible`](core::convert::Infallible).
    type Error: IntoResponse + Send;

    /// The form's markup, for whatever the model holds.
    ///
    /// Decisions go above `self.signals(key)` and markup below it: the handle
    /// declares the values the model holds when it is built, which is what
    /// puts a value chosen for the reader on the first render. On a revision
    /// `self` is whatever the reader sent, judged by nothing, so read it the
    /// way the page would read a request.
    fn render(&mut self, key: FormKey) -> impl Future<Output = Result<Markup, Self::Error>> + Send;

    /// The form, rendered.
    ///
    /// The one way in, for the page and the revision route alike.
    fn markup(mut self) -> impl Future<Output = Result<Markup, Self::Error>> + Send {
        async move { self.render(FormKey(())).await }
    }
}

/// What a form's handle is built from, and only [`Form::render`] is handed one.
///
/// So a form has one template: no page can bind its fields outside it.
#[derive(Clone, Copy, Debug)]
pub struct FormKey(());

// -----------------------------------------------------------------------------
//                                 THE REVISION
// -----------------------------------------------------------------------------

/// The route every form is revised through, addressed like the check route.
const REVISE: &str = "/_exos/revise/{form}";

/// A form the revision route can resolve.
///
/// Submitted by the `#[form]` expansion. There is no reason to name this type
/// yourself.
#[derive(Clone, Copy)]
pub struct ReviseEntry {
    form: &'static str,
    revise: Revise,
}

/// The revision, monomorphised for one form. Boxed for the reason
/// [`CheckEntry`](crate::CheckEntry)'s shim is.
type Revise = fn(Bytes) -> Pin<Box<dyn Future<Output = Response> + Send>>;

impl ReviseEntry {
    /// Describes a form for the route to resolve.
    ///
    /// Naming `T` as a form is what makes a `#[form]` without an `impl Form` a
    /// compile error.
    pub const fn new<T: Form>() -> Self {
        Self {
            form: T::STATE,
            revise: |body| Box::pin(revised::<T>(body)),
        }
    }
}

impl core::fmt::Debug for ReviseEntry {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ReviseEntry")
            .field("form", &self.form)
            .finish_non_exhaustive()
    }
}

inventory::collect!(ReviseEntry);

/// Mounts the route a form is revised through.
///
/// What holds it is what holds the check route: a same-origin `POST` with a
/// JSON body. What it may reveal is what the form's own render decides to.
pub(crate) fn routes() -> Router {
    Router::new().route(REVISE, post(revise))
}

async fn revise(Path(form): Path<String>, body: Bytes) -> Response {
    let found = inventory::iter::<ReviseEntry>
        .into_iter()
        .find(|entry| entry.form == form);

    match found {
        Some(entry) => (entry.revise)(body).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Renders a form again and says what the render changed.
///
/// Only what changed goes back, keyed as it travels: writing every field would
/// put back the value of one the reader edited while this was out.
async fn revised<T: Form>(body: Bytes) -> Response {
    let mut form: T = match read(&body) {
        Ok(form) => form,
        Err(rejection) => return rejection.into_response(),
    };

    let before = wire(&form);
    let patch = match form.render(FormKey(())).await {
        Ok(patch) => patch,
        Err(error) => return error.into_response(),
    };

    let signals: Map<String, Value> = wire(&form)
        .into_iter()
        .filter(|(key, value)| before.get(key) != Some(value))
        .collect();

    Json(json!({ "patch": patch.as_str(), "signals": signals })).into_response()
}
