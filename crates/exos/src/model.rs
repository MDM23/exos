//! Request bodies this framework's own client wrote.
//!
//! An action route is not a public API. The typed caller builds the body, the
//! extractor reads it, and both are generated from the same `#[model]`, so the
//! keys on the wire are the generated names from [`signal`](crate::signal)
//! rather than the field names:
//!
//! ```json
//! {"sc523a195": [1, 2], "s70c556ff": false}
//! ```
//!
//! That is not obfuscation for its own sake. Nothing outside the generated
//! pair can depend on the shape, so the shape stays free to change: batching
//! several actions into one request, sending only what changed, versioning the
//! envelope. A payload someone has written into a script is a payload that
//! cannot move again.
//!
//! # This is not authorization
//!
//! An opaque key is a "do not depend on this" marker, in the way an unstable
//! ABI is. It is not a control, and it does not try to be: the keys are in the
//! page's `data-signals` for anyone who opens the inspector. Every route still
//! authorizes for itself.
//!
//! # Bodies written elsewhere
//!
//! A body some other language writes needs names that language can spell, so
//! it keeps [`Json`](axum::Json) and a plain `Deserialize` struct. The
//! sortable plugin posting `{ order: $._order }` is the case in the tree, and
//! the extractor a handler names says which of the two it is.

use axum::{
    body::Bytes,
    extract::{FromRequest, Request, rejection::BytesRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

use crate::{Effect, Errors, Placement, Signal, Validate};

/// The wire name of every field of a model.
///
/// Implemented by `#[model]`, which is the only thing that can implement it
/// correctly, since it is the only thing that knows both names.
#[doc(hidden)]
pub trait ModelFields {
    /// `(wire key, field name)` per field, in declaration order.
    const FIELDS: &'static [(&'static str, &'static str)];

    /// Renames whatever `field` nests, in the direction `outwards` asks for.
    ///
    /// The identity for every field but a [`Rows`](crate::Rows), whose rows
    /// are models of their own and therefore have keys of their own to rename.
    /// Without it the renaming would stop one level down and the row would
    /// arrive as a body serde cannot read.
    #[doc(hidden)]
    #[must_use]
    fn nested(field: &str, value: Value, outwards: bool) -> Value {
        let _ = (field, outwards);
        value
    }
}

/// Renames the keys inside every row of a [`Rows`](crate::Rows) field.
///
/// Called by the `#[model]` expansion, which is the only thing that knows
/// which fields hold rows and of what.
#[doc(hidden)]
#[must_use]
pub fn nested_rows<T: ModelFields>(value: Value, outwards: bool) -> Value {
    let Value::Array(rows) = value else {
        return value;
    };

    let renamed = rows.into_iter().map(|row| {
        let Value::Object(fields) = row else {
            return row;
        };

        Value::Object(if outwards {
            outward::<T>(&fields)
        } else {
            inward::<T>(fields)
        })
    });

    Value::Array(renamed.collect())
}

/// A `#[model]` body, extracted from the wire form.
///
/// Stands where [`Json`](axum::Json) would:
///
/// ```ignore
/// #[exos::post("/files/archive")]
/// async fn archive(Model(selection): Model<Selection>) -> Effect
/// ```
///
/// Writing `Json<Selection>` instead still compiles, because a model is an
/// ordinary `Deserialize` type. It fails at runtime with a missing field,
/// since the keys that arrive are not the ones serde is looking for.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Model<T>(pub T);

impl<S, T> FromRequest<S> for Model<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ModelRejection;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(request, state).await?;
        let wire: Value = serde_json::from_slice(&bytes)?;

        let Value::Object(wire) = wire else {
            return Err(ModelRejection::NotAnObject);
        };

        let model: T = serde_json::from_value(Value::Object(inward::<T>(wire)))?;

        // Checked here rather than in the handler, so that there is no call
        // site to forget and a body runs only against a value whose shape
        // held. What a rule cannot answer, the handler still can.
        let errors = model.validate();

        if errors.is_empty() {
            Ok(Self(model))
        } else {
            Err(ModelRejection::Refused {
                state: T::STATE,
                errors,
            })
        }
    }
}

/// Renames the wire keys back to the field names serde is expecting.
///
/// Only the keys this model declares survive. Anything else was not written by
/// the caller this route has, and a field spelled the way the struct spells it
/// is exactly that: dropping it is what makes a hand-written body fail with
/// the missing field it is missing.
fn inward<T: ModelFields>(mut wire: Map<String, Value>) -> Map<String, Value> {
    let mut fields = Map::new();

    for (key, field) in T::FIELDS {
        if let Some(value) = wire.remove(*key) {
            fields.insert((*field).to_owned(), T::nested(field, value, false));
        }
    }

    fields
}

/// The same renaming, outwards, for [`to_wire`].
fn outward<T: ModelFields>(fields: &Map<String, Value>) -> Map<String, Value> {
    let mut wire = Map::new();

    for (key, field) in T::FIELDS {
        if let Some(value) = fields.get(*field) {
            wire.insert((*key).to_owned(), T::nested(field, value.clone(), true));
        }
    }

    wire
}

/// Serializes a model into the wire form [`Model`] reads.
///
/// The keys are private, so this is how anything other than the generated
/// caller builds a body: the `#[model]` expansion uses it when the server
/// already knows what to send, and a test posting to its own action wants it
/// rather than a JSON literal that would have to be kept in step by hand.
///
/// ```ignore
/// let body = exos::to_wire(&Selection { picked: vec![], fail: true });
/// ```
///
/// The value serializes under its field names and the keys are renamed
/// afterwards, so the model's own `Serialize` stays whatever it is for every
/// other use it has.
#[must_use]
pub fn to_wire<T: ModelFields + Serialize>(value: &T) -> String {
    let Ok(Value::Object(fields)) = serde_json::to_value(value) else {
        return String::from("{}");
    };

    Value::Object(outward::<T>(&fields)).to_string()
}

/// Why a [`Model`] body was refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ModelRejection {
    /// The body could not be read at all.
    #[error("could not read the request body")]
    Unreadable(#[from] BytesRejection),

    /// The body parsed, but a JSON object is what a model is.
    #[error("expected a JSON object")]
    NotAnObject,

    /// The body was not JSON, or a field did not match its declared type.
    ///
    /// The message names the field the model declares rather than the key that
    /// arrived, because the keys are renamed before serde sees them.
    #[error(transparent)]
    Invalid(#[from] serde_json::Error),

    /// The body arrived intact and broke a rule the model declares.
    ///
    /// The only variant a viewer is meant to see, and the only one that
    /// answers with something to do about it rather than with a sentence for a
    /// log.
    #[error("the body broke a rule this model declares")]
    Refused {
        /// The signal the record is written to.
        state: &'static str,
        /// What is wrong, per field.
        errors: Errors,
    },
}

impl IntoResponse for ModelRejection {
    fn into_response(self) -> Response {
        // A refusal a page can act on. Everything else here is a body nobody
        // wrote by hand and no page can do anything about, so it stays a
        // sentence the console prints.
        if let Self::Refused { state, errors } = self {
            let record = Signal::with_value(state, Value::Null, Placement::Document);

            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                // The caret goes to the first thing wrong, and the selector is
                // the same attribute the control marks itself with, so nothing
                // here has to know an id. The record is written first and the
                // effects that read it flush on a microtask queued before the
                // focus step's, so the mark is in place when focus looks.
                Effect::set(&record, errors).focus("[aria-invalid=\"true\"]"),
            )
                .into_response();
        }

        let status = match self {
            // Nothing arrived to be understood, so this is not the body being
            // wrong.
            Self::Unreadable(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };

        (status, self.to_string()).into_response()
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
    struct Selection {
        picked: Vec<u32>,
        fail: bool,
    }

    impl ModelFields for Selection {
        const FIELDS: &'static [(&'static str, &'static str)] =
            &[("sc523a195", "picked"), ("s70c556ff", "fail")];
    }

    fn read(body: &str) -> Result<Selection, serde_json::Error> {
        let wire: Map<String, Value> = serde_json::from_str(body).expect("valid json");
        serde_json::from_value(Value::Object(inward::<Selection>(wire)))
    }

    #[test]
    fn a_wire_key_arrives_as_the_field_it_names() {
        let selection = read(r#"{"sc523a195":[1,2],"s70c556ff":true}"#).expect("a body");

        assert_eq!(
            selection,
            Selection {
                picked: vec![1, 2],
                fail: true
            }
        );
    }

    /// The wire is private, so spelling a field name is not another way in.
    #[test]
    fn a_field_name_on_the_wire_is_not_accepted() {
        let error = read(r#"{"picked":[1,2],"fail":true}"#).expect_err("no such key");

        assert!(error.to_string().contains("picked"), "{error}");
    }

    #[test]
    fn what_the_server_writes_is_what_the_extractor_reads() {
        let selection = Selection {
            picked: vec![3],
            fail: false,
        };

        assert_eq!(read(&to_wire(&selection)).expect("a body"), selection);
    }

    /// A key nothing declares is dropped rather than passed through, so it
    /// cannot collide with a field name on the way in.
    #[test]
    fn an_unknown_key_is_dropped() {
        let selection = read(r#"{"sc523a195":[1],"s70c556ff":false,"other":9}"#).expect("a body");

        assert_eq!(selection.picked, vec![1]);
    }
}
