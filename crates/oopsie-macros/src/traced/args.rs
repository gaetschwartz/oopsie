//! Argument types for trace injection options.

#![allow(clippy::ref_patterns, reason = "darling's FromMeta derive emits them")]

use darling::FromMeta;

use crate::utils::{BetterFlag, FieldSetting, TracedDefaults};

/// A `FieldSetting` that also records whether it was written by the user.
///
/// `FieldSetting` collapses an absent key and an explicit `key = <default>` to
/// the same `Flag(DEFAULT)`, so it alone can't tell "defaulted" from "the user
/// chose the default". This wrapper keeps that distinction so a manifest default
/// only applies when the per-attribute toggle is truly absent.
#[derive(Clone, Debug)]
pub struct Tristate<const DEFAULT: bool, T: FromMeta> {
    inner: FieldSetting<DEFAULT, T>,
    explicit: bool,
}

impl<const DEFAULT: bool, T: FromMeta> Tristate<DEFAULT, T> {
    const fn flag(value: bool, explicit: bool) -> Self {
        Self {
            inner: FieldSetting::Flag(value),
            explicit,
        }
    }

    pub const fn inner(&self) -> &FieldSetting<DEFAULT, T> {
        &self.inner
    }

    /// `Some(state)` when the user wrote the toggle; `None` when it defaulted.
    /// A settings block (`key(...)`) counts as explicit, with the state taken
    /// from its `enabled` key (default on).
    pub fn explicit(&self) -> Option<bool> {
        self.explicit.then(|| self.inner.is_enabled())
    }
}

impl<const DEFAULT: bool, T: FromMeta> FromMeta for Tristate<DEFAULT, T> {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        FieldSetting::from_meta(item).map(|inner| Self {
            inner,
            explicit: true,
        })
    }

    fn from_none() -> Option<Self> {
        Some(Self::flag(DEFAULT, false))
    }
}

/// Inner arguments of `traced(...)`. Every part starts from its default
/// (backtrace + spantrace on, timestamp off) and is individually tunable:
/// mentioning one part never disables the others.
#[derive(Clone, Debug, darling::FromMeta)]
pub struct TracedArgs {
    pub backtrace: FieldSetting<true, TraceSettings>,
    pub spantrace: FieldSetting<true, TraceSettings>,
    pub timestamp: Tristate<false, TimestampSettings>,
    pub location: BetterFlag<true>,
    pub packed: BetterFlag<true>,
    pub boxed: BetterFlag<true>,
    pub code: Tristate<true, CodeSettings>,
}

impl Default for TracedArgs {
    fn default() -> Self {
        Self {
            backtrace: FieldSetting::Flag(true),
            spantrace: FieldSetting::Flag(true),
            timestamp: Tristate::flag(false, false),
            location: BetterFlag::Default,
            packed: BetterFlag::Default,
            boxed: BetterFlag::Default,
            code: Tristate::flag(true, false),
        }
    }
}

impl TracedArgs {
    /// Fold the per-attribute toggles against the manifest defaults.
    ///
    /// Precedence per toggle: an explicit per-attribute setting wins; otherwise
    /// the manifest default applies; otherwise the hardcoded const-generic
    /// default. `defaults` is empty when nothing is configured, so the manifest
    /// arm only fires for keys a project actually set.
    pub fn resolve(&self, defaults: &TracedDefaults) -> ResolvedTraceArgs<'_> {
        // `boxed` is a pair-level toggle: a per-trace `boxed` override (handled
        // in `trace_boxed`) still wins over both the manifest and this fold.
        let manifest_boxed = self.boxed.to_option().or(defaults.boxed);
        let trace_boxed = |trace: &FieldSetting<true, TraceSettings>| {
            trace
                .opt_settings()
                .and_then(|s| s.boxed.to_option())
                .or(manifest_boxed)
                .unwrap_or(true)
        };
        ResolvedTraceArgs {
            backtrace: self.backtrace.is_enabled(),
            spantrace: self.spantrace.is_enabled(),
            timestamp: fold(self.timestamp.explicit(), defaults.timestamp, false),
            location: fold(self.location.to_option(), defaults.location, true),
            packed: fold(self.packed.to_option(), defaults.packed, true),
            code: fold(self.code.explicit(), defaults.code, true),
            backtrace_boxed: trace_boxed(&self.backtrace),
            spantrace_boxed: trace_boxed(&self.spantrace),
            backtrace_type: self.backtrace.r#type(),
            spantrace_type: self.spantrace.r#type(),
            timestamp_chrono: self
                .timestamp
                .inner()
                .opt_settings()
                .is_some_and(|s| s.chrono.is_enabled()),
            timestamp_provide: self
                .timestamp
                .inner()
                .opt_settings()
                .is_some_and(|s| s.provide.is_enabled()),
        }
    }
}

/// Per-toggle precedence: explicit per-attribute setting, then manifest default,
/// then the hardcoded default.
fn fold(per_attr: Option<bool>, manifest: Option<bool>, hardcoded: bool) -> bool {
    per_attr.or(manifest).unwrap_or(hardcoded)
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
    pub location: bool,
    pub packed: bool,
    pub code: bool,
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

    /// Resolve with no manifest defaults — the hardcoded defaults apply.
    fn resolve(a: &TracedArgs) -> ResolvedTraceArgs<'_> {
        a.resolve(&TracedDefaults::default())
    }

    #[test]
    fn default_is_packed_and_boxed_with_both_traces() {
        let a = args_list(parse_quote!(traced()));
        let r = resolve(&a);
        assert!(r.packed);
        assert!(r.backtrace_boxed);
        assert!(r.spantrace_boxed);
        assert!(r.backtrace && r.spantrace);
        assert!(!r.timestamp);
    }

    #[test]
    fn packed_false_unpacks() {
        let a = args(&parse_quote!(traced(packed = false)));
        let r = resolve(&a);
        assert!(!r.packed);
        assert!(r.backtrace_boxed && r.spantrace_boxed);
    }

    #[test]
    fn boxed_false_is_inline() {
        let a = args(&parse_quote!(traced(boxed = false)));
        let r = resolve(&a);
        assert!(r.packed);
        assert!(!r.backtrace_boxed && !r.spantrace_boxed);
    }

    #[test]
    fn backtrace_false_keeps_spantrace_and_enables_timestamp() {
        let a = args(&parse_quote!(traced(backtrace(false), timestamp)));
        let r = resolve(&a);
        assert!(!r.backtrace);
        assert!(r.spantrace);
        assert!(r.timestamp);
    }

    #[test]
    fn spantrace_false_keeps_backtrace_only() {
        let a = args(&parse_quote!(traced(spantrace(false))));
        let r = resolve(&a);
        assert!(r.backtrace);
        assert!(!r.spantrace);
        assert!(!r.timestamp);
    }

    #[test]
    fn mentioning_one_trace_does_not_disable_others() {
        // `spantrace(boxed = false)` tunes spantrace; backtrace stays enabled.
        let a = args(&parse_quote!(traced(spantrace(boxed = false))));
        let r = resolve(&a);
        assert!(r.backtrace);
        assert!(r.spantrace);
        assert!(r.backtrace_boxed);
        assert!(!r.spantrace_boxed);
    }

    #[test]
    fn per_trace_boxed_override_when_unpacked() {
        let a = args(&parse_quote!(traced(
            packed = false,
            spantrace(boxed = false)
        )));
        let r = resolve(&a);
        assert!(!r.packed);
        assert!(r.backtrace);
        assert!(r.spantrace);
        assert!(r.backtrace_boxed);
        assert!(!r.spantrace_boxed);
    }

    #[test]
    fn validate_rejects_packed_incoherent_boxing() {
        // Both traces enabled, packed default, boxing disagrees.
        let a = args(&parse_quote!(traced(backtrace, spantrace(boxed = false))));
        let r = resolve(&a);
        assert!(r.backtrace && r.spantrace);
        assert!(r.packed);
        assert!(r.backtrace_boxed && !r.spantrace_boxed);
        r.validate(proc_macro2::Span::call_site()).unwrap_err();
    }

    #[test]
    fn validate_accepts_packed_uniform_inline() {
        let a = args(&parse_quote!(traced(boxed = false)));
        let r = resolve(&a);
        r.validate(proc_macro2::Span::call_site()).unwrap();
    }

    // ── timestamp parsing & flag resolution ──────────

    #[test]
    fn bare_timestamp_keeps_both_traces() {
        let a = args(&parse_quote!(traced(timestamp)));
        let r = resolve(&a);
        assert!(r.timestamp);
        assert!(r.backtrace);
        assert!(r.spantrace);
    }

    #[test]
    fn timestamp_chrono_flag_parses() {
        let a = args(&parse_quote!(traced(timestamp(
            chrono = true,
            provide = false
        ))));
        let r = resolve(&a);
        assert!(r.timestamp);
        assert!(r.timestamp_chrono);
    }

    #[test]
    fn bare_timestamp_chrono_resolves_false() {
        // The bare `timestamp` path (no settings list) has no `opt_settings`, so
        // both sub-flags resolve to false: default field type is `SystemTime`.
        let a = args(&parse_quote!(traced(timestamp)));
        let r = resolve(&a);
        assert!(r.timestamp);
        assert!(!r.timestamp_chrono);
        assert!(!r.timestamp_provide);
    }

    #[test]
    fn timestamp_settings_block_enables_timestamp() {
        // A `timestamp(...)` settings block without `enabled` counts as on,
        // despite the field's off-by-default.
        let a = args(&parse_quote!(traced(timestamp(provide = true))));
        let r = resolve(&a);
        assert!(r.timestamp);
    }

    #[test]
    fn timestamp_chrono_defaults_to_disabled_inside_settings_block() {
        // Opt-in: `chrono` omitted inside a settings block stays disabled, so
        // `timestamp(provide = true)` alone keeps the `SystemTime` field type.
        let a = args(&parse_quote!(traced(timestamp(provide = true))));
        let r = resolve(&a);
        assert!(!r.timestamp_chrono);
        assert!(r.timestamp_provide);
    }

    #[test]
    fn timestamp_chrono_false_inside_settings_block() {
        let a = args(&parse_quote!(traced(timestamp(chrono = false))));
        let r = resolve(&a);
        assert!(!r.timestamp_chrono);
    }

    #[test]
    fn timestamp_provide_flag_defaults_to_disabled() {
        // Opt-in: `provide` omitted stays disabled, so `timestamp(chrono = true)`
        // does not emit a provide attr.
        let a = args(&parse_quote!(traced(timestamp(chrono = true))));
        let r = resolve(&a);
        assert!(!r.timestamp_provide, "provide is opt-in when omitted");
        assert!(r.timestamp_chrono);
    }

    #[test]
    fn timestamp_provide_true_resolves_enabled() {
        let a = args(&parse_quote!(traced(timestamp(provide = true))));
        let r = resolve(&a);
        assert!(r.timestamp_provide);
    }

    // ── manifest-default fold precedence ──────────────────────────────
    // per-attribute setting > manifest default > hardcoded default.

    #[test]
    fn fold_prefers_per_attr_then_manifest_then_hardcoded() {
        assert!(fold(Some(true), Some(false), false));
        assert!(!fold(Some(false), Some(true), true));
        assert!(fold(None, Some(true), false));
        assert!(!fold(None, Some(false), true));
        assert!(fold(None, None, true));
        assert!(!fold(None, None, false));
    }

    /// All-`Some` manifest defaults that invert every hardcoded default, so a
    /// fold falling through to the manifest is unambiguous.
    fn inverted_defaults() -> TracedDefaults {
        TracedDefaults {
            traced: None,
            location: Some(false),
            timestamp: Some(true),
            packed: Some(false),
            boxed: Some(false),
            code: Some(false),
        }
    }

    #[test]
    fn manifest_fills_unset_toggles() {
        // A bare `traced()` sets no sub-toggles, so each falls through to the
        // manifest default rather than the hardcoded one.
        let a = args_list(parse_quote!(traced()));
        let r = a.resolve(&inverted_defaults());
        assert!(!r.location);
        assert!(r.timestamp);
        assert!(!r.packed);
        assert!(!r.backtrace_boxed && !r.spantrace_boxed);
        assert!(!r.code);
    }

    #[test]
    fn per_attr_location_wins_over_manifest() {
        let a = args(&parse_quote!(traced(location)));
        let r = a.resolve(&inverted_defaults());
        assert!(r.location, "explicit `location` overrides manifest off");

        let a = args(&parse_quote!(traced(location = false)));
        let r = a.resolve(&TracedDefaults {
            location: Some(true),
            ..TracedDefaults::default()
        });
        assert!(
            !r.location,
            "explicit `location = false` overrides manifest on"
        );
    }

    #[test]
    fn per_attr_packed_wins_over_manifest() {
        let a = args(&parse_quote!(traced(packed)));
        let r = a.resolve(&inverted_defaults());
        assert!(r.packed);

        let a = args(&parse_quote!(traced(packed = false)));
        let r = a.resolve(&TracedDefaults {
            packed: Some(true),
            ..TracedDefaults::default()
        });
        assert!(!r.packed);
    }

    #[test]
    fn per_attr_boxed_wins_over_manifest() {
        let a = args(&parse_quote!(traced(boxed = false)));
        let r = a.resolve(&TracedDefaults {
            boxed: Some(true),
            ..TracedDefaults::default()
        });
        assert!(!r.backtrace_boxed && !r.spantrace_boxed);
    }

    #[test]
    fn type_only_trace_block_inherits_pair_level_boxed() {
        // A trace block written only to set `type` must not collapse boxing to
        // the hardcoded default: both traces inherit the pair-level `boxed`.
        let a = args(&parse_quote!(traced(
            boxed = false,
            backtrace(r#type = MyBt)
        )));
        let r = resolve(&a);
        assert!(!r.backtrace_boxed);
        assert!(!r.spantrace_boxed);
        assert!(r.backtrace_type.is_some());
        r.validate(proc_macro2::Span::call_site()).unwrap();
    }

    #[test]
    fn explicit_per_trace_boxed_overrides_pair_level() {
        let a = args(&parse_quote!(traced(
            boxed = false,
            backtrace(boxed = true)
        )));
        let r = resolve(&a);
        assert!(r.backtrace_boxed);
        assert!(!r.spantrace_boxed);
        r.validate(proc_macro2::Span::call_site()).unwrap_err();
    }

    #[test]
    fn type_only_trace_block_inherits_manifest_boxed() {
        let a = args(&parse_quote!(traced(backtrace(r#type = MyBt))));
        let r = a.resolve(&TracedDefaults {
            boxed: Some(false),
            ..TracedDefaults::default()
        });
        assert!(!r.backtrace_boxed);
        assert!(!r.spantrace_boxed);
    }

    #[test]
    fn per_attr_timestamp_off_wins_over_manifest_on() {
        // The explicit-false case the `Tristate` wrapper exists for: a bare
        // `traced` (no timestamp) takes the manifest default, but
        // `timestamp = false` must override a manifest that turns it on.
        let on = TracedDefaults {
            timestamp: Some(true),
            ..TracedDefaults::default()
        };
        let a = args(&parse_quote!(traced(timestamp = false)));
        assert!(!a.resolve(&on).timestamp, "explicit off beats manifest on");

        let a = args_list(parse_quote!(traced()));
        assert!(a.resolve(&on).timestamp, "unset takes manifest on");
    }

    #[test]
    fn per_attr_code_off_wins_over_manifest_on() {
        let on = TracedDefaults {
            code: Some(true),
            ..TracedDefaults::default()
        };
        let a = args(&parse_quote!(traced(code = false)));
        assert!(!a.resolve(&on).code, "explicit off beats manifest on");

        // Symmetric explicit-true over a manifest that turns code off.
        let off = TracedDefaults {
            code: Some(false),
            ..TracedDefaults::default()
        };
        let a = args(&parse_quote!(traced(code)));
        assert!(a.resolve(&off).code, "explicit on beats manifest off");
    }

    #[test]
    fn code_settings_block_counts_as_explicit_on() {
        // A `code(type = ...)` block has no `enabled`, yet it is user-written, so
        // it overrides a manifest `code = false` and stays on.
        let off = TracedDefaults {
            code: Some(false),
            ..TracedDefaults::default()
        };
        let a = args(&parse_quote!(traced(code(r#type = MyCode))));
        assert!(a.resolve(&off).code);
    }

    #[test]
    fn empty_manifest_keeps_hardcoded_defaults() {
        let a = args_list(parse_quote!(traced()));
        let r = a.resolve(&TracedDefaults::default());
        assert!(r.location);
        assert!(!r.timestamp);
        assert!(r.packed);
        assert!(r.backtrace_boxed && r.spantrace_boxed);
        assert!(r.code);
    }
}
