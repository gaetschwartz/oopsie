//! Struct trace-field injection for `#[oopsie(traced)]`.

use proc_macro2::Span;
use syn::ext::IdentExt as _;

use super::args::TracedArgs;
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{add_provide_attrs, check_existing_fields, inject_fields};
use crate::derive::parse::{StructAttrs, field_forward};
use crate::utils::TracedDefaults;

pub fn expand_struct(
    args: &TracedArgs,
    defaults: &TracedDefaults,
    oopsie_path: &syn::Path,
    args_span: Span,
    input: &mut syn::ItemStruct,
) -> syn::Result<()> {
    let struct_name = input.ident.unraw().to_string();

    let resolved = args.resolve(defaults);
    resolved.validate(args_span)?;
    let config = FieldInjectorConfig::new(&resolved, args.code.inner(), oopsie_path);

    let existence = check_existing_fields(&input.fields, &config.timestamp_type)?;

    let forward = input
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

    // Packed only applies when both traces are enabled and no trace field
    // already exists; otherwise fall back to per-field (unpacked) injection,
    // which also covers the single-trace case.
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

    inject_fields(&mut input.fields, &config, &to_inject)?;

    // Read the auto-code suppression facts through the same parser the derive
    // layer uses, so injection and codegen can't disagree on what counts as a
    // user `code` or a `transparent` struct.
    let struct_attrs = StructAttrs::from_attrs(&input.attrs)?;
    let has_user_code = struct_attrs.code.is_some();
    let is_transparent = struct_attrs.transparent;

    // Add struct-level provide attrs
    add_provide_attrs(
        &mut input.attrs,
        &config,
        &struct_name,
        None,
        resolved.code,
        has_user_code,
        is_transparent,
    );

    Ok(())
}
