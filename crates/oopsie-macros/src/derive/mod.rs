//! `#[derive(Oopsie)]` implementation.

mod gen_display;
mod gen_error;
mod gen_module;
mod gen_selectors;
pub mod parse;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{DeriveInput, parse_quote};

use self::gen_display::{gen_enum_display, gen_struct_display};
use self::gen_error::{gen_enum_error, gen_struct_error};
use self::gen_module::wrap_in_module;
use self::gen_selectors::{gen_enum_selectors, gen_struct_selector};
use self::parse::ContainerAttrs;

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

fn oopsie_path(container: &ContainerAttrs) -> syn::Path {
    container
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie })
}

fn expand_enum(
    input: &DeriveInput,
    container_attrs: &ContainerAttrs,
) -> syn::Result<TokenStream2> {
    let path = oopsie_path(container_attrs);
    let selectors = gen_enum_selectors(input, container_attrs, &path)?;
    let display = gen_enum_display(input)?;
    let error = gen_enum_error(input)?;

    // Wrap selectors in module if enabled
    let effective_module = container_attrs.effective_module(true);
    let wrapped_selectors = wrap_in_module(&effective_module, &input.ident, &selectors);

    Ok(quote! {
        #wrapped_selectors
        #display
        #error
    })
}

fn expand_struct(
    input: &DeriveInput,
    container_attrs: &ContainerAttrs,
) -> syn::Result<TokenStream2> {
    let path = oopsie_path(container_attrs);
    let selector = gen_struct_selector(input, container_attrs, &path)?;
    let display = gen_struct_display(input)?;
    let error = gen_struct_error(input)?;

    // No module wrapping for structs
    Ok(quote! {
        #selector
        #display
        #error
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
        let result = expand(input);
        assert!(result.is_ok(), "Derive minimal should succeed: {result:?}");
        let output = result.unwrap().to_string();
        eprintln!("DERIVE MINIMAL OUTPUT:\n{output}");
    }

    #[test]
    fn test_derive_struct_with_display() {
        let input = quote! {
            #[oopsie("Test error: {message}")]
            pub struct MyError {
                message: String,
            }
        };
        let result = expand(input);
        assert!(result.is_ok(), "Derive with display should succeed: {result:?}");
        let output = result.unwrap().to_string();
        eprintln!("DERIVE DISPLAY OUTPUT:\n{output}");
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
        let result = expand(input);
        assert!(result.is_ok(), "Derive should succeed: {result:?}");
        let output = result.unwrap().to_string();
        eprintln!("DERIVE SIMPLE OUTPUT:\n{output}");
        assert!(output.contains("Display"), "Should generate Display: {output}");
    }

    #[test]
    fn test_derive_struct_with_provide() {
        let input = quote! {
            #[oopsie(vis = pub(crate))]
            #[oopsie(suffix)]
            #[oopsie(path = "crate")]
            #[oopsie("Test error: {message}")]
            #[oopsie(provide(ref, crate::Backtrace => __oopsie_backtrace.as_ref()))]
            pub struct MyError {
                message: String,
                #[oopsie(auto)]
                __oopsie_backtrace: ::std::boxed::Box<crate::Backtrace>,
            }
        };
        let result = expand(input);
        assert!(result.is_ok(), "Derive with provide should succeed: {result:?}");
        let output = result.unwrap().to_string();
        eprintln!("DERIVE PROVIDE OUTPUT:\n{output}");
        assert!(output.contains("Display"), "Should generate Display: {output}");
    }

    #[test]
    fn test_derive_enum_with_transparent() {
        let input = quote! {
            #[oopsie(module(error_with_span_trace_oopsies))]
            #[oopsie(vis = pub(crate))]
            #[oopsie(path = "crate")]
            pub enum ErrorWithSpanTrace {
                #[oopsie("Inner error happened", transparent)]
                #[oopsie(provide(ref, crate::Backtrace => __oopsie_backtrace.as_ref()))]
                Inner {
                    source: ErrorWithSpanTraceInner,
                    #[oopsie(auto)]
                    __oopsie_backtrace: ::std::boxed::Box<crate::Backtrace>,
                    #[oopsie(auto)]
                    __oopsie_spantrace: ::std::boxed::Box<crate::Spantrace>,
                },
            }
        };
        let result = expand(input);
        assert!(result.is_ok(), "Derive should succeed: {result:?}");
        let output = result.unwrap().to_string();
        eprintln!("DERIVE ENUM OUTPUT:\n{output}");
        assert!(output.contains("Display"), "Should generate Display: {output}");
    }
}
