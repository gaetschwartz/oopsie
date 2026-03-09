#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(unused, clippy::all)]

use oopsie::{IntoError, Oopsie};
use std::error::Error;
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
    let err: FromMarkedError = Wrapped.into_error(io_err);
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
    let err: FromTransformError = Transformed.into_error(io_err);
    assert!(matches!(err, FromTransformError::Transformed { .. }));
    let src = err.source().expect("should have a source");
    assert_eq!(src.to_string(), "not found");
}

// ---- Test 3: field named "source" auto-detected ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AutoSourceError {
    #[oopsie("io failed")]
    IoFailed { source: io::Error },
}

#[test]
fn source_auto_detected() {
    let io_err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
    let err: AutoSourceError = IoFailed.into_error(io_err);
    assert!(matches!(err, AutoSourceError::IoFailed { .. }));
    let src = err.source().expect("should have a source");
    assert_eq!(src.to_string(), "timed out");
}
