#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use oopsie::Oopsie;
use std::io;

// Test 1 & 2: Transparent variant generates From impl and uses custom display.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum TransparentError {
    #[oopsie(display("io error"), transparent)]
    Io { source: io::Error },
}

#[test]
fn transparent_generates_from() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: TransparentError = TransparentError::from(io_err);
    assert!(matches!(err, TransparentError::Io { .. }));
}

#[test]
fn transparent_display() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: TransparentError = TransparentError::from(io_err);
    // Display should use the format string "io error", not the source's display.
    assert_eq!(err.to_string(), "io error");
}

// Test 3: Transparent variant with auto fields.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum TracedError {
    #[oopsie(display("traced io error"), transparent)]
    TracedIo {
        source: io::Error,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
    },
}

#[test]
fn transparent_with_auto_fields() {
    let io_err = io::Error::new(io::ErrorKind::Other, "something");
    // From impl should auto-generate the backtrace field.
    let err: TracedError = TracedError::from(io_err);
    assert!(matches!(err, TracedError::TracedIo { .. }));
    assert_eq!(err.to_string(), "traced io error");
}

// Test 4: Mixed transparent and regular variants.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum MixedError {
    #[oopsie(display("wrapped io"), transparent)]
    Wrapped { source: io::Error },

    #[oopsie("custom: {msg}")]
    Custom { msg: String },
}

#[test]
fn mixed_transparent_and_regular() {
    // Transparent variant via From.
    let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
    let err: MixedError = MixedError::from(io_err);
    assert!(matches!(err, MixedError::Wrapped { .. }));
    assert_eq!(err.to_string(), "wrapped io");

    // Regular leaf variant via selector build().
    let err = Custom { msg: "bad thing" }.build();
    assert!(matches!(err, MixedError::Custom { .. }));
    assert_eq!(err.to_string(), "custom: bad thing");
}

// Real-macro coverage for the stable diagnostic accessors on a transparent,
// trace-injected wrapper. Uses the actual `#[oopsie(traced)]` attribute macro
// (inject → derive) rather than a hand-written post-injection shape, so it
// cannot drift from what injection emits.
#[cfg(feature = "unstable-error-generic-member-access")]
mod traced_transparent {
    use oopsie::Diagnostic as _;

    #[oopsie::oopsie(traced)]
    pub enum Leaf {
        #[oopsie("leaf boom: {detail}")]
        Boom { detail: String },
    }

    #[oopsie::oopsie(traced)]
    pub enum Wrapper {
        #[oopsie(display("wrapper around leaf"), transparent)]
        Around { source: Leaf },
    }

    // A transparent traced wrapper forwards to its source first in `provide()`,
    // so the deepest (leaf) trace fills the slot. The stable accessor must agree.
    #[test]
    fn transparent_traced_stable_matches_provider_deepest() {
        use leaf_oopsies::Boom;

        let leaf = Boom { detail: "x" }.build();
        let outer: Wrapper = Wrapper::from(leaf);

        let bt_provide = core::error::request_ref::<oopsie::Backtrace>(&outer)
            .expect("provider path yields a backtrace");
        let st_provide = core::error::request_ref::<oopsie::SpanTrace>(&outer)
            .expect("provider path yields a span trace");

        let bt_stable = outer
            .oopsie_backtrace()
            .expect("stable accessor yields a backtrace");
        let st_stable = outer
            .oopsie_spantrace()
            .expect("stable accessor yields a span trace");

        assert!(
            std::ptr::eq(bt_provide, bt_stable),
            "transparent traced wrapper: stable oopsie_backtrace() must surface the same \
             deepest backtrace as the provider API"
        );
        assert!(
            std::ptr::eq(st_provide, st_stable),
            "transparent traced wrapper: stable oopsie_spantrace() must surface the same \
             deepest span trace as the provider API"
        );
    }
}
