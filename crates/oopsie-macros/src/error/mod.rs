//! The `#[oopsie]` macro implementation.

mod args;
mod config;
mod expand_enum;
mod expand_struct;
mod inject;
mod snafu_attrs;
mod type_check;

use args::ErrorArgs;
use darling::FromMeta as _;
use darling::ast::NestedMeta;
use expand_enum::expand_enum;
use expand_struct::expand_struct;
use proc_macro2::TokenStream as TokenStream2;
use syn::spanned::Spanned as _;

pub fn expand(attrs: TokenStream2, input: TokenStream2) -> syn::Result<TokenStream2> {
    let attrs_span = attrs.span();
    let meta = NestedMeta::parse_meta_list(attrs)?;
    let args = ErrorArgs::from_list(&meta)?;
    match syn::parse2::<syn::Item>(input)? {
        syn::Item::Enum(item_enum) => expand_enum(&args, attrs_span, item_enum),
        syn::Item::Struct(item_struct) => expand_struct(&args, attrs_span, item_struct),
        other => Err(syn::Error::new_spanned(
            other,
            "`#[oopsie]` can only be applied to enums or structs",
        )),
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;

    #[test]
    fn extract_snafu_attr_works() {
        use darling::FromAttributes as _;
        let struct_def: syn::ItemStruct = syn::parse_quote! {
            #[derive(Snafu, Debug)]
            #[snafu(module, visibility(pub))]
            struct MyError;
        };
        let snafu_attr = snafu_attrs::SnafuAttrs::from_attributes(&struct_def.attrs);
        assert!(snafu_attr.is_ok());
        let snafu_attr = snafu_attr.unwrap();
        assert!(snafu_attr.module.is_some());
    }

    #[test]
    fn expand_struct_basic() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug, Snafu)]
                #[snafu(display("Connection failed: {reason}"))]
                pub struct ConnectionFailed {
                    reason: String,
                }
            },
        );
        assert!(result.is_ok(), "expand_struct should succeed: {result:?}");
        let output = result.unwrap().to_string();
        assert!(
            output.contains("__oopsie_backtrace"),
            "Should inject backtrace field"
        );
        assert!(
            output.contains("__oopsie_spantrace"),
            "Should inject spantrace field"
        );
    }

    #[test]
    fn expand_struct_unit() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug, Snafu)]
                #[snafu(display("Something failed"))]
                pub struct SomethingFailed;
            },
        );
        assert!(
            result.is_ok(),
            "expand_struct for unit should succeed: {result:?}"
        );
        let output = result.unwrap().to_string();
        assert!(
            output.contains("__oopsie_backtrace"),
            "Should inject backtrace field"
        );
    }

    #[test]
    fn expand_struct_rejects_tuple() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug, Snafu)]
                pub struct TupleError(String);
            },
        );
        assert!(result.is_err(), "Tuple structs should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("tuple"), "Error should mention tuple: {err}");
    }

    #[test]
    fn expand_struct_rejects_module_attr() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug, Snafu)]
                #[snafu(module)]
                pub struct MyError {
                    message: String,
                }
            },
        );
        assert!(
            result.is_err(),
            "#[snafu(module)] should be rejected for structs"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not applicable"),
            "Error should mention not applicable: {err}"
        );
    }

    #[test]
    fn expand_enum_with_help_and_code() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug, Snafu)]
                pub enum MyError {
                    #[snafu(display("Connection failed"))]
                    #[help("Check your network")]
                    #[code("custom::connection_failed")]
                    ConnectionFailed { address: String },
                }
            },
        );
        assert!(
            result.is_ok(),
            "Should handle #[help] and #[code]: {result:?}"
        );
        let output = result.unwrap().to_string();
        assert!(
            output.contains("HelpText"),
            "Should inject HelpText provider: {output}"
        );
        assert!(
            output.contains("\"custom::connection_failed\""),
            "Should use custom code: {output}"
        );
    }

    #[test]
    fn expand_struct_requires_derive_snafu() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug)]
                pub struct MyError {
                    message: String,
                }
            },
        );
        assert!(result.is_err(), "Should require #[derive(Snafu)]");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Snafu"), "Error should mention Snafu: {err}");
    }
}
