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
