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

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Attributes, IntoAttributes, Js, Markup, Presence, emit, quote_js};

/// Many of one model, in the order they are on screen.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Rows<T>(Vec<T>);

impl<T> Rows<T> {
    /// The rows, in order.
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.0.iter()
    }

    /// How many there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T> FromIterator<T> for Rows<T> {
    fn from_iter<I: IntoIterator<Item = T>>(rows: I) -> Self {
        Self(rows.into_iter().collect())
    }
}

impl<T> IntoIterator for Rows<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a Rows<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
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
pub struct Row<H>(H);

impl<H> Row<H> {
    /// Wraps a handle as one row. Called by [`RowsOf::each`].
    #[doc(hidden)]
    pub const fn new(handle: H) -> Self {
        Self(handle)
    }
}

impl<H> core::ops::Deref for Row<H> {
    type Target = H;

    fn deref(&self) -> &H {
        &self.0
    }
}

/// Declares the row's fields and says where one row ends.
///
/// The marker is what the payload counts and what [`RowsOf::remove`] walks up
/// to, so a row without it is a row nothing can find.
impl<H> IntoAttributes for &Row<H>
where
    for<'a> &'a H: IntoAttributes,
{
    fn write(self, attributes: &mut Attributes) {
        IntoAttributes::write(&self.0, attributes);
        attributes.set("data-row", "");
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
    #[must_use]
    pub fn new(key: &'static str, state: &'static str, initial: Vec<Value>) -> Self {
        Self {
            key,
            state,
            initial,
            marker: core::marker::PhantomData,
        }
    }

    /// The name the group is rendered under. For a test, and for debugging.
    #[must_use]
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

    /// What is wrong with the collection, or the empty string.
    ///
    /// A rule on a `Rows` field is about how many rows there are rather than
    /// about any one of them. Only the server answers it: the rows are not a
    /// signal, so there is nothing on the client to count.
    #[must_use]
    pub fn error(&self) -> Js<String> {
        Js::raw(format!(
            "($.{state}[{key}] ?? \"\")",
            state = self.state,
            key = quote_js(self.key),
        ))
    }

    /// Whether anything is.
    #[must_use]
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
    #[must_use]
    pub fn each(&self, render: impl Fn(&Row<T::Handle>) -> Markup) -> Markup {
        let empty = serde_json::to_value(T::default()).unwrap_or(Value::Null);
        let blank = render(&Row::new(T::row(&empty, self.state, self.key)));

        let rows: String = self
            .initial
            .iter()
            .map(|value| render(&Row::new(T::row(value, self.state, self.key))).into_string())
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
    #[must_use]
    pub fn payload(&self) -> String {
        let keys: Vec<String> = T::keys().iter().map(|key| quote_js(key)).collect();

        format!("rows(el, {}, [{}])", quote_js(self.key), keys.join(", "))
    }
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
}
