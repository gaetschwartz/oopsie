//! Struct trace-field injection for `#[oopsie(traced)]`.

use proc_macro2::Span;
use syn::ext::IdentExt as _;

use super::args::TracedArgs;
use super::config::FieldInjectorConfig;
use super::inject::{ItemFacts, add_provide_attrs, inject_fields, plan_injection};
use crate::derive::parse::StructAttrs;
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

    // Auto-code suppression facts come from the same parser the derive layer
    // uses, so injection and codegen can't disagree on what counts as a user
    // `code` or a `transparent` struct.
    let (to_inject, auto_code) = plan_injection(
        &resolved,
        &config,
        &input.attrs,
        &input.fields,
        input.ident.span(),
        |attrs| {
            let attrs = StructAttrs::from_attrs(attrs)?;
            Ok(ItemFacts {
                traced: true,
                has_user_code: attrs.code.is_some(),
                transparent: attrs.transparent,
            })
        },
    )?;

    inject_fields(&mut input.fields, &config, &to_inject)?;
    add_provide_attrs(&mut input.attrs, &config, &struct_name, None, &auto_code);

    Ok(())
}

#[cfg(test)]
mod tests {
    use darling::FromMeta as _;
    use quote::ToTokens as _;
    use syn::parse_quote;

    use super::*;

    fn expand(args: &TracedArgs, defaults: &TracedDefaults) -> syn::Result<String> {
        let mut item: syn::ItemStruct = parse_quote! {
            struct Seen { last_seen: ::std::time::SystemTime }
        };
        expand_struct(
            args,
            defaults,
            &parse_quote!(::oopsie),
            Span::call_site(),
            &mut item,
        )?;
        Ok(item.into_token_stream().to_string())
    }

    #[test]
    fn manifest_timestamp_skips_a_timestamp_typed_field_silently() {
        let defaults = TracedDefaults {
            traced: Some(true),
            timestamp: Some(true),
            ..TracedDefaults::default()
        };
        let out = expand(&TracedArgs::default(), &defaults).unwrap();
        assert!(!out.contains("__oopsie_timestamp"), "{out}");
    }

    #[test]
    fn explicit_timestamp_still_rejects_a_timestamp_typed_field() {
        let args = TracedArgs::from_meta(&parse_quote!(traced(timestamp))).unwrap();
        expand(&args, &TracedDefaults::default()).unwrap_err();
    }
}
