//! Repeating groups: a model field holding many of another model.
//!
//! A form with rows in it is one submission, and the rows are the browser's
//! until it is made. Adding one costs no round trip and the server holds
//! nothing half-typed:
//!
//! ```ignore
//! #[exos::model]
//! #[derive(Debug, Default, Deserialize, Serialize)]
//! struct Order {
//!     lines: Rows<Line>,
//! }
//!
//! #[exos::model]
//! #[derive(Debug, Default, Deserialize, Serialize)]
//! struct Line {
//!     #[valid(required)]
//!     sku: String,
//! }
//! ```
//!
//! ```ignore
//! { form.lines.each(|line| view! {
//!     <li {line}>
//!         <input {bind(&line.sku)}>
//!         <p class="error" {text(line.sku.error())}></p>
//!         <button {on_click(|_| form.lines.remove())}>"Remove"</button>
//!     </li>
//! }) }
//!
//! <button {on_click(|_| form.lines.add())}>"Add a line"</button>
//! ```
//!
//! # How it works, and why there are no ids
//!
//! [`each`](RowsOf::each) renders the row markup twice over: once into a
//! `<template>`, and once per row the model opened with. [`add`](RowsOf::add)
//! clones that template, and the runtime keys a signal scope per **element**,
//! so the clone's fields are its own without anything naming them. There is no
//! id to invent, no list for the server to keep, and no request until submit.
//!
//! That is the same mechanism a row's own [`signal`](crate::signal) has always
//! used. What is new is only that the submission can find them: the payload
//! reads the rows out of the group at the moment it is sent, in the order they
//! are on screen.
//!
//! # The handle goes on the row's root element
//!
//! `{line}` declares the row's fields and marks the element as one row. It has
//! to sit on the outermost element of the row, because that element is the
//! scope its fields live in and the one the payload collects.
//!
//! # Rows are numbered by position, not by identity
//!
//! What a rule says about the third row is written under the third row, so
//! removing a row would leave every message after it about a different one.
//! They are retired rather than renumbered: what was said about the rows from
//! there on goes when the row does, and the next submission says what is wrong
//! with the rows as they then are. Editing a row clears its own message either
//! way, because the control answers its rules as it is typed into.
//!
//! # Trashing a row rather than removing it
//!
//! [`remove`](RowsOf::remove) takes a row off the page there and then. Where a
//! row has something to lose, [`trash`](RowsOf::trash) is the other choice: a
//! row the browser added still goes, having nothing to lose, but one the form
//! opened with stays on screen, trashed and with its controls locked, until
//! the form is sent. The reader sees what the save will drop, and
//! [`restore`](RowsOf::restore) takes it back out of the trash.
//!
//! A trashed row says so in `data-row="trashed"`, and showing it is the
//! stylesheet's, which is why trashing is chosen rather than given:
//!
//! ```css
//! [data-row="trashed"] { opacity: 0.5; }
//! ```
//!
//! The submission carries what each row is, so a revision renders an added row
//! as added and a trashed one as trashed. A handler never meets a trashed row:
//! [`Rows`] keeps it out of everything it hands out, and a position counts only
//! the rows that stay.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de, de::DeserializeOwned, ser};
use serde_json::Value;

use crate::{Attributes, IntoAttributes, Js, Markup, Presence, emit, quote_js};

/// The wire key a row's kind travels under. No field can spell it, for the
/// reason the runtime's dirty flags give.
pub(crate) const KIND: &str = "~row";

/// Many of one model, in the order they are on screen.
///
/// Holds every row the form showed and hands out only the ones it keeps:
/// iterating, counting and validating never meet a row the reader trashed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Rows<T>(Vec<Entry<T>>);

impl<T> Rows<T> {
    /// The rows that stay, in order.
    pub fn iter(&self) -> RowsIter<'_, T> {
        RowsIter(self.0.iter())
    }

    /// How many stay.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Whether none do.
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

/// Rows built on the server are the ones a form opens with.
impl<T> FromIterator<T> for Rows<T> {
    fn from_iter<I: IntoIterator<Item = T>>(rows: I) -> Self {
        Self(rows.into_iter().map(Entry::Opened).collect())
    }
}

impl<T> IntoIterator for Rows<T> {
    type Item = T;
    type IntoIter = RowsIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        RowsIntoIter(self.0.into_iter())
    }
}

impl<'a, T> IntoIterator for &'a Rows<T> {
    type Item = &'a T;
    type IntoIter = RowsIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The rows that stay, borrowed. Returned by [`Rows::iter`].
#[derive(Clone, Debug)]
pub struct RowsIter<'a, T>(core::slice::Iter<'a, Entry<T>>);

impl<'a, T> Iterator for RowsIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        self.0.find_map(Entry::kept)
    }
}

/// The rows that stay, owned. Returned by [`Rows::into_iter`].
#[derive(Debug)]
pub struct RowsIntoIter<T>(std::vec::IntoIter<Entry<T>>);

impl<T> Iterator for RowsIntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.0.find_map(Entry::into_kept)
    }
}

/// A list on the wire. A row the form opened with is itself, and any other
/// carries its kind under `~row`, which is what lets a revision render it
/// back as what it was. A row that is not an object has nowhere to carry it
/// and goes as itself; only a model is a row the browser adds or trashes.
impl<T: Serialize> Serialize for Rows<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use ser::{Error, SerializeSeq};

        let mut rows = serializer.serialize_seq(Some(self.0.len()))?;

        for entry in &self.0 {
            match entry {
                Entry::Opened(row) => rows.serialize_element(row)?,
                Entry::Added(row) => {
                    let row = serde_json::to_value(row).map_err(S::Error::custom)?;
                    rows.serialize_element(&marked(row, "added"))?;
                }
                Entry::Trashed(row) => rows.serialize_element(&marked(row.clone(), "trashed"))?,
            }
        }

        rows.end()
    }
}

/// The same list, read back. A row without a kind is one the form opened
/// with, so a body written by hand, or by a client that predates the kinds,
/// reads as it always did.
impl<'de, T: DeserializeOwned> Deserialize<'de> for Rows<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use de::Error;

        Vec::<Value>::deserialize(deserializer)?
            .into_iter()
            .map(|mut row| {
                let kind = row.as_object_mut().and_then(|fields| fields.remove(KIND));

                match kind.as_ref().and_then(Value::as_str) {
                    Some("trashed") => Ok(Entry::Trashed(row)),
                    Some("added") => T::deserialize(row).map(Entry::Added),
                    _ => T::deserialize(row).map(Entry::Opened),
                }
                .map_err(D::Error::custom)
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }
}

/// A row is there or it is not, so `required` on a `Rows` field asks for one.
impl<T> Presence for Rows<T> {
    fn is_present(&self) -> bool {
        !self.is_empty()
    }

    fn present(_: Js<Self>) -> Js<bool> {
        // The rows are not a signal, so there is nothing here to count. The
        // macro never asks for this half; it is here because the trait is one
        // question asked twice and answering it wrongly would be worse.
        Js::raw("false")
    }
}

/// A model that can be one row of another's [`Rows`] field.
///
/// Implemented by `#[model]` for every model, because a model does not know
/// which of them it will be.
pub trait RowModel {
    /// The signal handle `#[model]` generates for it.
    type Handle;

    /// That handle, holding `initial`, with its fields on the row's element
    /// rather than on the document and its messages going to `state`.
    #[doc(hidden)]
    fn row(initial: &Value, state: &'static str, group: &'static str) -> Self::Handle;

    /// The wire key of every field, for the payload to collect.
    #[doc(hidden)]
    fn keys() -> &'static [&'static str];
}

/// One row's handle.
///
/// Everything the row model's handle does, through [`Deref`](core::ops::Deref),
/// plus the one thing a row adds: put it on the row's root element to declare
/// the row's fields and mark it as a row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row<H> {
    handle: H,
    kind: &'static str,
}

impl<H> Row<H> {
    /// Wraps a handle as one row of `kind`, which is what `data-row` says.
    /// Called by [`RowsOf::each`].
    #[doc(hidden)]
    pub const fn new(handle: H, kind: &'static str) -> Self {
        Self { handle, kind }
    }
}

impl<H> core::ops::Deref for Row<H> {
    type Target = H;

    fn deref(&self) -> &H {
        &self.handle
    }
}

/// Declares the row's fields and says where one row ends, and what it is.
///
/// The marker is what the payload counts and what [`RowsOf::remove`] walks up
/// to, so a row without it is a row nothing can find.
impl<H> IntoAttributes for &Row<H>
where
    for<'a> &'a H: IntoAttributes,
{
    fn write(self, attributes: &mut Attributes) {
        IntoAttributes::write(&self.handle, attributes);
        attributes.set("data-row", self.kind);
    }
}

/// The handle for a [`Rows`] field: the rows it opened with, and what a
/// template does with them.
///
/// Written out rather than derived for the reason [`Field`](crate::Field)
/// gives: a derive would put a `T: Clone` on the impl and a model is not
/// `Clone`. Nothing here holds a `T`.
pub struct RowsOf<T> {
    key: &'static str,
    state: &'static str,
    initial: Vec<Value>,
    marker: core::marker::PhantomData<fn() -> T>,
}

impl<T> Clone for RowsOf<T> {
    fn clone(&self) -> Self {
        Self::new(self.key, self.state, self.initial.clone())
    }
}

impl<T> core::fmt::Debug for RowsOf<T> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("RowsOf")
            .field("key", &self.key)
            .field("rows", &self.initial.len())
            .finish_non_exhaustive()
    }
}

impl<T> RowsOf<T> {
    /// Called by the `#[model]` expansion, which knows all three.
    #[doc(hidden)]
    pub fn new(key: &'static str, state: &'static str, initial: Vec<Value>) -> Self {
        Self {
            key,
            state,
            initial,
            marker: core::marker::PhantomData,
        }
    }

    /// The name the group is rendered under. For a test, and for debugging.
    pub const fn key(&self) -> &'static str {
        self.key
    }

    /// Adds an empty row, in the browser, without asking the server.
    ///
    /// What the server said about how many rows there are goes with the click,
    /// since it is about a form that no longer exists. What it said about the
    /// rows themselves stays: a new row goes on the end and moves nobody.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`](crate::emit).
    pub fn add(&self) {
        emit(format!(
            "addRow(el, {}, {})",
            quote_js(self.key),
            quote_js(self.state)
        ));
    }

    /// Removes the row this was recorded inside.
    ///
    /// Takes no argument because the click already says which row: the runtime
    /// walks up from the element the handler is on. Outside a row it removes
    /// nothing.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`](crate::emit).
    pub fn remove(&self) {
        emit(format!(
            "dropRow(el, {}, {})",
            quote_js(self.key),
            quote_js(self.state)
        ));
    }

    /// Removes the row this was recorded inside when the form is sent: drops
    /// one the browser added, and trashes one the form opened with until then
    /// as `data-row="trashed"`, its controls locked, for a stylesheet to show
    /// and [`restore`](Self::restore) to take back.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`](crate::emit).
    pub fn trash(&self) {
        emit(format!(
            "trashRow(el, {}, {})",
            quote_js(self.key),
            quote_js(self.state)
        ));
    }

    /// Takes the row this was recorded inside back out of the
    /// [`trash`](Self::trash), into the submission and counted again. On a
    /// row nobody trashed it does nothing.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`](crate::emit).
    pub fn restore(&self) {
        emit(format!(
            "restoreRow(el, {}, {})",
            quote_js(self.key),
            quote_js(self.state)
        ));
    }

    /// What is wrong with the collection, or the empty string.
    ///
    /// A rule on a `Rows` field is about how many rows there are rather than
    /// about any one of them. Only the server answers it: the rows are not a
    /// signal, so there is nothing on the client to count.
    pub fn error(&self) -> Js<String> {
        Js::raw(format!(
            "($.{state}[{key}] ?? \"\")",
            state = self.state,
            key = quote_js(self.key),
        ))
    }

    /// Whether anything is.
    pub fn invalid(&self) -> Js<bool> {
        !self.error().is_empty()
    }
}

impl<T: RowModel + Default + Serialize> RowsOf<T> {
    /// The rows, and the template [`add`](Self::add) clones.
    ///
    /// `render` is called once per row the model opened with, and once more
    /// with an empty row for the template, so the markup a new row gets is the
    /// markup every other row got and there is no second place to keep it.
    ///
    /// The empty row is the row model's `Default` rather than nothing at all,
    /// because a field that starts as `null` is a field the server cannot read
    /// back: a row added and never typed into still has to be a row.
    ///
    /// The template renders an added row, since that is what a clone of it
    /// is. The rows the model opened with say what they are, which after a
    /// revision may be added or trashed too.
    pub fn each(&self, render: impl Fn(&Row<T::Handle>) -> Markup) -> Markup {
        let empty = serde_json::to_value(T::default()).unwrap_or(Value::Null);
        let blank = render(&Row::new(T::row(&empty, self.state, self.key), "added"));

        let rows: String = self
            .initial
            .iter()
            .map(|value| {
                let kind = match value.get(KIND).and_then(Value::as_str) {
                    Some("added") => "added",
                    Some("trashed") => "trashed",
                    _ => "",
                };

                render(&Row::new(T::row(value, self.state, self.key), kind)).into_string()
            })
            .collect();

        Markup(format!(
            "<div data-rows=\"{key}\"><template>{blank}</template>{rows}</div>",
            key = self.key,
            blank = blank.into_string(),
        ))
    }

    /// Reads the rows out of the group, in the order they are on screen.
    ///
    /// Not a client-side loop over data: the values are in the DOM already and
    /// this walks them once, at the moment the body is built.
    #[doc(hidden)]
    pub fn payload(&self) -> String {
        let keys: Vec<String> = T::keys().iter().map(|key| quote_js(key)).collect();

        format!("rows(el, {}, [{}])", quote_js(self.key), keys.join(", "))
    }
}

/// One row, and what the browser did with it.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Entry<T> {
    /// The form opened with it.
    Opened(T),
    /// The browser added it since.
    Added(T),
    /// The form opened with it and the reader trashed it. Kept as it was sent,
    /// since nothing reads it as a `T` and a row on its way out need not be a
    /// valid one.
    Trashed(Value),
}

impl<T> Entry<T> {
    const fn kept(&self) -> Option<&T> {
        match self {
            Self::Opened(row) | Self::Added(row) => Some(row),
            Self::Trashed(_) => None,
        }
    }

    fn into_kept(self) -> Option<T> {
        match self {
            Self::Opened(row) | Self::Added(row) => Some(row),
            Self::Trashed(_) => None,
        }
    }
}

/// `row` saying it is of `kind`, where it is an object that can say anything.
fn marked(mut row: Value, kind: &str) -> Value {
    if let Some(fields) = row.as_object_mut() {
        fields.insert(KIND.to_owned(), Value::from(kind));
    }

    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_keep_the_order_they_were_given() {
        let rows: Rows<String> = ["b", "a"].into_iter().map(str::to_owned).collect();

        assert_eq!(
            rows.iter().map(String::as_str).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(rows.len(), 2);
    }

    /// The wire shape is the list itself, so the payload builds an array and
    /// the extractor reads one.
    #[test]
    fn rows_are_a_list_on_the_wire() {
        let rows: Rows<String> = ["a"].into_iter().map(str::to_owned).collect();

        assert_eq!(
            serde_json::to_string(&rows).expect("serializes"),
            r#"["a"]"#
        );
    }

    #[test]
    fn a_rows_field_is_present_when_it_holds_a_row() {
        let empty: Rows<String> = Rows::default();
        let one: Rows<String> = [String::new()].into_iter().collect();

        assert!(!empty.is_present());
        assert!(one.is_present());
    }

    /// A trashed row is carried so a revision can render it back, and met by
    /// nothing that reads the rows: not iterating, not counting, not a rule.
    #[test]
    fn a_trashed_row_is_carried_but_not_kept() {
        let wire = serde_json::json!([
            { "name": "Ada" },
            { "name": "Grace", "~row": "trashed" },
            { "name": "Edsger", "~row": "added" },
        ]);

        let rows: Rows<Value> = serde_json::from_value(wire.clone()).expect("deserializes");

        assert_eq!(
            rows.iter().map(|row| &row["name"]).collect::<Vec<_>>(),
            ["Ada", "Edsger"]
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(serde_json::to_value(&rows).expect("serializes"), wire);
    }

    /// A trashed row need not be valid, since nobody reads it as one.
    #[test]
    fn a_trashed_row_is_not_read_as_a_row() {
        let wire = serde_json::json!([{ "name": 1, "~row": "trashed" }]);

        let rows: Rows<Named> = serde_json::from_value(wire).expect("deserializes");

        assert!(rows.is_empty());
    }

    #[derive(Debug, Deserialize)]
    struct Named {
        #[expect(dead_code, reason = "read only to prove a trashed row is never read")]
        name: String,
    }
}
