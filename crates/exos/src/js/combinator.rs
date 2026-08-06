//! Building bigger expressions out of smaller ones.
//!
//! `!` is overloadable, so `!gone.get()` reads naturally. `==` and `&&` are
//! not, because [`PartialEq::eq`] must return a real `bool`, hence
//! [`Js::eq`](Js::eq) and [`Js::and`]. That is the one place this API is
//! uglier than the language it mimics, and there is no way around it.

use core::ops::Not;

use crate::js::{IntoJs, Js};

/// Serializable values become JSON literals.
macro_rules! literal {
    ($($type:ty),* $(,)?) => {
        $(
            impl IntoJs<$type> for $type {
                fn into_js(self) -> Js<$type> {
                    Js::raw(
                        serde_json::to_string(&self)
                            .unwrap_or_else(|_| String::from("null")),
                    )
                }
            }
        )*
    };
}

literal!(bool, f32, f64, i8, i16, i32, i64, String, u8, u16, u32, u64);

impl IntoJs<String> for &str {
    fn into_js(self) -> Js<String> {
        Js::raw(crate::js::quote(self))
    }
}

impl<T: serde::Serialize> IntoJs<Self> for Vec<T> {
    fn into_js(self) -> Js<Self> {
        Js::raw(serde_json::to_string(&self).unwrap_or_else(|_| String::from("[]")))
    }
}

impl Js<bool> {
    /// `a && b`.
    #[must_use]
    pub fn and(self, other: impl IntoJs<bool>) -> Self {
        Self::raw(format!(
            "{} && {}",
            self.grouped(),
            other.into_js().grouped()
        ))
    }

    /// `a || b`.
    #[must_use]
    pub fn or(self, other: impl IntoJs<bool>) -> Self {
        Self::raw(format!(
            "{} || {}",
            self.grouped(),
            other.into_js().grouped()
        ))
    }

    /// `!a`, also available as the `!` operator.
    ///
    /// Both exist because the method chains and the operator reads better on
    /// its own.
    #[must_use]
    #[expect(
        clippy::should_implement_trait,
        reason = "Not is implemented too; the method form is what chains"
    )]
    pub fn not(self) -> Self {
        Self::raw(format!("!{}", self.grouped()))
    }
}

impl Not for Js<bool> {
    type Output = Self;

    fn not(self) -> Self {
        Self::not(self)
    }
}

/// Equality, for the types that compare sensibly in JavaScript.
macro_rules! comparable {
    ($($type:ty),* $(,)?) => {
        $(
            impl Js<$type> {
                /// `===`, because `==` cannot be overloaded to return a
                /// [`Js<bool>`].
                #[must_use]
                pub fn eq(self, other: impl IntoJs<$type>) -> Js<bool> {
                    Js::raw(format!(
                        "{} === {}",
                        self.grouped(),
                        other.into_js().grouped()
                    ))
                }

                /// `!==`.
                #[must_use]
                pub fn ne(self, other: impl IntoJs<$type>) -> Js<bool> {
                    Js::raw(format!(
                        "{} !== {}",
                        self.grouped(),
                        other.into_js().grouped()
                    ))
                }
            }
        )*
    };
}

comparable!(String, bool, f64, i32, u32);

/// Ordering and arithmetic, for the number-ish types.
macro_rules! numeric {
    ($($type:ty),* $(,)?) => {
        $(
            impl Js<$type> {
                /// `a > b`.
                #[must_use]
                pub fn gt(self, other: impl IntoJs<$type>) -> Js<bool> {
                    Js::raw(format!("{} > {}", self.grouped(), other.into_js().grouped()))
                }

                /// `a < b`.
                #[must_use]
                pub fn lt(self, other: impl IntoJs<$type>) -> Js<bool> {
                    Js::raw(format!("{} < {}", self.grouped(), other.into_js().grouped()))
                }

                /// `a >= b`.
                #[must_use]
                pub fn ge(self, other: impl IntoJs<$type>) -> Js<bool> {
                    Js::raw(format!("{} >= {}", self.grouped(), other.into_js().grouped()))
                }

                /// `a <= b`.
                #[must_use]
                pub fn le(self, other: impl IntoJs<$type>) -> Js<bool> {
                    Js::raw(format!("{} <= {}", self.grouped(), other.into_js().grouped()))
                }

                /// `a + b`.
                #[must_use]
                pub fn plus(self, other: impl IntoJs<$type>) -> Self {
                    Self::raw(format!("{} + {}", self.grouped(), other.into_js().grouped()))
                }

                /// `a - b`.
                #[must_use]
                pub fn minus(self, other: impl IntoJs<$type>) -> Self {
                    Self::raw(format!("{} - {}", self.grouped(), other.into_js().grouped()))
                }
            }
        )*
    };
}

numeric!(f64, i32, u32);

impl Js<String> {
    /// Whether the string is empty.
    #[must_use]
    pub fn is_empty(self) -> Js<bool> {
        Js::raw(format!("{}.length === 0", self.grouped()))
    }

    /// The length in UTF-16 code units, which is what JavaScript counts.
    #[must_use]
    pub fn len(self) -> Js<u32> {
        Js::raw(format!("{}.length", self.grouped()))
    }

    /// Whether the string contains `needle`.
    #[must_use]
    pub fn contains(self, needle: impl IntoJs<Self>) -> Js<bool> {
        Js::raw(format!(
            "{}.includes({})",
            self.grouped(),
            needle.into_js().source()
        ))
    }

    /// The string without leading or trailing whitespace.
    #[must_use]
    pub fn trim(self) -> Self {
        Self::raw(format!("{}.trim()", self.grouped()))
    }
}

impl<T> Js<Vec<T>> {
    /// How many items there are.
    #[must_use]
    pub fn len(self) -> Js<u32> {
        Js::raw(format!("{}.length", self.grouped()))
    }

    /// Whether the collection is empty.
    #[must_use]
    pub fn is_empty(self) -> Js<bool> {
        Js::raw(format!("{}.length === 0", self.grouped()))
    }

    /// Whether it holds anything, which reads better than negating
    /// [`is_empty`](Self::is_empty) at a call site.
    #[must_use]
    pub fn any(self) -> Js<bool> {
        Js::raw(format!("{}.length > 0", self.grouped()))
    }

    /// Whether `value` is among the items.
    #[must_use]
    pub fn contains(self, value: impl IntoJs<T>) -> Js<bool> {
        Js::raw(format!(
            "{}.includes({})",
            self.grouped(),
            value.into_js().source()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_and_expressions_compose() {
        let count = Js::<u32>::raw("$.n");
        assert_eq!(count.gt(0_u32).source(), "$.n > 0");
    }

    #[test]
    fn grouping_preserves_what_the_call_says() {
        let a = Js::<bool>::raw("$.a");
        let b = Js::<bool>::raw("$.b");
        let c = Js::<bool>::raw("$.c");

        // Ungrouped this would read `$.a && $.b || $.c`, which parses
        // differently from the way the Rust reads.
        assert_eq!(a.and(b).or(c).source(), "($.a && $.b) || $.c");
    }

    #[test]
    fn not_is_available_as_an_operator() {
        assert_eq!((!Js::<bool>::raw("$.open")).source(), "!$.open");
    }

    #[test]
    fn strings_are_encoded_rather_than_pasted() {
        let name = Js::<String>::raw("$.name");

        assert_eq!(
            name.eq("o'brien\" or 1=1").source(),
            r#"$.name === ("o'brien\" or 1=1")"#
        );
    }
}
