//! The `#[traced]` macro implementation.

mod args;
mod config;
mod expand_enum;
mod expand_struct;
mod field_detect;
mod inject;

use args::TracedArgs;
use darling::FromMeta as _;
use darling::ast::NestedMeta;
use expand_enum::expand_enum;
use expand_struct::expand_struct;
use proc_macro2::TokenStream as TokenStream2;
use syn::spanned::Spanned as _;

pub fn expand(attrs: TokenStream2, input: TokenStream2) -> syn::Result<TokenStream2> {
    let attrs_span = attrs.span();
    let meta = NestedMeta::parse_meta_list(attrs)?;
    let args = TracedArgs::from_list(&meta)?;
    match syn::parse2::<syn::Item>(input)? {
        syn::Item::Enum(item_enum) => {
            require_derive_oopsie(&item_enum.attrs)?;
            expand_enum(&args, attrs_span, item_enum)
        }
        syn::Item::Struct(item_struct) => {
            require_derive_oopsie(&item_struct.attrs)?;
            expand_struct(&args, attrs_span, item_struct)
        }
        other => Err(syn::Error::new_spanned(
            other,
            "`#[traced]` can only be applied to enums or structs",
        )),
    }
}

fn require_derive_oopsie(attrs: &[syn::Attribute]) -> syn::Result<()> {
    let has_oopsie = attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let Ok(content) = attr.parse_args::<proc_macro2::TokenStream>() else {
            return false;
        };
        content
            .into_iter()
            .any(|tok| matches!(&tok, proc_macro2::TokenTree::Ident(ident) if ident == "Oopsie"))
    });
    if has_oopsie {
        return Ok(());
    }
    Err(syn::Error::new(
        proc_macro2::Span::call_site(),
        "`#[traced]` requires `#[derive(Oopsie)]` below it\n\
         \n\
         Usage:\n  \
           #[traced]\n  \
           #[derive(Debug, Oopsie)]\n  \
           pub enum MyError { ... }",
    ))
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
                #[derive(Debug, Oopsie)]
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
                #[derive(Debug, Oopsie)]
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
                #[derive(Debug, Oopsie)]
                pub struct TupleError(String);
            },
        );
        assert!(result.is_err(), "Tuple structs should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("tuple"), "Error should mention tuple: {err}");
    }

    #[test]
    fn expand_enum_preserves_user_code() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug, Oopsie)]
                pub enum MyError {
                    #[oopsie(display("connection failed"), help = "Check your network", code = "custom::connection_failed")]
                    ConnectionFailed { address: String },
                }
            },
        );
        assert!(
            result.is_ok(),
            "Should handle help and code in #[oopsie(...)]: {result:?}"
        );
        let output = result.unwrap().to_string();
        // help/code are on the variant attrs, traced should pass them through
        assert!(
            output.contains("Check your network"),
            "Should preserve help text: {output}"
        );
        assert!(
            output.contains("custom::connection_failed"),
            "Should preserve custom code: {output}"
        );
    }

    #[test]
    fn expand_with_path_crate() {
        let result = expand(
            quote! { path = "crate" },
            quote! {
                #[derive(Debug, Oopsie)]
                pub enum ErrorWithSpanTrace {
                    #[oopsie(display("Inner error happened"), transparent)]
                    Inner { source: ErrorWithSpanTraceInner },
                }
            },
        );
        assert!(result.is_ok(), "Should succeed: {result:?}");
    }

    #[test]
    fn expand_explicit_override_backtrace_only() {
        let result = expand(
            quote! { backtrace },
            quote! {
                #[derive(Debug, Oopsie)]
                pub struct MyError {
                    message: String,
                }
            },
        );
        assert!(result.is_ok(), "Should succeed: {result:?}");
        let output = result.unwrap().to_string();
        assert!(
            output.contains("__oopsie_backtrace"),
            "Should inject backtrace: {output}"
        );
        assert!(
            !output.contains("__oopsie_spantrace"),
            "Should NOT inject spantrace in explicit mode: {output}"
        );
    }

    #[test]
    fn expand_explicit_override_spantrace_only() {
        let result = expand(
            quote! { spantrace },
            quote! {
                #[derive(Debug, Oopsie)]
                pub struct MyError {
                    message: String,
                }
            },
        );
        assert!(result.is_ok(), "Should succeed: {result:?}");
        let output = result.unwrap().to_string();
        assert!(
            !output.contains("__oopsie_backtrace"),
            "Should NOT inject backtrace in explicit mode: {output}"
        );
        assert!(
            output.contains("__oopsie_spantrace"),
            "Should inject spantrace: {output}"
        );
    }

    #[test]
    fn expand_without_derive_oopsie_errors() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug)]
                pub struct MyError {
                    message: String,
                }
            },
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("derive(Oopsie)"),
            "Error should mention derive(Oopsie): {err}"
        );
    }

    #[test]
    fn expand_bare_injects_both_traces() {
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
        assert!(output.contains("__oopsie_backtrace"));
        assert!(output.contains("__oopsie_spantrace"));
    }
}
