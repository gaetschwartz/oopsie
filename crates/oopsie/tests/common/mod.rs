//! Shared helpers for oopsie integration tests.
#![allow(
    dead_code,
    reason = "each integration-test binary compiles this module fresh and uses only a subset of the helpers"
)]

use std::{path::PathBuf, sync::LazyLock};

use oopsie::{ResultExt as _, oopsie};
use tracing::instrument;
use tracing_subscriber::prelude::*;

#[oopsie(traced)]
pub enum MyError {
    #[oopsie(display("Inner error happened"))]
    Inner { source: MyErrorInner },
}

#[oopsie(traced)]
#[oopsie("Error: {message}")]
pub struct MyErrorInner {
    message: String,
}

/// Initialize a test subscriber with ErrorLayer for spantrace support.
pub fn init_test_subscriber() -> tracing::subscriber::DefaultGuard {
    let subscriber = tracing_subscriber::registry().with(oopsie::tracing::json_error_layer());
    tracing::subscriber::set_default(subscriber)
}

/// Force backtrace capture on the current thread so snapshots are deterministic
/// regardless of the ambient `RUST_BACKTRACE` environment.
pub fn force_backtrace() {
    oopsie::set_rust_backtrace_override(oopsie::RustBacktrace::Enabled);
}

/// Create a `MyError` within instrumented functions to capture spantrace.
#[expect(
    clippy::items_after_statements,
    reason = "instrumented helper fns must be items; defined inside make_error to capture the spantrace"
)]
pub fn make_error() -> MyError {
    force_backtrace();
    let _guard = init_test_subscriber();

    #[instrument(target = "sys", fields(id = 42))]
    fn inner_function(foo: bool, name: &str) -> Result<(), MyErrorInner> {
        MyErrorInnerOopsie {
            message: format!("Inner function failed (foo={foo}, name={name})"),
        }
        .fail()
    }

    #[instrument(target = "controller")]
    fn outer_function(foo: bool, name: &str) -> Result<(), MyError> {
        Ok(inner_function(foo, name).context(my_oopsies::Inner)?)
    }

    outer_function(true, "Alice").expect_err("Should produce an error")
}

pub static RUSTC_SYSROOT: LazyLock<String> = LazyLock::new(|| {
    String::from_utf8(
        std::process::Command::new("rustc")
            .arg("--print")
            .arg("sysroot")
            .output()
            .expect("failed to run rustc")
            .stdout,
    )
    .expect("invalid UTF-8 in rustc sysroot")
    .trim()
    .to_string()
});
pub static WORKSPACE_ROOT: LazyLock<String> = LazyLock::new(|| {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string()
});
pub static CARGO_HOME: LazyLock<String> = LazyLock::new(|| {
    std::env::var("CARGO_HOME")
        .map_or_else(
            |_| {
                PathBuf::from(std::env::var("HOME").expect("HOME environment variable not set"))
                    .join(".cargo")
            },
            |val| PathBuf::from(val),
        )
        .to_string_lossy()
        .to_string()
});

#[macro_export]
macro_rules! redact {
    (backtrace, $bl:block) => {
        insta::with_settings! {
          { filters => [
            (r"\[[0-9a-f]{7,16}\]", "[[HASH]]"),
            (r"::h[0-9a-f]{7,16}\b", "::h[HASH]"),
            (r"\/[a-f0-9]+\/", "/[HASH]/"),
            (r"rs:\d+(:\d+)?", "rs:[LOC]"),
            (&*$crate::common::WORKSPACE_ROOT, "[WORKSPACE]"),
            (&*$crate::common::RUSTC_SYSROOT, "[SYS_ROOT]"),
            (&*$crate::common::CARGO_HOME, "[CARGO_HOME]/"),
            // Stdlib path normalization: local `[SYS_ROOT]/lib/rustlib/src/rust/library/`
            // and CI `/rustc/[HASH]/library/` both → `[STDLIB]/library/`.
            (r"\[SYS_ROOT\]/lib/rustlib/src/rust/library/", "[STDLIB]/library/"),
            (r"/rustc/\[HASH\]/library/", "[STDLIB]/library/"),
        ] }, $bl }
    };
}

#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {{
        #[cfg(feature = "unstable")]
        {
            concat!($name, "_unstable")
        }
        #[cfg(not(feature = "unstable"))]
        {
            concat!($name, "_stable")
        }
    }};
}
