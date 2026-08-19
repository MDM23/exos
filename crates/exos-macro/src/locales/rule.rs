//! How a vendored rule becomes Rust.
//!
//! [`exos_cldr`] hands out comparisons rather than an evaluator, so this is
//! where a row of the table turns into the expression an application compiles.

use exos_cldr::{Category, Direction, Rule, Test};
use proc_macro2::{Literal, TokenStream};
use quote::quote;

/// The condition as a `bool` expression over `count`.
///
/// `None` where the rule always applies, since that is the arm every rule list
/// ends with and an `if true` would be a strange way to write it.
pub(crate) fn condition(rule: &Rule) -> Option<TokenStream> {
    let alone = rule.condition.len() == 1;

    let clauses: Vec<TokenStream> = rule
        .condition
        .iter()
        .map(|clause| {
            let tests = clause.iter().map(comparison);
            let joined = quote! { #(#tests)&&* };

            // Parenthesised only where an `or` sits above them, since rustc
            // has a lint for the pair a lone clause would carry.
            if alone || clause.len() == 1 {
                joined
            } else {
                quote! { (#joined) }
            }
        })
        .collect();

    (!clauses.is_empty()).then(|| quote! { #(#clauses)||* })
}

/// One comparison as a `bool` expression over `count`.
///
/// A single value becomes `==`, since `matches!(count, 1)` is a strange way to
/// write `count == 1` in code somebody may well read in an expansion.
fn comparison(test: &Test) -> TokenStream {
    let value = match test.modulus {
        Some(modulus) => {
            let modulus = Literal::u64_unsuffixed(modulus);
            quote! { count % #modulus }
        }
        None => quote! { count },
    };

    if let [(single, same)] = test.ranges
        && single == same
    {
        let single = Literal::u64_unsuffixed(*single);

        return if test.negated {
            quote! { #value != #single }
        } else {
            quote! { #value == #single }
        };
    }

    let patterns = test.ranges.iter().map(|&(low, high)| {
        let start = Literal::u64_unsuffixed(low);

        if low == high {
            quote! { #start }
        } else {
            let end = Literal::u64_unsuffixed(high);
            quote! { #start ..= #end }
        }
    });

    let matched = quote! { ::core::matches!(#value, #(#patterns)|*) };

    if test.negated {
        quote! { !#matched }
    } else {
        matched
    }
}

/// What a category is called as a variant of a generated `Plural`.
pub(crate) const fn category(category: Category) -> &'static str {
    match category {
        Category::Few => "Few",
        Category::Many => "Many",
        Category::One => "One",
        Category::Other => "Other",
        Category::Two => "Two",
        Category::Zero => "Zero",
    }
}

/// What a direction is called as a variant of `exos::Direction`.
pub(crate) const fn direction(direction: Direction) -> &'static str {
    match direction {
        Direction::LeftToRight => "LeftToRight",
        Direction::RightToLeft => "RightToLeft",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `matches!(count, 1)` is a strange thing to find in an expansion when
    /// the language has an operator for it.
    #[test]
    fn a_single_value_compares_rather_than_matches() {
        let test = Test {
            modulus: None,
            ranges: &[(1, 1)],
            negated: false,
        };

        assert_eq!(comparison(&test).to_string(), "count == 1");
    }

    #[test]
    fn a_range_becomes_a_pattern_over_the_modulus() {
        let test = Test {
            modulus: Some(100),
            ranges: &[(12, 14)],
            negated: true,
        };

        assert_eq!(
            comparison(&test).to_string(),
            "! :: core :: matches ! (count % 100 , 12 ..= 14)"
        );
    }

    /// A rule that always applies is the arm the chain ends in rather than a
    /// condition, and saying so is what keeps `if true` out of an expansion.
    #[test]
    fn an_unconditional_rule_has_no_expression() {
        let rule = Rule {
            category: Category::Other,
            samples: "",
            condition: &[],
        };

        assert!(condition(&rule).is_none());
    }

    /// Clauses are an `or` over `and`s, and the parentheses only appear where
    /// the two actually meet.
    #[test]
    fn clauses_are_parenthesised_only_where_an_or_sits_above_them() {
        const PAIR: &[Test] = &[
            Test {
                modulus: None,
                ranges: &[(1, 1)],
                negated: true,
            },
            Test {
                modulus: Some(10),
                ranges: &[(0, 1)],
                negated: false,
            },
        ];

        const LONE: &[Test] = &[Test {
            modulus: Some(10),
            ranges: &[(2, 4)],
            negated: false,
        }];

        let single = Rule {
            category: Category::Few,
            samples: "",
            condition: &[PAIR],
        };

        let several = Rule {
            category: Category::Many,
            samples: "",
            condition: &[PAIR, LONE],
        };

        let rendered = |rule| {
            condition(rule)
                .map(|tokens| tokens.to_string())
                .unwrap_or_default()
        };

        assert!(
            !rendered(&single).starts_with('('),
            "one clause needs no pair"
        );
        assert!(rendered(&several).starts_with('('), "two clauses do");
    }
}
