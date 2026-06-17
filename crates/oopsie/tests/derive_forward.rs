#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures trip style lints"
)]

mod common;

use oopsie::{Contextual as _, Diagnostic as _, oopsie};
use std::error::Error as _;
use std::io;

#[oopsie(traced)]
pub enum LeafError {
    #[oopsie("leaf: {msg}")]
    Boom { msg: String },
}

#[oopsie(traced)]
pub enum WrapError {
    #[oopsie("wrap")]
    Around {
        #[oopsie(forward)]
        source: LeafError,
    },
}

#[oopsie(traced)]
pub struct ForwardedSizeError {
    #[oopsie(forward)]
    source: LeafError,
}

#[oopsie(traced)]
pub struct CapturingSizeError {
    source: LeafError,
}

fn leaf() -> LeafError {
    common::force_backtrace();
    leaf_oopsies::Boom { msg: "boom" }.build()
}

#[test]
fn forwarded_wrapper_compiles_and_keeps_source() {
    let err: WrapError = wrap_oopsies::Around.build_error(leaf());
    assert_eq!(err.to_string(), "wrap"); // own message, NOT transparent
    assert!(err.source().is_some());
}

#[test]
fn forwarding_shrinks_the_value() {
    // The capturing variant keeps the boxed trace pair + location fields the
    // forwarded variant omits; both are pointer-aligned, so the difference can't
    // be absorbed by padding.
    assert!(
        std::mem::size_of::<ForwardedSizeError>() < std::mem::size_of::<CapturingSizeError>(),
        "forwarded ({}) must be smaller than capturing ({})",
        std::mem::size_of::<ForwardedSizeError>(),
        std::mem::size_of::<CapturingSizeError>(),
    );
}

#[oopsie(traced)]
pub struct BtWrapError {
    #[oopsie(forward)]
    source: LeafError,
}

#[test]
fn forwarded_backtrace_matches_source() {
    common::force_backtrace();
    let src = leaf_oopsies::Boom { msg: "x" }.build();
    let src_frames = src
        .oopsie_backtrace()
        .expect("forced capture => Some")
        .frames()
        .len();
    assert!(
        src_frames > 0,
        "force_backtrace must yield frames for this test to be probative"
    );

    let wrap: BtWrapError = bt_wrap_oopsies::BtWrap.build_error(src);
    let wrap_bt = wrap
        .oopsie_backtrace()
        .expect("Wrap must forward the source's backtrace");
    assert_eq!(
        wrap_bt.frames().len(),
        src_frames,
        "forwarded backtrace is the source's"
    );
}

#[cfg(feature = "tracing")]
#[test]
fn forwarded_spantrace_parity_with_source() {
    let _sub = common::init_test_subscriber();
    let src = leaf_oopsies::Boom { msg: "x" }.build();
    let src_st_present = src.oopsie_spantrace().is_some();
    let wrap: BtWrapError = bt_wrap_oopsies::BtWrap.build_error(src);
    assert_eq!(
        wrap.oopsie_spantrace().is_some(),
        src_st_present,
        "forwarded spantrace presence must match the source's"
    );
}

// ─── Mixed-enum: variants forward independently ──────────────────────────────

#[oopsie(traced)]
pub enum MixedError {
    #[oopsie("leaf {detail}")]
    LeafV { detail: String },
    #[oopsie("wrap")]
    WrapV {
        #[oopsie(forward)]
        source: LeafError,
    },
    #[oopsie("io")]
    IoV { source: io::Error },
}

#[test]
fn mixed_enum_variants_behave_independently() {
    common::force_backtrace();
    let leaf = leaf_oopsies::Boom { msg: "x" }.build();
    let leaf_some = leaf.oopsie_backtrace().is_some();
    let wrap: MixedError = mixed_oopsies::WrapV.build_error(leaf);
    assert_eq!(
        wrap.oopsie_backtrace().is_some(),
        leaf_some,
        "WrapV forwards the source's backtrace"
    );

    let io: MixedError = mixed_oopsies::IoV.build_error(io::Error::other("e"));
    assert!(
        io.oopsie_backtrace().is_some(),
        "IoV is foreign + non-forwarded, so it captures its own backtrace"
    );

    let leaf_v = mixed_oopsies::LeafV { detail: "d" }.build();
    assert!(
        leaf_v.oopsie_backtrace().is_some(),
        "LeafV is a source-less leaf, so it captures its own backtrace"
    );
}

// ─── Boxed source forwards ───────────────────────────────────────────────────

#[oopsie(traced)]
pub struct BoxWrapError {
    #[oopsie(forward)]
    source: Box<LeafError>,
}

#[test]
fn boxed_source_forwards() {
    common::force_backtrace();
    let leaf = leaf_oopsies::Boom { msg: "x" }.build();
    let some = leaf.oopsie_backtrace().is_some();
    let bw: BoxWrapError = box_wrap_oopsies::BoxWrap.build_error(leaf);
    assert_eq!(
        bw.oopsie_backtrace().is_some(),
        some,
        "BoxWrapError forwards the boxed source's backtrace"
    );
}

// ─── Foreign source: forward yields None ─────────────────────────────────────

#[oopsie(traced)]
pub struct ForeignWrapError {
    #[oopsie(forward)]
    source: io::Error,
}

#[test]
fn foreign_source_forward_yields_none() {
    let fw: ForeignWrapError = foreign_wrap_oopsies::ForeignWrap.build_error(io::Error::other("e"));
    assert!(
        fw.oopsie_backtrace().is_none(),
        "io::Error has no oopsie backtrace to forward"
    );
}

// ─── Partial forward: backtrace=false keeps own field ────────────────────────

#[oopsie(traced)]
pub struct PartialWrapError {
    #[oopsie(forward(backtrace = false))]
    source: LeafError,
}

#[test]
fn partial_forward_keeps_own_backtrace_field() {
    common::force_backtrace();
    let pw: PartialWrapError =
        partial_wrap_oopsies::PartialWrap.build_error(leaf_oopsies::Boom { msg: "x" }.build());
    // backtrace is NOT forwarded — the wrapper captures its own.
    assert!(
        pw.oopsie_backtrace().is_some(),
        "wrapper must capture its own backtrace when forward(backtrace = false)"
    );
}

#[cfg(feature = "tracing")]
#[test]
fn partial_forward_forwards_spantrace() {
    let _sub = common::init_test_subscriber();
    let src = leaf_oopsies::Boom { msg: "x" }.build();
    let src_st_present = src.oopsie_spantrace().is_some();
    // spantrace IS forwarded (no own spantrace field on the wrapper).
    let pw: PartialWrapError = partial_wrap_oopsies::PartialWrap.build_error(src);
    assert_eq!(
        pw.oopsie_spantrace().is_some(),
        src_st_present,
        "forwarded spantrace presence must match the source's"
    );
}

// ─── Generic forwarded source forwards (stable + nightly) ────────────────────
// The source is a bare generic param bounded only by `Error`. With a concrete
// `S` that IS `Diagnostic` the forwarded accessors must still reach the source —
// on stable (where the Provider path yields `None`) and on nightly.

#[oopsie(traced)]
pub struct GenWrapError<S: std::error::Error + std::fmt::Debug + 'static> {
    #[oopsie(forward)]
    source: S,
}

#[test]
fn generic_forwarded_backtrace_matches_source() {
    common::force_backtrace();
    let src = leaf_oopsies::Boom { msg: "x" }.build();
    let src_frames = src
        .oopsie_backtrace()
        .expect("forced capture => Some")
        .frames()
        .len();
    assert!(
        src_frames > 0,
        "force_backtrace must yield frames for this test to be probative"
    );

    let wrap: GenWrapError<LeafError> = gen_wrap_oopsies::GenWrap.build_error(src);
    let wrap_bt = wrap
        .oopsie_backtrace()
        .expect("generic wrapper must forward the source's backtrace");
    assert_eq!(
        wrap_bt.frames().len(),
        src_frames,
        "forwarded backtrace is the source's"
    );
}

#[oopsie(traced)]
pub struct GenLocWrapError<S: std::error::Error + std::fmt::Debug + 'static> {
    #[oopsie(forward(location))]
    source: S,
}

#[test]
fn generic_forwarded_location_matches_source() {
    let src = leaf_oopsies::Boom { msg: "x" }.build();
    let src_loc = src
        .oopsie_location()
        .expect("traced leaf captures a location");
    let wrap: GenLocWrapError<LeafError> = gen_loc_wrap_oopsies::GenLocWrap.build_error(src);
    let wrap_loc = wrap
        .oopsie_location()
        .expect("generic wrapper must forward the source's location");
    assert_eq!(
        (wrap_loc.file(), wrap_loc.line()),
        (src_loc.file(), src_loc.line()),
        "forwarded location is the source's"
    );
}

// ─── Explicit `from(Type, transform)` source forwards too ─────────────────────

#[oopsie(traced)]
pub struct FromWrapError {
    #[oopsie(from(LeafError, Box::new), forward)]
    source: Box<LeafError>,
}

#[test]
fn from_transform_source_forwards() {
    common::force_backtrace();
    let leaf = leaf_oopsies::Boom { msg: "x" }.build();
    let some = leaf.oopsie_backtrace().is_some();
    let fw: FromWrapError = from_wrap_oopsies::FromWrap.build_error(leaf);
    assert_eq!(
        fw.oopsie_backtrace().is_some(),
        some,
        "an explicit from(Type, transform) source forwards its backtrace"
    );
}
