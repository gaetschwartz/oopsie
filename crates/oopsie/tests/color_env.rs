#![cfg(feature = "fancy")]
//! Integration tests for environment-based color detection.
//!
//! `detect_env_color_support` is private, reads process-global env, and caches
//! its result in a `OnceLock` — and `std::env::set_var` is unsafe in edition
//! 2024 — so it can't be exercised from an in-process unit test. We re-exec
//! this test binary as a child (the same pattern as `panic_hook.rs`): with
//! `OOPSIE_COLOR_ENV_TEST_TRIGGER` set, the `color_probe_child` test prints
//! `has_color={}` for `ColorConfig::Auto.should_colorize()`; the parent spawns
//! it under a controlled env matrix and asserts on the result.

use std::process::Command;

const TRIGGER_ENV: &str = "OOPSIE_COLOR_ENV_TEST_TRIGGER";

/// The child entry point. A no-op unless re-exec'd with the trigger env set, so
/// it passes silently during a normal test run.
#[test]
#[expect(
    clippy::print_stdout,
    reason = "the parent reads the probe result back off the child's stdout"
)]
fn color_probe_child() {
    if std::env::var_os(TRIGGER_ENV).is_none() {
        return;
    }
    println!("has_color={}", oopsie::ColorConfig::Auto.should_colorize());
}

/// Re-exec this test binary, running only `color_probe_child` with the trigger
/// set and the given color env vars applied, and return whether the child
/// detected color. `NO_COLOR`/`FORCE_COLOR` are cleared first so the developer's
/// shell can't contaminate the matrix; the child's stdout is piped, so the
/// `IsTerminal` fallback is deterministically `false`.
fn probe(env: &[(&str, &str)]) -> bool {
    let exe = std::env::current_exe().expect("locate test binary");
    let mut cmd = Command::new(exe);
    cmd.arg("color_probe_child")
        .args(["--exact", "--nocapture", "--test-threads=1"])
        .env(TRIGGER_ENV, "1")
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR");
    for (key, value) in env {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("spawn child test process");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "child process failed\n--- stdout ---\n{stdout}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if stdout.contains("has_color=true") {
        true
    } else if stdout.contains("has_color=false") {
        false
    } else {
        panic!("child did not report a color result\n--- stdout ---\n{stdout}");
    }
}

#[test]
fn detect_env_color_support_matrix() {
    assert!(!probe(&[]), "piped baseline must not colorize");
    assert!(probe(&[("FORCE_COLOR", "1")]), "FORCE_COLOR=1 forces color");
    assert!(
        !probe(&[("FORCE_COLOR", "0")]),
        "FORCE_COLOR=0 is an explicit disable"
    );
    assert!(
        !probe(&[("FORCE_COLOR", "false")]),
        "FORCE_COLOR=false is an explicit disable"
    );
    assert!(
        !probe(&[("FORCE_COLOR", "")]),
        "empty FORCE_COLOR is ignored, falling back to the piped (false) terminal check"
    );
    assert!(
        !probe(&[("NO_COLOR", "1"), ("FORCE_COLOR", "1")]),
        "non-empty NO_COLOR wins over FORCE_COLOR"
    );
    assert!(
        probe(&[("NO_COLOR", ""), ("FORCE_COLOR", "1")]),
        "empty NO_COLOR is ignored, so FORCE_COLOR=1 still forces color"
    );
}
