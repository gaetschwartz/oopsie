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

    fn expand_item(
        args: &TracedArgs,
        defaults: &TracedDefaults,
        mut item: syn::ItemStruct,
    ) -> syn::Result<String> {
        expand_struct(
            args,
            defaults,
            &parse_quote!(::oopsie),
            Span::call_site(),
            &mut item,
        )?;
        Ok(item.into_token_stream().to_string())
    }

    fn expand(args: &TracedArgs, defaults: &TracedDefaults) -> syn::Result<String> {
        expand_item(
            args,
            defaults,
            parse_quote! { struct Seen { last_seen: ::std::time::SystemTime } },
        )
    }

    fn explicit_timestamp() -> TracedArgs {
        TracedArgs::from_meta(&parse_quote!(traced(
            timestamp,
            code = false,
            location = false
        )))
        .unwrap()
    }

    #[test]
    fn timestamp_conflict_names_the_configurations_it_occurs_in() {
        let err = expand_item(
            &explicit_timestamp(),
            &TracedDefaults::default(),
            parse_quote! {
                struct Seen { #[cfg(feature = "a")] at: ::std::time::SystemTime }
            },
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "{} (in configurations where `cfg(feature = \"a\")` holds)",
                crate::traced::inject::timestamp_conflict_error(Span::call_site())
            )
        );
    }

    #[test]
    fn impossible_timestamp_conflict_is_not_reported() {
        let out = expand_item(
            &explicit_timestamp(),
            &TracedDefaults::default(),
            parse_quote! {
                struct Seen {
                    #[cfg(all(feature = "a", not(feature = "a")))]
                    at: ::std::time::SystemTime,
                }
            },
        )
        .unwrap();
        insta::assert_snapshot!(out);
    }

    #[test]
    fn statically_absent_timestamp_field_is_no_conflict() {
        let out = expand_item(
            &explicit_timestamp(),
            &TracedDefaults::default(),
            parse_quote! {
                struct Seen { #[cfg(any())] at: ::std::time::SystemTime }
            },
        )
        .unwrap();
        insta::assert_snapshot!(out);
    }

    #[test]
    fn injection_gate_is_minimised() {
        let out = expand_item(
            &TracedArgs::from_meta(&parse_quote!(traced(packed = false, code = false))).unwrap(),
            &TracedDefaults::default(),
            parse_quote! {
                struct S {
                    #[cfg(all(feature = "a", feature = "b"))] bt: Backtrace,
                    #[cfg(all(feature = "a", not(feature = "b")))] bt2: Backtrace,
                    #[cfg(feature = "c")] st: SpanTrace,
                }
            },
        )
        .unwrap();
        insta::assert_snapshot!(out);
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
