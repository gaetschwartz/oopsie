//! The `#[traced]` macro implementation.

pub mod args;
pub mod config;
pub mod expand_enum;
pub mod expand_struct;
pub mod field_detect;
pub mod inject;

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
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
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
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
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
        let err = result.unwrap_err().to_string();
        insta::assert_snapshot!(err);
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
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
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
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
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
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
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
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
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
        let err = result.unwrap_err().to_string();
        insta::assert_snapshot!(err);
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
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
    }
}
