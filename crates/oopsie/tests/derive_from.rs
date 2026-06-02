#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use oopsie::{Contextual as _, Oopsie};
use std::error::Error as _;
use std::io;

// ---- Test 1: #[oopsie(from)] on non-"source"-named field ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum FromMarkedError {
    #[oopsie("wrapped: {inner}")]
    Wrapped {
        #[oopsie(from)]
        inner: io::Error,
    },
}

#[test]
fn from_marks_non_source_field() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: FromMarkedError = Wrapped.build_error(io_err);
    assert!(matches!(err, FromMarkedError::Wrapped { .. }));
    let src = err.source().expect("should have a source");
    assert_eq!(src.to_string(), "pipe broke");
}

// ---- Test 2: #[oopsie(from(OrigType, transform))] ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum FromTransformError {
    #[oopsie("transformed: {inner}")]
    Transformed {
        #[oopsie(from(io::Error, Box::new))]
        inner: Box<io::Error>,
    },
}

#[test]
fn from_with_transform() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "not found");
    let err: FromTransformError = Transformed.build_error(io_err);
    assert!(matches!(err, FromTransformError::Transformed { .. }));
    let src = err.source().expect("should have a source");
    assert_eq!(src.to_string(), "not found");
}

// ---- Test 3: auto-boxing Box<T> source field ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AutoBoxError {
    #[oopsie("boxed io: {source}")]
    BoxedIo { source: Box<io::Error> },
}

#[test]
fn auto_box_source() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "not found");
    // The selector should accept io::Error directly, not Box<io::Error>
    let err: AutoBoxError = BoxedIo.build_error(io_err);
    assert!(err.to_string().contains("boxed io"));
    let src = err.source().expect("should have a source");
    assert_eq!(src.to_string(), "not found");
}

// ---- Test 4: explicit from(T, transform) still takes precedence over auto-boxing ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum ExplicitOverAutoBoxError {
    #[oopsie("explicit boxed: {source}")]
    ExplicitBoxed {
        #[oopsie(from(io::Error, Box::new))]
        source: Box<io::Error>,
    },
}

#[test]
fn explicit_from_takes_precedence() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
    let err: ExplicitOverAutoBoxError = ExplicitBoxed.build_error(io_err);
    assert!(err.to_string().contains("explicit boxed"));
    let src = err.source().expect("should have a source");
    assert_eq!(src.to_string(), "denied");
}

// ---- Test 5: field named "source" auto-detected ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AutoSourceError {
    #[oopsie("io failed")]
    IoFailed { source: io::Error },
}

#[test]
fn source_auto_detected() {
    let io_err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
    let err: AutoSourceError = IoFailed.build_error(io_err);
    assert!(matches!(err, AutoSourceError::IoFailed { .. }));
    let src = err.source().expect("should have a source");
    assert_eq!(src.to_string(), "timed out");
}
