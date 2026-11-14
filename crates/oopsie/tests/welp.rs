//! Message-free `Welp` conversion (F3): `.welp()` / `Welp::from_error`.
//!
//! These exercise the public facade + prelude surface — propagation through
//! `?`, `Display` delegation to the source, the `source()` chain shape, and
//! downcasting back to the original error through the `Welp` itself.

#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use std::error::Error as _;

use oopsie::prelude::*;

/// `.welp()?` propagates a foreign `io::Error` into a `Result<_, Welp>` with no
/// invented message, reachable purely through the prelude (it rides
/// `WelpResultExt`).
#[test]
fn welp_propagates_foreign_io_error_through_question_mark() {
    fn read() -> Result<String, Welp> {
        let raw = std::fs::read_to_string("/nonexistent/oopsie-welp-test").welp()?;
        Ok(raw)
    }

    let err = read().unwrap_err();
    // No `Welp` message is invented; the display is the io error's own.
    assert!(err.message().is_none());
    assert!(err.downcast_ref::<std::io::Error>().is_some());
}

/// The wrapper's `Display` equals the source's, so its `source()` skips the
/// source and exposes the source's own cause instead of repeating the message.
#[test]
fn welp_delegates_display_to_source() {
    let result: Result<(), std::io::Error> = Err(std::io::Error::other("disk full"));
    let err = result.welp().unwrap_err();
    assert_eq!(err.to_string(), "disk full");
    assert!(err.source().is_none());
}

/// A message-free `Welp` is a transparent layer: its `source()` is the wrapped
/// error's own source, never the wrapped error itself.
#[test]
fn welp_source_chain_skips_the_wrapped_error() {
    let root = std::io::Error::other("root cause");
    let middle = Welp::wrap(root, "middle");
    let err = Welp::from_error(middle);

    let first = err.source().expect("first source is the middle's cause");
    assert_eq!(first.to_string(), "root cause");
    assert!(first.source().is_none());
}

/// The original error stays recoverable through the `Welp` itself.
#[test]
fn welp_downcasts_to_original_error() {
    let result: Result<(), std::io::Error> = Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "nope",
    ));
    let err = result.welp().unwrap_err();

    let io = err
        .downcast_ref::<std::io::Error>()
        .expect("downcasts back to the original io::Error");
    assert_eq!(io.kind(), std::io::ErrorKind::PermissionDenied);
}

/// `Welp::from_error` is the manual equivalent of `.welp()`.
#[test]
fn from_error_matches_welp_behavior() {
    let err = Welp::from_error(std::io::Error::other("manual"));
    assert!(err.message().is_none());
    assert_eq!(err.to_string(), "manual");
    assert!(err.source().is_none());
}

/// Traces are captured-or-extracted like `welp_context`: a `.welp()` wrapper
/// surfaces a backtrace (the source's origin-most one under the Provider API,
/// else the wrap-site capture).
#[test]
fn welp_surfaces_a_trace() {
    use oopsie::Diagnostic as _;

    oopsie_core::test_utils::force_backtrace();
    let result: Result<(), Welp> = Err(Welp::new("origin"));
    let err = result.welp().unwrap_err();
    assert!(err.oopsie_backtrace().is_some());
}
