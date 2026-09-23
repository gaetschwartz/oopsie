#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

//! Tests that `#[cfg(...)]`-gated variants and fields behave correctly in
//! generated code: gated-out items leave no dangling references to
//! compiled-out variants, fields, or types, and gated-in items work normally.
//!
//! `cfg(all())` and `cfg(any())` stand in for an active/inactive feature gate,
//! so these tests are deterministic regardless of which cargo features happen
//! to be enabled.

use oopsie::{Oopsie, oopsie};

/// Exists only under an always-false cfg — i.e. never. Any generated code that
/// names it must therefore also be gated out, or this file won't compile.
#[cfg(any())]
pub struct GhostType;

#[derive(Debug, Oopsie)]
#[oopsie(module(false), suffix)]
pub enum CfgError {
    #[cfg(all())]
    #[oopsie("present: {n}")]
    Present { n: u32 },

    #[cfg(any())]
    #[oopsie("gone")]
    Gone { ghost: GhostType },
}

#[test]
fn cfg_satisfied_variant_builds() {
    // The `cfg(all())` variant's selector is emitted and works.
    let err = PresentOopsie { n: 5u32 }.build();
    assert!(matches!(err, CfgError::Present { n: 5 }));
    assert_eq!(err.to_string(), "present: 5");
}

#[test]
fn cfg_gated_out_variant_emits_no_dangling_refs() {
    // This test's existence is incidental — the real assertion is that this
    // file compiles: the gated-out `Gone` variant must leave no generated code
    // referencing `CfgError::Gone` or `GhostType`, both compiled out.
    let err = PresentOopsie { n: 1u32 }.build();
    assert!(matches!(err, CfgError::Present { .. }));
}

// ---- same guarantee under the `#[oopsie(...)]` attribute-macro form ----
//
// One variant is gated behind an ACTIVE cfg (`cfg(test)`, since this compiles
// as a test binary) and another behind an INACTIVE cfg (`cfg(any())`), whose
// field type only exists under that same inactive cfg.

/// Only exists under the never-satisfied cfg; the gated-out variant references it.
#[cfg(any())]
pub struct AttrGhost;

#[oopsie]
#[oopsie(module(false), suffix)]
pub enum AttrCfgError {
    #[cfg(test)]
    #[oopsie("active: {n}")]
    Active { n: u32 },

    #[cfg(any())]
    #[oopsie("inactive")]
    Inactive { ghost: AttrGhost },
}

#[test]
fn attr_cfg_active_variant_builds() {
    // The `cfg(test)` variant is active in the test binary, so its selector
    // exists and works.
    let err = ActiveOopsie { n: 7u32 }.build();
    assert!(matches!(err, AttrCfgError::Active { n: 7 }));
    assert_eq!(err.to_string(), "active: 7");
}

// Two variants whose stripped selector names collide (`Read` / `ReadError`)
// are fine when each carries a mutually exclusive `#[cfg(...)]`: only one
// ever survives stripping, so there is no real collision.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
pub enum CfgCollisionError {
    #[cfg(all())]
    #[oopsie("read")]
    Read,
    #[cfg(any())]
    #[oopsie("read (io)")]
    ReadError,
}

#[test]
fn cfg_gated_variants_may_share_selector_name() {
    let err = Read.build();
    assert_eq!(err.to_string(), "read");
}

// ---- field-level cfg under the `#[oopsie(...)]` attribute-macro form ----
//
// A field gated out by `#[cfg(any())]` must be absent from every place
// generated code could mention it — the selector struct, `build()`, and the
// `Display` destructuring — or it references a field rustc stripped.

#[cfg(any())]
pub struct FieldGhost;

#[oopsie]
#[oopsie(module(false), suffix)]
pub enum FieldCfgError {
    #[oopsie("v: {keep}")]
    V {
        #[cfg(any())]
        extra: FieldGhost,
        keep: u32,
    },
}

#[test]
fn field_cfg_stripped_field_drops_its_generated_mentions() {
    // The selector has only the kept field; the stripped field left no dangling
    // references behind in the selector struct, `build()`, or Display.
    let err = VOopsie { keep: 9u32 }.build();
    assert!(matches!(err, FieldCfgError::V { keep: 9 }));
    assert_eq!(err.to_string(), "v: 9");
}

// Enabled-cfg twin: the field is present and works normally.
#[oopsie]
#[oopsie(module(false), suffix = "Ctx")]
pub enum FieldCfgPresentError {
    #[oopsie("v: {keep}")]
    V {
        #[cfg(all())]
        extra: String,
        keep: u32,
    },
}

#[test]
fn field_cfg_active_field_is_present_and_usable() {
    let err = VCtx {
        extra: "details".to_owned(),
        keep: 4u32,
    }
    .build();
    assert!(matches!(err, FieldCfgPresentError::V { keep: 4, .. }));
    assert_eq!(err.to_string(), "v: 4");
}

// ---- field-level cfg referencing a generic parameter ----
//
// `x` is `GenV`'s only reference to `T`, and it's gated out — so `GenV`'s
// selector must not declare `T` at all. `GenW`'s `y` field references `T`
// unconditionally and needs no special handling.

#[oopsie]
#[oopsie(module(false), suffix)]
pub enum CfgGenericFieldError<T: std::fmt::Debug> {
    #[oopsie("v")]
    GenV {
        #[cfg(any())]
        x: T,
        keep: u32,
    },
    #[oopsie("w: {y:?}")]
    GenW { y: T },
}

#[test]
fn cfg_only_generic_param_is_not_on_the_selector() {
    let err: CfgGenericFieldError<u32> = GenVOopsie { keep: 5u32 }.build();
    assert!(matches!(err, CfgGenericFieldError::GenV { keep: 5 }));
}

#[test]
fn unconditional_generic_field_needs_no_marker() {
    let err: CfgGenericFieldError<u32> = GenWOopsie { y: 9u32 }.build();
    assert!(matches!(err, CfgGenericFieldError::GenW { y: 9 }));
}

// ---- fully cfg-stripped enums under the attribute-macro path ----
//
// An enum whose variants are *all* gated out still exists as a type — `&Enum`
// stays inhabited — so any generated code matching over `self` must still
// compile with zero arms.

#[cfg(any())]
pub struct AllGoneGhost;

#[oopsie]
#[oopsie(module(false), suffix)]
pub enum AllStrippedError {
    #[cfg(any())]
    #[oopsie("a: {x}")]
    A { x: u32 },
    #[cfg(any())]
    #[oopsie("b")]
    B { ghost: AllGoneGhost },
}

#[test]
fn all_variants_stripped_still_compiles() {
    // No variant survives, so there is nothing to construct; the assertion is
    // that the generated `Display`/`Error::source`/`provide` matches over the
    // still-inhabited `&AllStrippedError` compile at all.
    fn _accepts(_: &AllStrippedError) {}
}

// Mixed enum: one variant stripped, one kept. The kept variant's Display and
// source arms must resolve normally with the other variant gone.

#[cfg(any())]
pub struct MixedGhost;

#[oopsie]
#[oopsie(module(false), suffix)]
pub enum MixedCfgError {
    #[oopsie("kept: {n}")]
    Kept { n: u32, source: std::io::Error },

    #[cfg(any())]
    #[oopsie("dropped")]
    Dropped { ghost: MixedGhost },
}

#[test]
fn mixed_cfg_kept_variant_display_and_source_work() {
    use oopsie::Contextual as _;
    use std::error::Error as _;

    let io = std::io::Error::other("boom");
    let err = KeptOopsie { n: 3u32 }.build_error(io);
    assert_eq!(err.to_string(), "kept: 3");
    assert!(err.source().is_some());
}

// ---- cfg-gated EXPLICIT trace FIELDS under the attribute-macro path ----
//
// The cfg sits on a trace FIELD (`#[oopsie(backtrace)]` / `spantrace` /
// `traces` / `location`) inside an otherwise-kept variant — distinct from
// gating the whole variant. A stripped trace field must not appear in its
// `Diagnostic` accessor, which falls through to `None` for that variant; the
// enabled twin (`cfg(all())` on the field) keeps the field and the accessor
// returns `Some`.

#[oopsie]
#[oopsie(module(false), suffix = "Bt")]
pub enum BacktraceFieldCfgError {
    #[oopsie("stripped: {keep}")]
    Stripped {
        #[cfg(any())]
        #[oopsie(backtrace)]
        bt: oopsie::Backtrace,
        keep: u32,
    },

    #[oopsie("kept: {keep}")]
    Kept {
        #[cfg(all())]
        #[oopsie(backtrace)]
        bt: oopsie::Backtrace,
        keep: u32,
    },
}

#[test]
fn cfg_stripped_backtrace_field_falls_through_to_none() {
    use oopsie::Diagnostic as _;
    let err = StrippedBt { keep: 1u32 }.build();
    assert!(err.oopsie_backtrace().is_none());
}

#[test]
fn cfg_kept_backtrace_field_accessor_returns_some() {
    use oopsie::Diagnostic as _;
    let err = KeptBt { keep: 2u32 }.build();
    assert!(err.oopsie_backtrace().is_some());
}

// Packed `traces` field under field-level cfg: same guarantee as above.
#[oopsie]
#[oopsie(module(false), suffix = "Tr")]
pub enum TracesFieldCfgError {
    #[oopsie("stripped: {keep}")]
    Stripped {
        #[cfg(any())]
        #[oopsie(traces)]
        t: (oopsie::Backtrace, oopsie::SpanTrace),
        keep: u32,
    },

    #[oopsie("kept: {keep}")]
    Kept {
        #[cfg(all())]
        #[oopsie(traces)]
        t: (oopsie::Backtrace, oopsie::SpanTrace),
        keep: u32,
    },
}

#[test]
fn cfg_stripped_packed_traces_field_falls_through_to_none() {
    use oopsie::Diagnostic as _;
    let err = StrippedTr { keep: 1u32 }.build();
    assert!(err.oopsie_backtrace().is_none());
    assert!(err.oopsie_spantrace().is_none());
}

#[test]
fn cfg_kept_packed_traces_field_accessor_returns_some() {
    use oopsie::Diagnostic as _;
    let err = KeptTr { keep: 2u32 }.build();
    assert!(err.oopsie_backtrace().is_some());
    assert!(err.oopsie_spantrace().is_some());
}

// Location field under field-level cfg.
#[oopsie]
#[oopsie(module(false), suffix = "Loc")]
pub enum LocationFieldCfgError {
    #[oopsie("stripped: {keep}")]
    Stripped {
        #[cfg(any())]
        #[oopsie(location)]
        at: &'static std::panic::Location<'static>,
        keep: u32,
    },

    #[oopsie("kept: {keep}")]
    Kept {
        #[cfg(all())]
        #[oopsie(location)]
        at: &'static std::panic::Location<'static>,
        keep: u32,
    },
}

#[test]
fn cfg_stripped_location_field_falls_through_to_none() {
    use oopsie::Diagnostic as _;
    let err = StrippedLoc { keep: 1u32 }.build();
    assert!(err.oopsie_location().is_none());
}

#[test]
fn cfg_kept_location_field_accessor_returns_some() {
    use oopsie::Diagnostic as _;
    let err = KeptLoc { keep: 2u32 }.build();
    assert!(err.oopsie_location().is_some());
}

// Struct form of the above.
#[oopsie]
#[oopsie(module(false))]
pub struct StructBtStrippedError {
    #[cfg(any())]
    #[oopsie(backtrace)]
    bt: oopsie::Backtrace,
    keep: u32,
}

#[test]
fn struct_cfg_stripped_backtrace_field_yields_none() {
    use oopsie::Diagnostic as _;
    let err = StructBtStrippedOopsie { keep: 1u32 }.build();
    assert!(err.oopsie_backtrace().is_none());
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructBtKeptError {
    #[cfg(all())]
    #[oopsie(backtrace)]
    bt: oopsie::Backtrace,
    keep: u32,
}

#[test]
fn struct_cfg_kept_backtrace_field_returns_some() {
    use oopsie::Diagnostic as _;
    let err = StructBtKeptOopsie { keep: 2u32 }.build();
    assert!(err.oopsie_backtrace().is_some());
}

// Spantrace needs the `tracing` feature (its `SpanTrace` capture is gated), so
// mirror the existing tracing-gated tests.
#[cfg(feature = "tracing")]
#[oopsie]
#[oopsie(module(false), suffix = "St")]
pub enum SpantraceFieldCfgError {
    #[oopsie("stripped: {keep}")]
    Stripped {
        #[cfg(any())]
        #[oopsie(spantrace)]
        st: oopsie::SpanTrace,
        keep: u32,
    },

    #[oopsie("kept: {keep}")]
    Kept {
        #[cfg(all())]
        #[oopsie(spantrace)]
        st: oopsie::SpanTrace,
        keep: u32,
    },
}

#[cfg(feature = "tracing")]
#[test]
fn cfg_stripped_spantrace_field_falls_through_to_none() {
    use oopsie::Diagnostic as _;
    let err = StrippedSt { keep: 1u32 }.build();
    assert!(err.oopsie_spantrace().is_none());
}

#[cfg(feature = "tracing")]
#[test]
fn cfg_kept_spantrace_field_accessor_returns_some() {
    use oopsie::Diagnostic as _;
    let err = KeptSt { keep: 2u32 }.build();
    assert!(err.oopsie_spantrace().is_some());
}

// ---- dynamic `#[oopsie(help)]` field under field-level cfg ----
//
// A struct help accessor names `self.<field>` and the `provide()` closure
// captures it, so a stripped help field must drop both the accessor and the
// provide statement with the field (the enum drops its match arm to `_ => None`).

#[oopsie]
#[oopsie(module(false))]
pub struct StructHelpStrippedError {
    #[cfg(any())]
    #[oopsie(help)]
    hint: String,
    keep: u32,
}

#[test]
fn struct_cfg_stripped_help_field_yields_none() {
    use oopsie::Diagnostic as _;
    let err = StructHelpStrippedOopsie { keep: 1u32 }.build();
    assert!(err.oopsie_help_text().is_none());
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructHelpKeptError {
    #[cfg(all())]
    #[oopsie(help)]
    hint: String,
    keep: u32,
}

#[test]
fn struct_cfg_kept_help_field_returns_some() {
    use oopsie::Diagnostic as _;
    let err = StructHelpKeptOopsie {
        hint: "do this".to_owned(),
        keep: 2u32,
    }
    .build();
    assert_eq!(&*err.oopsie_help_text().unwrap(), "do this");
}

// Enum form of the above.
#[oopsie]
#[oopsie(module(false), suffix = "He")]
pub enum EnumHelpCfgError {
    #[oopsie("stripped: {keep}")]
    Stripped {
        #[cfg(any())]
        #[oopsie(help)]
        hint: String,
        keep: u32,
    },

    #[oopsie("kept: {keep}")]
    Kept {
        #[cfg(all())]
        #[oopsie(help)]
        hint: String,
        keep: u32,
    },
}

#[test]
fn enum_cfg_stripped_help_field_falls_through_to_none() {
    use oopsie::Diagnostic as _;
    let err = StrippedHe { keep: 1u32 }.build();
    assert!(err.oopsie_help_text().is_none());
}

#[test]
fn enum_cfg_kept_help_field_returns_some() {
    use oopsie::Diagnostic as _;
    let err = KeptHe {
        hint: "fix it".to_owned(),
        keep: 2u32,
    }
    .build();
    assert_eq!(&*err.oopsie_help_text().unwrap(), "fix it");
}

// ---- field-level `provide(ErrorCode)` under field-level cfg ----
//
// A field-level provide on a stripped field goes with the field.

#[oopsie]
#[oopsie(module(false), suffix = "Ec")]
pub enum ErrorCodeFieldCfgError {
    #[oopsie("stripped: {keep}")]
    Stripped {
        #[cfg(any())]
        #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from(code.clone())))]
        code: String,
        keep: u32,
    },

    #[oopsie("kept: {keep}")]
    Kept {
        #[cfg(all())]
        #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from(code.clone())))]
        code: String,
        keep: u32,
    },
}

#[test]
fn cfg_stripped_error_code_field_falls_through_to_none() {
    use oopsie::Diagnostic as _;
    let err = StrippedEc { keep: 1u32 }.build();
    assert!(err.oopsie_error_code().is_none());
}

#[test]
fn cfg_kept_error_code_field_accessor_returns_some() {
    use oopsie::Diagnostic as _;
    let err = KeptEc {
        code: "kept::code".to_owned(),
        keep: 2u32,
    }
    .build();
    assert_eq!(err.oopsie_error_code().unwrap().as_str(), "kept::code");
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructErrorCodeStrippedError {
    #[cfg(any())]
    #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from(code.clone())))]
    code: String,
    keep: u32,
}

#[test]
fn struct_cfg_stripped_error_code_field_yields_none() {
    use oopsie::Diagnostic as _;
    let err = StructErrorCodeStrippedOopsie { keep: 1u32 }.build();
    assert!(err.oopsie_error_code().is_none());
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructErrorCodeKeptError {
    #[cfg(all())]
    #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from(code.clone())))]
    code: String,
    keep: u32,
}

#[test]
fn struct_cfg_kept_error_code_field_returns_some() {
    use oopsie::Diagnostic as _;
    let err = StructErrorCodeKeptOopsie {
        code: "struct::kept::code".to_owned(),
        keep: 2u32,
    }
    .build();
    assert_eq!(
        err.oopsie_error_code().unwrap().as_str(),
        "struct::kept::code"
    );
}

// ---- `cfg_attr`-wrapped helper attrs under the attribute-macro path ----
//
// A cfg_attr-wrapped oopsie(...) helper never survives as a bare attribute;
// sibling attrs stay.

#[oopsie]
#[oopsie(module(false), suffix = "Ca")]
pub enum CfgAttrHelperError {
    #[oopsie("kept: {keep}")]
    Kept {
        #[cfg_attr(all(), oopsie(help), doc = "sibling attr kept by the strip")]
        hint: String,
        keep: u32,
    },

    #[oopsie("gone: {keep}")]
    Gone {
        #[cfg_attr(any(), oopsie(help))]
        hint: String,
        keep: u32,
    },
}

#[test]
fn cfg_attr_wrapped_helper_attr_applies_with_predicate_on() {
    let err = KeptCa {
        hint: "plain field".to_owned(),
        keep: 1u32,
    }
    .build();
    assert!(matches!(err, CfgAttrHelperError::Kept { keep: 1, .. }));
    assert_eq!(err.to_string(), "kept: 1");
}

#[test]
fn cfg_attr_wrapped_helper_attr_is_absent_with_predicate_off() {
    let err = GoneCa {
        hint: "plain field".to_owned(),
        keep: 2u32,
    }
    .build();
    assert!(matches!(err, CfgAttrHelperError::Gone { keep: 2, .. }));
}

// ---- variant-level `cfg_attr`-wrapped `cfg` (the "requires-both" idiom) ----
//
// `#[cfg_attr(not(feature = "x"), cfg(feature = "x"))]` gates a variant on a
// feature without a literal `#[cfg]`: with the feature off the injected `cfg`
// strips the variant, with it on nothing is applied. A variant gated out this
// way must leave no dangling references, exactly like a literal `#[cfg]`.
// `all()`/`any()` pin the two feature states deterministically.

#[cfg(any())]
pub struct BothGhost;

// Feature off: the cfg_attr predicate holds, so `cfg(any())` is injected and
// rustc strips the variant.
#[oopsie]
#[oopsie(module(false), suffix = "Off")]
pub enum RequiresBothOffError {
    #[oopsie("plain: {n}")]
    Plain { n: u32 },

    #[cfg_attr(all(), cfg(any()))]
    #[oopsie("gated")]
    Gated { ghost: BothGhost },
}

#[test]
fn cfg_attr_gated_out_variant_emits_no_dangling_refs() {
    // The real assertion is that this file compiles: the gated-out `Gated`
    // variant must leave no generated code referencing
    // `RequiresBothOffError::Gated` or `BothGhost`, both stripped.
    let err = PlainOff { n: 1u32 }.build();
    assert!(matches!(err, RequiresBothOffError::Plain { n: 1 }));
}

// Feature on: the cfg_attr predicate fails, nothing is injected, the variant
// stays and works.
#[oopsie]
#[oopsie(module(false), suffix = "On")]
pub enum RequiresBothOnError {
    #[oopsie("plain: {n}")]
    Plain { n: u32 },

    #[cfg_attr(any(), cfg(all()))]
    #[oopsie("gated: {n}")]
    Gated { n: u32 },
}

#[test]
fn cfg_attr_kept_variant_builds() {
    let err = GatedOn { n: 7u32 }.build();
    assert!(matches!(err, RequiresBothOnError::Gated { n: 7 }));
    assert_eq!(err.to_string(), "gated: 7");
}

// Every variant stripped through cfg_attr: the type stays inhabited with
// zero variants, so generated code matching over `self` must still compile.
#[oopsie]
#[oopsie(module(false), suffix = "Ag")]
pub enum AllCfgAttrStrippedError {
    #[cfg_attr(all(), cfg(any()))]
    #[oopsie("a: {x}")]
    A { x: u32 },
}

#[test]
fn all_variants_stripped_via_cfg_attr_still_compiles() {
    fn _accepts(_: &AllCfgAttrStrippedError) {}
}

// ---- trace FIELDS gated by a `cfg_attr`-injected `cfg` ----
//
// Same, gated through #[cfg_attr(pred, cfg(...))].

#[oopsie]
#[oopsie(module(false))]
pub struct StructBtCfgAttrStrippedError {
    #[cfg_attr(all(), cfg(any()))]
    #[oopsie(backtrace)]
    bt: oopsie::Backtrace,
    keep: u32,
}

#[test]
fn struct_cfg_attr_stripped_backtrace_field_yields_none() {
    use oopsie::Diagnostic as _;
    let err = StructBtCfgAttrStrippedOopsie { keep: 1u32 }.build();
    assert!(err.oopsie_backtrace().is_none());
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructBtCfgAttrKeptError {
    #[cfg_attr(any(), cfg(any()))]
    #[oopsie(backtrace)]
    bt: oopsie::Backtrace,
    keep: u32,
}

#[test]
fn struct_cfg_attr_kept_backtrace_field_returns_some() {
    use oopsie::Diagnostic as _;
    let err = StructBtCfgAttrKeptOopsie { keep: 2u32 }.build();
    assert!(err.oopsie_backtrace().is_some());
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructLocCfgAttrStrippedError {
    #[cfg_attr(all(), cfg(any()))]
    #[oopsie(location)]
    at: &'static std::panic::Location<'static>,
    keep: u32,
}

#[test]
fn struct_cfg_attr_stripped_location_field_yields_none() {
    use oopsie::Diagnostic as _;
    let err = StructLocCfgAttrStrippedOopsie { keep: 1u32 }.build();
    assert!(err.oopsie_location().is_none());
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructLocCfgAttrKeptError {
    #[cfg_attr(any(), cfg(any()))]
    #[oopsie(location)]
    at: &'static std::panic::Location<'static>,
    keep: u32,
}

#[test]
fn struct_cfg_attr_kept_location_field_returns_some() {
    use oopsie::Diagnostic as _;
    let err = StructLocCfgAttrKeptOopsie { keep: 2u32 }.build();
    assert!(err.oopsie_location().is_some());
}

// A `cfg_attr` nested in a `cfg_attr` still reaches a `cfg`.
#[oopsie]
#[oopsie(module(false))]
pub struct StructBtNestedCfgAttrError {
    #[cfg_attr(all(), cfg_attr(all(), cfg(any())))]
    #[oopsie(backtrace)]
    bt: oopsie::Backtrace,
    keep: u32,
}

#[test]
fn struct_nested_cfg_attr_stripped_backtrace_field_yields_none() {
    use oopsie::Diagnostic as _;
    let err = StructBtNestedCfgAttrOopsie { keep: 1u32 }.build();
    assert!(err.oopsie_backtrace().is_none());
}

// Only some of the gated attrs are `cfg`s; the siblings gate nothing.
#[oopsie]
#[oopsie(module(false))]
pub struct StructBtMixedCfgAttrError {
    #[cfg_attr(all(), doc = "sibling", cfg(any()))]
    #[oopsie(backtrace)]
    bt: oopsie::Backtrace,
    keep: u32,
}

#[test]
fn struct_mixed_cfg_attr_stripped_backtrace_field_yields_none() {
    use oopsie::Diagnostic as _;
    let err = StructBtMixedCfgAttrOopsie { keep: 1u32 }.build();
    assert!(err.oopsie_backtrace().is_none());
}

// A `cfg_attr` gating no `cfg` conditions other attributes without removing the
// field, so it must not be read as an existence gate.
#[oopsie]
#[oopsie(module(false))]
pub struct StructBtNonCfgCfgAttrError {
    #[cfg_attr(all(), doc = "sibling")]
    #[oopsie(backtrace)]
    bt: oopsie::Backtrace,
    keep: u32,
}

#[test]
fn struct_non_cfg_cfg_attr_backtrace_field_returns_some() {
    use oopsie::Diagnostic as _;
    let err = StructBtNonCfgCfgAttrOopsie { keep: 1u32 }.build();
    assert!(err.oopsie_backtrace().is_some());
}

// With a source, a stripped own trace falls back to the source's.
#[derive(Debug, Oopsie)]
#[oopsie(module(false), suffix = "CaLeaf")]
pub enum CfgAttrLeafError {
    #[oopsie("leaf")]
    Boom {
        #[oopsie(backtrace)]
        bt: oopsie::Backtrace,
    },
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructBtCfgAttrNoOwnError {
    source: CfgAttrLeafError,
    keep: u32,
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructBtCfgAttrSourceStrippedError {
    #[cfg_attr(all(), cfg(any()))]
    #[oopsie(backtrace)]
    bt: oopsie::Backtrace,
    source: CfgAttrLeafError,
    keep: u32,
}

#[test]
fn struct_cfg_attr_stripped_backtrace_field_keeps_source_forwarding() {
    use oopsie::{Contextual as _, Diagnostic as _};
    let stripped =
        StructBtCfgAttrSourceStrippedOopsie { keep: 1u32 }.build_error(BoomCaLeaf {}.build());
    let baseline = StructBtCfgAttrNoOwnOopsie { keep: 1u32 }.build_error(BoomCaLeaf {}.build());
    assert_eq!(
        stripped.oopsie_backtrace().is_some(),
        baseline.oopsie_backtrace().is_some()
    );
}

#[oopsie]
#[oopsie(module(false))]
pub struct StructBtCfgAttrSourceKeptError {
    #[cfg_attr(any(), cfg(any()))]
    #[oopsie(backtrace)]
    bt: oopsie::Backtrace,
    source: CfgAttrLeafError,
    keep: u32,
}

#[test]
fn struct_cfg_attr_kept_backtrace_field_with_source_returns_some() {
    use oopsie::{Contextual as _, Diagnostic as _};
    let err = StructBtCfgAttrSourceKeptOopsie { keep: 2u32 }.build_error(BoomCaLeaf {}.build());
    assert!(err.oopsie_backtrace().is_some());
}

// Enum sibling: the stripped field's variant arm falls through to the
// accessor's `_ => None`.
#[oopsie]
#[oopsie(module(false), suffix = "Cb")]
pub enum EnumBtCfgAttrError {
    #[oopsie("stripped: {keep}")]
    Stripped {
        #[cfg_attr(all(), cfg(any()))]
        #[oopsie(backtrace)]
        bt: oopsie::Backtrace,
        keep: u32,
    },

    #[oopsie("kept: {keep}")]
    Kept {
        #[cfg_attr(any(), cfg(any()))]
        #[oopsie(backtrace)]
        bt: oopsie::Backtrace,
        keep: u32,
    },
}

#[test]
fn enum_cfg_attr_stripped_backtrace_field_falls_through_to_none() {
    use oopsie::Diagnostic as _;
    let err = StrippedCb { keep: 1u32 }.build();
    assert!(err.oopsie_backtrace().is_none());
}

#[test]
fn enum_cfg_attr_kept_backtrace_field_accessor_returns_some() {
    use oopsie::Diagnostic as _;
    let err = KeptCb { keep: 2u32 }.build();
    assert!(err.oopsie_backtrace().is_some());
}

// Enum form: a stripped own trace/location falls back to the source's.
#[oopsie]
#[oopsie(module(false), suffix = "Fl")]
pub enum FwdTraceLeafError {
    #[oopsie("leaf")]
    Boom {
        #[oopsie(backtrace)]
        bt: oopsie::Backtrace,
        #[oopsie(location)]
        at: &'static std::panic::Location<'static>,
    },
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false), suffix = "Fq")]
pub enum FwdQuietLeafError {
    #[oopsie("quiet")]
    Quiet { keep: u32 },
}

#[oopsie]
#[oopsie(module(false))]
pub enum FwdBtStrippedError {
    #[oopsie(transparent)]
    Wrap {
        source: FwdTraceLeafError,
        #[cfg(any())]
        #[oopsie(backtrace)]
        bt: oopsie::Backtrace,
    },
}

#[oopsie]
#[oopsie(module(false))]
pub enum FwdLocStrippedError {
    #[oopsie(transparent)]
    Wrap {
        source: FwdTraceLeafError,
        #[cfg(any())]
        #[oopsie(location)]
        at: &'static std::panic::Location<'static>,
    },
}

#[oopsie]
#[oopsie(module(false))]
pub enum FwdBtKeptError {
    #[oopsie(transparent)]
    Wrap {
        source: FwdQuietLeafError,
        #[cfg(all())]
        #[oopsie(backtrace)]
        bt: oopsie::Backtrace,
    },
}

#[test]
fn enum_cfg_stripped_backtrace_field_keeps_source_forwarding() {
    use oopsie::Diagnostic as _;
    let err: FwdBtStrippedError = BoomFl {}.build().into();
    assert!(err.oopsie_backtrace().is_some());
}

#[test]
fn enum_cfg_stripped_location_field_keeps_source_forwarding() {
    use oopsie::Diagnostic as _;
    let err: FwdLocStrippedError = BoomFl {}.build().into();
    assert!(err.oopsie_location().is_some());
}

#[test]
fn enum_cfg_kept_backtrace_field_with_quiet_source_uses_own_field() {
    use oopsie::Diagnostic as _;
    let err: FwdBtKeptError = QuietFq { keep: 1u32 }.build().into();
    assert!(err.oopsie_backtrace().is_some());
}
