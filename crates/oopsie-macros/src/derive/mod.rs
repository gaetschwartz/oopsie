//! `#[derive(Oopsie)]` implementation.

mod gen_display;
mod gen_error;
mod gen_module;
mod gen_selectors;
pub mod parse;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

pub use self::gen_display::{gen_enum_display, gen_struct_display};
pub use self::gen_error::{gen_enum_error, gen_struct_error};
pub use self::gen_module::wrap_in_module;
pub use self::gen_selectors::{gen_enum_selectors, gen_struct_selector};
pub use self::parse::{EnumContainerAttrs, SizeConstraint, StructAttrs};

pub fn expand(input: TokenStream2) -> syn::Result<TokenStream2> {
    let input: DeriveInput = syn::parse2(input)?;

    match &input.data {
        syn::Data::Enum(_) => {
            let attrs = EnumContainerAttrs::from_attrs(&input.attrs)?;
            expand_enum(&input, &attrs)
        }
        syn::Data::Struct(_) => {
            let attrs = StructAttrs::from_attrs(&input.attrs)?;
            expand_struct(&input, &attrs)
        }
        syn::Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "#[derive(Oopsie)] cannot be applied to unions",
        )),
    }
}

/// Reject generic types up-front so users get a clear error instead of an
/// `E0107` originating inside macro-generated code. The selector struct,
/// `Contextual` impl, and `transparent` `From` impl would all need to thread
/// `impl_generics` / `ty_generics` / `where_clause` through every emit site
/// to support this properly — left for a follow-up.
fn check_no_generics(input: &DeriveInput) -> syn::Result<()> {
    if input.generics.params.is_empty() {
        return Ok(());
    }
    Err(syn::Error::new_spanned(
        &input.generics,
        "oopsie does not yet support generic error types",
    ))
}

fn gen_size_assertion(ident: &syn::Ident, constraint: &SizeConstraint) -> TokenStream2 {
    match constraint {
        SizeConstraint::Exact(n) => {
            let msg = format!("{ident} size must be exactly {n} bytes");
            quote! {
                const _: () = {
                    ::core::assert!(
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
                    ::core::assert!(
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
                    ::core::assert!(
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
                    ::core::assert!(
                        ::core::mem::size_of::<#ident>() >= #lo,
                        #msg_lo
                    );
                    ::core::assert!(
                        ::core::mem::size_of::<#ident>() <= #hi,
                        #msg_hi
                    );
                };
            }
        }
    }
}

pub fn expand_enum(input: &DeriveInput, attrs: &EnumContainerAttrs) -> syn::Result<TokenStream2> {
    check_no_generics(input)?;
    let path = attrs.oopsie_path();
    let selectors = gen_enum_selectors(input, attrs, &path)?;
    let display = gen_enum_display(input)?;
    let error = gen_enum_error(input, &path)?;

    // Wrap selectors in module if enabled
    let effective_module = attrs.effective_module(true);
    let wrapped_selectors = wrap_in_module(&effective_module, &input.ident, &selectors);

    let size_assert = attrs
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

pub fn expand_struct(input: &DeriveInput, attrs: &StructAttrs) -> syn::Result<TokenStream2> {
    check_no_generics(input)?;
    let path = attrs.container.oopsie_path();
    let selector = gen_struct_selector(input, attrs, &path)?;
    let display = gen_struct_display(input, attrs)?;
    let error = gen_struct_error(input, attrs, &path)?;

    let size_assert = attrs
        .container
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
            #[oopsie(vis(pub(crate)))]
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
            #[oopsie(vis(pub(crate)))]
            #[oopsie(suffix)]
            #[oopsie(path = "crate")]
            #[oopsie("Test error: {message}")]
            #[oopsie(provide(ref, crate::Backtrace => __oopsie_backtrace.as_ref()))]
            pub struct MyError {
                message: String,
                #[oopsie(capture)]
                __oopsie_backtrace: ::std::boxed::Box<crate::Backtrace>,
            }
        };
        let output = expand(input).unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    // Locks derive codegen for a transparent variant as a user actually writes
    // it. Trace surfacing through a transparent, trace-injected wrapper runs the
    // full inject→derive pipeline and is covered by the `traced_transparent`
    // test in `oopsie/tests/derive_transparent.rs` — a derive-only unit test
    // can only fake post-injection fields, which drifts from real inject output.
    #[cfg(not(feature = "unstable-error-generic-member-access"))]
    #[test]
    fn test_derive_enum_with_transparent() {
        let input = quote! {
            #[oopsie(module(transparent_wrapper_oopsies))]
            #[oopsie(vis(pub(crate)))]
            #[oopsie(path = "crate")]
            pub enum TransparentWrapper {
                #[oopsie(display("Inner error happened"), transparent)]
                Inner { source: InnerError },
            }
        };
        let output = expand(input).unwrap().to_string();
        insta::assert_snapshot!(output);
    }
}
