//! Struct trace-field injection for `#[oopsie(traced)]`.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use super::args::{CodeSettings, TracedArgs};
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{
    add_provide_attrs, check_existing_fields, has_oopsie_flag, has_oopsie_meta, inject_fields,
};
use crate::utils::FieldSetting;

pub fn expand_struct(
    args: &TracedArgs,
    code: &FieldSetting<true, CodeSettings>,
    oopsie_path: &syn::Path,
    args_span: Span,
    mut input: syn::ItemStruct,
) -> syn::Result<TokenStream2> {
    let struct_name = input.ident.to_string();

    let resolved = args.resolve();
    resolved.validate(args_span)?;
    let config = FieldInjectorConfig::new(&resolved, code, oopsie_path);

    let existence = check_existing_fields(&input.fields, &config.timestamp_type);

    // Packed only applies when both traces are enabled and no trace field
    // already exists; otherwise fall back to per-field (unpacked) injection,
    // which also covers the single-trace case.
    let packed = resolved.packed
        && resolved.backtrace
        && resolved.spantrace
        && !existence.has_backtrace
        && !existence.has_spantrace
        && !existence.has_traces;

    let to_inject = FieldsToInject {
        backtrace: !packed && resolved.backtrace && !existence.has_backtrace,
        spantrace: !packed && resolved.spantrace && !existence.has_spantrace,
        timestamp: resolved.timestamp && !existence.has_timestamp,
        traces: packed,
    };

    inject_fields(&mut input.fields, &config, &to_inject)?;

    // A user-supplied `code` (any form) suppresses the auto-code.
    let has_user_code = has_oopsie_meta(&input.attrs, "code");
    let is_transparent = has_oopsie_flag(&input.attrs, "transparent");

    // Add struct-level provide attrs
    add_provide_attrs(
        &mut input.attrs,
        &config,
        &struct_name,
        None,
        code.is_enabled(),
        has_user_code,
        is_transparent,
    );

    Ok(quote! { #input })
}
