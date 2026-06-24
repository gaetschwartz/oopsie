//! Enum trace-field injection for `#[oopsie(traced)]`.

use proc_macro2::Span;
use syn::ext::IdentExt as _;

use super::args::TracedArgs;
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{add_provide_attrs, check_existing_fields, inject_fields};
use crate::derive::parse::{ResolvedForward, VariantAttrs, field_forward};
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

    // Process variants
    for variant in &mut input.variants {
        // Parse once so injection and auto-code decisions share the same
        // variant-level attribute view.
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let variant_traced_enabled = variant_attrs.traced.is_enabled();

        let existence = check_existing_fields(&variant.fields, &config.timestamp_type);

        let forward = if variant_traced_enabled {
            variant
                .fields
                .iter()
                .map(field_forward)
                .collect::<syn::Result<Vec<_>>>()?
                .into_iter()
                .find(|f| f.any())
                .unwrap_or_default()
        } else {
            ResolvedForward::default()
        };

        let inject_backtrace = variant_traced_enabled && resolved.backtrace && !forward.backtrace;
        let inject_spantrace = variant_traced_enabled && resolved.spantrace && !forward.spantrace;
        let inject_location = variant_traced_enabled && resolved.location && !forward.location;

        let packed = resolved.packed
            && inject_backtrace
            && inject_spantrace
            && !existence.has_backtrace
            && !existence.has_spantrace
            && !existence.has_traces;

        let to_inject = FieldsToInject {
            backtrace: !packed && inject_backtrace && !existence.has_backtrace,
            spantrace: !packed && inject_spantrace && !existence.has_spantrace,
            timestamp: variant_traced_enabled && resolved.timestamp && !existence.has_timestamp,
            traces: packed,
            location: inject_location && !existence.has_location,
        };

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

        let has_user_code = variant_attrs.code.is_some();
        let is_transparent = variant_attrs.transparent;

        let variant_name = variant.ident.unraw().to_string();
        add_provide_attrs(
            &mut variant.attrs,
            &config,
            &enum_name,
            Some(&variant_name),
            variant_traced_enabled && resolved.code,
            has_user_code,
            is_transparent,
        );
    }

    Ok(())
}
