//! Trace reach through a *type-erased* source — the one axis on which the
//! Provider API changes rendered output.
//!
//! `Welp::wrap` is generic, so it cannot extract the source's traces at
//! construction time and must reach them through `core::error::request_ref` at
//! accessor time. That reach exists only under
//! `unstable-error-generic-member-access`: with it, the origin-most capture wins
//! and the backtrace bottoms out in the frame that built the inner error;
//! without it, the lookup yields `None` and the wrap-site capture is rendered
//! instead. These snapshots are therefore keyed on the channel, unlike the
//! typed-source fixtures elsewhere in this suite.
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
