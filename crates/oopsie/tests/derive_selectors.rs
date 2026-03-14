#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(unused, clippy::all)]

use oopsie::{Contextual as _, NoSource, Oopsie};
use std::error::Error as _;
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
fn source_enum_build_error() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
    let err: AppError = IoFailed {
        context: "reading config",
    }
    .build_error(io_err);
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
    let err: AppError = Timeout.build_error(io_err);
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
    // ParseError uses #[oopsie(suffix)], so selector is ParseOopsie ("Error" stripped).
    let err = ParseOopsie {
        msg: "unexpected token",
    }
    .build();
    assert_eq!(err.msg, "unexpected token");

    let result: Result<(), ParseError> = ParseOopsie { msg: "bad" }.fail();
    assert!(result.is_err());
}

#[test]
fn struct_source_build_error() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "file missing");
    let err: WrapError = WrapOopsie {
        detail: "while reading",
    }
    .build_error(io_err);
    assert_eq!(err.detail, "while reading");
    assert!(err.source().is_some());
    assert_eq!(err.source().unwrap().to_string(), "file missing");
}

// ---- Error suffix stripping ----

/// Variant names ending in "Error" get the suffix stripped in selector names.
#[derive(Debug, Oopsie)]
#[oopsie(module(strip_oopsies))]
enum ServiceError {
    #[oopsie("connection error")]
    ConnectionError { addr: String },

    #[oopsie("timed out")]
    TimeoutError,

    #[oopsie("ok variant")]
    NotAnIssue { msg: String },
}

#[test]
fn error_suffix_stripped_from_selector() {
    // ConnectionError variant → selector is `Connection` (not `ConnectionError`)
    let err = strip_oopsies::Connection { addr: "localhost" }.build();
    assert!(matches!(err, ServiceError::ConnectionError { .. }));
    assert_eq!(err.to_string(), "connection error");

    // TimeoutError variant → selector is `Timeout` (not `TimeoutError`)
    let err = strip_oopsies::Timeout.build();
    assert!(matches!(err, ServiceError::TimeoutError));

    // NotAnIssue variant → selector stays `NotAnIssue` (no "Error" suffix to strip)
    let err = strip_oopsies::NotAnIssue { msg: "fine" }.build();
    assert!(matches!(err, ServiceError::NotAnIssue { .. }));
}

#[derive(Debug, Oopsie)]
#[oopsie(module(suffix_strip_oopsies), suffix)]
enum SuffixStripError {
    #[oopsie("conn failed")]
    ConnectionError { addr: String },
}

#[test]
fn error_suffix_stripped_with_oopsie_suffix() {
    // ConnectionError + suffix → `ConnectionOopsie` (not `ConnectionErrorOopsie`)
    let err = suffix_strip_oopsies::ConnectionOopsie { addr: "db" }.build();
    assert!(matches!(err, SuffixStripError::ConnectionError { .. }));
}

#[test]
fn leaf_selector_build_error_no_source() {
    // Leaf selectors implement Contextual<E, Source=NoSource> for OptionExt support.
    let err: AppError = NotFound { path: "x" }.build_error(NoSource);
    assert!(matches!(err, AppError::NotFound { ref path } if path == "x"));
}
