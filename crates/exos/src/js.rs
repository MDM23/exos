//! Client-side expressions, and the recorder that collects them.
//!
//! A [`Js<T>`] is a JavaScript expression that will produce a `T` in the
//! browser. You never write one by hand: they come out of signal handles,
//! route callers and the combinators here, all of which are ordinary typed
//! Rust. Renaming a signal or changing a payload is a compile error rather
//! than a string that quietly stops matching.
//!
//! # How a handler works
//!
//! A handler closure runs at render time, on the server. It does not do
//! anything in the browser; it records:
//!
//! ```ignore
//! on_click(|_| {
//!     gone.set(true);              // appends "$.gone = true"
//!     delete_file::post(entry.id); // appends "post(\"/files/3/delete\")"
//! })
//! ```
//!
//! So the whole Rust language is available while rendering, and only the
//! values that must survive to the browser are `Js<T>`.
//!
//! The flip side is that native control flow cannot record. [`Js<bool>`] is
//! deliberately not `bool`, so `if gone.get() { .. }` does not compile. That
//! is the intended failure: loud, at compile time, at the exact spot. Use
//! [`when`] to branch in the browser.

use core::{cell::RefCell, fmt, marker::PhantomData};

mod combinator;

// -----------------------------------------------------------------------------
//                                  EXPRESSIONS
// -----------------------------------------------------------------------------

/// A JavaScript expression yielding a `T` in the browser.
///
/// `T` is a claim about what the expression evaluates to. For everything this
/// API builds that claim holds by construction; [`Js::raw`] is the one place
/// you assert it yourself.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Js<T: ?Sized> {
    source: String,
    marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> Js<T> {
    /// Hand-written JavaScript.
    ///
    /// The type parameter is an assertion the compiler cannot check: you are
    /// promising this expression yields a `T`. It is the only unchecked thing
    /// in the API.
    ///
    /// ```
    /// # use exos::Js;
    /// let coarse = Js::<bool>::raw("matchMedia('(hover: none)').matches");
    /// assert_eq!(coarse.source(), "matchMedia('(hover: none)').matches");
    /// ```
    #[must_use]
    pub fn raw(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            marker: PhantomData,
        }
    }

    /// The JavaScript source.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Consumes the expression and returns its source.
    #[must_use]
    pub fn into_source(self) -> String {
        self.source
    }

    /// Reinterprets the expression's type without changing it.
    ///
    /// Carries the same caveat as [`Js::raw`]: nothing verifies the new claim.
    #[must_use]
    pub fn cast<U>(self) -> Js<U> {
        Js {
            source: self.source,
            marker: PhantomData,
        }
    }

    /// Parenthesises unless the expression is already atomic, so composing
    /// never changes precedence.
    fn grouped(&self) -> String {
        let atomic = self
            .source
            .chars()
            .all(|character| character.is_alphanumeric() || "_$.[]'\"".contains(character));

        if atomic {
            self.source.clone()
        } else {
            format!("({})", self.source)
        }
    }
}

impl<T: ?Sized> fmt::Display for Js<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.source)
    }
}

// -----------------------------------------------------------------------------
//                                  CONVERSIONS
// -----------------------------------------------------------------------------

/// Anything that can stand in for a client-side `T`.
///
/// This is what lets `gone.set(true)` and `gone.set(other.get())` both work: a
/// plain Rust value is serialized into the script as a literal, and a [`Js`]
/// expression is spliced in as source.
pub trait IntoJs<T> {
    /// The expression this value becomes.
    fn into_js(self) -> Js<T>;
}

impl<T> IntoJs<T> for Js<T> {
    fn into_js(self) -> Self {
        self
    }
}

/// What an action sends as its body.
///
/// Implemented both by a concrete value, serialized as a literal, and by a
/// model's signal handle, compiled to an object of signal reads. So a call
/// works whether the payload is state you already have or state the browser
/// holds.
pub trait IntoPayload<T> {
    /// A JavaScript object expression.
    fn payload(&self) -> String;
}

// -----------------------------------------------------------------------------
//                                 THE RECORDER
// -----------------------------------------------------------------------------

thread_local! {
    /// A stack, so a handler nested inside a [`when`] branch records into its
    /// own frame.
    static FRAMES: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

/// Runs `body` with a fresh recording frame and returns the script it built.
///
/// ```
/// # use exos::{Js, emit, record};
/// let script = record(|| {
///     emit("a()");
///     emit("b()");
/// });
///
/// assert_eq!(script, "a(); b()");
/// ```
#[must_use]
pub fn record(body: impl FnOnce()) -> String {
    FRAMES.with(|frames| frames.borrow_mut().push(Vec::new()));
    body();

    FRAMES.with(|frames| frames.borrow_mut().pop().unwrap_or_default().join("; "))
}

/// Appends a statement to the innermost recording frame.
///
/// # Panics
///
/// If no handler is being recorded. A signal write outside a handler has no
/// meaning, since it is not a value and there is nowhere for it to go, so this
/// says so rather than silently doing nothing.
pub fn emit(statement: impl Into<String>) {
    FRAMES.with(|frames| match frames.borrow_mut().last_mut() {
        Some(frame) => frame.push(statement.into()),
        None => panic!(
            "this only works inside a handler, such as on_click(|_| ..); \
             called outside one there is nothing to record into"
        ),
    });
}

/// Branches in the browser.
///
/// Native `if` cannot record, because [`Js<bool>`] is deliberately not `bool`.
///
/// ```
/// # use exos::{Js, emit, record, when};
/// let script = record(|| {
///     when(Js::<bool>::raw("$.ok"), |()| emit("go()"));
/// });
///
/// assert_eq!(script, "if ($.ok) { go() }");
/// ```
pub fn when(condition: impl IntoJs<bool>, body: impl FnOnce(())) {
    let script = record(|| body(()));
    emit(format!(
        "if ({}) {{ {script} }}",
        condition.into_js().source()
    ));
}

// -----------------------------------------------------------------------------
//                                  STATEMENTS
// -----------------------------------------------------------------------------

/// A speculative DOM write, for optimistic updates over server-owned state.
///
/// Deliberately not a signal. Mirroring server state into a signal gives one
/// attribute two sources of truth, and they drift the moment a patch lands.
/// This has no second copy, so the next patch overwrites it either way.
///
/// # Panics
///
/// If called outside a handler; see [`emit`].
pub fn attr_now(name: &str, value: impl IntoJs<bool>) {
    emit(format!(
        "attr({}, {})",
        quote_js(name),
        value.into_js().source()
    ));
}

/// Moves the keyboard focus to the first element matching `selector`.
///
/// The client half of [`Effect::focus`](crate::Effect::focus), for the field a
/// handler has just revealed. Focusing waits for the bindings this handler
/// scheduled, since an element still hidden when the handler returns cannot
/// take focus and the same click is usually what unhides it.
///
/// # Panics
///
/// If called outside a handler; see [`emit`].
pub fn focus_now(selector: &str) {
    emit(format!("focus({})", quote_js(selector)));
}

/// Clones a `<template>` into a container.
///
/// Adding a form row does not need a reactive list; it needs a copy. The clone
/// gets a fresh id, so per-row signals work inside it.
///
/// # Panics
///
/// If called outside a handler; see [`emit`].
pub fn append(template: &str, into: &str) {
    emit(format!(
        "append({}, {})",
        quote_js(template),
        quote_js(into)
    ));
}

/// Records a call to a route.
///
/// Generated by the method attributes; there is no need to call it directly.
///
/// # Panics
///
/// If called outside a handler; see [`emit`].
pub fn call(method: &str, url: &str, payload: Option<String>) {
    // `delete` is a reserved word in some positions, so the runtime exposes it
    // as `del`.
    let function = if method == "delete" { "del" } else { method };
    let url = quote_js(url);

    match payload {
        Some(body) => emit(format!("{function}({url}, {body})")),
        None => emit(format!("{function}({url})")),
    }
}

/// JSON-encodes a string so it can be spliced into an expression.
///
/// Everything crossing into JavaScript goes through this rather than being
/// pasted, so a quote in a value cannot break out of the expression.
#[must_use]
pub fn quote_js(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| String::from("\"\""))
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_collects_statements_in_order() {
        assert_eq!(
            record(|| {
                emit("a()");
                emit("b()");
            }),
            "a(); b()"
        );
    }

    #[test]
    fn a_nested_branch_records_into_its_own_frame() {
        let script = record(|| {
            emit("before()");
            when(Js::<bool>::raw("$.ok"), |()| emit("inner()"));
            emit("after()");
        });

        assert_eq!(script, "before(); if ($.ok) { inner() }; after()");
    }

    #[test]
    #[should_panic(expected = "only works inside a handler")]
    fn emitting_outside_a_handler_says_so() {
        emit("orphan()");
    }

    #[test]
    fn focus_is_a_statement_rather_than_a_round_trip() {
        assert_eq!(record(|| focus_now("#edit-3")), "focus(\"#edit-3\")");
    }

    #[test]
    fn a_url_is_encoded_rather_than_pasted() {
        let script = record(|| call("post", "/files/\"; evil(); \"", None));
        assert_eq!(script, r#"post("/files/\"; evil(); \"")"#);
    }

    #[test]
    fn delete_is_spelled_del_in_the_browser() {
        assert_eq!(
            record(|| call("delete", "/files/1", None)),
            r#"del("/files/1")"#
        );
    }
}
