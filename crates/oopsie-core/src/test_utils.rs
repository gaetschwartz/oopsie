//! Shared helpers for the integration-test suites of the consumer crates.
//!
//! The snapshot filters are deliberately light: they only erase build-to-build
//! noise (compilation hashes, line/column numbers, machine-specific paths).

#[cfg(feature = "tracing")]
use tracing_subscriber::prelude::*;

#[doc(hidden)]
pub mod __private {
    pub use konst;
    pub use target_tuple::TARGET;
}

/// The toolchain channel embedded in file names of snapshots that differ between
/// the two. Keyed on the granular feature, not the `unstable` umbrella.
pub const CHANNEL: &str = if cfg!(feature = "unstable-error-generic-member-access") {
    "unstable"
} else {
    "stable"
};

#[cfg(feature = "unstable-error-generic-member-access")]
const _: () = assert!(matches!(CHANNEL.as_bytes(), b"unstable"));

/// Build a snapshot name from a base label plus the target triple. Use
/// [`snap_name_by_channel`](crate::snap_name_by_channel) when a fixture erases a source.
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {{
        $crate::test_utils::__private::konst::string::str_join!(
            "_",
            &[$name, $crate::test_utils::__private::TARGET]
        )
    }};
}

/// [`snap_name`](crate::snap_name) plus the toolchain [`CHANNEL`], for fixtures
/// whose trace reach goes through a type-erased source and so differs by channel.
#[macro_export]
macro_rules! snap_name_by_channel {
    ($name:literal) => {{
        $crate::test_utils::__private::konst::string::str_join!(
            "_",
            &[
                $name,
                $crate::test_utils::CHANNEL,
                $crate::test_utils::__private::TARGET
            ]
        )
    }};
}

/// Install a test subscriber with an `ErrorLayer` so span traces are captured
/// for the duration of the returned guard.
#[cfg(feature = "tracing")]
#[must_use]
pub fn init_test_subscriber() -> tracing::subscriber::DefaultGuard {
    #[cfg(feature = "serde")]
    let error_layer = crate::tracing::json_error_layer();
    #[cfg(not(feature = "serde"))]
    let error_layer = tracing_error::ErrorLayer::default();
    let subscriber = tracing_subscriber::registry().with(error_layer);
    tracing::subscriber::set_default(subscriber)
}

/// Install a test subscriber *without* an `ErrorLayer`, so captured span traces
/// report [`Unsupported`](crate::SpanTraceStatus::Unsupported) — distinct from
/// the no-subscriber [`Empty`](crate::SpanTraceStatus::Empty) case — for the
/// duration of the returned guard.
#[cfg(feature = "tracing")]
#[must_use]
pub fn init_test_subscriber_without_error_layer() -> tracing::subscriber::DefaultGuard {
    tracing::subscriber::set_default(tracing_subscriber::registry())
}

/// Force backtrace capture on the current thread so snapshots are deterministic
/// regardless of the ambient `RUST_BACKTRACE` environment.
pub fn force_backtrace() {
    crate::set_rust_backtrace_override(crate::RustBacktrace::Enabled);
}

/// `insta` snapshot redaction profiles that erase build-to-build noise
/// (compilation hashes, line/column numbers, machine-specific paths) from
/// backtrace and spantrace snapshots.
pub mod settings {
    use std::{
        env,
        path::{Path, PathBuf},
        process::Command,
        sync::LazyLock,
    };

    /// `insta` filter patterns are regexes; escape paths so they match literally.
    fn regex_escape(text: &str) -> String {
        let mut escaped = String::with_capacity(text.len());
        for ch in text.chars() {
            if ch.is_ascii_punctuation() {
                escaped.push('\\');
            }
            escaped.push(ch);
        }
        escaped
    }

    /// `insta::Settings` carrying the light, build-to-build normalization filters
    /// shared by every backtrace/spantrace snapshot. Bind with [`insta::Settings::bind`].
    #[must_use]
    pub fn backtrace() -> insta::Settings {
        static RUSTC_SYSROOT: LazyLock<String> = LazyLock::new(|| {
            String::from_utf8(
                Command::new("rustc")
                    .arg("--print")
                    .arg("sysroot")
                    .output()
                    .expect("failed to run rustc")
                    .stdout,
            )
            .expect("invalid UTF-8 in rustc sysroot")
            .trim()
            .to_owned()
        });

        static WORKSPACE_ROOT: LazyLock<String> = LazyLock::new(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        });

        static CARGO_HOME: LazyLock<Option<String>> = LazyLock::new(|| {
            env::var_os("CARGO_HOME")
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
                .or_else(|| {
                    env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo"))
                })
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.to_string_lossy().into_owned())
        });

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(r"\[[0-9a-f]{7,16}\]", "[[HASH]]");
        settings.add_filter(r"::h[0-9a-f]{7,16}\b", "::h[HASH]");
        settings.add_filter(r"\/[a-f0-9]+\/", "/[HASH]/");
        settings.add_filter(r"rs:\d+(:\d+)?", "rs:[LOC]");
        settings.add_filter(&regex_escape(&WORKSPACE_ROOT), "[WORKSPACE]");
        settings.add_filter(&regex_escape(&RUSTC_SYSROOT), "[SYS_ROOT]");
        if let Some(cargo_home) = CARGO_HOME.as_deref() {
            settings.add_filter(&regex_escape(cargo_home), "[CARGO_HOME]/");
        }
        // Stdlib path normalization: local `[SYS_ROOT]/lib/rustlib/src/rust/library/`
        // and CI `/rustc/[HASH]/library/` both → `[STDLIB]/library/`.
        settings.add_filter(
            r"\[SYS_ROOT\]/lib/rustlib/src/rust/library/",
            "[STDLIB]/library/",
        );
        settings.add_filter(r"/rustc/\[HASH\]/library/", "[STDLIB]/library/");
        // JSON snapshots carry line/column as numeric fields rather than `rs:N:C`.
        settings.add_filter(r#""line":\s*\d+"#, r#""line": 42"#);
        settings.add_filter(r#""column":\s*\d+"#, r#""column": 69"#);
        // macOS test threads bottom out in a libc frame whose exact symbol
        // varies between runs; the render path peels this OS tail, but the
        // erased path serializes raw frames, so normalize it here.
        settings.add_filter(r"__pthread\w*", "[OS_TAIL]");
        settings
    }

    #[cfg(test)]
    mod tests {
        use std::{env, process::Command};

        use super::{backtrace, regex_escape};

        #[test]
        fn regex_escape_escapes_punctuation_only() {
            assert_eq!(regex_escape("/home/me/ws"), "\\/home\\/me\\/ws");
            assert_eq!(
                regex_escape(r"C:\Users\me\.cargo"),
                "C\\:\\\\Users\\\\me\\\\\\.cargo"
            );
            assert_eq!(
                regex_escape("/tmp/ws [v2] (x)+/repo"),
                "\\/tmp\\/ws \\[v2\\] \\(x\\)\\+\\/repo"
            );
        }

        #[test]
        fn windows_style_path_registers_and_redacts() {
            let mut settings = insta::Settings::clone_current();
            settings.add_filter(&regex_escape(r"C:\Users\me\.cargo"), "[CARGO_HOME]/");
            settings.bind(|| {
                insta::assert_snapshot!(
                    r"C:\Users\me\.cargo\registry\src",
                    @r"[CARGO_HOME]/\registry\src"
                );
            });
        }

        #[test]
        fn metachar_path_registers_and_redacts() {
            let mut settings = insta::Settings::clone_current();
            settings.add_filter(&regex_escape("/tmp/ws [v2] (x)+/repo"), "[WORKSPACE]");
            settings.bind(|| {
                insta::assert_snapshot!(
                    "/tmp/ws [v2] (x)+/repo/src/lib.rs",
                    @"[WORKSPACE]/src/lib.rs"
                );
            });
        }

        const NO_HOME_TRIGGER: &str = "OOPSIE_TEST_UTILS_NO_HOME_PROBE";

        /// Child entry point for [`missing_home_env_does_not_panic`]; a no-op
        /// unless re-exec'd with the trigger env set.
        #[test]
        fn no_home_probe_child() {
            if env::var_os(NO_HOME_TRIGGER).is_none() {
                return;
            }
            drop(backtrace());
        }

        /// `std::env::set_var` is unsafe in edition 2024, so the no-HOME case
        /// runs in a re-exec'd child with every home-dir variable scrubbed.
        #[test]
        fn missing_home_env_does_not_panic() {
            let exe = env::current_exe().expect("locate test binary");
            let output = Command::new(exe)
                .arg("test_utils::settings::tests::no_home_probe_child")
                .args(["--exact", "--nocapture", "--test-threads=1"])
                .env(NO_HOME_TRIGGER, "1")
                .env_remove("CARGO_HOME")
                .env_remove("HOME")
                .env_remove("USERPROFILE")
                .output()
                .expect("spawn child test process");
            assert!(
                output.status.success(),
                "child failed without HOME/CARGO_HOME/USERPROFILE\n--- stderr ---\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

/// Run a block with a named [`settings`] redaction profile bound, so the
/// snapshots asserted inside it use the shared normalization filters.
#[macro_export]
macro_rules! redact {
    ($name:ident, $bl:block) => {
        $crate::test_utils::settings::$name().bind(|| $bl)
    };
}
