//! Configuration types for field injection.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse_quote;

use super::args::{ResolvedTraceArgs, TracedArgs};

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
    pub timestamp_provide_attr: Option<TokenStream2>,

    pub traces_ident: syn::Ident,
    pub traces_type: TokenStream2,
    pub traces_attrs: TokenStream2,

    pub code_type: TokenStream2,
}

impl FieldInjectorConfig {
    pub fn new(
        args: &TracedArgs,
        resolved: &ResolvedTraceArgs<'_>,
        oopsie_path: &syn::Path,
    ) -> Self {
        let backtrace_ident = format_ident!("__oopsie_backtrace");
        let spantrace_ident = format_ident!("__oopsie_spantrace");
        let traces_ident = format_ident!("__oopsie_traces");
        let timestamp_ident = format_ident!("__oopsie_timestamp");

        // Element types: honor `type =` overrides, else the oopsie defaults.
        let backtrace_elem = resolved
            .backtrace_type()
            .map_or_else(|| quote! { #oopsie_path::Backtrace }, |p| quote! { #p });
        let spantrace_elem = resolved
            .spantrace_type()
            .map_or_else(|| quote! { #oopsie_path::SpanTrace }, |p| quote! { #p });

        let maybe_box = |inner: &TokenStream2, boxed: bool| -> TokenStream2 {
            if boxed {
                quote! { ::std::boxed::Box<#inner> }
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

        let timestamp_type: syn::Type = if resolved.timestamp_chrono() {
            // Leading `::` so the injected field does not depend on `chrono`
            // being nameable (unshadowed, unrenamed) in the caller's scope.
            parse_quote! { ::chrono::DateTime<::chrono::Local> }
        } else {
            parse_quote! { ::std::time::SystemTime }
        };
        let timestamp_provide_attr = resolved
            .timestamp_provide()
            .then(|| quote! { #[oopsie(provide)] });
        let backtrace_attrs = quote! { #[oopsie(backtrace)] };
        let spantrace_attrs = quote! { #[oopsie(spantrace)] };
        let traces_attrs = quote! { #[oopsie(traces)] };
        let code_type = args
            .code
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
            timestamp_provide_attr,
            traces_ident,
            traces_type,
            traces_attrs,
            code_type,
        }
    }
}

/// Tracks which fields already exist in a variant/struct.
#[derive(Default)]
pub(super) struct FieldExistence {
    pub has_backtrace: bool,
    pub has_spantrace: bool,
    pub has_timestamp: bool,
    pub has_traces: bool,
}

/// Tracks which fields should be injected.
pub(super) struct FieldsToInject {
    pub backtrace: bool,
    pub spantrace: bool,
    pub timestamp: bool,
    pub traces: bool,
}

#[cfg(test)]
mod tests {
    use quote::ToTokens as _;
    use syn::parse_quote;

    use super::super::args::TracedArgs;
    use super::*;
    use darling::FromMeta as _;

    fn config_for(meta: syn::Meta) -> FieldInjectorConfig {
        let args = TracedArgs::from_meta(&meta).unwrap();
        let resolved = args.resolve();
        let path: syn::Path = parse_quote!(::oopsie);
        FieldInjectorConfig::new(&args, &resolved, &path)
    }

    #[test]
    fn packed_default_builds_boxed_tuple_traces_type() {
        let cfg = config_for(parse_quote!(traced()));
        let s = cfg.traces_type.to_token_stream().to_string();
        assert!(s.contains("Box"), "{s}");
        assert!(s.contains("Backtrace") && s.contains("SpanTrace"), "{s}");
    }

    #[test]
    fn inline_packed_builds_unboxed_tuple() {
        let cfg = config_for(parse_quote!(traced(boxed = false)));
        let s = cfg.traces_type.to_token_stream().to_string();
        assert!(!s.contains("Box"), "{s}");
    }

    #[test]
    fn unpacked_inline_backtrace_type_has_no_box() {
        let cfg = config_for(parse_quote!(traced(packed = false, boxed = false)));
        let s = cfg.backtrace_type.to_token_stream().to_string();
        assert!(!s.contains("Box"), "{s}");
    }

    #[test]
    fn unpacked_boxed_backtrace_type_has_box() {
        let cfg = config_for(parse_quote!(traced(packed = false)));
        let s = cfg.backtrace_type.to_token_stream().to_string();
        assert!(s.contains("Box"), "{s}");
    }
}
