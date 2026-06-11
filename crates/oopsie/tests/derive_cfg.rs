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
