#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(clippy::all)]

mod common;

use erased_oopsie::ErasedError;
use oopsie::{Oopsie, traced};

#[traced]
#[derive(Debug, Oopsie)]
#[oopsie("Something went wrong: {message}")]
#[oopsie(help = "Try restarting the service")]
pub struct ErrorWithHelp {
    message: String,
}

#[traced]
#[derive(Debug, Oopsie)]
#[oopsie("Code-only error: {message}")]
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
    redact!(json, {
        insta::assert_json_snapshot!(snap_name!("erased_error_json"), error);
    });
}

#[test]
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
fn test_extract_backtrace_returns_some_when_provided() {
    let error = common::make_error();
    let bt = oopsie::BackTrace::extract_from_error(&error);
    assert!(
        bt.is_some(),
        "extract_backtrace should return Some for oopsie errors"
    );
}

#[test]
fn test_extract_error_code_returns_some_for_oopsie_errors() {
    let error = ErrorWithHelpOopsie { message: "test" }.build();
    let erased = ErasedError::from_error(error);
    assert!(
        erased.diagnostics.code().is_some(),
        "oopsie errors with help should have an error code"
    );
}

// --- Diagnostics::is_none() ---

#[test]
fn test_diagnostics_is_none_for_default() {
    let diag = erased_oopsie::Diagnostics::default();
    assert!(
        diag.is_none(),
        "default Diagnostics should have no code or help"
    );
}

#[test]
fn test_diagnostics_is_not_none_when_code_present() {
    let error = ErrorWithCodeOnlyOopsie {
        message: "has code",
    }
    .build();
    let erased = ErasedError::from_error(error);
    assert!(
        !erased.diagnostics.is_none(),
        "Diagnostics with a code should not be is_none()"
    );
}

// --- write_text ---

#[test]
fn test_write_text_contains_message_and_help() {
    let error = ErrorWithHelpOopsie {
        message: "text output test",
    }
    .build();
    let erased = ErasedError::from_error(error);
    let mut buf = Vec::new();
    erased.write_text(&mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    assert!(
        text.contains("text output test"),
        "write_text should include the error message"
    );
    assert!(
        text.contains("Try restarting the service"),
        "write_text should include help text"
    );
}

// --- write_json ---

#[test]
fn test_write_json_output_is_valid_json_with_expected_fields() {
    let error = ErrorWithHelpOopsie {
        message: "json output test",
    }
    .build();
    let erased = ErasedError::from_error(error);
    let mut buf = Vec::new();
    erased.write_json(&mut buf).unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&buf).expect("write_json should produce valid JSON");
    assert_eq!(
        json["message"], "Something went wrong: json output test",
        "JSON message field should match Display"
    );
    assert_eq!(
        json["diagnostics"]["help"], "Try restarting the service",
        "JSON diagnostics.help should match oopsie help attribute"
    );
}
