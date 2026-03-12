//! Field injection helpers.

use syn::punctuated::Punctuated;
use syn::{Fields, FieldsNamed, parse_quote, token};

use super::args::ErrorArgs;
use super::config::{FieldExistence, FieldInjectorConfig, FieldsToInject};
use super::type_check::{is_backtrace_type, is_spantrace_type};

/// Check which fields already exist in a `Fields` collection.
pub(super) fn check_existing_fields(fields: &Fields, timestamp_type: &syn::Type) -> FieldExistence {
    let mut existence = FieldExistence::default();

    let iter: Box<dyn Iterator<Item = &syn::Field>> = match fields {
        Fields::Named(f) => Box::new(f.named.iter()),
        Fields::Unnamed(f) => Box::new(f.unnamed.iter()),
        Fields::Unit => return existence,
    };

    for field in iter {
        if is_backtrace_type(&field.ty) {
            existence.has_backtrace = true;
        }
        if is_spantrace_type(&field.ty) {
            existence.has_spantrace = true;
        }
        if field.ty == *timestamp_type {
            existence.has_timestamp = true;
        }
    }
    existence
}

/// Inject backtrace/spantrace/timestamp fields into a `Fields` collection.
pub(super) fn inject_fields(
    fields: &mut Fields,
    config: &FieldInjectorConfig,
    to_inject: &FieldsToInject,
) -> syn::Result<()> {
    match fields {
        Fields::Named(named) => {
            inject_into_named(named, config, to_inject);
            Ok(())
        }
        Fields::Unit => {
            let mut named = FieldsNamed {
                brace_token: token::Brace::default(),
                named: Punctuated::default(),
            };
            inject_into_named(&mut named, config, to_inject);
            *fields = Fields::Named(named);
            Ok(())
        }
        Fields::Unnamed(unnamed) => Err(syn::Error::new_spanned(
            unnamed,
            "`#[oopsie]` does not support tuple variants/structs; use named fields instead",
        )),
    }
}

fn inject_into_named(
    fields: &mut FieldsNamed,
    config: &FieldInjectorConfig,
    to_inject: &FieldsToInject,
) {
    let FieldInjectorConfig {
        backtrace_ident,
        backtrace_type,
        backtrace_attrs,
        spantrace_ident,
        spantrace_type,
        spantrace_attrs,
        timestamp_ident,
        timestamp_type,
        timestamp_provide_attr,
        ..
    } = config;

    if to_inject.backtrace {
        fields
            .named
            .push(parse_quote! { #backtrace_attrs #backtrace_ident: #backtrace_type });
    }
    if to_inject.spantrace {
        fields
            .named
            .push(parse_quote! { #spantrace_attrs #spantrace_ident: #spantrace_type });
    }
    if to_inject.timestamp {
        fields
            .named
            .push(parse_quote! { #timestamp_provide_attr #timestamp_ident: #timestamp_type });
    }
}

/// Check whether any `#[oopsie(...)]` attribute on this item contains a
/// name-value entry like `code = "..."` or `help = "..."`.
pub(super) fn has_oopsie_name_value(attrs: &[syn::Attribute], key: &str) -> bool {
    for attr in attrs {
        if !attr.path().is_ident("oopsie") {
            continue;
        }
        let Ok(tokens) = attr.parse_args::<proc_macro2::TokenStream>() else {
            continue;
        };
        let mut iter = tokens.into_iter().peekable();
        while let Some(tok) = iter.next() {
            if let proc_macro2::TokenTree::Ident(ident) = &tok
                && ident == key
                && let Some(proc_macro2::TokenTree::Punct(p)) = iter.peek()
                && p.as_char() == '='
            {
                return true;
            }
        }
    }
    false
}

/// Add Oopsie provide attributes for backtrace, spantrace, and auto-generated error code.
///
///
/// Help text and user-specified error codes are handled by the derive macro
/// via `#[oopsie(help = "...", code = "...")]` on variants/structs.
#[expect(clippy::too_many_arguments)]
pub(super) fn add_provide_attrs(
    attrs: &mut Vec<syn::Attribute>,
    config: &FieldInjectorConfig,
    args: &ErrorArgs,
    type_name: &str,
    variant_name: Option<&str>,
    added_backtrace: bool,
    added_spantrace: bool,
    has_user_code: bool,
) {
    let FieldInjectorConfig {
        backtrace_ident,
        spantrace_ident,
        code_type,
        magnetite_utils_path,
        ..
    } = config;

    if added_backtrace {
        attrs.push(
            parse_quote! { #[oopsie(provide(ref, #magnetite_utils_path::Backtrace => #backtrace_ident.as_ref()))] },
        );
    }

    if added_spantrace {
        attrs.push(
            parse_quote! { #[oopsie(provide(ref, #magnetite_utils_path::Spantrace => #spantrace_ident.as_ref()))] },
        );
    }

    // Only generate auto-code from module_path!() when the code feature is enabled
    // AND the user did not specify their own `code = "..."` on the variant/struct.
    if args.code.is_enabled() && !has_user_code {
        let mut name = type_name.to_owned();
        if let Some(v) = variant_name {
            name.push_str("::");
            name.push_str(v);
        }
        let attr = parse_quote! { #[oopsie(provide(#code_type => #code_type::from(concat!(module_path!(), "::", #name))))] };
        attrs.push(attr);
    }
}

#[cfg(test)]
mod tests {
    use quote::{format_ident, quote};
    use syn::parse_quote;

    use super::*;
    use crate::utils::{BetterFlag, FieldSetting};

    /// Parse fields from a struct definition.
    fn parse_fields(tokens: proc_macro2::TokenStream) -> syn::Fields {
        let item: syn::ItemStruct = syn::parse2(tokens).expect("failed to parse struct");
        item.fields
    }

    // ── Helper to build a minimal FieldInjectorConfig ────────────────

    fn test_config() -> FieldInjectorConfig {
        FieldInjectorConfig {
            backtrace_ident: format_ident!("__oopsie_backtrace"),
            backtrace_type: quote! { ::std::boxed::Box<Backtrace> },
            backtrace_attrs: quote! { #[oopsie(auto)] },
            spantrace_ident: format_ident!("__oopsie_spantrace"),
            spantrace_type: quote! { ::std::boxed::Box<Spantrace> },
            spantrace_attrs: quote! { #[oopsie(auto)] },
            timestamp_ident: format_ident!("__oopsie_timestamp"),
            timestamp_type: parse_quote! { std::time::SystemTime },
            timestamp_provide_attr: None,
            code_type: quote! { ErrorCode },
            magnetite_utils_path: parse_quote! { oopsie },
        }
    }

    fn test_args(code_enabled: bool) -> ErrorArgs {
        ErrorArgs {
            spantrace: FieldSetting::Flag(true),
            backtrace: FieldSetting::Flag(true),
            timestamp: FieldSetting::Flag(false),
            code: FieldSetting::Flag(code_enabled),
            path: None,
            module: BetterFlag::Default,
            no_suffix: BetterFlag::Default,
            display: None,
        }
    }

    // ── check_existing_fields ────────────────────────────────────────

    #[test]
    fn check_existing_fields_detects_backtrace() {
        let fields = parse_fields(quote! { struct S { backtrace: Backtrace, message: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = check_existing_fields(&fields, &ts);
        assert!(existence.has_backtrace);
        assert!(!existence.has_spantrace);
        assert!(!existence.has_timestamp);
    }

    #[test]
    fn check_existing_fields_detects_spantrace() {
        let fields = parse_fields(quote! { struct S { trace: Spantrace, message: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = check_existing_fields(&fields, &ts);
        assert!(!existence.has_backtrace);
        assert!(existence.has_spantrace);
        assert!(!existence.has_timestamp);
    }

    #[test]
    fn check_existing_fields_detects_timestamp() {
        let fields = parse_fields(quote! { struct S { ts: std::time::Instant, message: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = check_existing_fields(&fields, &ts);
        assert!(!existence.has_backtrace);
        assert!(!existence.has_spantrace);
        assert!(existence.has_timestamp);
    }

    #[test]
    fn check_existing_fields_detects_all() {
        let fields = parse_fields(
            quote! { struct S { bt: Backtrace, st: Spantrace, ts: std::time::Instant } },
        );
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = check_existing_fields(&fields, &ts);
        assert!(existence.has_backtrace);
        assert!(existence.has_spantrace);
        assert!(existence.has_timestamp);
    }

    #[test]
    fn check_existing_fields_detects_none() {
        let fields = parse_fields(quote! { struct S { message: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = check_existing_fields(&fields, &ts);
        assert!(!existence.has_backtrace);
        assert!(!existence.has_spantrace);
        assert!(!existence.has_timestamp);
    }

    #[test]
    fn check_existing_fields_unit_returns_default() {
        let fields: syn::Fields = syn::Fields::Unit;
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = check_existing_fields(&fields, &ts);
        assert!(!existence.has_backtrace);
        assert!(!existence.has_spantrace);
        assert!(!existence.has_timestamp);
    }

    #[test]
    fn check_existing_fields_unnamed() {
        let fields = parse_fields(quote! { struct S(Backtrace, Spantrace); });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = check_existing_fields(&fields, &ts);
        assert!(existence.has_backtrace);
        assert!(existence.has_spantrace);
    }

    #[test]
    fn check_existing_fields_timestamp_type_mismatch() {
        // Field type is Instant, but we check for SystemTime — should not match.
        let fields = parse_fields(quote! { struct S { ts: std::time::Instant } });
        let ts: syn::Type = parse_quote!(std::time::SystemTime);
        let existence = check_existing_fields(&fields, &ts);
        assert!(!existence.has_timestamp);
    }

    // ── has_oopsie_name_value ────────────────────────────────────────

    #[test]
    fn has_oopsie_name_value_finds_code() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[oopsie(code = "my::error")]
        };
        assert!(has_oopsie_name_value(&attrs, "code"));
    }

    #[test]
    fn has_oopsie_name_value_missing_key() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[oopsie(code = "my::error")]
        };
        assert!(!has_oopsie_name_value(&attrs, "help"));
    }

    #[test]
    fn has_oopsie_name_value_non_oopsie_attr() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[derive(Debug)]
        };
        assert!(!has_oopsie_name_value(&attrs, "code"));
    }

    #[test]
    fn has_oopsie_name_value_display_key() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[oopsie(display = "error")]
        };
        assert!(has_oopsie_name_value(&attrs, "display"));
        assert!(!has_oopsie_name_value(&attrs, "code"));
    }

    #[test]
    fn has_oopsie_name_value_empty_attrs() {
        let attrs: Vec<syn::Attribute> = vec![];
        assert!(!has_oopsie_name_value(&attrs, "code"));
    }

    #[test]
    fn has_oopsie_name_value_multiple_attrs() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[derive(Debug)]
            #[oopsie(help = "some help")]
        };
        assert!(has_oopsie_name_value(&attrs, "help"));
        assert!(!has_oopsie_name_value(&attrs, "code"));
    }

    #[test]
    fn has_oopsie_name_value_ident_without_equals() {
        // `code` as a path (no `=`) should NOT match.
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[oopsie(code)]
        };
        assert!(!has_oopsie_name_value(&attrs, "code"));
    }

    // ── inject_fields / inject_into_named ────────────────────────────

    #[test]
    fn inject_fields_adds_backtrace_to_named() {
        let mut fields = parse_fields(quote! { struct S { existing: String } });
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: true,
            spantrace: false,
            timestamp: false,
        };
        inject_fields(&mut fields, &config, &to_inject).unwrap();
        match &fields {
            syn::Fields::Named(named) => assert_eq!(named.named.len(), 2),
            _ => panic!("expected named fields"),
        }
    }

    #[test]
    fn inject_fields_adds_all_three() {
        let mut fields = parse_fields(quote! { struct S { existing: String } });
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: true,
            spantrace: true,
            timestamp: true,
        };
        inject_fields(&mut fields, &config, &to_inject).unwrap();
        match &fields {
            syn::Fields::Named(named) => assert_eq!(named.named.len(), 4),
            _ => panic!("expected named fields"),
        }
    }

    #[test]
    fn inject_fields_adds_nothing_when_all_false() {
        let mut fields = parse_fields(quote! { struct S { existing: String } });
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: false,
            spantrace: false,
            timestamp: false,
        };
        inject_fields(&mut fields, &config, &to_inject).unwrap();
        match &fields {
            syn::Fields::Named(named) => assert_eq!(named.named.len(), 1),
            _ => panic!("expected named fields"),
        }
    }

    #[test]
    fn inject_fields_converts_unit_to_named() {
        let mut fields = syn::Fields::Unit;
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: true,
            spantrace: false,
            timestamp: false,
        };
        inject_fields(&mut fields, &config, &to_inject).unwrap();
        assert!(matches!(fields, syn::Fields::Named(_)));
    }

    #[test]
    fn inject_fields_rejects_unnamed() {
        let mut fields = parse_fields(quote! { struct S(String, u32); });
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: true,
            spantrace: false,
            timestamp: false,
        };
        assert!(inject_fields(&mut fields, &config, &to_inject).is_err());
    }

    // ── add_provide_attrs ────────────────────────────────────────────

    #[test]
    fn add_provide_attrs_adds_backtrace_attr() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        let args = test_args(false);
        add_provide_attrs(
            &mut attrs, &config, &args, "MyError", None, true, false, false,
        );
        assert_eq!(attrs.len(), 1);
    }

    #[test]
    fn add_provide_attrs_adds_spantrace_attr() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        let args = test_args(false);
        add_provide_attrs(
            &mut attrs, &config, &args, "MyError", None, false, true, false,
        );
        assert_eq!(attrs.len(), 1);
    }

    #[test]
    fn add_provide_attrs_adds_both_trace_attrs() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        let args = test_args(false);
        add_provide_attrs(
            &mut attrs, &config, &args, "MyError", None, true, true, false,
        );
        assert_eq!(attrs.len(), 2);
    }

    #[test]
    fn add_provide_attrs_adds_code_when_enabled_and_no_user_code() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        let args = test_args(true);
        add_provide_attrs(
            &mut attrs, &config, &args, "MyError", None, false, false, false,
        );
        assert_eq!(attrs.len(), 1);
    }

    #[test]
    fn add_provide_attrs_skips_code_when_user_code_present() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        let args = test_args(true);
        add_provide_attrs(
            &mut attrs, &config, &args, "MyError", None, false, false, true,
        );
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn add_provide_attrs_skips_code_when_disabled() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        let args = test_args(false);
        add_provide_attrs(
            &mut attrs, &config, &args, "MyError", None, false, false, false,
        );
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn add_provide_attrs_with_variant_name() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        let args = test_args(true);
        add_provide_attrs(
            &mut attrs,
            &config,
            &args,
            "MyError",
            Some("Variant"),
            false,
            false,
            false,
        );
        assert_eq!(attrs.len(), 1);
        // The generated attr should contain "MyError::Variant"
        let attr_str = quote! { #(#attrs)* }.to_string();
        assert!(
            attr_str.contains("MyError::Variant"),
            "expected attr to contain MyError::Variant, got: {attr_str}"
        );
    }

    #[test]
    fn add_provide_attrs_nothing_when_all_false() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        let args = test_args(false);
        add_provide_attrs(
            &mut attrs, &config, &args, "MyError", None, false, false, false,
        );
        assert!(attrs.is_empty());
    }
}
