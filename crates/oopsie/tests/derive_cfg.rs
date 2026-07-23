#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

//! `#[cfg(...)]` propagation onto generated selectors and the generated
//! `From`/`Contextual`/`Display`/source match arms.
//!
//! The derive extracts each variant's `#[cfg(...)]` attributes and re-applies
//! them to the code it generates for that variant. If that forwarding breaks, a
//! gated-*out* variant's selector and impl arms would still be emitted and would
//! reference a variant (and field types) that no longer exist — a compile error.
//!
//! These tests pin the behavior deterministically without depending on which
//! cargo features happen to be enabled: `cfg(all())` is always satisfied and
//! `cfg(any())` never is. The extraction logic treats them exactly like a
//! `cfg(feature = "...")` gate, so this exercises the same propagation path.

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
    // file compiles at all. If the `#[cfg(any())]` on `Gone` were not forwarded
    // to its generated selector / From / Display arms, that code would reference
    // `CfgError::Gone` and `GhostType` (both compiled out) and fail to build.
    let err = PresentOopsie { n: 1u32 }.build();
    assert!(matches!(err, CfgError::Present { .. }));
}

// ---- cfg propagation under the `#[oopsie(...)]` attribute-macro form ----
//
// The attribute macro (oopsie_attr/mod.rs) delegates to `derive::expand_enum`,
// so the same cfg-forwarding path must apply. Here one variant is gated behind
// an ACTIVE cfg (`cfg(test)` — these are compiled as a test binary) and another
// behind an INACTIVE cfg (`cfg(any())`). The inactive variant references a type
// that only exists under the inactive cfg, so if forwarding broke, the generated
// selector/From/Display arms would name a compiled-out type and fail to build.

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
// are allowed when each carries a mutually exclusive `#[cfg(...)]`: only one
// selector is ever emitted, so the collision check must exempt them.
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
// Attribute macros run before rustc strips `#[cfg]`, so a field gated out by an
// inactive cfg is still visible to the macro. Its cfg attrs must ride onto every
// generated mention (selector struct field, `build()` initializer, Display
// destructure binding); if they don't, the generated code references a field
// rustc removed and fails with E0559/E0026/E0063.

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

// Enabled-cfg twin: the field is present and usable. A cfg-gated field takes its
// concrete type (not the `Into` selector param), so `keep` is passed concretely.
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
// `x`'s cfg-gated type is the only reference to `T` in variant `GenV`;
// stripping it would leave `T` declared on the selector but never named by a
// surviving field (E0392) unless a `PhantomData` marker keeps it used.
// `GenW`'s `y` field references `T` unconditionally, so its selector needs no
// marker.

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
fn cfg_only_generic_param_gets_phantom_marker() {
    let err: CfgGenericFieldError<u32> = GenVOopsie {
        keep: 5u32,
        __oopsie_phantom: std::marker::PhantomData,
    }
    .build();
    assert!(matches!(err, CfgGenericFieldError::GenV { keep: 5 }));
}

#[test]
fn unconditional_generic_field_needs_no_marker() {
    let err: CfgGenericFieldError<u32> = GenWOopsie { y: 9u32 }.build();
    assert!(matches!(err, CfgGenericFieldError::GenW { y: 9 }));
}

// ---- fully cfg-stripped enums under the attribute-macro path ----
//
// The attribute macro expands before rustc strips `#[cfg]`, so an enum whose
// variants are *all* gated out reaches the generators with every match arm
// present. After stripping the arms vanish but `&Enum` is still inhabited, so a
// bare `match self {}` would be E0004. The generated matches over `self` carry a
// wildcard fallback whenever any variant is cfg-gated.

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

// Mixed enum: one variant stripped, one kept. The wildcard fallback is emitted
// because a variant is cfg-gated, yet the kept variant's Display and source
// arms must still resolve normally.

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
// The cfg sits on the trace FIELD (`#[oopsie(backtrace)]` / `spantrace` /
// `traces` / `location`), inside a kept variant — distinct from a cfg on the
// whole variant. The attribute macro runs before rustc strips `#[cfg]`, so the
// field's own cfg must ride onto its generated `Diagnostic` accessor arm (and
// `provide()` stmt); otherwise the arm names a field rustc later removed and
// fails with E0026 (enum) / E0609 (struct). The stripped variant has no other
// own trace, so its arm must drop and fall through to the accessor's
// `_ => None`. The enabled twin (`cfg(all())` on the field) keeps it.

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

// Packed `traces` field under field-level cfg: same arm/stmt-naming gap.
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

// Struct path: the accessor methods name `self.<field>` directly (no match
// arm), so a stripped own trace field there would be E0609 unless the whole
// method drops with the field. Selector name is the struct name with a trailing
// `Error` stripped (default suffix off).
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

// Enum sibling: the help accessor arm is already cfg-gated, but the `provide()`
// closure references the help field too and must drop with it under `unstable`.
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
// The provide expr references the field itself, so a cfg-stripped field's
// binding drops from the destructure while the expr still names it — unless
// the whole arm/method drops with the field (the enum drops to `_ => None`;
// the struct method drops entirely with the field's cfg). Before the fix this
// failed with E0425 ("cannot find value `code`") on the stable accessor.

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
