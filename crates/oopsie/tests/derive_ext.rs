#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use oopsie::{Oopsie, OptionExt as _, ResultExt as _};
use std::io;

// Error with a source variant (io::Error) and a leaf variant (no source).
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AppError {
    #[oopsie("io failed: {detail}")]
    IoFailed { source: io::Error, detail: String },

    #[oopsie("missing key: {key}")]
    MissingKey { key: String },

    #[oopsie("unit error")]
    UnitError,
}

#[test]
fn result_context() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "file gone");
    let result: Result<(), io::Error> = Err(io_err);
    let converted: Result<(), AppError> = result.context(IoFailed {
        detail: "reading config",
    });
    let err = converted.unwrap_err();
    assert!(matches!(err, AppError::IoFailed { .. }));
    assert_eq!(err.to_string(), "io failed: reading config");
}

#[test]
fn result_with_context() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "no access");
    let result: Result<(), io::Error> = Err(io_err);
    let converted: Result<(), AppError> = result.with_context(|_e| IoFailed {
        detail: "writing output",
    });
    let err = converted.unwrap_err();
    assert!(matches!(err, AppError::IoFailed { .. }));
    assert_eq!(err.to_string(), "io failed: writing output");
}

#[test]
fn option_context_none() {
    let opt: Option<i32> = None;
    let result: Result<i32, AppError> = opt.context(MissingKey { key: "abc" });
    let err = result.unwrap_err();
    assert!(matches!(err, AppError::MissingKey { .. }));
    assert_eq!(err.to_string(), "missing key: abc");
}

#[test]
fn option_context_some() {
    let opt: Option<i32> = Some(42);
    let result: Result<i32, AppError> = opt.context(MissingKey { key: "unused" });
    assert_eq!(result.unwrap(), 42);
}

#[test]
fn option_with_context() {
    let opt: Option<i32> = None;
    let result: Result<i32, AppError> = opt.with_context(|| MissingKey { key: "lazy_key" });
    let err = result.unwrap_err();
    assert!(matches!(err, AppError::MissingKey { .. }));
    assert_eq!(err.to_string(), "missing key: lazy_key");
}

#[test]
fn result_context_unit_selector() {
    // UnitError is a leaf (no source), so we cannot use it with ResultExt.
    // Instead, verify that unit selectors work with OptionExt.
    let opt: Option<i32> = None;
    let result: Result<i32, AppError> = opt.context(Unit);
    let err = result.unwrap_err();
    assert!(matches!(err, AppError::UnitError));
    assert_eq!(err.to_string(), "unit error");

    // Also verify build() and fail() work on unit selectors.
    let built = Unit.build();
    assert!(matches!(built, AppError::UnitError));

    let failed: Result<i32, AppError> = Unit.fail();
    failed.unwrap_err();
}
