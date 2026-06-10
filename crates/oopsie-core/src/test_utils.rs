//! Shared helpers for the integration-test suites of the consumer crates.
//!
//! Backtrace and spantrace snapshots are platform- and toolchain-specific, so
//! each snapshot is blessed and asserted only on the toolchain that produced
//! it; the variance lives in the snapshot *filename* (channel + target triple)
//! rather than in a cross-toolchain normalization battery. The filters here are
//! therefore deliberately light: they only erase build-to-build noise
//! (compilation hashes, line/column numbers, machine-specific paths).

#[cfg(feature = "tracing")]
use tracing_subscriber::prelude::*;

#[doc(hidden)]
pub mod __private {
    pub use konst;
    pub use target_triple::TARGET;
}

// Keyed on the granular feature (not the `unstable` umbrella) because the
// Provider-API trace surfacing it gates is what actually changes snapshot
// content.
pub const CHANNEL: &str = if cfg!(feature = "unstable-error-generic-member-access") {
    "unstable"
} else {
    "stable"
};

#[cfg(feature = "unstable-error-generic-member-access")]
const _: () = assert!(matches!(CHANNEL.as_bytes(), b"unstable"));

#[macro_export]
macro_rules! snap_name {
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

/// Install a test subscriber with the JSON `ErrorLayer` so spantraces are
/// captured for the duration of the returned guard.
#[cfg(feature = "tracing")]
#[must_use]
pub fn init_test_subscriber() -> tracing::subscriber::DefaultGuard {
    let subscriber = tracing_subscriber::registry().with(crate::json_error_layer());
    tracing::subscriber::set_default(subscriber)
}

/// Force backtrace capture on the current thread so snapshots are deterministic
/// regardless of the ambient `RUST_BACKTRACE` environment.
pub fn force_backtrace() {
    crate::set_rust_backtrace_override(crate::RustBacktrace::Enabled);
}

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
        settings
    }
}

#[macro_export]
macro_rules! redact {
    ($name:ident, $bl:block) => {
        $crate::test_utils::settings::$name().bind(|| $bl)
    };
}
