#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::all,
    reason = "integration test fixtures intentionally trip style lints"
)]

mod common;

use std::process::Termination as _;

use oopsie::{Contextual as _, Report, oopsie};

#[oopsie(traced)]
#[oopsie("Test error: {message}")]
pub struct TestError {
    message: String,
}

#[oopsie(traced)]
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
    common::force_backtrace();
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
    common::force_backtrace();
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
    common::force_backtrace();
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

/// The colored render path must actually emit ANSI escapes. `test_report_colored`
/// strips ANSI *before* snapshotting, so its snapshot is byte-identical to the
/// plain one — a regression that silently dropped all styling would still pass.
/// This is the positive counterpart to `test_with_colors_never_no_ansi`: it pins
/// that `force_colors()` both colorizes (escapes present, incl. the specific red
/// header SGR) and leaves the rendered text intact when the escapes are stripped.
#[test]
fn test_report_colored_emits_ansi() {
    common::force_backtrace();
    let error = TestOopsie {
        message: "colored test",
    }
    .build();
    let output = Report::from_std(error).force_colors().to_string();

    assert!(
        output.contains('\u{1b}'),
        "force_colors() output should contain ANSI escapes, got: {output:?}"
    );
    assert!(
        output.contains("\u{1b}[31m"),
        "expected the red (SGR 31) `Error` header in colored output, got: {output:?}"
    );
    assert!(
        strip_ansi(&output).contains("Error[report::TestError]: Test error: colored test"),
        "stripping ANSI must leave the rendered text intact"
    );
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

#[oopsie(traced)]
#[oopsie("Something went wrong: {message}")]
#[oopsie(help = "Try restarting the service")]
pub struct ErrorWithHelp {
    message: String,
}

#[test]
fn test_report_with_help() {
    common::force_backtrace();
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

/// The colored span renderer (`TracePrinter::write_span_frame`) is only reached
/// when a report both carries spans *and* is colorized. Every existing spantrace
/// test renders `.no_colors()`, which routes through core's `Display` instead, so
/// `write_span_frame` was never exercised. The two renderers are distinguishable:
/// `write_span_frame` numbers spans 1-based (`index` starts at 1) while core's
/// `Display` numbers them 0-based — so the lines asserted below can *only* be
/// produced by the colored path. We also confirm the span section is actually
/// colorized (bright-red SGR 91 frame names), isolated from the backtrace.
#[test]
fn test_report_colored_spantrace_renders_frames() {
    let error = common::make_error();
    let raw = Report::from_std(error).force_colors().to_string();
    let stripped = strip_ansi(&raw);

    assert!(
        stripped.contains("1: sys::inner_function"),
        "colored path renders 1-based span frames; got:\n{stripped}"
    );
    assert!(
        stripped.contains("2: controller::outer_function"),
        "colored path renders 1-based span frames; got:\n{stripped}"
    );
    assert!(
        stripped.contains("with ") && stripped.contains("at "),
        "span frames should render their fields (`with`) and location (`at`)"
    );

    let span_start = raw
        .find("SPANTRACE")
        .expect("colored output has a SPANTRACE header");
    let after = &raw[span_start..];
    let span_section = after.find("BACKTRACE").map_or(after, |i| &after[..i]);
    assert!(
        span_section.contains("\u{1b}[91m"),
        "span frame names should be bright-red (SGR 91) styled"
    );
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
