//! IDE hover documentation for `oopsie` attribute keywords, both the
//! `#[oopsie(...)]` helper attributes and the `#[oopsie]` attribute macro's
//! own arguments.

use darling::ast::NestedMeta;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{Ident, Meta, Token};

/// Generate a hidden block of `use ... as _;` statements pointing every
/// keyword at its entry in `__private::documented::<scope>`.
///
/// Each interpolated ident is the one parsed from the user's attribute and
/// keeps its original span — that span identity is what lets rust-analyzer
/// resolve a hover on the keyword through the `use` path to the documented
/// item. Rebuilding the ident at call-site span would still compile but
/// silently kill hover. For the same reason every occurrence emits its own
/// `use` (no dedup): each one carries a distinct usage-site span.
pub fn gen_use_block(oopsie_path: &syn::Path, scopes: &[(&str, &[Ident])]) -> TokenStream2 {
    if scopes.iter().all(|(_, idents)| idents.is_empty()) {
        return TokenStream2::new();
    }

    let uses = scopes.iter().flat_map(|(scope, idents)| {
        let scope = format_ident!("{scope}");
        idents.iter().map(move |id| {
            quote! { use #oopsie_path::__private::documented::#scope::#id as _; }
        })
    });

    quote! {
        #[doc(hidden)]
        #[allow(unused_imports)]
        const _: () = {
            #(#uses)*
        };
    }
}

/// Collect the keyword ident of every meta in every `#[oopsie(...)]` attr into
/// `out`, and the `oopsie` attribute-name ident of each such attr into
/// `helpers` (so the attribute name itself becomes a hover target, not only its
/// contents). Attributes whose body is not a meta list (the short-display form
/// `#[oopsie("fmt {}", arg)]`) contribute their name but no keywords: this
/// pre-pass must never error — validity checking is darling's job.
fn collect_into(attrs: &[syn::Attribute], out: &mut Vec<Ident>, helpers: &mut Vec<Ident>) {
    for attr in attrs {
        let Some(name) = attr.path().get_ident() else {
            continue;
        };
        if name != "oopsie" {
            continue;
        }
        helpers.push(name.clone());
        let Ok(metas) = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        else {
            continue;
        };
        out.extend(
            metas
                .iter()
                .filter_map(|meta| meta.path().get_ident().cloned()),
        );
    }
}

fn is_container_key(ident: &Ident) -> bool {
    ["module", "suffix", "size", "path", "vis"]
        .iter()
        .any(|k| ident == k)
}

/// Hover-doc block for every `#[oopsie(...)]` helper-attribute keyword on a
/// derive input, scoped to `container` / `variant` / `field`.
pub fn gen_keyword_docs(input: &syn::DeriveInput, oopsie_path: &syn::Path) -> TokenStream2 {
    let mut collected = Vec::new();
    let mut helper = Vec::new();
    collect_into(&input.attrs, &mut collected, &mut helper);
    // A struct's `#[oopsie(...)]` mixes container and variant scopes in one
    // list, so container attrs are routed by key name. The same split is a
    // no-op for enums, whose container attrs hold container keys only.
    let (container, mut variant): (Vec<_>, Vec<_>) =
        collected.into_iter().partition(is_container_key);

    let mut field = Vec::new();
    match &input.data {
        syn::Data::Enum(data) => {
            for v in &data.variants {
                collect_into(&v.attrs, &mut variant, &mut helper);
                for f in &v.fields {
                    collect_into(&f.attrs, &mut field, &mut helper);
                }
            }
        }
        syn::Data::Struct(data) => {
            for f in &data.fields {
                collect_into(&f.attrs, &mut field, &mut helper);
            }
        }
        syn::Data::Union(_) => {}
    }

    gen_use_block(
        oopsie_path,
        &[
            ("helper", &helper),
            ("container", &container),
            ("variant", &variant),
            ("field", &field),
        ],
    )
}

/// Collect the keyword idents of the `#[oopsie]` attribute macro's own
/// argument list: top-level keys go to the `attr` scope, keys nested inside a
/// list body (at any depth) to the `traced` scope. List bodies that are not
/// meta lists (the bare-bool form `traced(false)`) are skipped — the outer
/// keyword is already collected and validity checking is darling's job.
pub fn collect_attr_keywords(meta: &[NestedMeta]) -> (Vec<Ident>, Vec<Ident>) {
    fn recurse_lists(meta: &Meta, out: &mut Vec<Ident>) {
        let Meta::List(list) = meta else { return };
        let Ok(metas) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        else {
            return;
        };
        for inner in &metas {
            out.extend(inner.path().get_ident().cloned());
            recurse_lists(inner, out);
        }
    }

    let mut attr = Vec::new();
    let mut traced = Vec::new();
    for m in meta {
        let NestedMeta::Meta(m) = m else { continue };
        attr.extend(m.path().get_ident().cloned());
        recurse_lists(m, &mut traced);
    }
    (attr, traced)
}
