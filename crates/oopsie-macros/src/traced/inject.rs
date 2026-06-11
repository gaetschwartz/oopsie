//! Field injection helpers.

use syn::punctuated::Punctuated;
use syn::{Fields, FieldsNamed, parse_quote, token};

use super::config::{FieldExistence, FieldInjectorConfig, FieldsToInject};
use super::field_detect::{
    is_backtrace_type, is_spantrace_type, is_timestamp_type, is_traces_type,
};

/// Check which fields already exist in a `Fields` collection.
pub(super) fn check_existing_fields(fields: &Fields, timestamp_type: &syn::Type) -> FieldExistence {
    let mut existence = FieldExistence::default();

    let iter: Box<dyn Iterator<Item = &syn::Field>> = match fields {
        Fields::Named(f) => Box::new(f.named.iter()),
        Fields::Unnamed(f) => Box::new(f.unnamed.iter()),
        Fields::Unit => return existence,
    };

    for field in iter {
        // The mangled name guards re-expansion of already-injected fields; a
        // user field merely *named* `timestamp` of an unrelated type is an
        // ordinary field and does not suppress injection (same rule as
        // backtrace/spantrace detection).
        let is_injected_timestamp = field
            .ident
            .as_ref()
            .is_some_and(|id| id == "__oopsie_timestamp");

        if is_traces_type(&field.ty) {
            // One packed field supplies both traces; mark all three so neither
            // separate field is also injected.
            existence.has_traces = true;
            existence.has_backtrace = true;
            existence.has_spantrace = true;
        }

        if is_backtrace_type(&field.ty) {
            existence.has_backtrace = true;
        }
        if is_spantrace_type(&field.ty) {
            existence.has_spantrace = true;
        }
        if is_injected_timestamp || is_timestamp_type(&field.ty) || field.ty == *timestamp_type {
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
            "trace injection does not support tuple variants/structs; use named fields instead",
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
        timestamp_attrs,
        traces_ident,
        traces_type,
        traces_attrs,
        ..
    } = config;

    if to_inject.traces {
        fields
            .named
            .push(parse_quote! { #traces_attrs #traces_ident: #traces_type });
    }
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
            .push(parse_quote! { #timestamp_attrs #timestamp_ident: #timestamp_type });
    }
}

/// Whether any `#[oopsie(...)]` attribute carries a top-level entry named `key`
/// in either name-value (`key = "..."`) or list (`key(...)`) form — every shape
/// the derive layer accepts for `code`.
///
/// Parses each attribute as a comma-separated meta list so a string literal at
/// the head (the short-display form `#[oopsie("fmt", key = expr)]`, whose
/// `key = expr` is a format argument, not an attribute key) fails the meta parse
/// and is correctly ignored rather than matching the inner assignment.
pub(super) fn has_oopsie_meta(attrs: &[syn::Attribute], key: &str) -> bool {
    use syn::punctuated::Punctuated;

    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("oopsie"))
        .filter_map(|attr| {
            attr.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
                .ok()
        })
        .flatten()
        .any(|meta| meta.path().is_ident(key))
}

/// Check whether any `#[oopsie(...)]` attribute on this item contains a bare
/// flag like `transparent` — an ident not followed by `=` (name-value) or a
/// `(...)` group (call/list form). Complements [`has_oopsie_meta`].
pub(super) fn has_oopsie_flag(attrs: &[syn::Attribute], key: &str) -> bool {
    for attr in attrs {
        if !attr.path().is_ident("oopsie") {
            continue;
        }
        let Ok(tokens) = attr.parse_args::<proc_macro2::TokenStream>() else {
            continue;
        };
        let mut iter = tokens.into_iter().peekable();
        while let Some(tok) = iter.next() {
            let proc_macro2::TokenTree::Ident(ident) = &tok else {
                continue;
            };
            if ident != key {
                continue;
            }
            match iter.peek() {
                Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '=' => {}
                Some(proc_macro2::TokenTree::Group(_)) => {}
                _ => return true,
            }
        }
    }
    false
}

/// Add Oopsie provide attributes for auto-generated error code.
///
/// Backtrace and spantrace are handled by Diagnostic via field detection, so
/// only ErrorCode needs a provide attr for nightly Error::provide support.
///
/// A `transparent` item is skipped: its code is forwarded from the source, and
/// an injected auto-code would shadow that forward in code-resolution.
pub(super) fn add_provide_attrs(
    attrs: &mut Vec<syn::Attribute>,
    config: &FieldInjectorConfig,
    type_name: &str,
    variant_name: Option<&str>,
    code_enabled: bool,
    has_user_code: bool,
    is_transparent: bool,
) {
    let FieldInjectorConfig { code_type, .. } = config;

    // Auto-code is the fallback: skip it when the feature is off, when the user
    // wrote their own `code = "..."`, or when the item is `transparent` (its code
    // comes from the source).
    if code_enabled && !has_user_code && !is_transparent {
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

    fn parse_fields(tokens: proc_macro2::TokenStream) -> syn::Fields {
        let item: syn::ItemStruct = syn::parse2(tokens).expect("failed to parse struct");
        item.fields
    }

    fn test_config() -> FieldInjectorConfig {
        FieldInjectorConfig {
            backtrace_ident: format_ident!("__oopsie_backtrace"),
            backtrace_type: quote! { ::std::boxed::Box<Backtrace> },
            backtrace_attrs: quote! { #[oopsie(backtrace)] },
            spantrace_ident: format_ident!("__oopsie_spantrace"),
            spantrace_type: quote! { ::std::boxed::Box<SpanTrace> },
            spantrace_attrs: quote! { #[oopsie(spantrace)] },
            timestamp_ident: format_ident!("__oopsie_timestamp"),
            timestamp_type: parse_quote! { std::time::SystemTime },
            timestamp_attrs: quote! { #[oopsie(capture)] },
            traces_ident: format_ident!("__oopsie_traces"),
            traces_type: quote! { ::std::boxed::Box<(Backtrace, SpanTrace)> },
            traces_attrs: quote! { #[oopsie(traces)] },
            code_type: quote! { ErrorCode },
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
        let fields = parse_fields(quote! { struct S { trace: SpanTrace, message: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = check_existing_fields(&fields, &ts);
        assert!(!existence.has_backtrace);
        assert!(existence.has_spantrace);
        assert!(!existence.has_timestamp);
    }

    #[test]
    fn wrongly_typed_timestamp_name_does_not_suppress_injection() {
        let fields = parse_fields(quote! { struct S { timestamp: u64, msg: String } });
        let ts: syn::Type = parse_quote!(::std::time::SystemTime);
        assert!(!check_existing_fields(&fields, &ts).has_timestamp);
    }

    #[test]
    fn timestamp_typed_field_suppresses_regardless_of_name() {
        let fields = parse_fields(quote! { struct S { when: SystemTime } });
        let ts: syn::Type = parse_quote!(::std::time::SystemTime);
        assert!(check_existing_fields(&fields, &ts).has_timestamp);
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
    }

    #[test]
    fn inject_fields_converts_unit_to_named() {
        let mut fields = syn::Fields::Unit;
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: true,
            spantrace: false,
            timestamp: false,
            traces: false,
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
            traces: false,
        };
        assert!(inject_fields(&mut fields, &config, &to_inject).is_err());
    }

    // ── add_provide_attrs ────────────────────────────────────────────

    #[test]
    fn add_provide_attrs_no_code_when_disabled() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(&mut attrs, &config, "MyError", None, false, false, false);
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn add_provide_attrs_adds_code_when_enabled() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(&mut attrs, &config, "MyError", None, true, false, false);
        assert_eq!(attrs.len(), 1);
    }

    #[test]
    fn add_provide_attrs_skips_code_when_user_code_present() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(&mut attrs, &config, "MyError", None, true, true, false);
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn add_provide_attrs_skips_code_when_transparent() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(
            &mut attrs,
            &config,
            "MyError",
            Some("Variant"),
            true,
            false,
            true,
        );
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn add_provide_attrs_with_variant_name() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(
            &mut attrs,
            &config,
            "MyError",
            Some("Variant"),
            true,
            false,
            false,
        );
        assert_eq!(attrs.len(), 1);
        let attr_str = quote! { #(#attrs)* }.to_string();
        insta::assert_snapshot!(attr_str);
    }

    // ── has_oopsie_meta ──────────────────────────────────────────────

    #[test]
    fn has_oopsie_meta_finds_name_value_code() {
        let attrs: Vec<syn::Attribute> = parse_quote! { #[oopsie(code = "my::error")] };
        assert!(has_oopsie_meta(&attrs, "code"));
    }

    #[test]
    fn has_oopsie_meta_finds_list_form_code() {
        let attrs: Vec<syn::Attribute> = parse_quote! { #[oopsie(code("app::{}", kind))] };
        assert!(has_oopsie_meta(&attrs, "code"));
    }

    #[test]
    fn has_oopsie_meta_missing_key() {
        let attrs: Vec<syn::Attribute> = parse_quote! { #[oopsie(code = "my::error")] };
        assert!(!has_oopsie_meta(&attrs, "help"));
    }

    #[test]
    fn has_oopsie_meta_ignores_display_format_arg() {
        // The `code = expr` here is a format argument of the short-display string,
        // not a `code` attribute, and must not suppress the auto-code.
        let attrs: Vec<syn::Attribute> = parse_quote! { #[oopsie("fmt {}", code = x)] };
        assert!(!has_oopsie_meta(&attrs, "code"));
    }

    // ── has_oopsie_flag ──────────────────────────────────────────────

    #[test]
    fn has_oopsie_flag_finds_bare_flag() {
        let attrs: Vec<syn::Attribute> = parse_quote! { #[oopsie(display("x"), transparent)] };
        assert!(has_oopsie_flag(&attrs, "transparent"));
    }

    #[test]
    fn has_oopsie_flag_ignores_name_value() {
        let attrs: Vec<syn::Attribute> = parse_quote! { #[oopsie(code = "my::error")] };
        assert!(!has_oopsie_flag(&attrs, "code"));
    }

    #[test]
    fn has_oopsie_flag_ignores_call_form() {
        let attrs: Vec<syn::Attribute> = parse_quote! { #[oopsie(display("x"))] };
        assert!(!has_oopsie_flag(&attrs, "display"));
    }

    #[test]
    fn inject_fields_adds_packed_traces_field() {
        let mut fields = parse_fields(quote! { struct S { msg: String } });
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: false,
            spantrace: false,
            timestamp: false,
            traces: true,
        };
        inject_fields(&mut fields, &config, &to_inject).unwrap();
        let rendered = quote! { #fields }.to_string();
        assert!(rendered.contains("__oopsie_traces"), "{rendered}");
    }

    #[test]
    fn check_existing_detects_packed_traces_by_type() {
        let fields =
            parse_fields(quote! { struct S { t: Box<(Backtrace, SpanTrace)>, msg: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = check_existing_fields(&fields, &ts);
        assert!(existence.has_traces);
        // A packed field stands in for both, suppressing separate injection.
        assert!(existence.has_backtrace && existence.has_spantrace);
    }
}
