#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(unused, clippy::all)]

use oopsie::{IntoError, NoneError, Oopsie};
use std::error::Error;
use std::io;

// ---- Enum definitions ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AppError {
    NotFound { path: String },

    IoFailed { source: io::Error, context: String },

    Timeout { source: io::Error },

    Config { host: String, port: u16 },
}

// ---- Struct definitions ----

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
struct ParseError {
    msg: String,
}

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
struct WrapError {
    source: io::Error,
    detail: String,
}

// ---- Tests ----

#[test]
fn leaf_enum_build() {
    let err = NotFound {
        path: "/tmp/missing",
    }
    .build();
    assert!(matches!(err, AppError::NotFound { ref path } if path == "/tmp/missing"));
}

#[test]
fn leaf_enum_fail() {
    let result: Result<(), AppError> = NotFound { path: "gone" }.fail();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, AppError::NotFound { ref path } if path == "gone"));
}

#[test]
fn source_enum_into_error() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
    let err: AppError = IoFailed {
        context: "reading config",
    }
    .into_error(io_err);
    match &err {
        AppError::IoFailed { source, context } => {
            assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(context, "reading config");
        }
        other => panic!("expected IoFailed, got {other:?}"),
    }
    // Error::source() should return the io::Error
    assert!(err.source().is_some());
}

#[test]
fn source_only_unit_selector() {
    // Timeout has only a source field, so its selector is a unit struct.
    let io_err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
    let err: AppError = Timeout.into_error(io_err);
    assert!(matches!(err, AppError::Timeout { .. }));
    assert!(err.source().is_some());
}

#[test]
fn selector_into_bounds() {
    // NotFound selector field `path` has type String, but accepts &str via Into.
    let err = NotFound { path: "abc" }.build();
    assert!(matches!(err, AppError::NotFound { ref path } if path == "abc"));
}

#[test]
fn multi_field_selector() {
    let err = Config {
        host: "localhost",
        port: 8080u16,
    }
    .build();
    match &err {
        AppError::Config { host, port } => {
            assert_eq!(host, "localhost");
            assert_eq!(*port, 8080);
        }
        other => panic!("expected Config, got {other:?}"),
    }
}

#[test]
fn struct_leaf_build_fail() {
    // ParseError uses #[oopsie(suffix)], so selector is ParseErrorOopsie.
    let err = ParseErrorOopsie {
        msg: "unexpected token",
    }
    .build();
    assert_eq!(err.msg, "unexpected token");

    let result: Result<(), ParseError> = ParseErrorOopsie { msg: "bad" }.fail();
    assert!(result.is_err());
}

#[test]
fn struct_source_into_error() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "file missing");
    let err: WrapError = WrapErrorOopsie {
        detail: "while reading",
    }
    .into_error(io_err);
    assert_eq!(err.detail, "while reading");
    assert!(err.source().is_some());
    assert_eq!(err.source().unwrap().to_string(), "file missing");
}

#[test]
fn leaf_selector_into_error_none_error() {
    // Leaf selectors implement IntoError<E, Source=NoneError> for OptionExt support.
    let err: AppError = NotFound { path: "x" }.into_error(NoneError);
    assert!(matches!(err, AppError::NotFound { ref path } if path == "x"));
}
