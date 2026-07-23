//! Configuration types for field injection.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse_quote;

use super::args::{CodeSettings, ResolvedTraceArgs};
use crate::utils::FieldSetting;

/// Configuration for injecting backtrace, spantrace, and timestamp fields.
pub(super) struct FieldInjectorConfig {
    pub backtrace_ident: syn::Ident,
    pub backtrace_type: TokenStream2,
    pub backtrace_attrs: TokenStream2,

    pub spantrace_ident: syn::Ident,
    pub spantrace_type: TokenStream2,
    pub spantrace_attrs: TokenStream2,

    pub timestamp_ident: syn::Ident,
    pub timestamp_type: syn::Type,
    pub timestamp_attrs: TokenStream2,

    pub traces_ident: syn::Ident,
    pub traces_type: TokenStream2,
    pub traces_attrs: TokenStream2,

    pub location_ident: syn::Ident,
    pub location_type: TokenStream2,
    pub location_attrs: TokenStream2,

    pub code_type: TokenStream2,
}

impl FieldInjectorConfig {
    pub fn new(
        resolved: &ResolvedTraceArgs<'_>,
        code: &FieldSetting<true, CodeSettings>,
        oopsie_path: &syn::Path,
    ) -> Self {
        let backtrace_ident = format_ident!("__oopsie_backtrace");
        let spantrace_ident = format_ident!("__oopsie_spantrace");
        let traces_ident = format_ident!("__oopsie_traces");
        let timestamp_ident = format_ident!("__oopsie_timestamp");
        let location_ident = format_ident!("__oopsie_location");

        // Element types: honor `type =` overrides, else the oopsie defaults.
        let backtrace_elem = resolved
            .backtrace_type
            .map_or_else(|| quote! { #oopsie_path::Backtrace }, |p| quote! { #p });
        let spantrace_elem = resolved
            .spantrace_type
            .map_or_else(|| quote! { #oopsie_path::SpanTrace }, |p| quote! { #p });

        let maybe_box = |inner: &TokenStream2, boxed: bool| -> TokenStream2 {
            if boxed {
                quote! { #oopsie_path::__private::alloc::boxed::Box<#inner> }
            } else {
                quote! { #inner }
            }
        };

        let backtrace_type = maybe_box(&backtrace_elem, resolved.backtrace_boxed);
        let spantrace_type = maybe_box(&spantrace_elem, resolved.spantrace_boxed);
        // Packed validation guarantees backtrace_boxed == spantrace_boxed when
        // packed; use backtrace_boxed as the tuple's boxing decision.
        let tuple = quote! { (#backtrace_elem, #spantrace_elem) };
        let traces_type = maybe_box(&tuple, resolved.backtrace_boxed);

        let timestamp_type: syn::Type = if resolved.timestamp_chrono {
            // Routed through the facade so the caller needs no direct chrono
            // dependency and the type unifies with oopsie-core's Capturable impl
            // regardless of any chrono version in the caller's tree.
            parse_quote! {
                #oopsie_path::__private::chrono::DateTime<#oopsie_path::__private::chrono::Local>
            }
        } else {
            parse_quote! { #oopsie_path::__private::SystemTime }
        };
        // Surface the injected timestamp through the `Provider` API by value
        // (both `SystemTime` and `chrono::DateTime` are `Copy`). The field is
        // bound by reference in the generated `provide` arm, hence the deref.
        let timestamp_provide_attr = resolved.timestamp_provide.then(|| {
            quote! { #[oopsie(provide(#timestamp_type => *#timestamp_ident))] }
        });
        // `capture` keeps the injected field off the context selectors: the
        // derive auto-fills it via `Capturable` at build time.
        let timestamp_attrs = quote! { #timestamp_provide_attr #[oopsie(capture)] };
        let backtrace_attrs = quote! { #[oopsie(backtrace)] };
        let spantrace_attrs = quote! { #[oopsie(spantrace)] };
        let traces_attrs = quote! { #[oopsie(traces)] };
        // Inline and unboxed: a `&'static Location` is `Copy` and pointer-sized,
        // so it stays reachable without touching any boxed trace slot.
        let location_type = quote! { &'static ::core::panic::Location<'static> };
        let location_attrs = quote! { #[oopsie(location)] };
        let code_type = code
            .opt_settings()
            .and_then(|s| s.r#type.clone())
            .map_or_else(|| quote! { #oopsie_path::ErrorCode }, |p| quote! { #p });

        Self {
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
            location_ident,
            location_type,
            location_attrs,
            code_type,
        }
    }
}

/// Tracks which fields already exist in a variant/struct.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one presence flag per injectable trace/diagnostic field"
)]
#[derive(Default)]
pub(super) struct FieldExistence {
    pub has_backtrace: bool,
    pub has_spantrace: bool,
    pub has_timestamp: bool,
    pub has_traces: bool,
    pub has_location: bool,
    /// Span of a pre-existing, user-authored SystemTime/DateTime-typed field
    /// that set `has_timestamp`; `None` when the re-expansion of an
    /// already-injected field set it instead. Lets callers diagnose a
    /// requested-but-suppressed `timestamp` without misfiring on re-expansion.
    pub timestamp_conflict: Option<proc_macro2::Span>,
}

/// Tracks which fields should be injected.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one injection flag per injectable trace/diagnostic field"
)]
pub(super) struct FieldsToInject {
    pub backtrace: bool,
    pub spantrace: bool,
    pub timestamp: bool,
    pub traces: bool,
    pub location: bool,
}

impl FieldsToInject {
    pub(super) const fn any(&self) -> bool {
        self.backtrace || self.spantrace || self.timestamp || self.traces || self.location
    }
}

#[cfg(test)]
mod tests {
    use quote::ToTokens as _;
    use syn::parse_quote;

    use super::super::args::TracedArgs;
    use super::*;
    use darling::FromMeta as _;

    fn config_for(meta: &syn::Meta) -> FieldInjectorConfig {
        let args = TracedArgs::from_meta(meta).unwrap();
        let resolved = args.resolve(&crate::utils::TracedDefaults::default());
        let code = FieldSetting::<true, CodeSettings>::Flag(true);
        let path: syn::Path = parse_quote!(::oopsie);
        FieldInjectorConfig::new(&resolved, &code, &path)
    }

    #[test]
    fn packed_default_builds_boxed_tuple_traces_type() {
        let cfg = config_for(&parse_quote!(traced()));
        let s = cfg.traces_type.to_token_stream().to_string();
        assert!(s.contains("Box"), "{s}");
        assert!(s.contains("Backtrace") && s.contains("SpanTrace"), "{s}");
    }

    #[test]
    fn inline_packed_builds_unboxed_tuple() {
        let cfg = config_for(&parse_quote!(traced(boxed = false)));
        let s = cfg.traces_type.to_token_stream().to_string();
        assert!(!s.contains("Box"), "{s}");
    }

    #[test]
    fn unpacked_inline_backtrace_type_has_no_box() {
        let cfg = config_for(&parse_quote!(traced(packed = false, boxed = false)));
        let s = cfg.backtrace_type.to_token_stream().to_string();
        assert!(!s.contains("Box"), "{s}");
    }

    #[test]
    fn unpacked_boxed_backtrace_type_has_box() {
        let cfg = config_for(&parse_quote!(traced(packed = false)));
        let s = cfg.backtrace_type.to_token_stream().to_string();
        assert!(s.contains("Box"), "{s}");
    }

    #[test]
    fn custom_type_composes_into_packed_tuple() {
        // Both traces listed (explicit mode keeps both); custom backtrace type
        // must land as the first tuple element of the packed box.
        let cfg = config_for(&parse_quote!(traced(backtrace(r#type = MyBt), spantrace)));
        let s = cfg.traces_type.to_token_stream().to_string();
        assert!(s.contains("MyBt"), "{s}");
        assert!(s.contains("SpanTrace"), "{s}");
    }

    // ── timestamp field type & attrs codegen ──

    #[test]
    fn timestamp_default_type_is_system_time() {
        // Bare `timestamp` (no settings block) → SystemTime, no provide.
        let cfg = config_for(&parse_quote!(traced(timestamp)));
        let s = cfg.timestamp_type.to_token_stream().to_string();
        assert!(s.contains("SystemTime"), "{s}");
        assert!(!s.contains("DateTime"), "{s}");
        let attrs = cfg.timestamp_attrs.to_string();
        assert!(attrs.contains("capture"), "{attrs}");
        assert!(!attrs.contains("provide"), "provide is opt-in: {attrs}");
    }

    #[test]
    fn timestamp_chrono_type_is_datetime_local() {
        // `chrono` is opt-in and independent of `provide`.
        let cfg = config_for(&parse_quote!(traced(timestamp(chrono = true))));
        let s = cfg.timestamp_type.to_token_stream().to_string();
        assert!(s.contains("__private"), "{s}");
        assert!(s.contains("chrono"), "{s}");
        assert!(s.contains("DateTime"), "{s}");
        assert!(s.contains("Local"), "{s}");
        let attrs = cfg.timestamp_attrs.to_string();
        assert!(attrs.contains("capture"), "{attrs}");
        assert!(!attrs.contains("provide"), "provide is opt-in: {attrs}");
    }

    #[test]
    fn timestamp_attrs_always_mark_capture() {
        // The injected field must auto-fill at build time and never surface on
        // the context selector — `capture` is unconditional.
        for meta in [
            parse_quote!(traced(timestamp)),
            parse_quote!(traced(timestamp(chrono = true))),
            parse_quote!(traced(timestamp(provide = true))),
        ] {
            let cfg = config_for(&meta);
            let attrs = cfg.timestamp_attrs.to_string();
            assert!(attrs.contains("capture"), "{attrs}");
        }
    }

    #[test]
    fn timestamp_provide_emits_valid_provide_attr() {
        // `provide = true` emits a real `provide(Type => expr)` attr — NOT a bare
        // `#[oopsie(provide)]`, which the provide parser rejects.
        let cfg = config_for(&parse_quote!(traced(timestamp(provide = true))));
        let s = cfg.timestamp_attrs.to_string();
        assert!(s.contains("oopsie") && s.contains("provide"), "{s}");
        assert!(
            s.contains("=>"),
            "must be the valid `Type => expr` form: {s}"
        );
        assert!(
            s.contains("SystemTime"),
            "provides the timestamp's type: {s}"
        );
        assert!(
            s.contains("__oopsie_timestamp"),
            "references the field: {s}"
        );
        assert!(s.contains("capture"), "{s}");
    }
}
