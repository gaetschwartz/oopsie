//! Enum trace-field injection for `#[oopsie(traced)]`.

use proc_macro2::Span;
use syn::ext::IdentExt as _;

use super::args::TracedArgs;
use super::config::FieldInjectorConfig;
use super::inject::{ItemFacts, add_provide_attrs, inject_fields, plan_injection};
use crate::derive::parse::VariantAttrs;
use crate::utils::TracedDefaults;

pub fn expand_enum(
    args: &TracedArgs,
    defaults: &TracedDefaults,
    oopsie_path: &syn::Path,
    args_span: Span,
    input: &mut syn::ItemEnum,
) -> syn::Result<()> {
    let enum_name = input.ident.unraw().to_string();

    let resolved = args.resolve(defaults);
    resolved.validate(args_span)?;
    let config = FieldInjectorConfig::new(&resolved, args.code.inner(), oopsie_path);

    for variant in &mut input.variants {
        let (to_inject, auto_code) = plan_injection(
            &resolved,
            &config,
            &variant.attrs,
            &variant.fields,
            variant.ident.span(),
            |attrs| {
                let attrs = VariantAttrs::from_attrs(attrs)?;
                Ok(ItemFacts {
                    traced: attrs.traced.is_enabled(),
                    has_user_code: attrs.code.is_some(),
                    transparent: attrs.transparent,
                })
            },
        )?;

        // An explicit discriminant requires a fieldless variant; injecting trace
        // fields would change its shape, which rustc rejects. Refuse here so the
        // user sees a discriminant-spanning message instead of one pointing into
        // the rewritten enum.
        if let Some((_, disc)) = &variant.discriminant
            && to_inject.any()
        {
            return Err(syn::Error::new_spanned(
                disc,
                "traced cannot inject fields into a variant with an explicit discriminant; \
                 remove the discriminant or the trace fields from this variant",
            ));
        }

        inject_fields(&mut variant.fields, &config, &to_inject)?;

        let variant_name = variant.ident.unraw().to_string();
        add_provide_attrs(
            &mut variant.attrs,
            &config,
            &enum_name,
            Some(&variant_name),
            &auto_code,
        );
    }

    Ok(())
}
