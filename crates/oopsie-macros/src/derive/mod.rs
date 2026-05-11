//! `#[derive(Oopsie)]` implementation.

mod gen_display;
mod gen_error;
mod gen_module;
mod gen_selectors;
pub mod parse;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{DeriveInput, parse_quote};

pub use self::gen_display::{gen_enum_display, gen_struct_display};
pub use self::gen_error::{gen_enum_error, gen_struct_error};
pub use self::gen_module::wrap_in_module;
pub use self::gen_selectors::{gen_enum_selectors, gen_struct_selector};
pub use self::parse::{ContainerAttrs, SizeConstraint};

pub fn expand(input: TokenStream2) -> syn::Result<TokenStream2> {
    let input: DeriveInput = syn::parse2(input)?;
    let container_attrs = ContainerAttrs::from_attrs(&input.attrs)?;

    match &input.data {
        syn::Data::Enum(_) => expand_enum(&input, &container_attrs),
        syn::Data::Struct(_) => expand_struct(&input, &container_attrs),
        syn::Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "#[derive(Oopsie)] cannot be applied to unions",
        )),
    }
}

pub fn oopsie_path(container: &ContainerAttrs) -> syn::Path {
    container
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie })
}

fn gen_size_assertion(ident: &syn::Ident, constraint: &SizeConstraint) -> TokenStream2 {
    match constraint {
        SizeConstraint::Exact(n) => {
            let msg = format!("{ident} size must be exactly {n} bytes");
            quote! {
                const _: () = {
                    assert!(
                        ::core::mem::size_of::<#ident>() == #n,
                        #msg
                    );
                };
            }
        }
        SizeConstraint::AtMost(n) => {
            let msg = format!("{ident} exceeds size limit of {n} bytes");
            quote! {
                const _: () = {
                    assert!(
                        ::core::mem::size_of::<#ident>() <= #n,
                        #msg
                    );
                };
            }
        }
        SizeConstraint::AtLeast(n) => {
            let msg = format!("{ident} must be at least {n} bytes");
            quote! {
                const _: () = {
                    assert!(
                        ::core::mem::size_of::<#ident>() >= #n,
                        #msg
                    );
                };
            }
        }
        SizeConstraint::Range(lo, hi) => {
            let msg_lo = format!("{ident} must be at least {lo} bytes");
            let msg_hi = format!("{ident} exceeds size limit of {hi} bytes");
            quote! {
                const _: () = {
                    assert!(
                        ::core::mem::size_of::<#ident>() >= #lo,
                        #msg_lo
                    );
                    assert!(
                        ::core::mem::size_of::<#ident>() <= #hi,
                        #msg_hi
                    );
                };
            }
        }
    }
}

pub fn expand_enum(
    input: &DeriveInput,
    container_attrs: &ContainerAttrs,
) -> syn::Result<TokenStream2> {
    let path = oopsie_path(container_attrs);
    let selectors = gen_enum_selectors(input, container_attrs, &path)?;
    let display = gen_enum_display(input)?;
    let error = gen_enum_error(input, &path)?;

    // Wrap selectors in module if enabled
    let effective_module = container_attrs.effective_module(true);
    let wrapped_selectors = wrap_in_module(&effective_module, &input.ident, &selectors);

    let size_assert = container_attrs
        .size
        .as_ref()
        .map(|c| gen_size_assertion(&input.ident, c));

    Ok(quote! {
        #wrapped_selectors
        #display
        #error
        #size_assert
    })
}

pub fn expand_struct(
    input: &DeriveInput,
    container_attrs: &ContainerAttrs,
) -> syn::Result<TokenStream2> {
    let path = oopsie_path(container_attrs);
    let selector = gen_struct_selector(input, container_attrs, &path)?;
    let display = gen_struct_display(input)?;
    let error = gen_struct_error(input, &path)?;

    let size_assert = container_attrs
        .size
        .as_ref()
        .map(|c| gen_size_assertion(&input.ident, c));

    // No module wrapping for structs
    Ok(quote! {
        #selector
        #display
        #error
        #size_assert
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_derive_struct_minimal() {
        let input = quote! {
            pub struct MyError {
                message: String,
            }
        };
        let output = expand(input).unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_derive_struct_with_display() {
        let input = quote! {
            #[oopsie("Test error: {message}")]
            pub struct MyError {
                message: String,
            }
        };
        let output = expand(input).unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_derive_struct_simple() {
        let input = quote! {
            #[oopsie(vis = pub(crate))]
            #[oopsie(suffix)]
            #[oopsie(path = "crate")]
            #[oopsie("Test error: {message}")]
            pub struct MyError {
                message: String,
            }
        };
        let output = expand(input).unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    // Output includes the `provide` impl when the unstable feature is on,
    // so the snapshot only matches in the default-features build.
    #[cfg(not(feature = "unstable-error-generic-member-access"))]
    #[test]
    fn test_derive_struct_with_provide() {
        let input = quote! {
            #[oopsie(vis = pub(crate))]
            #[oopsie(suffix)]
            #[oopsie(path = "crate")]
            #[oopsie("Test error: {message}")]
            #[oopsie(provide(ref, crate::BackTrace => __oopsie_backtrace.as_ref()))]
            pub struct MyError {
                message: String,
                #[oopsie(capture)]
                __oopsie_backtrace: ::std::boxed::Box<crate::BackTrace>,
            }
        };
        let output = expand(input).unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    #[cfg(not(feature = "unstable-error-generic-member-access"))]
    #[test]
    fn test_derive_enum_with_transparent() {
        let input = quote! {
            #[oopsie(module(error_with_span_trace_oopsies))]
            #[oopsie(vis = pub(crate))]
            #[oopsie(path = "crate")]
            pub enum ErrorWithSpanTrace {
                #[oopsie(display("Inner error happened"), transparent)]
                #[oopsie(provide(ref, crate::BackTrace => __oopsie_backtrace.as_ref()))]
                Inner {
                    source: ErrorWithSpanTraceInner,
                    #[oopsie(capture)]
                    __oopsie_backtrace: ::std::boxed::Box<crate::BackTrace>,
                    #[oopsie(capture)]
                    __oopsie_spantrace: ::std::boxed::Box<crate::SpanTrace>,
                },
            }
        };
        let output = expand(input).unwrap().to_string();
        insta::assert_snapshot!(output);
    }
}
