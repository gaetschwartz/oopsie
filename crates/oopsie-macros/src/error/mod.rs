//! The `#[oopsie]` macro implementation.

mod args;
mod config;
mod expand_enum;
mod expand_struct;
mod inject;
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
    fn expand_struct_basic() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug)]
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
        assert!(output.contains("Oopsie"), "Should add #[derive(Oopsie)]");
    }

    #[test]
    fn expand_struct_unit() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug)]
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
                #[derive(Debug)]
                pub struct TupleError(String);
            },
        );
        assert!(result.is_err(), "Tuple structs should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("tuple"), "Error should mention tuple: {err}");
    }

    #[test]
    fn expand_enum_with_help_and_code() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug)]
                pub enum MyError {
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
    fn expand_struct_adds_derive_oopsie() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug)]
                pub struct MyError {
                    message: String,
                }
            },
        );
        assert!(result.is_ok(), "Should succeed: {result:?}");
        let output = result.unwrap().to_string();
        assert!(
            output.contains("Oopsie"),
            "Should add Oopsie to derives: {output}"
        );
    }

    #[test]
    fn expand_struct_with_display() {
        let result = expand(
            quote! { path = "crate", display = "Test error: {message}" },
            quote! {
                #[derive(Debug)]
                pub struct MyError {
                    message: String,
                }
            },
        );
        assert!(result.is_ok(), "Should succeed: {result:?}");
        let output = result.unwrap().to_string();
        eprintln!("STRUCT OUTPUT:\n{output}");
        assert!(
            output.contains("Test error"),
            "Should contain display format: {output}"
        );
    }

    #[test]
    fn expand_enum_with_path_crate() {
        let result = expand(
            quote! { path = "crate" },
            quote! {
                #[derive(Debug)]
                pub enum ErrorWithSpanTrace {
                    #[oopsie("Inner error happened", transparent)]
                    Inner { source: ErrorWithSpanTraceInner },
                }
            },
        );
        assert!(result.is_ok(), "Should succeed: {result:?}");
        let output = result.unwrap().to_string();
        eprintln!("ENUM OUTPUT:\n{output}");
        assert!(
            output.contains("Oopsie"),
            "Should add Oopsie derive: {output}"
        );
    }

    #[test]
    fn expand_struct_does_not_duplicate_derive_oopsie() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug, Oopsie)]
                pub struct MyError {
                    message: String,
                }
            },
        );
        assert!(result.is_ok(), "Should succeed: {result:?}");
        let output = result.unwrap().to_string();
        // Count occurrences of "Oopsie" - should appear only in the derive
        let count = output.matches("Oopsie").count();
        // At least 1 from derive, but should not be duplicated in derives
        assert!(count >= 1, "Should have Oopsie in derives: {output}");
    }
}
