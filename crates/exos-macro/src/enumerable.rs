//! Expansion of `#[derive(Enumerable)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

/// Expands the enum into the list of its own values.
pub(crate) fn expand(item: TokenStream) -> TokenStream {
    let input = match syn::parse2::<DeriveInput>(item) {
        Ok(input) => input,
        Err(error) => return error.to_compile_error(),
    };

    let Data::Enum(data) = &input.data else {
        return syn::Error::new_spanned(
            &input.ident,
            "a domain a message branches on is an enum, since branching is picking a string per \
             value and there has to be a knowable set of them",
        )
        .to_compile_error();
    };

    let carrying = data
        .variants
        .iter()
        .find(|variant| !matches!(variant.fields, Fields::Unit));

    if let Some(variant) = carrying {
        return syn::Error::new_spanned(
            &variant.fields,
            "a value a message branches on carries nothing, since a message would need a string \
             for every value of whatever it carried",
        )
        .to_compile_error();
    }

    let name = &input.ident;
    let (implemented, declared, predicates) = input.generics.split_for_impl();
    let variants = data.variants.iter().map(|variant| &variant.ident);

    quote! {
        impl #implemented ::exos::Enumerable for #name #declared #predicates {
            const ALL: &'static [Self] = &[#(Self::#variants),*];
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    fn expand_ok(item: &str) -> String {
        expand(item.parse().expect("valid item")).to_string()
    }

    #[test]
    fn lists_the_values_in_the_order_they_were_declared() {
        let expanded = expand_ok("enum Assignee { Me, Somebody }");

        assert!(expanded.contains("impl :: exos :: Enumerable for Assignee"));
        assert!(expanded.contains("& [Self :: Me , Self :: Somebody]"));
    }

    #[test]
    fn a_struct_has_no_values_to_pick_between() {
        assert!(expand_ok("struct Assignee;").contains("compile_error"));
    }

    /// A message picks one string per value, so a value that carries something
    /// would need one string per value of that too.
    #[test]
    fn a_variant_that_carries_something_is_refused() {
        assert!(expand_ok("enum Assignee { Me, Somebody(u32) }").contains("compile_error"));
    }
}
