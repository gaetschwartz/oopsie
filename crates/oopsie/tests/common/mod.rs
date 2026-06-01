//! Shared helpers for oopsie integration tests.

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

/// Create an `ErrorWithSpanTrace` within instrumented functions to capture spantrace.
#[expect(clippy::items_after_statements)]
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

#[macro_export]
macro_rules! redact {
    (backtrace, $bl:block) => {
        insta::with_settings! {
          { filters => [
            // Strip nightly's `crate[hash]` bracket form AND stable's
            // `::h<16hex>` suffix form to empty — they appear at different
            // positions per toolchain, so collapsing both to nothing is the
            // only way to make a single snapshot match both.
            (r"\[[0-9a-f]{7,16}\]", ""),
            (r"::h[0-9a-f]{16}\b", ""),
            // Normalize demangling differences between toolchains:
            // - `Box<concrete>` (nightly) / `Box<T>` (stable)
            // - `<__Tn>` synthetic param names (stable)
            // - `<MyType<X>>::method` outer-wrap (nightly) vs
            //   `MyType<__T0>::method` (stable)
            // - `::<()>` empty-return turbofish (nightly)
            (r"Box<[^,>]+>", "Box<T>"),
            (r"<__T\d+>", "<T>"),
            (r"<(\w+(?:::\w+)*)<[^<>]+>>::", "$1<T>::"),
            (r"::<\(\)>", ""),
            (r"\{closure#\d+\}", "{closure}"),
            (r"\{\{closure\}\}", "{closure}"),
            (r" as (\w+(?:::\w+)*)<[^<>]+>", " as $1"),
            (r"<[^<>]+ as (\w+(?:::\w+)*)>::", "$1::"),
            (r"rs:\d+(:\d+)?", "rs:[LOC]"),
            (r"\/[a-f0-9]+\/", "/[HASH]/"),
            (&env!("CARGO_MANIFEST_DIR"), "[CRATE_DIR]"),
            (String::from_utf8(
              std::process::Command::new("rustc")
                .arg("--print")
                .arg("sysroot")
                .output()
                .expect("failed to run rustc")
                .stdout
            ).expect("invalid UTF-8 in rustc sysroot").trim(), "[SYS_ROOT]"),
            (&format!("{}/.cargo/registry/src/", env!("HOME")), "[CARGO_REGISTRY]/"),
            // Stdlib path normalization: local `[SYS_ROOT]/lib/rustlib/src/rust/library/`
            // and CI `/rustc/[HASH]/library/` both → `[STDLIB]/library/`.
            (r"\[SYS_ROOT\]/lib/rustlib/src/rust/library/", "[STDLIB]/library/"),
            (r"/rustc/\[HASH\]/library/", "[STDLIB]/library/"),
        ] }, $bl }
    };
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {
        concat!($name, "_unstable")
    };
}

#[cfg(not(feature = "unstable-error-generic-member-access"))]
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {
        concat!($name, "_stable")
    };
}
