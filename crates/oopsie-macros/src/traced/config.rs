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

    pub code_type: TokenStream2,
    pub magnetite_utils_path: syn::Path,
}

impl FieldInjectorConfig {
    pub fn new(
        args: &TracedArgs,
        resolved: &ResolvedTraceArgs<'_>,
        magnetite_utils_path: syn::Path,
    ) -> Self {
        let backtrace_ident = format_ident!("__oopsie_backtrace");
        let spantrace_ident = format_ident!("__oopsie_spantrace");
        let timestamp_ident = format_ident!("__oopsie_timestamp");

        let backtrace_type = resolved.backtrace_type().map_or_else(
            || quote! { ::std::boxed::Box<#magnetite_utils_path::BackTrace> },
            |p| quote! { #p },
        );
        let spantrace_type = resolved.spantrace_type().map_or_else(
            || quote! { ::std::boxed::Box<#magnetite_utils_path::SpanTrace> },
            |p| quote! { #p },
        );
        let timestamp_type: syn::Type = if resolved.timestamp_chrono() {
            parse_quote! { chrono::DateTime<chrono::Local> }
        } else {
            parse_quote! { std::time::SystemTime }
        };
        let timestamp_provide_attr = resolved
            .timestamp_provide()
            .then(|| quote! { #[oopsie(provide)] });
        let backtrace_attrs = quote! { #[oopsie(backtrace)] };
        let spantrace_attrs = quote! { #[oopsie(spantrace)] };
        let code_type = args
            .code
            .opt_settings()
            .and_then(|s| s.r#type.clone())
            .map_or_else(
                || quote! { #magnetite_utils_path::ErrorCode },
                |p| quote! { #p },
            );

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
            code_type,
            magnetite_utils_path,
        }
    }
}

/// Tracks which fields already exist in a variant/struct.
#[derive(Default)]
pub(super) struct FieldExistence {
    pub has_backtrace: bool,
    pub has_spantrace: bool,
    pub has_timestamp: bool,
}

/// Tracks which fields should be injected.
pub(super) struct FieldsToInject {
    pub backtrace: bool,
    pub spantrace: bool,
    pub timestamp: bool,
}
