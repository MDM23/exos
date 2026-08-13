//! The attribute helpers a template reaches for.

use crate::{
    Js, Signal,
    attributes::{Attributes, IntoAttributes, source},
    quote_js,
};

/// One `name="value"` pair produced by a helper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attr(String, String);

impl Attr {
    /// A raw attribute, for the cases the helpers do not cover.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self(name.into(), value.into())
    }
}

impl IntoAttributes for Attr {
    fn write(self, attributes: &mut Attributes) {
        attributes.set(self.0, self.1);
    }
}

/// One class, toggled by a client-side condition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class(&'static str, Js<bool>);

impl IntoAttributes for Class {
    fn write(self, attributes: &mut Attributes) {
        attributes.merge_object("data-class", self.0, self.1.source());
    }
}

/// Both halves of a binding: the signal's name and its type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bind(String, &'static str);

impl IntoAttributes for Bind {
    fn write(self, attributes: &mut Attributes) {
        attributes.set("data-bind", self.0);
        attributes.set("data-bind-kind", self.1);
    }
}

/// What a bound control's value should become before it is sent.
///
/// A DOM input always hands back a string, but the signal's Rust type decides
/// what the server accepts: pushing `"1"` into a `Vec<u32>` fails to
/// deserialize. That type is known here, so it travels with the binding rather
/// than being guessed at runtime.
pub trait BindKind {
    /// The tag the runtime coerces by.
    const KIND: &'static str;
}

macro_rules! bind_kind {
    ($kind:literal: $($type:ty),* $(,)?) => {
        $(
            impl BindKind for $type {
                const KIND: &'static str = $kind;
            }
        )*
    };
}

bind_kind!("bool": bool);
bind_kind!("number": f32, f64, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);
bind_kind!("string": String);

/// A collection binds like its element: a `Vec<u32>` of checkbox values still
/// needs each value parsed as a number.
impl<T: BindKind> BindKind for Vec<T> {
    const KIND: &'static str = T::KIND;
}

/// Text content, kept in sync with the expression.
#[must_use]
pub fn text<T>(expression: Js<T>) -> Attr {
    Attr::new("data-text", source(expression))
}

/// Toggles the `hidden` attribute.
#[must_use]
pub fn show(condition: impl crate::IntoJs<bool>) -> Attr {
    Attr::new("data-show", condition.into_js().into_source())
}

/// One class toggle. Repeat the block for more.
#[must_use]
pub fn class(name: &'static str, condition: impl crate::IntoJs<bool>) -> Class {
    Class(name, condition.into_js())
}

/// One reactive attribute.
#[must_use]
pub fn attr<T>(name: &'static str, value: Js<T>) -> Attr {
    Attr::new(
        "data-attr",
        format!("{{{}: {}}}", quote_js(name), value.into_source()),
    )
}

/// One reactive property: `value`, `checked`, `indeterminate` and the like.
#[must_use]
pub fn prop<T>(name: &'static str, value: Js<T>) -> Attr {
    Attr::new(
        "data-prop",
        format!("{{{}: {}}}", quote_js(name), value.into_source()),
    )
}

/// Two-way binding for a form control.
///
/// On a checkbox whose signal is a `Vec`, checking collects the element's
/// `value` into the array, which is what lets selecting many rows work with no
/// per-row bookkeeping.
#[must_use]
pub fn bind<T: BindKind>(signal: &Signal<T>) -> Bind {
    Bind(signal.name().to_owned(), T::KIND)
}

/// Never morph this element: a media player, a third-party widget.
#[must_use]
pub fn preserve() -> Attr {
    Attr::new("data-preserve", String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal;

    #[test]
    fn a_binding_carries_the_signals_rust_type() {
        // Without this the runtime pushes the string "1" into what the server
        // declared as Vec<u32>, and the request fails to deserialize.
        let picked = signal(Vec::<u32>::new());
        let query = signal(String::new());

        let mut attributes = Attributes::new();
        bind(&picked).write(&mut attributes);
        assert!(attributes.render().contains("data-bind-kind=\"number\""));

        let mut attributes = Attributes::new();
        bind(&query).write(&mut attributes);
        assert!(attributes.render().contains("data-bind-kind=\"string\""));
    }

    #[test]
    fn show_renders_the_condition_verbatim() {
        let mut attributes = Attributes::new();
        show(Js::<bool>::raw("!$.gone")).write(&mut attributes);

        assert_eq!(attributes.render(), " data-show=\"!$.gone\"");
    }
}
