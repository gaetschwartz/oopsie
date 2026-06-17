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
