#![cfg(feature = "fancy")]
#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::all,
    reason = "integration test fixtures intentionally trip style lints"
)]

mod common;

use std::fmt;
use std::process::Termination as _;

use oopsie::trace_printer::{BacktraceFrame, BacktraceProvider, TracePrinter};
use oopsie::{
    Contextual as _, Report, RustBacktrace, Theme, get_theme, oopsie, set_theme,
};
use oopsie_core::{redact, snap_name};

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
#[test_with::env(OOPSIE_BACKTRACE_SNAPSHOT_TESTS)]
fn test_report_basic() {
    common::force_backtrace();
    assert_eq!(
        oopsie::backtrace::current(),
        RustBacktrace::Enabled,
        "backtrace override should be enabled for deterministic snapshots"
    );
    let error = TestOopsie {
        message: "something failed",
    }
    .build();
    let report = Report::new(error).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_basic"), report);
    });
}

#[test]
#[test_with::env(OOPSIE_BACKTRACE_SNAPSHOT_TESTS)]
fn test_report_chain() {
    common::force_backtrace();
    let inner = TestOopsie {
        message: "root cause",
    }
    .build();
    let outer: OuterError = OuterOopsie.build_error(inner);
    let report = Report::new(outer).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_chain"), report);
    });
}

#[test]
#[test_with::env(OOPSIE_BACKTRACE_SNAPSHOT_TESTS)]
fn test_report_colored() {
    common::force_backtrace();
    let error = TestOopsie {
        message: "colored test",
    }
    .build();
    let report = Report::new(error).force_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_colored_stripped"), report);
    });
}

/// The colored render path must actually emit ANSI escapes. `test_report_colored`
/// strips ANSI *before* snapshotting, so its snapshot is byte-identical to the
/// plain one — a regression that silently dropped all styling would still pass.
/// This is the positive counterpart to `test_no_colors_never_no_ansi`: it pins
/// that `force_colors()` both colorizes (escapes present, incl. the specific red
/// header SGR) and leaves the rendered text intact when the escapes are stripped.
#[test]
fn test_report_colored_emits_ansi() {
    common::force_backtrace();
    let error = TestOopsie {
        message: "colored test",
    }
    .build();
    let output = Report::new(error).force_colors().to_string();

    assert!(
        output.contains('\u{1b}'),
        "force_colors() output should contain ANSI escapes, got: {output:?}"
    );
    assert!(
        output.contains("\u{1b}[38;2;243;139;168;1m"),
        "expected the bold Catppuccin-red `Error` header in colored output, got: {output:?}"
    );
    assert!(
        strip_ansi(&output).contains("Error[report::TestError]: Test error: colored test"),
        "stripping ANSI must leave the rendered text intact"
    );
}

/// A per-report `with_theme` override must reach the rendered colors. Rendering
/// one report (one backtrace) two ways isolates the theme as the only variable.
#[test]
fn report_theme_override_changes_output() {
    let base = Report::from_std(TestOopsie { message: "themed" }.build()).force_colors();
    let default_render = base.to_string();
    let nord_render = base.with_theme(Theme::NORD).to_string();

    assert_ne!(
        default_render, nord_render,
        "with_theme(NORD) must change the colored output vs the global default"
    );
}

/// A report with no override must follow the process-global theme set by
/// `set_theme`. Re-render the same report under two globals; only color differs.
#[test]
fn report_follows_global_theme() {
    let report = Report::from_std(TestOopsie { message: "themed" }.build()).force_colors();

    let original = get_theme();
    set_theme(Theme::NORD);
    let nord_render = report.to_string();
    set_theme(Theme::CATPPUCCIN_MOCHA);
    let mocha_render = report.to_string();
    set_theme(original);

    assert_ne!(
        nord_render, mocha_render,
        "set_theme must change a report that carries no per-report override"
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
#[test_with::env(OOPSIE_BACKTRACE_SNAPSHOT_TESTS)]
fn test_report_with_help() {
    common::force_backtrace();
    let error = ErrorWithHelpOopsie {
        message: "connection refused",
    }
    .build();
    let report = Report::new(error).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_with_help"), report.to_string());
    });
}

#[cfg(feature = "tracing")]
#[test]
#[test_with::env(OOPSIE_BACKTRACE_SNAPSHOT_TESTS)]
fn test_report_with_spantrace() {
    let error = common::make_error();
    let report = Report::new(error).no_colors();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("report_with_spantrace"), report);
    });
}

#[cfg(feature = "tracing")]
#[test]
#[test_with::env(OOPSIE_BACKTRACE_SNAPSHOT_TESTS)]
fn test_report_with_spantrace_debug() {
    let error = common::make_error();

    redact!(backtrace, {
        insta::assert_snapshot!(
            snap_name!("report_with_spantrace_debug"),
            format!("{error:#}")
        );
    });
}

#[cfg(feature = "tracing")]
#[test]
fn test_report_colored_spantrace_renders_frames() {
    let error = common::make_error();
    let raw = Report::new(error).force_colors().to_string();
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
        span_section.contains("\u{1b}[38;2;243;139;168m"),
        "span frame names should be styled with the function-name color"
    );
}

// --- Accessor method tests ---

#[test]
fn test_error_returns_some_when_err() {
    let error = TestOopsie {
        message: "accessor test",
    }
    .build();
    let report = Report::new(error);
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
    let report = Report::new(error);
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
    let report = Report::new(error).no_colors();
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
    let _code = Report::new(error).no_colors().report();
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

#[test]
fn run_restores_prior_hook_even_on_unwind() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static PRIOR_HOOK_FIRED: AtomicBool = AtomicBool::new(false);

    std::panic::set_hook(Box::new(|_| {
        PRIOR_HOOK_FIRED.store(true, Ordering::SeqCst);
    }));

    let _ = std::panic::catch_unwind(|| {
        let _report: Report<TestError> = Report::run(|| panic!("boom"));
    });

    PRIOR_HOOK_FIRED.store(false, Ordering::SeqCst);
    let _ = std::panic::catch_unwind(|| panic!("again"));
    assert!(PRIOR_HOOK_FIRED.load(Ordering::SeqCst));
}

#[test]
fn run_concurrent_overlap_restores_prior_hook_after_last_exit() {
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicBool, Ordering};
    static PRIOR_HOOK_FIRED: AtomicBool = AtomicBool::new(false);

    std::panic::set_hook(Box::new(|_| {
        PRIOR_HOOK_FIRED.store(true, Ordering::SeqCst);
    }));

    // Both threads inside run() simultaneously, then staggered exits:
    // thread A leaves run() fully before thread B's closure returns.
    let both_inside = Barrier::new(2);
    let a_exited = Barrier::new(2);
    std::thread::scope(|s| {
        s.spawn(|| {
            let _report: Report<TestError> = Report::run(|| {
                both_inside.wait();
                Ok(())
            });
            a_exited.wait();
        });
        s.spawn(|| {
            let _report: Report<TestError> = Report::run(|| {
                both_inside.wait();
                a_exited.wait();
                Ok(())
            });
        });
    });

    let _ = std::panic::catch_unwind(|| panic!("probe"));
    assert!(
        PRIOR_HOOK_FIRED.load(Ordering::SeqCst),
        "prior hook must be restored after the last overlapping run exits"
    );
}

#[test]
fn run_nested_restores_prior_hook() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static PRIOR_HOOK_FIRED: AtomicBool = AtomicBool::new(false);

    std::panic::set_hook(Box::new(|_| {
        PRIOR_HOOK_FIRED.store(true, Ordering::SeqCst);
    }));

    let _outer: Report<TestError> = Report::run(|| {
        let inner: Report<TestError> = Report::run(|| Ok(()));
        assert!(inner.error().is_none());
        Ok(())
    });

    let _ = std::panic::catch_unwind(|| panic!("probe"));
    assert!(PRIOR_HOOK_FIRED.load(Ordering::SeqCst));
}

// --- Report::no_colors() test ---

#[test]
fn test_no_colors_never_no_ansi() {
    let error = TestOopsie {
        message: "no_colors test",
    }
    .build();
    let report = Report::new(error).no_colors();
    let output = report.to_string();
    assert!(output.contains("no_colors test"));
    // No ANSI escape codes when color is disabled
    assert!(!output.contains("\x1b["));
}

// ─────────────────────────────────────────────────────────────────────────────
// TracePrinter synthetic-backtrace tests
//
// `TracePrinter::write_backtrace` takes a `&impl BacktraceProvider`, so a
// hand-built provider lets us pin down rendering of frame shapes that a real
// captured backtrace can't deterministically produce: a non-`None` column
// number, and a frame list whose top/bottom the default filter actually trims
// (yielding a non-zero hidden count).
// ─────────────────────────────────────────────────────────────────────────────

/// A `BacktraceProvider` over a fixed list of frames, so rendering is fully
/// deterministic and independent of the real call stack.
struct FixedFrames(Vec<BacktraceFrame>);

impl BacktraceProvider for FixedFrames {
    fn frames(&self) -> Vec<BacktraceFrame> {
        self.0.iter().map(frame_clone).collect()
    }
}

fn frame(name: &str, lineno: Option<u32>, colno: Option<u32>) -> BacktraceFrame {
    BacktraceFrame {
        ip: 0,
        name: Some(name.into()),
        filename: Some(std::path::Path::new("src/lib.rs").into()),
        lineno,
        colno,
    }
}

fn frame_clone(f: &BacktraceFrame) -> BacktraceFrame {
    BacktraceFrame {
        ip: f.ip,
        name: f.name.clone(),
        filename: f.filename.clone(),
        lineno: f.lineno,
        colno: f.colno,
    }
}

/// Adapts a `TracePrinter` + provider into a `Display` so we can drive the
/// `fmt::Formatter`-based `write_backtrace` from a test and capture its output.
struct RenderBacktrace<'a, P>(&'a TracePrinter, &'a P);

impl<P: BacktraceProvider> fmt::Display for RenderBacktrace<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.write_backtrace(f, self.1)
    }
}

fn render_backtrace<P: BacktraceProvider>(printer: &TracePrinter, provider: &P) -> String {
    RenderBacktrace(printer, provider).to_string()
}

/// The default filter trims backtrace-capture frames off the top and
/// runtime-init frames off the bottom; the trimmed count must surface as a
/// "... N frames hidden ..." notice.
#[test]
fn test_backtrace_hidden_frame_count_message() {
    // Two capture frames (top) + two app frames + one runtime frame (bottom).
    // The default filter removes 3, leaving the 2 app frames.
    let provider = FixedFrames(vec![
        frame("std::backtrace_rs::backtrace::libunwind::trace", None, None),
        frame("<std::backtrace::Backtrace>::create::inner", None, None),
        frame("my_crate::function_a", Some(10), None),
        frame("my_crate::function_b", Some(20), None),
        frame("std::rt::lang_start_internal::invoke", None, None),
    ]);

    let printer = TracePrinter::new().plain();
    let output = render_backtrace(&printer, &provider);

    // Two capture frames trim off the top, one runtime frame off the bottom;
    // each notice must render at the end it was trimmed from.
    let top_notice = "... 2 frames hidden ...";
    let bottom_notice = "... 1 frames hidden ...";
    assert!(
        output.contains(top_notice),
        "expected top hidden-frame notice (count 2), got:\n{output}"
    );
    assert!(
        output.contains(bottom_notice),
        "expected bottom hidden-frame notice (count 1), got:\n{output}"
    );
    assert!(output.contains("my_crate::function_a"));
    assert!(output.contains("my_crate::function_b"));
    assert!(
        output.find(top_notice).unwrap() < output.find("my_crate::function_a").unwrap(),
        "top notice must precede the first kept frame, got:\n{output}"
    );
    assert!(
        output.rfind(bottom_notice).unwrap() > output.find("my_crate::function_b").unwrap(),
        "bottom notice must follow the last kept frame, got:\n{output}"
    );
    assert!(
        !output.contains("lang_start_internal"),
        "runtime-init frame should be filtered out"
    );
}

/// When nothing is trimmed the notice must not appear.
#[test]
fn test_backtrace_no_hidden_frames_no_message() {
    let provider = FixedFrames(vec![
        frame("my_crate::function_a", Some(10), None),
        frame("my_crate::function_b", Some(20), None),
    ]);

    let printer = TracePrinter::new().plain();
    let output = render_backtrace(&printer, &provider);

    assert!(
        !output.contains("frames hidden"),
        "no frames were trimmed, so no notice should render, got:\n{output}"
    );
}

/// A frame carrying a column number renders `:lineno:colno`.
#[test]
fn test_backtrace_frame_renders_colno() {
    let provider = FixedFrames(vec![frame("my_crate::function_a", Some(42), Some(7))]);

    let printer = TracePrinter::new().plain();
    let output = render_backtrace(&printer, &provider);

    assert!(
        output.contains("src/lib.rs:42:7"),
        "expected line:col `:42:7` in output, got:\n{output}"
    );
}

/// With no column number only `:lineno` is rendered, never a trailing `:`.
#[test]
fn test_backtrace_frame_no_colno_renders_only_lineno() {
    let provider = FixedFrames(vec![frame("my_crate::function_a", Some(42), None)]);

    let printer = TracePrinter::new().plain();
    let output = render_backtrace(&printer, &provider);

    assert!(
        output.contains("src/lib.rs:42"),
        "expected `:42` in output, got:\n{output}"
    );
    assert!(
        !output.contains("src/lib.rs:42:"),
        "no column number means no trailing `:`, got:\n{output}"
    );
}

/// `with_filter` installs a fully custom filter, replacing the default.
#[test]
fn test_trace_printer_with_filter_custom_filter() {
    let provider = FixedFrames(vec![
        frame("keep::alpha", Some(1), None),
        frame("drop::beta", Some(2), None),
        frame("keep::gamma", Some(3), None),
    ]);

    // Custom filter: drop any frame whose name starts with "drop::".
    let printer = TracePrinter::with_filter(|frames| {
        for slot in frames.iter_mut() {
            if slot.is_some_and(|frame| {
                frame
                    .name
                    .as_deref()
                    .is_some_and(|name| name.starts_with("drop::"))
            }) {
                *slot = None;
            }
        }
    });
    let output = render_backtrace(&printer, &provider);

    assert!(output.contains("keep::alpha"), "got:\n{output}");
    assert!(output.contains("keep::gamma"), "got:\n{output}");
    assert!(
        !output.contains("drop::beta"),
        "custom filter should remove `drop::beta`, got:\n{output}"
    );
    // One of three frames removed -> hidden notice with count 1, rendered
    // at the gap's position, between the two kept frames.
    let alpha = output.find("keep::alpha").unwrap();
    let notice = output.find("... 1 frames hidden ...").unwrap();
    let gamma = output.find("keep::gamma").unwrap();
    assert!(
        alpha < notice && notice < gamma,
        "notice must sit at the gap, got:\n{output}"
    );
}

/// `add_frame_filter` composes ON TOP of the existing filter — both the
/// default (capture/runtime trimming) and the added predicate apply in sequence.
#[test]
fn test_trace_printer_add_frame_filter_composes() {
    let provider = FixedFrames(vec![
        frame("std::backtrace_rs::backtrace::libunwind::trace", None, None),
        frame("keep::alpha", Some(1), None),
        frame("drop::beta", Some(2), None),
        frame("std::rt::lang_start_internal::invoke", None, None),
    ]);

    // Start from the default filter (which trims the capture + runtime frames),
    // then add a second filter removing `drop::` frames.
    let printer = TracePrinter::new().plain().add_frame_filter(|frames| {
        for slot in frames.iter_mut() {
            if slot.is_some_and(|frame| {
                frame
                    .name
                    .as_deref()
                    .is_some_and(|name| name.starts_with("drop::"))
            }) {
                *slot = None;
            }
        }
    });
    let output = render_backtrace(&printer, &provider);

    assert!(output.contains("keep::alpha"), "got:\n{output}");
    assert!(
        !output.contains("libunwind"),
        "default filter (under) should still trim capture frames, got:\n{output}"
    );
    assert!(
        !output.contains("lang_start_internal"),
        "default filter (under) should still trim runtime frames, got:\n{output}"
    );
    assert!(
        !output.contains("drop::beta"),
        "added filter (over) should remove `drop::beta`, got:\n{output}"
    );
    // 3 removed: 1 capture frame off the top, `drop::beta` + runtime off the bottom.
    assert!(
        output.contains("... 1 frames hidden ..."),
        "expected top notice (count 1), got:\n{output}"
    );
    assert!(
        output.contains("... 2 frames hidden ..."),
        "expected bottom notice (count 2), got:\n{output}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// RUST_BACKTRACE=full unfiltered rendering
// ─────────────────────────────────────────────────────────────────────────────

/// `TracePrinter::unfiltered()` keeps every frame — including the
/// backtrace-capture and runtime-init frames that `TracePrinter::new()` (the
/// default filter) would trim — so it never emits a "frames hidden" notice.
/// This is the deterministic counterpart to the integration test below, which
/// cannot rely on a real captured stack matching the filter's prefixes.
#[test]
fn test_trace_printer_unfiltered_keeps_all_frames() {
    let frames = vec![
        frame("std::backtrace_rs::backtrace::libunwind::trace", None, None),
        frame("my_crate::function_a", Some(10), None),
        frame("std::rt::lang_start_internal::invoke", None, None),
    ];

    let unfiltered_out = render_backtrace(
        &TracePrinter::unfiltered().plain(),
        &FixedFrames(frames.iter().map(frame_clone).collect()),
    );
    let filtered_out = render_backtrace(
        &TracePrinter::new().plain(),
        &FixedFrames(frames.iter().map(frame_clone).collect()),
    );

    // Unfiltered keeps the capture + runtime frames the default filter removes.
    assert!(
        unfiltered_out.contains("libunwind"),
        "got:\n{unfiltered_out}"
    );
    assert!(
        unfiltered_out.contains("lang_start_internal"),
        "got:\n{unfiltered_out}"
    );
    assert!(
        !unfiltered_out.contains("frames hidden"),
        "unfiltered must never report hidden frames, got:\n{unfiltered_out}"
    );

    // The default filter drops one frame off each end, as two separate notices.
    assert!(!filtered_out.contains("libunwind"), "got:\n{filtered_out}");
    assert_eq!(
        filtered_out.matches("frames hidden").count(),
        2,
        "expected a top and a bottom hidden-frames notice, got:\n{filtered_out}"
    );
    assert!(
        filtered_out.contains("... 1 frames hidden ..."),
        "got:\n{filtered_out}"
    );
    assert!(unfiltered_out.len() > filtered_out.len());
}

/// With the effective backtrace setting forced to `Full`,
/// `Report` routes through `TracePrinter::unfiltered()`, so its backtrace render
/// never carries a "frames hidden" notice.
///
/// The override is thread-local (set via `backtrace::set_override`), so this
/// is safe under test parallelism; we restore `Enabled` before returning.
///
/// NOTE: we cannot assert "Full is longer than the filtered render" here. The
/// default filter only trims frames whose names match its hardcoded
/// capture/runtime prefixes, and under the nextest harness the real captured
/// stack matches none of them (capture frames go through `oopsie_core`'s
/// `Capturable`, and the bottom frames are the test-harness thread, not
/// `main`/`lang_start`). So for a *real* error the filtered and unfiltered
/// renders are identical — the length difference only shows on synthetic frames
/// (see `test_trace_printer_unfiltered_keeps_all_frames`).
#[test]
fn test_report_backtrace_full_renders_without_hidden_notice() {
    common::force_backtrace();

    oopsie::backtrace::set_override(RustBacktrace::Full);
    let error = TestOopsie {
        message: "full backtrace",
    }
    .build();
    let output = Report::new(error).no_colors().to_string();
    oopsie::backtrace::set_override(RustBacktrace::Enabled);

    assert!(
        output.contains("BACKTRACE"),
        "Full mode should still render a backtrace section, got:\n{output}"
    );
    assert!(
        !output.contains("frames hidden"),
        "unfiltered (Full) render must not emit a hidden-frames notice, got:\n{output}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Report FromResidual — `?` in a fn returning Report<E>
// Requires the nightly `unstable-try-trait-v2` feature.
// ─────────────────────────────────────────────────────────────────────────────

/// The `?` operator works directly in a function returning `Report<E>`
/// via the `FromResidual` impl. An `Err` short-circuits into a `Report` carrying
/// the error; an `Ok` flows through to the explicit return.
#[cfg(feature = "unstable-try-trait-v2")]
#[test]
fn test_report_from_residual_question_mark() {
    fn fallible(fail: bool) -> Result<u8, TestError> {
        if fail {
            TestOopsie {
                message: "residual failure",
            }
            .fail()
        } else {
            Ok(7)
        }
    }

    // `?` on an `Err` short-circuits the function, converting the residual into a
    // `Report` via `FromResidual`.
    fn run(fail: bool) -> Report<TestError> {
        let value = fallible(fail)?;
        assert_eq!(value, 7);
        Report::ok()
    }

    common::force_backtrace();

    let err_report = run(true);
    assert!(err_report.error().is_some());
    assert!(
        err_report
            .into_error()
            .unwrap()
            .to_string()
            .contains("residual failure")
    );

    let ok_report = run(false);
    assert!(ok_report.error().is_none());
    assert!(ok_report.to_string().is_empty());
}

// ─── transparent wrapper surfaces a leaf's forwarded code + help ───
//
// Kept at the end of the file: the colored-report snapshots above capture
// unredacted source line numbers, so inserting fixtures earlier would shift
// them.

#[derive(Debug, oopsie::Oopsie)]
#[oopsie(module(false))]
#[oopsie(
    display("leaf failed: {what}"),
    code = "leaf::failed",
    help = "turn it off and on again"
)]
pub struct LeafError {
    what: String,
}

#[derive(Debug, oopsie::Oopsie)]
#[oopsie(module(false))]
#[oopsie(transparent)]
pub struct TransparentRootError {
    source: LeafError,
}

#[test]
fn test_report_transparent_forwards_code_and_help() {
    // A transparent root forwards the leaf's code + help to the top level, so
    // `Report` (which reads only the top error) now renders them. The leaf has
    // no backtrace, so the output is deterministic across stable/nightly.
    //
    // Per the no-renderer-change decision, the delegated headline equals the
    // immediate source's message, so it shows both as the headline and as the
    // first `╰─▶` chain entry.
    let leaf = LeafOopsie { what: "disk" }.build();
    let root: TransparentRootError = TransparentRootError::from(leaf);
    let report = Report::new(root).no_colors();
    let rendered = strip_ansi(&report.to_string());

    assert_eq!(
        rendered,
        "\
Error[leaf::failed]: leaf failed: disk
  ╰─▶ leaf failed: disk

  help: turn it off and on again
"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Cyclic source-chain safety
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Cyclic;

impl std::fmt::Display for Cyclic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cyclic")
    }
}

impl std::error::Error for Cyclic {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self)
    }
}

impl oopsie::Diagnostic for Cyclic {}

#[test]
fn cyclic_source_chain_terminates_with_truncation_note() {
    let rendered = oopsie::Report::new(Cyclic).no_colors().to_string();
    assert!(rendered.contains("source chain truncated"), "{rendered}");
}

#[derive(Debug)]
struct PlainWrapper(TestError);

impl fmt::Display for PlainWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("plain wrapper")
    }
}

impl std::error::Error for PlainWrapper {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl oopsie::Diagnostic for PlainWrapper {}

/// A hand-written wrapper with no `Diagnostic` accessors over a traced source:
/// per the `Report` docs, no BACKTRACE/SPANTRACE section may render even though
/// the source carries a trace.
#[test]
fn report_does_not_search_chain_for_traces() {
    common::force_backtrace();

    let traced = TestOopsie { message: "root" }.build();
    let rendered = Report::new(PlainWrapper(traced)).no_colors().to_string();

    assert!(rendered.contains("╰─▶"), "chain messages still render");
    assert!(
        !rendered.contains("BACKTRACE"),
        "no chain search for traces:\n{rendered}"
    );
}
