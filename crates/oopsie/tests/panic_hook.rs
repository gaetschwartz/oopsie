//! Integration tests for [`oopsie::install_panic_hook`].
//!
//! A panic hook can only be observed by actually panicking, which a normal
//! in-process test can't do without failing. So we re-exec this very test
//! binary as a child: with `OOPSIE_PANIC_HOOK_TEST_TRIGGER` set, the
//! `panic_child` test installs the hook and panics; the parent captures the
//! child's stderr and asserts on the rendered report.
#![cfg(feature = "fancy")]

use std::process::Command;

use oopsie::oopsie;

const TRIGGER_ENV: &str = "OOPSIE_PANIC_HOOK_TEST_TRIGGER";
const PANIC_MESSAGE: &str = "integration boom";

/// The child entry point. A no-op unless re-exec'd with the trigger env set, so
/// it passes silently during a normal test run.
#[test]
fn panic_child() {
    if std::env::var_os(TRIGGER_ENV).is_none() {
        return;
    }
    oopsie::install_panic_hook();
    panic!("{PANIC_MESSAGE}");
}

/// Re-exec this test binary, running only the named child test with the trigger
/// set, and return its (ANSI-stripped) stderr.
fn run_panicking_child(child_test: &str, extra_env: &[(&str, &str)]) -> String {
    let exe = std::env::current_exe().expect("locate test binary");
    let mut cmd = Command::new(exe);
    cmd.arg(child_test)
        .args(["--exact", "--nocapture", "--test-threads=1"])
        .env(TRIGGER_ENV, "1");
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("spawn child test process");

    let stderr = strip_ansi_escapes::strip(&output.stderr);
    let stderr = String::from_utf8_lossy(&stderr).into_owned();
    assert!(
        !output.status.success(),
        "child process should have panicked\n--- stderr ---\n{stderr}"
    );
    stderr
}

#[test]
fn renders_header_message_and_hint_without_backtrace() {
    let stderr = run_panicking_child("panic_child", &[("NO_COLOR", "1"), ("RUST_BACKTRACE", "0")]);

    assert!(
        stderr.contains("The application panicked (crashed)."),
        "missing panic header\n{stderr}"
    );
    assert!(
        stderr.contains(&format!("Message:  {PANIC_MESSAGE}")),
        "missing panic message\n{stderr}"
    );
    assert!(stderr.contains("Location:"), "missing location\n{stderr}");
    // With capture disabled, the hook points at the env var instead of a trace.
    assert!(
        stderr.contains("RUST_BACKTRACE=1"),
        "missing backtrace hint\n{stderr}"
    );
    // ...and renders no BACKTRACE banner (the hint mentions the var, not " BACKTRACE ").
    assert!(
        !stderr.contains(" BACKTRACE "),
        "unexpected backtrace banner\n{stderr}"
    );
}

#[test]
fn renders_backtrace_with_user_frame_on_top_and_no_panic_plumbing() {
    let stderr = run_panicking_child("panic_child", &[("NO_COLOR", "1"), ("RUST_BACKTRACE", "1")]);

    assert!(
        stderr.contains("The application panicked (crashed)."),
        "missing panic header\n{stderr}"
    );
    assert!(
        stderr.contains(" BACKTRACE "),
        "missing backtrace banner\n{stderr}"
    );
    // The panicking test function should be present as a user frame.
    assert!(
        stderr.contains("panic_child"),
        "panic site frame missing from backtrace\n{stderr}"
    );
    // The panic runtime above the call site must be trimmed.
    for plumbing in [
        "core::panicking",
        "rust_begin_unwind",
        "__rust_end_short_backtrace",
        "panic_with_hook",
    ] {
        assert!(
            !stderr.contains(plumbing),
            "panic plumbing `{plumbing}` leaked into backtrace\n{stderr}"
        );
    }
}

#[oopsie(traced)]
#[oopsie("never constructed: {message}")]
pub struct NeverError {
    message: String,
}

/// Child entry point for the `Report::run` panic path. No-op unless re-exec'd.
#[test]
fn panic_child_in_run() {
    if std::env::var_os(TRIGGER_ENV).is_none() {
        return;
    }
    let _report = oopsie::Report::run(|| -> Result<(), NeverError> {
        panic!("{PANIC_MESSAGE}");
    });
}

#[test]
fn report_run_panic_backtrace_has_no_run_or_runtime_frames() {
    let stderr = run_panicking_child(
        "panic_child_in_run",
        &[("NO_COLOR", "1"), ("RUST_BACKTRACE", "1")],
    );

    assert!(
        stderr.contains(" BACKTRACE "),
        "missing backtrace banner\n{stderr}"
    );
    assert!(
        stderr.contains("panic_child_in_run"),
        "panic site frame missing\n{stderr}"
    );
    // Inclusive marker: Report::run's own frame and everything below it.
    assert!(
        !stderr.contains("oopsie::report::"),
        "Report::run frame leaked\n{stderr}"
    );
    assert!(
        !stderr.contains("lang_start"),
        "runtime tail leaked\n{stderr}"
    );
    assert!(
        !stderr.contains("catch_unwind"),
        "catch_unwind cluster leaked\n{stderr}"
    );
}

#[test]
fn full_backtrace_still_bypasses_marker_stripping() {
    let stderr = run_panicking_child(
        "panic_child_in_run",
        &[("NO_COLOR", "1"), ("RUST_BACKTRACE", "full")],
    );

    // `full` shows everything — including frames the marker would hide.
    assert!(
        stderr.contains("oopsie::report::"),
        "full mode must not strip Report::run\n{stderr}"
    );
}
