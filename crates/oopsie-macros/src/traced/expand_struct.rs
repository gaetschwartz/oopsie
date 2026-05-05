//! Struct expansion for `#[traced]`.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse_quote;

use super::args::TracedArgs;
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{
    add_provide_attrs, check_existing_fields, has_oopsie_name_value, inject_fields,
};

pub(crate) fn expand_struct(
    args: &TracedArgs,
    _args_span: Span,
    mut input: syn::ItemStruct,
) -> syn::Result<TokenStream2> {
    let struct_name = input.ident.to_string();
    let oopsie_path = args
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie });

    let resolved = args.resolve();
    let config = FieldInjectorConfig::new(args, &resolved, &oopsie_path);

    // Check existing fields and inject
    let existence = check_existing_fields(&input.fields, &config.timestamp_type);
    let to_inject = FieldsToInject {
        backtrace: resolved.backtrace && !existence.has_backtrace,
        spantrace: resolved.spantrace && !existence.has_spantrace,
        timestamp: resolved.timestamp && !existence.has_timestamp,
    };

    inject_fields(&mut input.fields, &config, &to_inject)?;

    // Check if user specified `code = "..."` in #[oopsie(...)] to suppress auto-code
    let has_user_code = has_oopsie_name_value(&input.attrs, "code");

    // Add struct-level provide attrs
    add_provide_attrs(
        &mut input.attrs,
        &config,
        &struct_name,
        None,
        args.code.is_enabled(),
        has_user_code,
    );

    Ok(quote! { #input })
}
