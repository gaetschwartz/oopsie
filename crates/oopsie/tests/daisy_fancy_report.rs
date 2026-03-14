#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(clippy::all)]

mod common;

use std::process::Termination;

use oopsie::{FancyReport, IntoError as _, Oopsie, traced};

#[traced]
#[derive(Debug, Oopsie)]
#[oopsie("Test error: {message}")]
pub struct TestError {
    message: String,
}

#[traced]
#[derive(Debug, Oopsie)]
#[oopsie("Outer error")]
pub struct OuterError {
    source: TestError,
}

/// Strip ANSI escape codes for consistent snapshot testing.
fn strip_ansi(s: &str) -> String {
    String::from_utf8(strip_ansi_escapes::strip(s)).unwrap()
}

#[test]
fn test_fancy_report_basic() {
    let error = TestOopsie {
        message: "something failed",
    }
    .build();
    let report = FancyReport::from_std(error).no_colors();

    insta::assert_snapshot!(snap_name!("fancy_report_basic"), report.to_string());
}

#[test]
fn test_fancy_report_chain() {
    let inner = TestOopsie {
        message: "root cause",
    }
    .build();
    let outer: OuterError = OuterOopsie.into_error(inner);
    let report = FancyReport::from_std(outer).no_colors();

    insta::assert_snapshot!(snap_name!("fancy_report_chain"), report.to_string());
}

#[test]
fn test_fancy_report_colored() {
    let error = TestOopsie {
        message: "colored test",
    }
    .build();
    let report = FancyReport::from_std(error).force_colors();

    let output = strip_ansi(&report.to_string());
    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("fancy_report_colored_stripped"), output);
    });
}

#[test]
fn test_fancy_report_from() {
    let error = TestOopsie {
        message: "from test",
    }
    .build();
    let report: FancyReport<_> = error.into();
    assert!(report.to_string().contains("from test"));
}

#[traced]
#[derive(Debug, Oopsie)]
#[oopsie("Something went wrong: {message}")]
#[oopsie(help = "Try restarting the service")]
pub struct ErrorWithHelp {
    message: String,
}

#[test]
fn test_fancy_report_with_help() {
    let error = ErrorWithHelpOopsie {
        message: "connection refused",
    }
    .build();
    let report = FancyReport::from_std(error).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("fancy_report_with_help"), report.to_string());
    });
}

#[test]
fn test_fancy_report_with_spantrace() {
    let error = common::make_error();
    let report = FancyReport::from_std(error).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("fancy_report_with_spantrace"), report);
    });
}

#[test]
fn test_fancy_report_with_spantrace_debug() {
    let error = common::make_error();

    redact!(backtrace, {
        insta::assert_snapshot!(
            snap_name!("fancy_report_with_spantrace_debug"),
            format!("{error:#}")
        );
    });
}

// --- Accessor method tests ---

#[test]
fn test_error_returns_some_when_err() {
    let error = TestOopsie {
        message: "accessor test",
    }
    .build();
    let report = FancyReport::from_std(error);
    assert!(report.error().is_some());
}

#[test]
fn test_error_returns_none_when_ok() {
    let report = FancyReport::<TestError>::ok();
    assert!(report.error().is_none());
}

#[test]
fn test_into_error_returns_some_when_err() {
    let error = TestOopsie {
        message: "into_error test",
    }
    .build();
    let report = FancyReport::from_std(error);
    let err = report.into_error();
    assert!(err.is_some());
    assert!(err.unwrap().to_string().contains("into_error test"));
}

#[test]
fn test_into_error_returns_none_when_ok() {
    let report = FancyReport::<TestError>::ok();
    assert!(report.into_error().is_none());
}

// --- Debug/Display tests ---

#[test]
fn test_debug_fmt_non_empty() {
    let error = TestOopsie {
        message: "debug test",
    }
    .build();
    let report = FancyReport::from_std(error).no_colors();
    let debug_output = format!("{report:?}");
    assert!(!debug_output.is_empty());
    assert!(debug_output.contains("debug test"));
}

#[test]
fn test_display_ok_is_empty() {
    let report = FancyReport::<TestError>::ok();
    let output = report.to_string();
    assert!(output.is_empty());
}

// --- Termination::report() tests ---

#[test]
fn test_termination_report_ok() {
    let _code = FancyReport::<TestError>::ok().report();
}

#[test]
fn test_termination_report_error() {
    let error = TestOopsie {
        message: "termination test",
    }
    .build();
    let _code = FancyReport::from_std(error).no_colors().report();
}
