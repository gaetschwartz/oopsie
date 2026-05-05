#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(clippy::all)]

mod common;

use std::process::Termination as _;

use oopsie::{Contextual as _, Oopsie, Report, traced};

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
fn test_report_basic() {
    let error = TestOopsie {
        message: "something failed",
    }
    .build();
    let report = Report::from_std(error).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_basic"), report.to_string());
    });
}

#[test]
fn test_report_chain() {
    let inner = TestOopsie {
        message: "root cause",
    }
    .build();
    let outer: OuterError = OuterOopsie.build_error(inner);
    let report = Report::from_std(outer).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_chain"), report.to_string());
    });
}

#[test]
fn test_report_colored() {
    let error = TestOopsie {
        message: "colored test",
    }
    .build();
    let report = Report::from_std(error).force_colors();

    let output = strip_ansi(&report.to_string());
    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_colored_stripped"), output);
    });
}

#[test]
fn test_report_from() {
    let error = TestOopsie {
        message: "from test",
    }
    .build();
    let report: Report<_> = error.into();
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
fn test_report_with_help() {
    let error = ErrorWithHelpOopsie {
        message: "connection refused",
    }
    .build();
    let report = Report::from_std(error).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_with_help"), report.to_string());
    });
}

#[test]
fn test_report_with_spantrace() {
    let error = common::make_error();
    let report = Report::from_std(error).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_with_spantrace"), report);
    });
}

#[test]
fn test_report_with_spantrace_debug() {
    let error = common::make_error();

    redact!(backtrace, {
        insta::assert_snapshot!(
            snap_name!("report_with_spantrace_debug"),
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
    let report = Report::from_std(error);
    assert!(report.error().is_some());
}

#[test]
fn test_error_returns_none_when_ok() {
    let report = Report::<TestError>::ok();
    assert!(report.error().is_none());
}

#[test]
fn test_into_error_returns_some_when_err() {
    let error = TestOopsie {
        message: "into_error test",
    }
    .build();
    let report = Report::from_std(error);
    let err = report.into_error();
    assert!(err.is_some());
    assert!(err.unwrap().to_string().contains("into_error test"));
}

#[test]
fn test_into_error_returns_none_when_ok() {
    let report = Report::<TestError>::ok();
    assert!(report.into_error().is_none());
}

// --- Debug/Display tests ---

#[test]
fn test_debug_fmt_non_empty() {
    let error = TestOopsie {
        message: "debug test",
    }
    .build();
    let report = Report::from_std(error).no_colors();
    let debug_output = format!("{report:?}");
    assert!(!debug_output.is_empty());
    assert!(debug_output.contains("debug test"));
}

#[test]
fn test_display_ok_is_empty() {
    let report = Report::<TestError>::ok();
    let output = report.to_string();
    assert!(output.is_empty());
}

// --- Termination::report() tests ---

#[test]
fn test_termination_report_ok() {
    let _code = Report::<TestError>::ok().report();
}

#[test]
fn test_termination_report_error() {
    let error = TestOopsie {
        message: "termination test",
    }
    .build();
    let _code = Report::from_std(error).no_colors().report();
}

// --- Report::run() tests ---

#[test]
fn test_report_run_ok() {
    let report = Report::<TestError>::run(|| Ok(()));
    assert!(report.error().is_none());
    assert!(report.to_string().is_empty());
}

#[test]
fn test_report_run_err() {
    let report = Report::run(|| {
        Err(TestOopsie {
            message: "run failed",
        }
        .build())
    });
    assert!(report.error().is_some());
    assert!(report.to_string().contains("run failed"));
}

// --- Report::with_colors() test ---

#[test]
fn test_with_colors_never_no_ansi() {
    let error = TestOopsie {
        message: "with_colors test",
    }
    .build();
    let report = Report::with_colors(error, oopsie::ColorConfig::Never);
    let output = report.to_string();
    assert!(output.contains("with_colors test"));
    // No ANSI escape codes when color is disabled
    assert!(!output.contains("\x1b["));
}
