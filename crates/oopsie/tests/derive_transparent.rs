#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(unused, clippy::all)]

use oopsie::Oopsie;
use std::io;

// Test 1 & 2: Transparent variant generates From impl and uses custom display.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum TransparentError {
    #[oopsie("io error", transparent)]
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
    #[oopsie("traced io error", transparent)]
    TracedIo {
        source: io::Error,
        #[oopsie(auto)]
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
    #[oopsie("wrapped io", transparent)]
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
