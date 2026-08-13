//! Typed signal handles.
//!
//! A signal is a piece of state in the browser. It is declared once, in Rust,
//! and the handle is the only way to read or write it, so there is no name to
//! keep in step with anything.
//!
//! ```ignore
//! let gone = signal(false);   // Signal<bool>
//!
//! view! {
//!     <li id={ row_id } {&gone} {show(!gone.get())}>
//!         <button {on_click(|_| gone.set(true))}>"Delete"</button>
//!     </li>
//! }
//! ```
//!
//! Signals hold state the server does **not** own: a row pending deletion, a
//! modal, a draft input, a selection. Server-owned state lives in markup and
//! changes only by a patch. Mirroring it into a signal gives one value two
//! sources of truth, and they drift as soon as a patch lands.
//!
//! # Which signals have a name
//!
//! None of them, as far as anything that writes a template is concerned. Every
//! signal is keyed by a name in the client store, because the store is a map,
//! but no name here is written by hand:
//!
//! - A signal from [`signal`] is named after where it was declared.
//! - A `#[model]` field is named after its model and itself, so that every
//!   `signals()` call agrees and the template that declares one and the
//!   handler that writes it with [`Effect::set`](crate::Effect::set) name the
//!   same signal. The *field* name is still the JSON key of the request body,
//!   which is a contract; the signal name is not, and they are two different
//!   strings in the generated call.
//!
//! The exception is a name some other language owns, such as the sortable
//! plugin's `_order`. Those are written in JavaScript, so they reach a
//! template as a raw expression and [`view!`](crate::view) declares them where
//! it finds them. Nothing in Rust can hand out a handle to one.

use core::{marker::PhantomData, panic::Location};

use serde::Serialize;
use serde_json::Value;

use crate::{IntoJs, Js, emit};

/// A piece of client state.
///
/// The handle carries the type and the name. Names resolve against the DOM
/// scope the signal is declared on, so a hundred rows can each hold their own
/// without colliding, and nothing has to invent `gone_3`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signal<T> {
    name: String,
    initial: Value,
    marker: PhantomData<fn() -> T>,
}

impl<T> Signal<T> {
    /// A signal with a starting value, under a name the caller chooses.
    ///
    /// Private, because a name is a contract and the two things entitled to
    /// one build it themselves; see the module docs. `initial` is taken by
    /// value so that it also fixes `T`, sparing every call site a turbofish.
    fn new(name: impl Into<String>, initial: T) -> Self
    where
        T: Serialize,
    {
        let initial = serde_json::to_value(&initial).unwrap_or(Value::Null);
        Self::with_value(name, initial)
    }

    /// A signal whose starting value is already serialized.
    ///
    /// `#[model]` uses this: it has the model's `Default` as one JSON object
    /// and splits it per field, so it never needs `T: Serialize` for each
    /// field on its own. Public only because that expansion lands in another
    /// crate.
    #[doc(hidden)]
    #[must_use]
    pub fn with_value(name: impl Into<String>, initial: Value) -> Self {
        Self {
            name: name.into(),
            initial,
            marker: PhantomData,
        }
    }

    /// The name this signal is declared under.
    ///
    /// For a signal from [`signal`] this is generated and carries no promise:
    /// it is here to be read while debugging, not to be written into a
    /// template. Reach the signal through the handle instead.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The value it starts at.
    #[must_use]
    pub fn initial(&self) -> &Value {
        &self.initial
    }

    /// Reads the signal in the browser.
    #[must_use]
    pub fn get(&self) -> Js<T> {
        Js::raw(format!("$.{}", self.name))
    }

    /// Writes it.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`].
    pub fn set(&self, value: impl IntoJs<T>) {
        emit(format!("$.{} = {}", self.name, value.into_js().source()));
    }
}

impl Signal<bool> {
    /// Flips the signal.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`].
    pub fn toggle(&self) {
        emit(format!("$.{name} = !$.{name}", name = self.name));
    }
}

impl<T> Signal<Vec<T>> {
    /// Empties the collection.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`].
    pub fn clear(&self) {
        emit(format!("$.{} = []", self.name));
    }

    /// Appends a value.
    ///
    /// A fresh array is assigned rather than mutating in place, because that
    /// is what makes the runtime notice the change.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`].
    pub fn push(&self, value: impl IntoJs<T>) {
        emit(format!(
            "$.{name} = [...$.{name}, {}]",
            value.into_js().source(),
            name = self.name
        ));
    }

    /// Adds the value when absent and removes it when present, which is what
    /// a checkbox does.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`].
    pub fn toggle_member(&self, value: impl IntoJs<T>) {
        let value = value.into_js().into_source();

        emit(format!(
            "$.{name} = $.{name}.includes({value}) \
             ? $.{name}.filter(held => held !== {value}) \
             : [...$.{name}, {value}]",
            name = self.name
        ));
    }
}

/// A signal nothing off the page names.
///
/// This is how client state is declared. The handle is the whole interface:
/// put it in an attribute block to declare it, call [`get`](Signal::get) and
/// [`set`](Signal::set) to use it.
///
/// ```
/// # use exos::signal;
/// let gone = signal(false);          // Signal<bool>
/// assert_eq!(gone.get().source(), format!("$.{}", gone.name()));
/// ```
///
/// Where the same state is also what an action sends, use `#[model]`, which
/// names its signals per field rather than per call site so that every
/// `signals()` call hands back the same ones.
#[must_use]
#[track_caller]
pub fn signal<T: Serialize>(initial: T) -> Signal<T> {
    Signal::new(generated(Location::caller()), initial)
}

/// A typed reference to one field of a model.
///
/// Generated by `#[model]` as an associated constant, so error reporting and
/// form handling key off something the compiler checks rather than a string
/// that quietly stops matching when the field is renamed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Field<M> {
    name: &'static str,
    marker: PhantomData<fn() -> M>,
}

impl<M> Field<M> {
    /// Names a field of `M`.
    ///
    /// Called by the `#[model]` expansion, which lands in another crate. There
    /// is no reason to name a field by hand.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            marker: PhantomData,
        }
    }

    /// The field's name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

/// A name derived from where the signal was declared.
///
/// Being a function of the call site rather than a counter is what a live
/// fragment needs: its body renders inline and again from whatever publishes
/// it, and the two must agree byte for byte.
///
/// A helper called once per row hands every row the same name, which is
/// exactly right, because each row is its own scope. Two signals collide only
/// when one element declares both, and that is the clash a hand-written name
/// could always have.
fn generated(at: &Location<'_>) -> String {
    // FNV-1a over the call site, for a short name that needs no dependency.
    // Truncated to 32 bits: a collision has to survive landing on the same
    // element to matter at all.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;

    for byte in at.file().bytes() {
        hash = mix(hash, byte);
    }

    for byte in at.line().to_le_bytes() {
        hash = mix(hash, byte);
    }

    for byte in at.column().to_le_bytes() {
        hash = mix(hash, byte);
    }

    // Leading letter, because a JavaScript identifier cannot start with a
    // digit and this name is read back as `$.<name>`.
    format!("s{:08x}", hash >> 32)
}

const fn mix(hash: u64, byte: u8) -> u64 {
    (hash ^ byte as u64).wrapping_mul(0x0000_0100_0000_01b3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record;

    #[test]
    fn a_handle_reads_and_writes_by_name() {
        let gone = Signal::new("gone", false);

        assert_eq!(gone.get().source(), "$.gone");
        assert_eq!(record(|| gone.set(true)), "$.gone = true");
        assert_eq!(record(|| gone.toggle()), "$.gone = !$.gone");
    }

    #[test]
    fn collection_helpers_assign_rather_than_mutate() {
        let picked: Signal<Vec<u32>> = Signal::new("picked", Vec::new());

        assert_eq!(record(|| picked.clear()), "$.picked = []");
        assert!(record(|| picked.push(3_u32)).starts_with("$.picked = [...$.picked,"));
    }

    #[test]
    fn toggling_a_member_is_symmetric() {
        let picked: Signal<Vec<u32>> = Signal::new("picked", Vec::new());
        let script = record(|| picked.toggle_member(7_u32));

        assert!(script.contains("includes(7)"));
        assert!(script.contains("filter(held => held !== 7)"));
    }

    #[test]
    fn a_generated_name_is_a_javascript_identifier() {
        let gone = signal(false);

        assert!(gone.name().starts_with('s'));
        assert!(gone.name().chars().all(|c| c.is_ascii_alphanumeric()));
        assert_eq!(gone.initial(), &serde_json::json!(false));
    }

    /// What a live fragment depends on: its body renders inline and again from
    /// whatever publishes it, so one call site has to keep producing one name.
    #[test]
    fn one_call_site_always_produces_the_same_name() {
        fn declare() -> Signal<bool> {
            signal(false)
        }

        assert_eq!(declare().name(), declare().name());
    }

    #[test]
    fn two_call_sites_produce_different_names() {
        let first = signal(false);
        let second = signal(false);

        assert_ne!(first.name(), second.name());
    }

    #[test]
    fn a_field_token_carries_its_name() {
        struct Draft;
        const SKU: Field<Draft> = Field::new("sku");

        assert_eq!(SKU.name(), "sku");
    }
}
