#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(unused, clippy::all)]

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

use oopsie::Oopsie;

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
