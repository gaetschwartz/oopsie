//! `#[derive(Oopsie)]` implementation.

mod gen_display;
mod gen_error;
mod gen_module;
mod gen_selectors;
mod generics;
pub mod model;
pub mod parse;

use std::env;
use std::num::IntErrorKind;

use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote, quote_spanned};
use syn::DeriveInput;

pub use self::gen_display::{gen_enum_display, gen_struct_display};
pub use self::gen_error::{gen_enum_error, gen_struct_error};
pub use self::gen_module::wrap_in_module;
pub use self::gen_selectors::{gen_enum_selectors, gen_struct_selector};
pub use self::model::{ResolvedEnum, ResolvedStruct};
pub use self::parse::{EnumContainerAttrs, SizeAttr, SizeConstraint, StructAttrs};

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

/// Register the invocation site in the renderer's generated-frame registry.
/// The call is call_site-spanned, anchoring the registration macro's location
/// builtins at the user's attribute; it expands to nothing unless oopsie's
/// `fancy` feature is enabled.
fn gen_site_registration(oopsie_path: &syn::Path) -> TokenStream2 {
    quote! {
        #oopsie_path::__register_generated_site!();
    }
}

/// A `size(...)` constraint asserts `size_of::<E>()` against a fixed byte count,
/// but a generic `E<T>` has no single size — it depends on `T`. Reject the
/// combination up front so the error names the conflict rather than surfacing as
/// an inscrutable const-eval failure inside the generated assertion.
fn reject_size_with_generics(input: &DeriveInput, size: Option<&SizeAttr>) -> syn::Result<()> {
    if size.is_some() && !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "`size(...)` cannot be combined with generic parameters: the size of \
             a generic type depends on its arguments and is unknown here",
        ));
    }
    Ok(())
}

fn wrap_size_assertion_in_const(assertion: &TokenStream2) -> TokenStream2 {
    if assertion.is_empty() {
        return quote! {};
    }
    quote! {
        // Compile-time assertion that the size of the error type meets the specified constraint, if any.
        const _: () = { mod assert_size { use super::*; const _: () = { #assertion }; } };
    }
}

fn gen_size_assertion(ident: &syn::Ident, size: &SizeAttr) -> TokenStream2 {
    match &size.constraint {
        SizeConstraint::Exact(n) => {
            let msg = format!("the size of {ident} must be exactly {n} bytes");
            quote_spanned! {size.span=>
                ::core::assert!(
                    ::core::mem::size_of::<#ident>() == #n,
                    #msg
                );
            }
        }
        SizeConstraint::AtMost(n) => {
            let msg = format!("the size of {ident} must be at most {n} bytes");
            quote_spanned! {size.span=>
                ::core::assert!(
                    ::core::mem::size_of::<#ident>() <= #n,
                    #msg
                );
            }
        }
        SizeConstraint::AtLeast(n) => {
            let msg = format!("the size of {ident} must be at least {n} bytes");
            quote_spanned! {size.span=>
                ::core::assert!(
                    ::core::mem::size_of::<#ident>() >= #n,
                    #msg
                );
            }
        }
        SizeConstraint::Range(lo, hi) => {
            let msg_lo = format!("the size of {ident} must be at least {lo} bytes");
            let msg_hi = format!("the size of {ident} must be at most {hi} bytes");
            quote_spanned! {size.span=>
                ::core::assert!(
                    ::core::mem::size_of::<#ident>() >= #lo,
                    #msg_lo
                );
                ::core::assert!(
                    ::core::mem::size_of::<#ident>() <= #hi,
                    #msg_hi
                );
            }
        }
    }
}

const OOPSIE_MAX_ERROR_SIZE_ENV_VAR: &str = "OOPSIE_MAX_ERROR_SIZE";

/// Validates a raw `OOPSIE_MAX_ERROR_SIZE` value.
///
/// `Ok(None)` is empty/whitespace (no cap); `Ok(Some(n))` is a positive cap;
/// `Err` carries a user-facing message for an invalid value (zero, non-numeric,
/// or larger than `usize`).
fn parse_max_size(raw: &str) -> Result<Option<usize>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match trimmed.parse::<usize>() {
        Ok(0) => Err(format!(
            "{OOPSIE_MAX_ERROR_SIZE_ENV_VAR} is set to 0, which is not a valid size limit; \
             unset it (or leave it empty) to disable the cap instead"
        )),
        Ok(n) => Ok(Some(n)),
        Err(e) => Err(match *e.kind() {
            IntErrorKind::InvalidDigit => {
                format!("{OOPSIE_MAX_ERROR_SIZE_ENV_VAR} is set to a non-numeric value: {raw:?}")
            }
            IntErrorKind::PosOverflow => format!(
                "{OOPSIE_MAX_ERROR_SIZE_ENV_VAR} is set to {raw:?}, which is too large to fit in a usize (max {})",
                usize::MAX
            ),
            _ => format!("{OOPSIE_MAX_ERROR_SIZE_ENV_VAR} is set to an invalid value {raw:?}: {e}"),
        }),
    }
}

/// Reads the optional size cap from `OOPSIE_MAX_ERROR_SIZE`
fn read_default_max_size() -> Result<Option<usize>, String> {
    // Enforce it only for crates the user builds directly,
    // never for dependencies (whose error sizes they can't change).
    if env::var_os("CARGO_PRIMARY_PACKAGE").is_none() {
        return Ok(None);
    }
    match env::var(OOPSIE_MAX_ERROR_SIZE_ENV_VAR) {
        Ok(raw) => parse_max_size(&raw),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(s)) => Err(format!(
            "{OOPSIE_MAX_ERROR_SIZE_ENV_VAR} is set to a value that is not valid Unicode: {}",
            s.to_string_lossy()
        )),
    }
}

fn gen_default_size_assertion(ident: &syn::Ident) -> TokenStream2 {
    let max = match read_default_max_size() {
        Ok(None) => return quote! {},
        Ok(Some(max)) => max,
        Err(msg) => return quote! { ::core::compile_error!(#msg); },
    };
    let msg = format!(
        "the size of {ident} must be at most {max} bytes, the global cap set by \
         {OOPSIE_MAX_ERROR_SIZE_ENV_VAR}. Use #[oopsie(size(...))] to set a per-type \
         limit, or unset {OOPSIE_MAX_ERROR_SIZE_ENV_VAR} to remove the cap",
    );
    quote! {
        ::core::assert!(
            ::core::mem::size_of::<#ident>() <= #max,
            #msg
        );
    }
}

pub fn expand_enum(input: &DeriveInput, attrs: &EnumContainerAttrs) -> syn::Result<TokenStream2> {
    reject_size_with_generics(input, attrs.size.as_ref())?;
    let resolved = ResolvedEnum::resolve(input, attrs)?;
    let path = attrs.oopsie_path();
    let selectors = gen_enum_selectors(&resolved, &path)?;
    let display = gen_enum_display(&resolved);
    let error = gen_enum_error(&resolved, &path)?;

    // Wrap selectors in module if enabled
    let effective_module = attrs.effective_module(true);
    let module_vis = attrs
        .visibility()
        .cloned()
        .unwrap_or_else(|| input.vis.clone());
    let wrapped_selectors =
        wrap_in_module(&effective_module, &input.ident, &module_vis, &selectors);

    let size_assert = attrs
        .size
        .as_ref()
        .map_or_else(
            || gen_default_size_assertion(&input.ident),
            |c| gen_size_assertion(&input.ident, c),
        )
        .wrap(wrap_size_assertion_in_const);

    let keyword_docs = crate::keyword_docs::gen_keyword_docs(input, &path);
    let site_registration = gen_site_registration(&path);

    Ok(quote! {
        #wrapped_selectors
        #display
        #error
        #size_assert
        #keyword_docs
        #site_registration
    })
}

pub fn expand_struct(input: &DeriveInput, attrs: &StructAttrs) -> syn::Result<TokenStream2> {
    reject_size_with_generics(input, attrs.container.size.as_ref())?;
    let resolved = ResolvedStruct::resolve(input, attrs)?;
    let path = attrs.container.oopsie_path();
    let selector = gen_struct_selector(&resolved, &path)?;
    let display = gen_struct_display(&resolved);
    let error = gen_struct_error(&resolved, &path)?;

    let size_assert = attrs
        .container
        .size
        .as_ref()
        .map_or_else(
            || gen_default_size_assertion(&input.ident),
            |c| gen_size_assertion(&input.ident, c),
        )
        .wrap(wrap_size_assertion_in_const);

    let effective_module = attrs.container.effective_module(false);
    let module_vis = attrs
        .visibility()
        .cloned()
        .unwrap_or_else(|| input.vis.clone());
    let wrapped_selector = if attrs.transparent {
        selector
    } else {
        wrap_in_module(
            &effective_module,
            &input.ident,
            &module_vis,
            std::slice::from_ref(&selector),
        )
    };

    let keyword_docs = crate::keyword_docs::gen_keyword_docs(input, &path);
    let site_registration = gen_site_registration(&path);

    Ok(quote! {
        #wrapped_selector
        #display
        #error
        #size_assert
        #keyword_docs
        #site_registration
    })
}

pub trait TokenStreamExt {
    fn wrap(&self, apply: impl FnOnce(&Self) -> TokenStream2) -> TokenStream2;
}

impl<T: ToTokens> TokenStreamExt for T {
    fn wrap(&self, apply: impl FnOnce(&Self) -> TokenStream2) -> TokenStream2 {
        apply(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn parse_max_size_classifies_values() {
        assert_eq!(parse_max_size(""), Ok(None));
        assert_eq!(parse_max_size("   "), Ok(None));
        assert_eq!(parse_max_size("16"), Ok(Some(16)));
        assert_eq!(parse_max_size("  128  "), Ok(Some(128)));
        // zero, non-numeric, negative, and overflow are all rejected with a message
        assert!(parse_max_size("0").is_err());
        assert!(parse_max_size("abc").is_err());
        assert!(parse_max_size("-5").is_err());
        assert!(parse_max_size("99999999999999999999999999999").is_err());
    }

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

    // A representative generic enum: one variant whose field references the
    // type param `T` (selector carries `T`, no `Into` param since the param is
    // named directly), one transparent source-only variant whose source type is
    // the param `E` (a `From<E>` impl, no selector), and a where-clause the impls
    // carry but the selector struct does not.
    #[cfg(not(feature = "unstable-error-generic-member-access"))]
    #[test]
    fn test_derive_generic_enum() {
        let input = quote! {
            #[oopsie(module(false))]
            pub enum GenericError<T, E>
            where
                T: ::core::fmt::Debug,
                E: ::core::error::Error,
            {
                #[oopsie("payload was {payload:?}")]
                Wrap { payload: T },
                #[oopsie(display("inner failed"), transparent)]
                Inner { source: E },
            }
        };
        let output = expand(input).unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn module_vis_mirrors_error_type_vis() {
        let out = expand(quote! {
            pub(crate) enum InternalError { #[oopsie("x")] X { f: String } }
        })
        .unwrap()
        .to_string();
        assert!(out.contains("pub (crate) mod internal_oopsies"), "{out}");

        let out = expand(quote! {
            enum PrivError { #[oopsie("x")] X { f: String } }
        })
        .unwrap()
        .to_string();
        assert!(
            out.contains("mod priv_oopsies") && !out.contains("pub mod priv_oopsies"),
            "{out}"
        );
    }

    #[test]
    fn generated_public_items_are_documented() {
        let out = expand(quote! {
            pub enum AppError { #[oopsie("x")] Connect { host: String } }
        })
        .unwrap()
        .to_string();
        assert!(
            out.contains("Auto-generated context selectors for `AppError`"),
            "{out}"
        );
        assert!(
            out.contains("Context selector for `AppError::Connect`"),
            "{out}"
        );
        assert!(out.contains("Value for the `host` field"), "{out}");
    }

    #[test]
    fn selector_collision_after_error_stripping_errors() {
        let err = expand(quote! {
            pub enum AppError {
                #[oopsie("read failed")] Read,
                #[oopsie("read failed (io)")] ReadError,
            }
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("both generate a selector named"),
            "{err}"
        );
    }

    #[test]
    fn pub_enum_selector_defaults_to_pub() {
        let out = expand(quote! {
            #[oopsie(module(false))]
            pub enum PubError { #[oopsie("x")] X { f: String } }
        })
        .unwrap()
        .to_string();
        assert!(out.contains("pub struct X"), "{out}");
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
