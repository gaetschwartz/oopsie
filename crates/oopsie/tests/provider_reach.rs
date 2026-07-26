//! Trace reach through a type-erased source: the Provider API surfaces the
//! origin-most capture, while stable falls back to the wrap-site one.
#![cfg(all(feature = "fancy", feature = "tracing"))]
#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::all,
    reason = "integration test fixtures intentionally trip style lints"
)]

mod common;

use oopsie::{Report, Welp, oopsie};
use oopsie_core::{redact, snap_name_by_channel};

#[oopsie(traced)]
#[oopsie("inner failed: {message}")]
pub struct InnerError {
    message: String,
}

#[inline(never)]
fn origin_frame() -> InnerError {
    InnerOopsie {
        message: "disk offline".to_owned(),
    }
    .build()
}

#[inline(never)]
fn wrap_site(inner: InnerError) -> Welp {
    Welp::wrap(inner, "outer")
}

#[test_with::env(OOPSIE_BACKTRACE_SNAPSHOT_TESTS)]
fn test_welp_wrap_reaches_erased_source_trace() {
    common::force_backtrace();
    let report = Report::new(wrap_site(origin_frame())).no_colors();
    redact!(backtrace, {
        insta::assert_snapshot!(snap_name_by_channel!("welp_wrap_erased_source"), report);
    });
}
