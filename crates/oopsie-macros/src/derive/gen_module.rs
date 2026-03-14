//! Module wrapping for context selectors.

use convert_case::{Case, Casing as _};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Ident;

use super::parse::ModuleSetting;

/// Wrap tokens in a module if module wrapping is enabled.
pub fn wrap_in_module(
    module_setting: &ModuleSetting,
    enum_ident: &Ident,
    tokens: &[TokenStream2],
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
                let name = enum_ident.to_string();
                let stripped = name.strip_suffix("Error").unwrap_or(&name);
                let mut module_name = stripped.to_case(Case::Snake);
                if !module_name.is_empty() {
                    module_name.push('_');
                }
                module_name.push_str("oopsies");
                Ident::new(&module_name, enum_ident.span())
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
