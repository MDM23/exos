//! Attribute blocks: `<li {gone} {show(..)} {class("busy", ..)}>`.
//!
//! Each block contributes attributes to the element it sits on. Blocks are
//! merged rather than concatenated, because the style this API encourages is
//! to repeat them: two `class` blocks must produce one `class` attribute, and
//! two signal handles one `data-signals`. Emitting duplicates would be
//! silently wrong, since browsers keep the first and drop the rest.

use serde_json::{Map, Value};

use crate::{Js, Signal, escape_into, js::record, quote_js};

mod handler;
mod helper;

pub use crate::attributes::{
    handler::{Event, Target, on, on_change, on_click, on_input, on_submit},
    helper::{Attr, Bind, BindKind, Class, attr, bind, class, preserve, prop, show, text},
};

// -----------------------------------------------------------------------------
//                                  ATTRIBUTES
// -----------------------------------------------------------------------------

/// The attributes an element has collected from its blocks.
#[derive(Debug, Default)]
pub struct Attributes {
    classes: Vec<String>,
    signals: Map<String, Value>,
    /// Everything else. A later write wins, which is how a reader expects two
    /// settings of the same attribute to resolve.
    other: Vec<(String, String)>,
}

impl Attributes {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one class.
    pub fn class(&mut self, class: impl Into<String>) {
        self.classes.push(class.into());
    }

    /// Declares one signal on this element's scope.
    pub fn signal(&mut self, name: impl Into<String>, initial: Value) {
        self.signals.insert(name.into(), initial);
    }

    /// Sets one attribute, replacing any previous value.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();

        match self.other.iter_mut().find(|(held, _)| *held == name) {
            Some((_, slot)) => *slot = value,
            None => self.other.push((name, value)),
        }
    }

    /// Sets a bare attribute with no value, such as `disabled`.
    pub fn flag(&mut self, name: impl Into<String>) {
        self.set(name, String::new());
    }

    /// The value currently held for `name`, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.other
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, value)| value.as_str())
    }

    /// Renders as ` name="value"` pairs, ready to sit inside an open tag.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();

        if !self.classes.is_empty() {
            out.push_str(" class=\"");
            escape_into(&self.classes.join(" "), &mut out);
            out.push('"');
        }

        if !self.signals.is_empty() {
            out.push_str(" data-signals=\"");
            escape_into(&Value::Object(self.signals.clone()).to_string(), &mut out);
            out.push('"');
        }

        for (name, value) in &self.other {
            out.push(' ');
            out.push_str(name);

            if !value.is_empty() {
                out.push_str("=\"");
                escape_into(value, &mut out);
                out.push('"');
            }
        }

        out
    }

    /// Merges `expression` into an object-valued attribute such as
    /// `data-class`, so repeating a block accumulates rather than replaces.
    pub(crate) fn merge_object(&mut self, key: &str, name: &str, expression: &str) {
        let entry = format!("{}: {expression}", quote_js(name));

        let merged = match self.get(key) {
            // Strip the closing brace and append.
            Some(existing) if existing.len() > 2 => {
                format!("{}, {entry}}}", &existing[..existing.len() - 1])
            }
            _ => format!("{{{entry}}}"),
        };

        self.set(key, merged);
    }
}

// -----------------------------------------------------------------------------
//                                INTO ATTRIBUTES
// -----------------------------------------------------------------------------

/// Something that can be written into an element's attributes.
///
/// Implemented by every helper here, and by signal handles, which is what
/// makes `{gone}` declare a signal.
pub trait IntoAttributes {
    /// Contributes this value's attributes to `attributes`.
    fn write(self, attributes: &mut Attributes);
}

impl<T> IntoAttributes for &Signal<T> {
    fn write(self, attributes: &mut Attributes) {
        attributes.signal(self.name(), self.initial().clone());
    }
}

impl<T> IntoAttributes for Signal<T> {
    fn write(self, attributes: &mut Attributes) {
        attributes.signal(self.name(), self.initial().clone());
    }
}

/// Tuples, for declaring several one-off signals on one element.
macro_rules! tuple_attributes {
    ($($name:ident),+) => {
        impl<$($name: IntoAttributes),+> IntoAttributes for ($($name,)+) {
            #[expect(non_snake_case, reason = "one binding per tuple position")]
            fn write(self, attributes: &mut Attributes) {
                let ($($name,)+) = self;
                $($name.write(attributes);)+
            }
        }
    };
}

tuple_attributes!(A);
tuple_attributes!(A, B);
tuple_attributes!(A, B, C);
tuple_attributes!(A, B, C, D);
tuple_attributes!(A, B, C, D, E);
tuple_attributes!(A, B, C, D, E, F);

// -----------------------------------------------------------------------------
//                               INTERNAL HELPERS
// -----------------------------------------------------------------------------

// Shared by the helper and handler modules, which is the only reason these are
// not private to one of them.

/// Records a script and returns it, for helpers that build one.
pub(crate) fn recorded(body: impl FnOnce()) -> String {
    record(body)
}

/// The source of an expression, for helpers that take one.
pub(crate) fn source<T>(expression: Js<T>) -> String {
    expression.into_source()
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal;

    #[test]
    fn repeated_classes_merge_into_one_attribute() {
        let mut attributes = Attributes::new();
        class("is-open", Js::<bool>::raw("$.open")).write(&mut attributes);
        class("busy", Js::<bool>::raw("$.loading")).write(&mut attributes);

        let rendered = attributes.render();

        assert_eq!(rendered.matches("data-class").count(), 1);
        assert!(rendered.contains("&quot;is-open&quot;: $.open"));
        assert!(rendered.contains("&quot;busy&quot;: $.loading"));
    }

    #[test]
    fn repeated_signal_declarations_merge() {
        let first = signal(false);
        let second = signal(0_u32);

        let mut attributes = Attributes::new();
        (&first, &second).write(&mut attributes);

        let rendered = attributes.render();

        assert_eq!(rendered.matches("data-signals").count(), 1);
        assert!(rendered.contains(&format!("&quot;{}&quot;:false", first.name())));
        assert!(rendered.contains(&format!("&quot;{}&quot;:0", second.name())));
    }

    /// Two declarations on one line are still two signals, because a call site
    /// is a column as well as a line.
    #[test]
    fn signals_declared_side_by_side_get_different_names() {
        let (first, second) = (signal(false), signal(false));

        assert_ne!(first.name(), second.name());
    }

    #[test]
    fn setting_the_same_attribute_twice_keeps_the_last() {
        let mut attributes = Attributes::new();
        attributes.set("data-show", "$.a");
        attributes.set("data-show", "$.b");

        assert_eq!(attributes.render(), " data-show=\"$.b\"");
    }
}
