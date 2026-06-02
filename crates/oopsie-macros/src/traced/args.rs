//! Argument types for trace injection options.

use crate::utils::{BetterFlag, FieldSetting};

/// Raw parsed arguments — trace args are `Option` so we can detect explicit specification.
#[derive(Debug, darling::FromMeta)]
pub struct TracedArgs {
    #[darling(default)]
    pub backtrace: Option<FieldSetting<true, TraceSettings>>,
    #[darling(default)]
    pub spantrace: Option<FieldSetting<true, TraceSettings>>,
    #[darling(default)]
    pub timestamp: Option<FieldSetting<true, TimestampSettings>>,
    #[darling(default)]
    pub packed: BetterFlag<true>,
    #[darling(default)]
    pub boxed: BetterFlag<true>,
    pub code: FieldSetting<true, CodeSettings>,
    pub path: Option<syn::Path>,
}

impl TracedArgs {
    /// Per-trace boxing: the trace's own `boxed` override if present, else the
    /// pair-level `boxed`.
    fn trace_boxed(&self, settings: Option<&FieldSetting<true, TraceSettings>>) -> bool {
        settings
            .and_then(super::super::utils::FieldSetting::opt_settings)
            .map_or_else(|| self.boxed.is_enabled(), |s| s.boxed.is_enabled())
    }

    /// Resolve the explicit override model:
    /// - Bare `#[oopsie(traced)]` or no explicit trace args specified
    ///   → backtrace + spantrace enabled, timestamp disabled
    /// - Any trace arg specified → you get exactly what's listed
    pub fn resolve(&self) -> ResolvedTraceArgs<'_> {
        let any_trace_specified =
            self.backtrace.is_some() || self.spantrace.is_some() || self.timestamp.is_some();

        if any_trace_specified {
            // Explicit mode: each trace is enabled only if explicitly listed and enabled
            ResolvedTraceArgs {
                backtrace: self
                    .backtrace
                    .as_ref()
                    .is_some_and(super::super::utils::FieldSetting::is_enabled),
                backtrace_settings: self.backtrace.as_ref(),
                spantrace: self
                    .spantrace
                    .as_ref()
                    .is_some_and(super::super::utils::FieldSetting::is_enabled),
                spantrace_settings: self.spantrace.as_ref(),
                timestamp: self
                    .timestamp
                    .as_ref()
                    .is_some_and(super::super::utils::FieldSetting::is_enabled),
                timestamp_settings: self.timestamp.as_ref(),
                packed: self.packed.is_enabled(),
                backtrace_boxed: self.trace_boxed(self.backtrace.as_ref()),
                spantrace_boxed: self.trace_boxed(self.spantrace.as_ref()),
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
                packed: self.packed.is_enabled(),
                backtrace_boxed: self.boxed.is_enabled(),
                spantrace_boxed: self.boxed.is_enabled(),
            }
        }
    }
}

/// Resolved trace settings after applying the explicit override model.
#[expect(clippy::struct_excessive_bools)]
pub struct ResolvedTraceArgs<'a> {
    pub backtrace: bool,
    pub backtrace_settings: Option<&'a FieldSetting<true, TraceSettings>>,
    pub spantrace: bool,
    pub spantrace_settings: Option<&'a FieldSetting<true, TraceSettings>>,
    pub timestamp: bool,
    pub timestamp_settings: Option<&'a FieldSetting<true, TimestampSettings>>,
    pub packed: bool,
    pub backtrace_boxed: bool,
    pub spantrace_boxed: bool,
}

impl ResolvedTraceArgs<'_> {
    /// Reject `packed` with backtrace/spantrace boxed inconsistently: a packed
    /// tuple is one field, so its two elements cannot be boxed independently.
    /// Only meaningful when both traces are enabled (single-trace packing is a
    /// no-op handled by the injector).
    pub fn validate(&self, span: proc_macro2::Span) -> syn::Result<()> {
        if self.packed
            && self.backtrace
            && self.spantrace
            && self.backtrace_boxed != self.spantrace_boxed
        {
            return Err(syn::Error::new(
                span,
                "`packed` requires backtrace and spantrace to share one boxing \
                 mode; set both `boxed` the same, or use `packed = false` for \
                 per-trace boxing",
            ));
        }
        Ok(())
    }

    #[inline]
    pub fn backtrace_type(&self) -> Option<&syn::Path> {
        let s = self.backtrace_settings?;
        s.r#type()
    }

    #[inline]
    pub fn spantrace_type(&self) -> Option<&syn::Path> {
        let s = self.spantrace_settings?;
        s.r#type()
    }

    #[inline]
    pub fn timestamp_chrono(&self) -> bool {
        self.timestamp_settings
            .and_then(|s| s.opt_settings())
            .is_some_and(|s| s.chrono.is_enabled())
    }

    #[inline]
    pub fn timestamp_provide(&self) -> bool {
        self.timestamp_settings
            .and_then(|s| s.opt_settings())
            .is_some_and(|s| s.provide.is_enabled())
    }
}

#[derive(Debug, darling::FromMeta)]
pub struct TraceSettings {
    pub r#type: Option<syn::Path>,
    #[darling(default)]
    pub boxed: BetterFlag<true>,
}

#[derive(Debug, darling::FromMeta)]
pub struct TimestampSettings {
    pub chrono: BetterFlag<true>,
    pub provide: BetterFlag<true>,
}

#[derive(Debug, darling::FromMeta)]
pub struct CodeSettings {
    pub r#type: Option<syn::Path>,
}

impl<const DEFAULT: bool> FieldSetting<DEFAULT, TraceSettings> {
    #[inline]
    pub fn r#type(&self) -> Option<&syn::Path> {
        let s = self.opt_settings()?;
        s.r#type.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use darling::FromMeta as _;
    use syn::parse_quote;

    use super::*;

    fn args(meta: &syn::Meta) -> TracedArgs {
        // `#[oopsie(traced(...))]` arrives as the inner meta list; build the
        // `traced(...)` meta darling expects.
        TracedArgs::from_meta(meta).expect("parse traced args")
    }

    fn args_list(meta: syn::MetaList) -> TracedArgs {
        use darling::ast::NestedMeta;
        let nested = NestedMeta::parse_meta_list(meta.tokens).expect("parse meta list");
        TracedArgs::from_list(&nested).expect("parse traced args list")
    }

    #[test]
    fn default_is_packed_and_boxed() {
        let a = args_list(parse_quote!(traced()));
        let r = a.resolve();
        assert!(r.packed);
        assert!(r.backtrace_boxed);
        assert!(r.spantrace_boxed);
        assert!(r.backtrace && r.spantrace);
    }

    #[test]
    fn packed_false_unpacks() {
        let a = args(&parse_quote!(traced(packed = false)));
        let r = a.resolve();
        assert!(!r.packed);
        assert!(r.backtrace_boxed && r.spantrace_boxed);
    }

    #[test]
    fn boxed_false_is_inline() {
        let a = args(&parse_quote!(traced(boxed = false)));
        let r = a.resolve();
        assert!(r.packed);
        assert!(!r.backtrace_boxed && !r.spantrace_boxed);
    }

    #[test]
    fn per_trace_boxed_override_when_unpacked() {
        // Both traces listed (explicit mode keeps both enabled); spantrace inline.
        let a = args(&parse_quote!(traced(
            packed = false,
            backtrace,
            spantrace(boxed = false)
        )));
        let r = a.resolve();
        assert!(!r.packed);
        assert!(r.backtrace && r.spantrace);
        assert!(r.backtrace_boxed);
        assert!(!r.spantrace_boxed);
    }

    #[test]
    fn naming_one_trace_block_disables_the_other() {
        // Explicit-override model: mentioning only spantrace turns backtrace OFF.
        // This is why the mixed case must list both traces.
        let a = args(&parse_quote!(traced(spantrace(boxed = false))));
        let r = a.resolve();
        assert!(r.spantrace);
        assert!(!r.backtrace);
    }

    #[test]
    fn validate_rejects_packed_incoherent_boxing() {
        // Both traces enabled (explicit), packed default, boxing disagrees.
        let a = args(&parse_quote!(traced(backtrace, spantrace(boxed = false))));
        let r = a.resolve();
        assert!(r.backtrace && r.spantrace);
        assert!(r.packed);
        assert!(r.backtrace_boxed && !r.spantrace_boxed);
        r.validate(proc_macro2::Span::call_site()).unwrap_err();
    }

    #[test]
    fn validate_accepts_packed_uniform_inline() {
        let a = args(&parse_quote!(traced(boxed = false)));
        let r = a.resolve();
        r.validate(proc_macro2::Span::call_site()).unwrap();
    }
}
