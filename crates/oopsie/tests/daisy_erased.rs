#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(clippy::all)]

mod common;

use oopsie::{ErasedError, Oopsie, oopsie};

#[oopsie(display = "Something went wrong: {message}")]
#[derive(Debug)]
#[oopsie(help = "Try restarting the service")]
pub struct ErrorWithHelp {
    message: String,
}

#[oopsie(display = "Code-only error: {message}")]
#[derive(Debug)]
pub struct ErrorWithCodeOnly {
    message: String,
}

#[test]
fn test_erased_error_display() {
    let error = ErasedError::from_error(common::make_error());
    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("erased_error_display"), error);
    });
}

#[test]
fn test_erased_error_json() {
    let error = ErasedError::from_error(common::make_error());
    insta::assert_json_snapshot!(
        snap_name!("erased_error_json"),
        error, {
        ".backtrace.frames" => "[..]"
    });
}

#[test]
#[cfg_attr(not(feature = "unstable"), ignore = "requires unstable provider API")]
fn test_help_extraction() {
    let error = ErrorWithHelpOopsie {
        message: "connection refused",
    }
    .build();
    let erased = ErasedError::from_error(error);

    assert_eq!(
        erased.diagnostics.help(),
        Some("Try restarting the service")
    );
    assert!(erased.diagnostics.code().is_some());
}

#[test]
#[cfg_attr(not(feature = "unstable"), ignore = "requires unstable provider API")]
fn test_code_only_extraction() {
    let error = ErrorWithCodeOnlyOopsie { message: "timeout" }.build();
    let erased = ErasedError::from_error(error);

    assert!(erased.diagnostics.code().is_some());
    assert_eq!(erased.diagnostics.help(), None);
}

#[test]
fn test_format_short_includes_help() {
    let error = ErrorWithHelpOopsie {
        message: "connection refused",
    }
    .build();
    let erased = ErasedError::from_error(error);
    let short = erased.format_short();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("format_short_with_help"), short);
    });
}

#[test]
#[cfg_attr(not(feature = "unstable"), ignore = "requires unstable provider API")]
fn test_extract_backtrace_returns_some_when_provided() {
    let error = common::make_error();
    let bt = oopsie::BackTrace::extract_from_error(&error);
    assert!(
        bt.is_some(),
        "extract_backtrace should return Some for oopsie errors"
    );
}

#[test]
#[cfg_attr(not(feature = "unstable"), ignore = "requires unstable provider API")]
fn test_extract_error_code_returns_some_for_oopsie_errors() {
    let error = ErrorWithHelpOopsie { message: "test" }.build();
    let erased = ErasedError::from_error(error);
    assert!(
        erased.diagnostics.code().is_some(),
        "oopsie errors with help should have an error code"
    );
}
