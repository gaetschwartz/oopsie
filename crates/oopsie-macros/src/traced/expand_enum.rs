//! Enum expansion for `#[traced]`.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse_quote;

use super::args::TracedArgs;
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{
    add_provide_attrs, check_existing_fields, has_oopsie_name_value, inject_fields,
};

pub(super) fn expand_enum(
    args: &TracedArgs,
    _args_span: Span,
    mut input: syn::ItemEnum,
) -> syn::Result<TokenStream2> {
    let enum_name = input.ident.to_string();
    let magnetite_utils_path = args
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie });

    let resolved = args.resolve();
    let config = FieldInjectorConfig::new(args, &resolved, magnetite_utils_path);

    // Process variants
    for variant in &mut input.variants {
        let existence = check_existing_fields(&variant.fields, &config.timestamp_type);
        let to_inject = FieldsToInject {
            backtrace: resolved.backtrace && !existence.has_backtrace,
            spantrace: resolved.spantrace && !existence.has_spantrace,
            timestamp: resolved.timestamp && !existence.has_timestamp,
        };

        inject_fields(&mut variant.fields, &config, &to_inject)?;

        // Check if user specified `code = "..."` in #[oopsie(...)] to suppress auto-code
        let has_user_code = has_oopsie_name_value(&variant.attrs, "code");

        let variant_name = variant.ident.to_string();
        add_provide_attrs(
            &mut variant.attrs,
            &config,
            &enum_name,
            Some(&variant_name),
            to_inject.backtrace,
            to_inject.spantrace,
            args.code.is_enabled(),
            has_user_code,
        );
    }

    Ok(quote! { #input })
}
