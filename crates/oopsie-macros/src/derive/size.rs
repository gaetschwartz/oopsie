//! Compile-time `size(...)` assertions.
//!
//! A `size(...)` constraint lowers to a `const` block that compares `size_of` of
//! the error type against the requested byte bounds and, on violation, panics
//! with a message naming the measured size. For enums an upper-bound violation
//! is attributed to the largest variant, so the diagnostic points there rather
//! than at the attribute.

use proc_macro2::{Literal, Span, TokenStream as TokenStream2};
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
        const _: () = {
            #[allow(non_upper_case_globals)]
            const size_check: () = { #assertion };
        };
    }
}

/// The validated project-wide size cap from `[package.metadata.oopsie]` or
/// `[workspace.metadata.oopsie]`, paired with its source section label, plus any
/// `compile_error!` to surface a malformed manifest. `(None, empty)` when the
/// `settings` feature is off or no cap is configured.
pub(super) fn manifest_size_cap() -> (Option<(usize, &'static str)>, TokenStream2) {
    #[cfg(feature = "settings")]
    {
        match crate::utils::settings::cap() {
            Ok(Some((cap, section))) => (Some((cap, section)), quote! {}),
            Ok(None) => (None, quote! {}),
            Err(msg) => (None, quote! { ::core::compile_error!(#msg); }),
        }
    }
    #[cfg(not(feature = "settings"))]
    {
        (None, quote! {})
    }
}

/// The cap-source line shown on its own line under the size violation, naming the
/// manifest section the cap came from. Leads with a newline so it forms its own
/// line.
fn cap_source_line(section: &str) -> String {
    format!("\nset by `{section}` in Cargo.toml")
}

/// Human phrase describing the allowed sizes, e.g. `≤ 64` or `in 32..=64`.
fn bound_phrase(constraint: &SizeConstraint) -> String {
    match constraint {
        SizeConstraint::Exact(n) => format!("exactly {n}"),
        SizeConstraint::AtMost(n) => format!("≤ {n}"),
        SizeConstraint::AtLeast(n) => format!("≥ {n}"),
        SizeConstraint::Range(lo, hi) => format!("in {lo}..={hi}"),
        SizeConstraint::Below(n) => format!("< {n}"),
        SizeConstraint::RangeHalfOpen(lo, hi) => format!("in {lo}..{hi}"),
    }
}

/// The upper bound as `(limit, inclusive)`, if the constraint has one.
/// `inclusive` ⇒ a violation is `size_of > limit`; exclusive ⇒ `size_of >= limit`.
const fn upper_bound(constraint: &SizeConstraint) -> Option<(usize, bool)> {
    match constraint {
        SizeConstraint::Exact(n) | SizeConstraint::AtMost(n) => Some((*n, true)),
        SizeConstraint::Range(_, hi) => Some((*hi, true)),
        SizeConstraint::AtLeast(_) => None,
        SizeConstraint::Below(n) => Some((*n, false)),
        SizeConstraint::RangeHalfOpen(_, hi) => Some((*hi, false)),
    }
}

/// The (always-inclusive) lower bound, if any. A violation is `size_of < limit`.
const fn lower_bound(constraint: &SizeConstraint) -> Option<usize> {
    match constraint {
        SizeConstraint::Exact(n) | SizeConstraint::AtLeast(n) => Some(*n),
        SizeConstraint::Range(lo, _) => Some(*lo),
        SizeConstraint::AtMost(_) => None,
        SizeConstraint::Below(_) => None,
        SizeConstraint::RangeHalfOpen(lo, _) => Some(*lo),
    }
}

/// Emit `panic!("{}", ConstStr::<N>::new().str(p0).usize(n0)….as_str())`, spanned
/// at `span`. `parts` and `nums` interleave: `parts[0], nums[0], parts[1], …,
/// parts[k]` with `parts.len() == nums.len() + 1`. `nums` are token streams that
/// evaluate to `usize` in the generated context (e.g. `size_of::<E>()`).
fn const_panic_msg(
    oopsie_path: &syn::Path,
    span: Span,
    parts: &[String],
    nums: &[TokenStream2],
) -> TokenStream2 {
    debug_assert_eq!(
        parts.len(),
        nums.len() + 1,
        "const_panic_msg: parts must interleave around nums (parts.len() == nums.len() + 1)"
    );
    // 20 = max decimal digits of a usize (u64::MAX is 20 chars); reserve that per number.
    let cap = parts.iter().map(String::len).sum::<usize>() + 20 * nums.len();
    let cap = Literal::usize_unsuffixed(cap);
    let mut chain = quote! { #oopsie_path::__private::ConstStr::<#cap>::new() };
    for (i, part) in parts.iter().enumerate() {
        chain = quote! { #chain.str(#part) };
        if let Some(n) = nums.get(i) {
            chain = quote! { #chain.usize(#n) };
        }
    }
    quote_spanned! {span=> ::core::panic!("{}", #chain.as_str()) }
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

/// Whole-type checks for a struct (or an enum's lower bound). `note` is appended
/// to the message (empty for `size(...)`, the cap-source line for a manifest cap).
fn gen_whole_type_checks(
    oopsie_path: &syn::Path,
    ident: &syn::Ident,
    span: Span,
    upper: Option<(usize, bool)>,
    lower: Option<usize>,
    phrase: &str,
    note: &str,
) -> TokenStream2 {
    let parts = [
        format!("`{ident}` is "),
        format!(" bytes, must be {phrase}{note}"),
    ];
    let panic = |sp| {
        const_panic_msg(
            oopsie_path,
            sp,
            &parts,
            &[quote! { ::core::mem::size_of::<#ident>() }],
        )
    };
    let upper = upper.map(|(limit, inclusive)| {
        let cmp = if inclusive { quote!(>) } else { quote!(>=) };
        let p = panic(span);
        quote! { if ::core::mem::size_of::<#ident>() #cmp #limit { #p } }
    });
    let lower = lower.map(|limit| {
        let p = panic(span);
        quote! { if ::core::mem::size_of::<#ident>() < #limit { #p } }
    });
    quote! { #upper #lower }
}

/// Upper-bound check for an enum: if the enum exceeds `limit`, blame whichever
/// variant holds the largest payload, spanned at that variant. With no
/// field-bearing variant there's nothing to attribute, so it falls back to a
/// whole-type panic at `attr_span`.
struct EnumUpperParams<'a> {
    oopsie_path: &'a syn::Path,
    ident: &'a syn::Ident,
    variants: &'a [ResolvedVariant<'a>],
    attr_span: Span,
    limit: usize,
    inclusive: bool,
    phrase: &'a str,
    note: &'a str,
}

fn gen_enum_upper(p: &EnumUpperParams<'_>) -> TokenStream2 {
    let EnumUpperParams {
        oopsie_path,
        ident,
        variants,
        attr_span,
        limit,
        inclusive,
        phrase,
        note,
    } = p;
    let (attr_span, limit, inclusive) = (*attr_span, *limit, *inclusive);
    let cmp = if inclusive { quote!(>) } else { quote!(>=) };
    let whole_parts = [
        format!("`{ident}` is "),
        format!(" bytes, must be {phrase}{note}"),
    ];
    let whole_panic = |sp| {
        const_panic_msg(
            oopsie_path,
            sp,
            &whole_parts,
            &[quote! { ::core::mem::size_of::<#ident>() }],
        )
    };

    let fielded: Vec<&ResolvedVariant<'_>> = variants
        .iter()
        .filter(|v| !v.variant.fields.is_empty())
        .collect();

    if fielded.is_empty() {
        let p = whole_panic(attr_span);
        return quote! {
            if ::core::mem::size_of::<#ident>() #cmp #limit { #p }
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
        let vname = v.variant.ident.to_string();
        let parts = [
            format!("`{ident}` is "),
            format!(" bytes, must be {phrase}; largest variant `{vname}` is "),
            format!(" bytes{note}"),
        ];
        let p = const_panic_msg(
            oopsie_path,
            v.variant.ident.span(),
            &parts,
            &[quote! { ::core::mem::size_of::<#ident>() }, size.clone()],
        );
        quote! { #( #cfg )* if #size == max_payload { #p } }
    });

    // Reached only when every field-bearing variant is cfg-stripped on this
    // target: report the whole-type violation rather than blaming a variant.
    let fallback = whole_panic(attr_span);

    quote! {
        if ::core::mem::size_of::<#ident>() #cmp #limit {
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

/// The size assertion for a struct (whole-type, no variant attribution).
pub(super) fn gen_size_assertion(
    oopsie_path: &syn::Path,
    ident: &syn::Ident,
    size: &SizeAttr,
) -> TokenStream2 {
    gen_whole_type_checks(
        oopsie_path,
        ident,
        size.span,
        upper_bound(&size.constraint),
        lower_bound(&size.constraint),
        &bound_phrase(&size.constraint),
        "",
    )
}

/// The size assertion for an enum. An upper-bound violation is blamed on the
/// largest variant; a lower-bound violation (too *small*) isn't any one
/// variant's fault and stays a whole-type assertion at the `size(...)` attribute.
pub(super) fn gen_enum_size_assertion(
    oopsie_path: &syn::Path,
    ident: &syn::Ident,
    variants: &[ResolvedVariant<'_>],
    size: &SizeAttr,
) -> TokenStream2 {
    let phrase = bound_phrase(&size.constraint);
    let upper = upper_bound(&size.constraint).map(|(limit, inclusive)| {
        gen_enum_upper(&EnumUpperParams {
            oopsie_path,
            ident,
            variants,
            attr_span: size.span,
            limit,
            inclusive,
            phrase: &phrase,
            note: "",
        })
    });
    let lower = lower_bound(&size.constraint).map(|limit| {
        let parts = [
            format!("`{ident}` is "),
            format!(" bytes, must be {phrase}"),
        ];
        let p = const_panic_msg(
            oopsie_path,
            size.span,
            &parts,
            &[quote! { ::core::mem::size_of::<#ident>() }],
        );
        quote! { if ::core::mem::size_of::<#ident>() < #limit { #p } }
    });
    quote! { #upper #lower }
}

/// The manifest-cap assertion for a struct: a plain `<= cap` upper bound spanned
/// at the type, since a struct has no variants to attribute the blame to.
pub(super) fn gen_default_size_cap_struct(
    oopsie_path: &syn::Path,
    ident: &syn::Ident,
    cap: usize,
    section: &str,
) -> TokenStream2 {
    gen_whole_type_checks(
        oopsie_path,
        ident,
        ident.span(),
        Some((cap, true)),
        None,
        &format!("≤ {cap}"),
        &cap_source_line(section),
    )
}

/// The manifest-cap assertion for an enum, reusing the variant-blaming upper-bound
/// path so an over-cap enum points at its largest variant (with the cap source on
/// its own line at the end).
pub(super) fn gen_default_size_cap_enum(
    oopsie_path: &syn::Path,
    ident: &syn::Ident,
    variants: &[ResolvedVariant<'_>],
    cap: usize,
    section: &str,
) -> TokenStream2 {
    gen_enum_upper(&EnumUpperParams {
        oopsie_path,
        ident,
        variants,
        attr_span: ident.span(),
        limit: cap,
        inclusive: true,
        phrase: &format!("≤ {cap}"),
        note: &cap_source_line(section),
    })
}
