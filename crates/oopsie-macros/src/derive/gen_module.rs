//! Module wrapping for context selectors.

use convert_case::{Case, Casing as _};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Ident;

use super::parse::ModuleSetting;

/// Wrap tokens in a module if module wrapping is enabled.
pub(crate) fn wrap_in_module(
    module_setting: &ModuleSetting,
    enum_ident: &Ident,
    tokens: Vec<TokenStream2>,
) -> TokenStream2 {
    match module_setting {
        ModuleSetting::Off | ModuleSetting::Default => {
            // No wrapping
            quote! { #(#tokens)* }
        }
        ModuleSetting::On(custom_name) => {
            let module_name = if let Some(name) = custom_name {
                name.clone()
            } else {
                let snake = enum_ident.to_string().to_case(Case::Snake);
                Ident::new(&snake, enum_ident.span())
            };
            quote! {
                pub mod #module_name {
                    use super::*;
                    #(#tokens)*
                }
            }
        }
    }
}
