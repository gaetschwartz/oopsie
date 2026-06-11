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

use oopsie::{Report, oopsie};

#[oopsie(traced)]
#[oopsie("kaboom: {message}")]
pub struct KaboomError {
    message: String,
}

fn fail_deep() -> Result<(), KaboomError> {
    Err(KaboomOopsie {
        message: "deep failure",
    }
    .build())
}

#[test]
fn report_run_hides_its_own_plumbing_and_the_runtime_tail() {
    common::force_backtrace();
    let report = Report::run(fail_deep).no_colors();
    let rendered = report.to_string();

    assert!(
        rendered.contains("fail_deep"),
        "user frame missing\n{rendered}"
    );
    // Inclusive marker hides Report::run's own frame, data-driven.
    assert!(
        !rendered.contains("::report::"),
        "Report::run frame leaked\n{rendered}"
    );
    // The catch_unwind cluster between closure and run must be peeled.
    assert!(
        !rendered.contains("catch_unwind"),
        "catch_unwind cluster leaked\n{rendered}"
    );
    // Everything below run (test harness, thread spawn) is IP-stripped.
    assert!(
        !rendered.contains("thread_start"),
        "frames below Report::run leaked\n{rendered}"
    );
}

#[test]
fn start_marker_macro_cuts_in_a_spawned_thread() {
    let rendered = std::thread::spawn(|| {
        common::force_backtrace();
        oopsie::start_marker!();
        let err = KaboomOopsie {
            message: "in thread",
        }
        .build();
        use oopsie::Diagnostic as _;
        let bt = err
            .oopsie_backtrace()
            .expect("traced error has a backtrace");
        assert!(
            bt.marker_hidden_ips().is_some(),
            "exclusive marker must produce a cut on its own thread"
        );
        Report::from_std(err).no_colors().to_string()
    })
    .join()
    .unwrap();

    // Exclusive boundary: the closure that called start_marker! stays visible.
    assert!(
        rendered.contains("start_marker_macro_cuts_in_a_spawned_thread"),
        "marker caller frame missing\n{rendered}"
    );
    // Spawn plumbing below the marker is IP-stripped (no prefix list covers it).
    assert!(
        !rendered.contains("thread_start"),
        "thread spawn plumbing leaked\n{rendered}"
    );
    assert!(
        !rendered.contains("::thread::"),
        "thread spawn plumbing leaked\n{rendered}"
    );
}

#[test]
fn mid_stack_catch_unwind_user_frames_survive() {
    common::force_backtrace();
    fn supervisor() -> KaboomError {
        std::panic::catch_unwind(|| {
            KaboomOopsie {
                message: "inside supervised section",
            }
            .build()
        })
        .unwrap()
    }
    let rendered = Report::from_std(supervisor()).no_colors().to_string();

    // The old drain-from-anywhere rule ate every frame below catch_unwind.
    assert!(
        rendered.contains("supervisor"),
        "user frame below mid-stack catch_unwind was over-trimmed\n{rendered}"
    );
}

#[test]
fn report_run_mid_stack_catch_unwind_survives_with_marker() {
    common::force_backtrace();
    let report = Report::run(|| -> Result<(), KaboomError> {
        fn supervised() -> Result<(), KaboomError> {
            std::panic::catch_unwind(|| {
                Err(KaboomOopsie {
                    message: "inside guarded section",
                }
                .build())
            })
            .unwrap()
        }
        supervised()
    })
    .no_colors();
    let rendered = report.to_string();

    // The marker cut removes run/tail; the contiguous peel must still stop
    // at `supervised` instead of draining from its mid-stack catch_unwind.
    assert!(
        rendered.contains("supervised"),
        "user frame below mid-stack catch_unwind was over-trimmed\n{rendered}"
    );
    assert!(
        !rendered.contains("::report::"),
        "Report::run frame leaked\n{rendered}"
    );
}

#[test]
fn marker_from_another_thread_never_applies() {
    common::force_backtrace();
    oopsie::start_marker!();
    let err = std::thread::spawn(|| {
        common::force_backtrace();
        KaboomOopsie {
            message: "cross-thread",
        }
        .build()
    })
    .join()
    .unwrap();

    use oopsie::Diagnostic as _;
    let bt = err
        .oopsie_backtrace()
        .expect("traced error has a backtrace");
    assert!(bt.marker_hidden_ips().is_none());
}

#[test]
fn report_run_restores_previous_marker_on_return() {
    common::force_backtrace();
    oopsie::start_marker!();
    let _ = Report::run(fail_deep);
    // The original marker still applies to captures after `run` returns.
    let err = KaboomOopsie {
        message: "after run",
    }
    .build();
    use oopsie::Diagnostic as _;
    let bt = err
        .oopsie_backtrace()
        .expect("traced error has a backtrace");
    assert!(bt.marker_hidden_ips().is_some());
}

#[test]
fn report_run_restores_previous_marker_on_unwind() {
    common::force_backtrace();
    oopsie::start_marker!();
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = Report::run(|| -> Result<(), KaboomError> {
            panic!("boom through run");
        });
    }));
    assert!(payload.is_err());
    let err = KaboomOopsie {
        message: "after unwound run",
    }
    .build();
    use oopsie::Diagnostic as _;
    let bt = err
        .oopsie_backtrace()
        .expect("traced error has a backtrace");
    assert!(bt.marker_hidden_ips().is_some());
}

#[test]
fn full_backtrace_bypasses_marker_on_error_path() {
    oopsie::with_rust_backtrace_override(oopsie::RustBacktrace::Full, || {
        let rendered = Report::run(fail_deep).no_colors().to_string();
        // `full` shows everything — including the run frame the marker hides.
        assert!(
            rendered.contains("::report::"),
            "full mode must not strip Report::run\n{rendered}"
        );
    });
}
