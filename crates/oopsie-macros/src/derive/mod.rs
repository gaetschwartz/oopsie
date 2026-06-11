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

/// Register the invocation site in the renderer's generated-frame registry.
/// The call is call_site-spanned, anchoring the registration macro's location
/// builtins at the user's attribute; it expands to nothing unless oopsie's
/// `fancy` feature is enabled.
fn gen_site_registration(oopsie_path: &syn::Path) -> TokenStream2 {
    quote! {
        #oopsie_path::__register_generated_site!();
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

/// Reject a `#[cfg(...)]`/`#[cfg_attr(...)]` on a source field. Other fields'
/// cfg attrs ride through to every generated reference, but a source field
/// drives the `Contextual<Source>`/`From`/`Error::source` shape of the whole
/// variant: gating it would leave those impls referencing a type that stripping
/// removed, with no sound sourceless fallback to switch to.
fn check_no_cfg_on_source(input: &DeriveInput) -> syn::Result<()> {
    let field_sets: Vec<&syn::Fields> = match &input.data {
        syn::Data::Enum(data) => data.variants.iter().map(|v| &v.fields).collect(),
        syn::Data::Struct(data) => std::vec![&data.fields],
        syn::Data::Union(_) => std::vec![],
    };
    for fields in field_sets {
        let categorized = parse::CategorizedFields::from_fields(fields)?;
        if let Some(source) = &categorized.source
            && let Some(cfg) = source.cfg_attrs.first()
        {
            return Err(syn::Error::new_spanned(
                cfg,
                "`#[cfg(...)]` is not supported on a source field; gating the \
                 source would strip the `From`/`Contextual` impls that depend on it",
            ));
        }
    }
    Ok(())
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
    check_no_cfg_on_source(input)?;
    let path = attrs.oopsie_path();
    let selectors = gen_enum_selectors(input, attrs, &path)?;
    let display = gen_enum_display(input)?;
    let error = gen_enum_error(input, &path)?;

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
        .map(|c| gen_size_assertion(&input.ident, c));

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
    check_no_generics(input)?;
    check_no_cfg_on_source(input)?;
    let path = attrs.container.oopsie_path();
    let selector = gen_struct_selector(input, attrs, &path)?;
    let display = gen_struct_display(input, attrs)?;
    let error = gen_struct_error(input, attrs, &path)?;

    let size_assert = attrs
        .container
        .size
        .as_ref()
        .map(|c| gen_size_assertion(&input.ident, c));

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
    #[cfg(all(
        feature = "tracing",
        not(feature = "unstable-error-generic-member-access")
    ))]
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

    #[cfg(all(
        not(feature = "tracing"),
        not(feature = "unstable-error-generic-member-access")
    ))]
    #[test]
    fn test_derive_enum_with_transparent_no_tracing() {
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
