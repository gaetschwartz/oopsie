//! Shared helpers for the integration-test suites of the consumer crates.
//!
//! Backtrace and spantrace snapshots are platform-specific, so the variance
//! lives in the snapshot *filename* (see [`snap_name`]) rather than in a
//! cross-toolchain normalization battery. The filters here are therefore
//! deliberately light: they only erase build-to-build noise (compilation
//! hashes, line/column numbers, machine-specific paths).

#[cfg(feature = "tracing")]
use tracing_subscriber::prelude::*;

#[doc(hidden)]
pub mod __private {
    pub use konst;
    pub use target_triple::TARGET;
}

/// The toolchain channel (`stable` or `unstable`) embedded in the file names of
/// snapshots whose content genuinely differs between the two.
// Keyed on the granular feature (not the `unstable` umbrella) because Provider-
// API reach into a *type-erased* source is the only thing that changes rendered
// content; a typed source is reached the same way on both channels.
pub const CHANNEL: &str = if cfg!(feature = "unstable-error-generic-member-access") {
    "unstable"
} else {
    "stable"
};

#[cfg(feature = "unstable-error-generic-member-access")]
const _: () = assert!(matches!(CHANNEL.as_bytes(), b"unstable"));

/// Build a snapshot name from a base label plus the target triple, keeping
/// platform-specific snapshots in separate files.
///
/// Deliberately *not* keyed on [`CHANNEL`]: a fixture whose source errors are
/// concretely typed renders identically on both channels, so a shared snapshot
/// asserts that the Provider API is inert there. Reach for
/// [`snap_name_by_channel`] instead when a fixture erases a source.
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {{
        $crate::test_utils::__private::konst::string::str_join!(
            "_",
            &[$name, $crate::test_utils::__private::TARGET]
        )
    }};
}

/// [`snap_name`] plus the toolchain [`CHANNEL`], for fixtures that reach a trace
/// through a type-erased source: the Provider API surfaces the origin-most trace
/// on nightly, while stable falls back to the wrapper's own capture.
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

        static CARGO_HOME: LazyLock<String> = LazyLock::new(|| {
            env::var("CARGO_HOME")
                .map_or_else(
                    |_| {
                        PathBuf::from(env::var("HOME").expect("HOME environment variable not set"))
                            .join(".cargo")
                    },
                    PathBuf::from,
                )
                .to_string_lossy()
                .into_owned()
        });

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(r"\[[0-9a-f]{7,16}\]", "[[HASH]]");
        settings.add_filter(r"::h[0-9a-f]{7,16}\b", "::h[HASH]");
        settings.add_filter(r"\/[a-f0-9]+\/", "/[HASH]/");
        settings.add_filter(r"rs:\d+(:\d+)?", "rs:[LOC]");
        settings.add_filter(&WORKSPACE_ROOT, "[WORKSPACE]");
        settings.add_filter(&RUSTC_SYSROOT, "[SYS_ROOT]");
        settings.add_filter(&CARGO_HOME, "[CARGO_HOME]/");
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
}

/// Run a block with a named [`settings`] redaction profile bound, so the
/// snapshots asserted inside it use the shared normalization filters.
#[macro_export]
macro_rules! redact {
    ($name:ident, $bl:block) => {
        $crate::test_utils::settings::$name().bind(|| $bl)
    };
}
