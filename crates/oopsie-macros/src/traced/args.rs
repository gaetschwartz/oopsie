//! Argument types for trace injection options.

use crate::utils::{BetterFlag, FieldSetting};

/// Inner arguments of `traced(...)`. Every part starts from its default
/// (backtrace + spantrace on, timestamp off) and is individually tunable:
/// mentioning one part never disables the others.
#[derive(Clone, Debug, darling::FromMeta)]
pub struct TracedArgs {
    pub backtrace: FieldSetting<true, TraceSettings>,
    pub spantrace: FieldSetting<true, TraceSettings>,
    pub timestamp: FieldSetting<false, TimestampSettings>,
    pub packed: BetterFlag<true>,
    pub boxed: BetterFlag<true>,
    pub code: FieldSetting<true, CodeSettings>,
}

impl Default for TracedArgs {
    fn default() -> Self {
        Self {
            backtrace: FieldSetting::Flag(true),
            spantrace: FieldSetting::Flag(true),
            timestamp: FieldSetting::Flag(false),
            packed: BetterFlag::Default,
            boxed: BetterFlag::Default,
            code: FieldSetting::Flag(true),
        }
    }
}

impl TracedArgs {
    /// Per-trace boxing: the trace's own `boxed` override if a settings block
    /// exists, else the pair-level `boxed`.
    fn trace_boxed(&self, trace: &FieldSetting<true, TraceSettings>) -> bool {
        trace
            .opt_settings()
            .map_or_else(|| self.boxed.is_enabled(), |s| s.boxed.is_enabled())
    }

    pub fn resolve(&self) -> ResolvedTraceArgs<'_> {
        ResolvedTraceArgs {
            backtrace: self.backtrace.is_enabled(),
            spantrace: self.spantrace.is_enabled(),
            timestamp: self.timestamp.is_enabled(),
            packed: self.packed.is_enabled(),
            backtrace_boxed: self.trace_boxed(&self.backtrace),
            spantrace_boxed: self.trace_boxed(&self.spantrace),
            backtrace_type: self.backtrace.r#type(),
            spantrace_type: self.spantrace.r#type(),
            timestamp_chrono: self
                .timestamp
                .opt_settings()
                .is_some_and(|s| s.chrono.is_enabled()),
            timestamp_provide: self
                .timestamp
                .opt_settings()
                .is_some_and(|s| s.provide.is_enabled()),
        }
    }
}

/// Flat view of [`TracedArgs`] after folding flags, per-trace overrides, and
/// type overrides.
#[expect(
    clippy::struct_excessive_bools,
    reason = "resolved enable/box flags, one per trace dimension"
)]
pub struct ResolvedTraceArgs<'a> {
    pub backtrace: bool,
    pub spantrace: bool,
    pub timestamp: bool,
    pub packed: bool,
    pub backtrace_boxed: bool,
    pub spantrace_boxed: bool,
    pub backtrace_type: Option<&'a syn::Path>,
    pub spantrace_type: Option<&'a syn::Path>,
    pub timestamp_chrono: bool,
    pub timestamp_provide: bool,
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
}

#[derive(Clone, Debug, darling::FromMeta)]
pub struct TraceSettings {
    pub r#type: Option<syn::Path>,
    #[darling(default)]
    pub boxed: BetterFlag<true>,
}

#[derive(Clone, Debug, darling::FromMeta)]
pub struct TimestampSettings {
    // Opt-in: an omitted flag inside a `timestamp(...)` block stays disabled, so
    // `timestamp(chrono = true)` does not silently also enable `provide`.
    pub chrono: BetterFlag<false>,
    pub provide: BetterFlag<false>,
}

#[derive(Clone, Debug, darling::FromMeta)]
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
        TracedArgs::from_meta(meta).expect("parse traced args")
    }

    fn args_list(meta: syn::MetaList) -> TracedArgs {
        use darling::ast::NestedMeta;
        let nested = NestedMeta::parse_meta_list(meta.tokens).expect("parse meta list");
        TracedArgs::from_list(&nested).expect("parse traced args list")
    }

    #[test]
    fn default_is_packed_and_boxed_with_both_traces() {
        let a = args_list(parse_quote!(traced()));
        let r = a.resolve();
        assert!(r.packed);
        assert!(r.backtrace_boxed);
        assert!(r.spantrace_boxed);
        assert!(r.backtrace && r.spantrace);
        assert!(!r.timestamp);
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
    fn backtrace_false_keeps_spantrace_and_enables_timestamp() {
        let a = args(&parse_quote!(traced(backtrace(false), timestamp)));
        let r = a.resolve();
        assert!(!r.backtrace);
        assert!(r.spantrace);
        assert!(r.timestamp);
    }

    #[test]
    fn spantrace_false_keeps_backtrace_only() {
        let a = args(&parse_quote!(traced(spantrace(false))));
        let r = a.resolve();
        assert!(r.backtrace);
        assert!(!r.spantrace);
        assert!(!r.timestamp);
    }

    #[test]
    fn mentioning_one_trace_does_not_disable_others() {
        // `spantrace(boxed = false)` tunes spantrace; backtrace stays enabled.
        let a = args(&parse_quote!(traced(spantrace(boxed = false))));
        let r = a.resolve();
        assert!(r.backtrace && r.spantrace);
        assert!(r.backtrace_boxed);
        assert!(!r.spantrace_boxed);
    }

    #[test]
    fn per_trace_boxed_override_when_unpacked() {
        let a = args(&parse_quote!(traced(
            packed = false,
            spantrace(boxed = false)
        )));
        let r = a.resolve();
        assert!(!r.packed);
        assert!(r.backtrace && r.spantrace);
        assert!(r.backtrace_boxed);
        assert!(!r.spantrace_boxed);
    }

    #[test]
    fn validate_rejects_packed_incoherent_boxing() {
        // Both traces enabled, packed default, boxing disagrees.
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

    // ── timestamp parsing & flag resolution ──────────

    #[test]
    fn bare_timestamp_keeps_both_traces() {
        let a = args(&parse_quote!(traced(timestamp)));
        let r = a.resolve();
        assert!(r.timestamp);
        assert!(r.backtrace && r.spantrace);
    }

    #[test]
    fn timestamp_chrono_flag_parses() {
        let a = args(&parse_quote!(traced(timestamp(
            chrono = true,
            provide = false
        ))));
        let r = a.resolve();
        assert!(r.timestamp);
        assert!(r.timestamp_chrono);
    }

    #[test]
    fn bare_timestamp_chrono_resolves_false() {
        // The bare `timestamp` path (no settings list) has no `opt_settings`, so
        // both sub-flags resolve to false: default field type is `SystemTime`.
        let a = args(&parse_quote!(traced(timestamp)));
        let r = a.resolve();
        assert!(r.timestamp);
        assert!(!r.timestamp_chrono);
        assert!(!r.timestamp_provide);
    }

    #[test]
    fn timestamp_settings_block_enables_timestamp() {
        // A `timestamp(...)` settings block without `enabled` counts as on,
        // despite the field's off-by-default.
        let a = args(&parse_quote!(traced(timestamp(provide = true))));
        let r = a.resolve();
        assert!(r.timestamp);
    }

    #[test]
    fn timestamp_chrono_defaults_to_disabled_inside_settings_block() {
        // Opt-in: `chrono` omitted inside a settings block stays disabled, so
        // `timestamp(provide = true)` alone keeps the `SystemTime` field type.
        let a = args(&parse_quote!(traced(timestamp(provide = true))));
        let r = a.resolve();
        assert!(!r.timestamp_chrono);
        assert!(r.timestamp_provide);
    }

    #[test]
    fn timestamp_chrono_false_inside_settings_block() {
        let a = args(&parse_quote!(traced(timestamp(chrono = false))));
        let r = a.resolve();
        assert!(!r.timestamp_chrono);
    }

    #[test]
    fn timestamp_provide_flag_defaults_to_disabled() {
        // Opt-in: `provide` omitted stays disabled, so `timestamp(chrono = true)`
        // does not emit a provide attr.
        let a = args(&parse_quote!(traced(timestamp(chrono = true))));
        let r = a.resolve();
        assert!(!r.timestamp_provide, "provide is opt-in when omitted");
        assert!(r.timestamp_chrono);
    }

    #[test]
    fn timestamp_provide_true_resolves_enabled() {
        let a = args(&parse_quote!(traced(timestamp(provide = true))));
        let r = a.resolve();
        assert!(r.timestamp_provide);
    }
}
