//! Enum trace-field injection for `#[oopsie(traced)]`.

use proc_macro2::Span;
use syn::ext::IdentExt as _;

use super::args::{CodeSettings, TracedArgs};
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{add_provide_attrs, check_existing_fields, inject_fields};
use crate::derive::parse::{VariantAttrs, field_forward};
use crate::utils::FieldSetting;

pub fn expand_enum(
    args: &TracedArgs,
    code: &FieldSetting<true, CodeSettings>,
    oopsie_path: &syn::Path,
    args_span: Span,
    input: &mut syn::ItemEnum,
) -> syn::Result<()> {
    let enum_name = input.ident.unraw().to_string();

    let resolved = args.resolve();
    resolved.validate(args_span)?;
    let config = FieldInjectorConfig::new(&resolved, code, oopsie_path);

    // Process variants
    for variant in &mut input.variants {
        let existence = check_existing_fields(&variant.fields, &config.timestamp_type);

        let forward = variant
            .fields
            .iter()
            .map(field_forward)
            .collect::<syn::Result<Vec<_>>>()?
            .into_iter()
            .find(|f| f.any())
            .unwrap_or_default();

        let inject_backtrace = resolved.backtrace && !forward.backtrace;
        let inject_spantrace = resolved.spantrace && !forward.spantrace;
        let inject_location = resolved.location && !forward.location;

        let packed = resolved.packed
            && inject_backtrace
            && inject_spantrace
            && !existence.has_backtrace
            && !existence.has_spantrace
            && !existence.has_traces;

        let to_inject = FieldsToInject {
            backtrace: !packed && inject_backtrace && !existence.has_backtrace,
            spantrace: !packed && inject_spantrace && !existence.has_spantrace,
            timestamp: resolved.timestamp && !existence.has_timestamp,
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

        // Read the auto-code suppression facts through the same parser the
        // derive layer uses, so injection and codegen can't disagree on what
        // counts as a user `code` or a `transparent` variant.
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let has_user_code = variant_attrs.code.is_some();
        let is_transparent = variant_attrs.transparent;

        let variant_name = variant.ident.unraw().to_string();
        add_provide_attrs(
            &mut variant.attrs,
            &config,
            &enum_name,
            Some(&variant_name),
            code.is_enabled(),
            has_user_code,
            is_transparent,
        );
    }

    Ok(())
}
