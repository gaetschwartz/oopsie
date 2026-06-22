//! Compile-time `size(...)` assertions.
//!
//! A `size(...)` constraint lowers to a `const` block asserting `size_of` of the
//! error type against the requested byte bounds. For enums an upper-bound
//! violation is attributed to the largest variant, so the diagnostic points
//! there rather than at the attribute.

use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::DeriveInput;

use super::model::ResolvedVariant;
use super::parse::{SizeAttr, SizeConstraint};

/// A `size(...)` constraint asserts `size_of::<E>()` against a fixed byte count,
/// but a generic `E<T>` has no single size — it depends on `T`. Reject the
/// combination up front so the error names the conflict rather than surfacing as
/// an inscrutable const-eval failure inside the generated assertion.
pub(super) fn reject_size_with_generics(
    input: &DeriveInput,
    size: Option<&SizeAttr>,
) -> syn::Result<()> {
    if size.is_some() && !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "`size(...)` cannot be combined with generic parameters: the size of \
             a generic type depends on its arguments and is unknown here",
        ));
    }
    Ok(())
}

pub(super) fn wrap_size_assertion_in_const(assertion: &TokenStream2) -> TokenStream2 {
    if assertion.is_empty() {
        return quote! {};
    }
    quote! {
        // Compile-time assertion that the size of the error type meets the specified constraint, if any.
        const _: () = { mod size_check { use super::*; const _: () = { #assertion }; } };
    }
}

pub(super) fn gen_size_assertion(ident: &syn::Ident, size: &SizeAttr) -> TokenStream2 {
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

/// The validated project-wide size cap from `[package.metadata.oopsie]`, plus any
/// `compile_error!` to surface a malformed manifest. `(None, empty)` when the
/// `settings` feature is off or no cap is configured.
pub(super) fn manifest_size_cap() -> (Option<usize>, TokenStream2) {
    #[cfg(feature = "settings")]
    {
        match crate::utils::settings::cap() {
            Ok(cap) => (cap, quote! {}),
            Err(msg) => (None, quote! { ::core::compile_error!(#msg); }),
        }
    }
    #[cfg(not(feature = "settings"))]
    {
        (None, quote! {})
    }
}

/// Where the manifest cap comes from, shown on its own line under the size
/// violation. Leads with a newline so it forms its own line.
const CAP_SOURCE: &str = "\nset by `[package.metadata.oopsie] max-size` in Cargo.toml";

/// The manifest-cap assertion for a struct: a plain `<= cap` check spanned at the
/// type, since a struct has no variants to attribute the blame to.
pub(super) fn gen_default_size_cap_struct(ident: &syn::Ident, cap: usize) -> TokenStream2 {
    let msg = format!("the size of {ident} must be at most {cap} bytes{CAP_SOURCE}");
    quote! {
        ::core::assert!(
            ::core::mem::size_of::<#ident>() <= #cap,
            #msg
        );
    }
}

/// The manifest-cap assertion for an enum, reusing the variant-blaming upper-bound
/// path so an over-cap enum points at its largest variant (on its own line, below
/// the cap source).
pub(super) fn gen_default_size_cap_enum(
    ident: &syn::Ident,
    variants: &[ResolvedVariant<'_>],
    cap: usize,
) -> TokenStream2 {
    let headline = format!("the size of {ident} must be at most {cap} bytes");
    gen_upper_bound(
        ident,
        variants,
        ident.span(),
        cap,
        &headline,
        "\n",
        CAP_SOURCE,
    )
}

/// A variant's payload size, as the sum of its field sizes. Each term carries
/// the field's `#[cfg]` so a cfg-stripped field drops out instead of leaving its
/// (now-removed) type referenced — the attribute-macro form expands before rustc
/// strips `#[cfg]`. The sum ignores layout padding, but this value only ranks
/// variants to attribute the blame; the size check itself uses `size_of::<E>()`.
fn payload_size(variant: &ResolvedVariant<'_>) -> TokenStream2 {
    let terms = variant.variant.fields.iter().map(|f| {
        let cfg = f
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"));
        let ty = &f.ty;
        quote! { #( #cfg )* { __payload += ::core::mem::size_of::<#ty>(); } }
    });
    quote! {{
        let mut __payload = 0usize;
        #( #terms )*
        __payload
    }}
}

/// The size assertion for an enum. An upper-bound violation is blamed on the
/// largest variant (the assertion is spanned at that variant); a lower-bound
/// violation — being too *small* — isn't any one variant's fault and stays a
/// whole-type assertion at the `size(...)` attribute.
pub(super) fn gen_enum_size_assertion(
    ident: &syn::Ident,
    variants: &[ResolvedVariant<'_>],
    size: &SizeAttr,
) -> TokenStream2 {
    let span = size.span;
    let (upper, lower) = match &size.constraint {
        SizeConstraint::Exact(n) => {
            let msg = format!("the size of {ident} must be exactly {n} bytes");
            (Some((*n, msg.clone())), Some((*n, msg)))
        }
        SizeConstraint::AtMost(n) => (
            Some((*n, format!("the size of {ident} must be at most {n} bytes"))),
            None,
        ),
        SizeConstraint::AtLeast(n) => (
            None,
            Some((
                *n,
                format!("the size of {ident} must be at least {n} bytes"),
            )),
        ),
        SizeConstraint::Range(lo, hi) => (
            Some((
                *hi,
                format!("the size of {ident} must be at most {hi} bytes"),
            )),
            Some((
                *lo,
                format!("the size of {ident} must be at least {lo} bytes"),
            )),
        ),
    };

    let upper_assert =
        upper.map(|(limit, msg)| gen_upper_bound(ident, variants, span, limit, &msg, "; ", ""));
    let lower_assert = lower.map(|(limit, msg)| {
        quote_spanned! {span=>
            ::core::assert!(::core::mem::size_of::<#ident>() >= #limit, #msg);
        }
    });
    quote! { #upper_assert #lower_assert }
}

/// Upper-bound (`<= limit`) half of an enum size assertion: if the enum exceeds
/// `limit`, blame whichever variant holds the largest payload, spanned at that
/// variant. With no field-bearing variant there's nothing to attribute, so it
/// falls back to a whole-type assertion at the attribute.
fn gen_upper_bound(
    ident: &syn::Ident,
    variants: &[ResolvedVariant<'_>],
    attr_span: proc_macro2::Span,
    limit: usize,
    headline: &str,
    blame_join: &str,
    note: &str,
) -> TokenStream2 {
    let whole_type_msg = format!("{headline}{note}");
    let fielded: Vec<&ResolvedVariant<'_>> = variants
        .iter()
        .filter(|v| !v.variant.fields.is_empty())
        .collect();

    if fielded.is_empty() {
        return quote_spanned! {attr_span=>
            ::core::assert!(::core::mem::size_of::<#ident>() <= #limit, #whole_type_msg);
        };
    }

    // Track the largest payload imperatively — const-eval has no `usize::max`.
    let updates = fielded.iter().map(|v| {
        let cfg = &v.cfg_attrs;
        let size = payload_size(v);
        quote! { #( #cfg )* if #size > largest { largest = #size; } }
    });

    let checks = fielded.iter().map(|v| {
        let cfg = &v.cfg_attrs;
        let size = payload_size(v);
        // `note` (e.g. the cap source) sits between the headline and the blame so
        // it reads as part of the size statement, not the variant clause.
        let variant_msg = format!(
            "{headline}{note}{blame_join}{} is its largest variant",
            v.variant.ident
        );
        let blame = quote_spanned! {v.variant.ident.span()=> ::core::panic!(#variant_msg) };
        quote! { #( #cfg )* if #size == max_payload { #blame } }
    });

    // Reached only when every field-bearing variant is cfg-stripped on this
    // target: report the whole-type violation rather than blaming a variant.
    let fallback = quote_spanned! {attr_span=> ::core::panic!(#whole_type_msg) };

    quote! {
        if ::core::mem::size_of::<#ident>() > #limit {
            let max_payload: usize = {
                let mut largest = 0usize;
                #( #updates )*
                largest
            };
            #( #checks )*
            #fallback
        }
    }
}
