//! Enum trace-field injection for `#[oopsie(traced)]`.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;

use super::args::{CodeSettings, TracedArgs};
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{
    add_provide_attrs, check_existing_fields, has_oopsie_flag, has_oopsie_meta, inject_fields,
};
use crate::utils::FieldSetting;

pub fn expand_enum(
    args: &TracedArgs,
    code: &FieldSetting<true, CodeSettings>,
    oopsie_path: &syn::Path,
    args_span: Span,
    mut input: syn::ItemEnum,
) -> syn::Result<TokenStream2> {
    let enum_name = input.ident.to_string();

    let resolved = args.resolve();
    resolved.validate(args_span)?;
    let config = FieldInjectorConfig::new(&resolved, code, oopsie_path);

    // Process variants
    for variant in &mut input.variants {
        let existence = check_existing_fields(&variant.fields, &config.timestamp_type);

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

        inject_fields(&mut variant.fields, &config, &to_inject)?;

        // A user-supplied `code` (any form) suppresses the auto-code.
        let has_user_code = has_oopsie_meta(&variant.attrs, "code");
        let is_transparent = has_oopsie_flag(&variant.attrs, "transparent");

        let variant_name = variant.ident.to_string();
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

    Ok(quote! { #input })
}
