//! Module wrapping for context selectors.

use convert_case::{Case, Casing as _};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Ident;
use syn::ext::IdentExt as _;

use super::parse::ModuleSetting;

/// Wrap tokens in a module if module wrapping is enabled.
pub fn wrap_in_module(
    module_setting: &ModuleSetting,
    type_ident: &Ident,
    vis: &syn::Visibility,
    tokens: &[TokenStream2],
) -> TokenStream2 {
    match module_setting {
        ModuleSetting::Off => {
            // No wrapping
            quote! { #(#tokens)* }
        }
        ModuleSetting::On(custom_name) => {
            let module_name = if let Some(name) = custom_name {
                name.clone()
            } else {
                let name = type_ident.unraw().to_string();
                let stripped = name.strip_suffix("Error").unwrap_or(&name);
                let mut module_name = stripped.to_case(Case::Snake);
                if !module_name.is_empty() {
                    module_name.push('_');
                }
                let suffix = crate::utils::manifest_module_suffix();
                module_name.push_str(suffix.as_deref().unwrap_or("oopsies"));
                Ident::new(&module_name, type_ident.span())
            };
            let doc = format!("Auto-generated context selectors for `{type_ident}`.");
            quote! {
                #[doc = #doc]
                #vis mod #module_name {
                    use super::*;
                    #(#tokens)*
                }
            }
        }
    }
}
