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

/// A set of signals declared on an element, rendered as `data-signals`.
///
/// Built by [`signals!`](crate::signals). The element it sits on becomes the
/// scope those names belong to, so nothing has to invent unique names.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SignalScope(pub Value);

impl SignalScope {
    /// Wraps a JSON object of starting values.
    #[must_use]
    pub fn new(value: Value) -> Self {
        Self(value)
    }

    /// Adds `null` for any name not already present.
    ///
    /// Called by `view!` with the names it found referenced in this element's
    /// subtree. A name given a value is left alone, so an explicit declaration
    /// always wins over an inferred default.
    pub fn default_null(&mut self, names: &[&str]) {
        let Some(map) = self.0.as_object_mut() else {
            return;
        };

        for name in names {
            map.entry(*name).or_insert(Value::Null);
        }
    }
}

impl crate::Render for SignalScope {
    fn render_to(&self, out: &mut String) {
        // Escaped like any other attribute value: the JSON is full of quotes,
        // and `&quot;` is what keeps them inside the attribute.
        escape_into(&self.0.to_string(), out);
    }
}

impl crate::AttributeValue for SignalScope {
    type Output<'value>
        = &'value Self
    where
        Self: 'value;

    fn attribute_value(&self) -> Option<&Self> {
        Some(self)
    }
}

/// Declares signals on an element.
///
/// ```
/// # use exos::signals;
/// let scope = signals! { fav: true, gone: false };
/// assert_eq!(scope.0["fav"], serde_json::json!(true));
/// ```
///
/// Names are written as identifiers and become JSON keys; values are anything
/// `Serialize`.
#[macro_export]
macro_rules! signals {
    ($($name:ident : $value:expr),* $(,)?) => {
        $crate::SignalScope::new($crate::serde_json::json!({
            $(::core::stringify!($name): $value),*
        }))
    };
}

/// Records a script and returns it, for helpers that build one.
pub(crate) fn recorded(body: impl FnOnce()) -> String {
    record(body)
}

/// The source of an expression, for helpers that take one.
pub(crate) fn source<T>(expression: Js<T>) -> String {
    expression.into_source()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let first = Signal::new("a", false);
        let second = Signal::new("b", 0_u32);

        let mut attributes = Attributes::new();
        (&first, &second).write(&mut attributes);

        let rendered = attributes.render();

        assert_eq!(rendered.matches("data-signals").count(), 1);
        assert!(rendered.contains("&quot;a&quot;:false"));
        assert!(rendered.contains("&quot;b&quot;:0"));
    }

    #[test]
    fn setting_the_same_attribute_twice_keeps_the_last() {
        let mut attributes = Attributes::new();
        attributes.set("data-show", "$.a");
        attributes.set("data-show", "$.b");

        assert_eq!(attributes.render(), " data-show=\"$.b\"");
    }

    #[test]
    fn an_inferred_default_never_overwrites_a_declared_value() {
        let mut scope = signals! { fav: true };
        scope.default_null(&["fav", "gone"]);

        assert_eq!(scope.0["fav"], serde_json::json!(true));
        assert_eq!(scope.0["gone"], Value::Null);
    }
}
