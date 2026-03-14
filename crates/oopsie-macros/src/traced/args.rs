//! Argument types for the `#[traced]` macro.

use crate::utils::{BetterFlag, FieldSetting};

/// Raw parsed arguments — trace args are `Option` so we can detect explicit specification.
#[derive(Debug, darling::FromMeta)]
pub(super) struct TracedArgs {
    #[darling(default)]
    pub backtrace: Option<FieldSetting<true, TraceSettings>>,
    #[darling(default)]
    pub spantrace: Option<FieldSetting<true, TraceSettings>>,
    #[darling(default)]
    pub timestamp: Option<FieldSetting<true, TimestampSettings>>,
    pub code: FieldSetting<true, CodeSettings>,
    pub path: Option<syn::Path>,
}

impl TracedArgs {
    /// Resolve the explicit override model:
    /// - Bare `#[traced]` (no trace args specified) → backtrace + spantrace enabled, timestamp disabled
    /// - Any trace arg specified → you get exactly what's listed
    pub fn resolve(&self) -> ResolvedTraceArgs<'_> {
        let any_trace_specified =
            self.backtrace.is_some() || self.spantrace.is_some() || self.timestamp.is_some();

        if any_trace_specified {
            // Explicit mode: each trace is enabled only if explicitly listed and enabled
            ResolvedTraceArgs {
                backtrace: self.backtrace.as_ref().is_some_and(|s| s.is_enabled()),
                backtrace_settings: self.backtrace.as_ref(),
                spantrace: self.spantrace.as_ref().is_some_and(|s| s.is_enabled()),
                spantrace_settings: self.spantrace.as_ref(),
                timestamp: self.timestamp.as_ref().is_some_and(|s| s.is_enabled()),
                timestamp_settings: self.timestamp.as_ref(),
            }
        } else {
            // Default mode: backtrace + spantrace enabled, timestamp disabled
            ResolvedTraceArgs {
                backtrace: true,
                backtrace_settings: None,
                spantrace: true,
                spantrace_settings: None,
                timestamp: false,
                timestamp_settings: None,
            }
        }
    }
}

/// Resolved trace settings after applying the explicit override model.
pub(super) struct ResolvedTraceArgs<'a> {
    pub backtrace: bool,
    pub backtrace_settings: Option<&'a FieldSetting<true, TraceSettings>>,
    pub spantrace: bool,
    pub spantrace_settings: Option<&'a FieldSetting<true, TraceSettings>>,
    pub timestamp: bool,
    pub timestamp_settings: Option<&'a FieldSetting<true, TimestampSettings>>,
}

impl ResolvedTraceArgs<'_> {
    pub fn backtrace_type(&self) -> Option<&syn::Path> {
        self.backtrace_settings.and_then(|s| s.r#type())
    }

    pub fn spantrace_type(&self) -> Option<&syn::Path> {
        self.spantrace_settings.and_then(|s| s.r#type())
    }

    pub fn timestamp_chrono(&self) -> bool {
        self.timestamp_settings
            .and_then(|s| s.opt_settings())
            .is_some_and(|s| s.chrono.is_enabled())
    }

    pub fn timestamp_provide(&self) -> bool {
        self.timestamp_settings
            .and_then(|s| s.opt_settings())
            .is_some_and(|s| s.provide.is_enabled())
    }
}

#[derive(Debug, darling::FromMeta)]
pub(super) struct TraceSettings {
    pub r#type: Option<syn::Path>,
}

#[derive(Debug, darling::FromMeta)]
pub(super) struct TimestampSettings {
    pub chrono: BetterFlag<true>,
    pub provide: BetterFlag<true>,
}

#[derive(Debug, darling::FromMeta)]
pub(super) struct CodeSettings {
    pub r#type: Option<syn::Path>,
}

impl<const DEFAULT: bool> FieldSetting<DEFAULT, TraceSettings> {
    pub fn r#type(&self) -> Option<&syn::Path> {
        let s = self.opt_settings()?;
        s.r#type.as_ref()
    }
}
