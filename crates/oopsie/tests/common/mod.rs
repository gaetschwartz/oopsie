//! Shared helpers for oopsie integration tests.

use oopsie::{NewJsonErrorLayer as _, Oopsie, ResultExt as _, traced};
use tracing::instrument;
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;

#[traced]
#[derive(Debug, Oopsie)]
pub enum MyError {
    #[oopsie(display("Inner error happened"))]
    Inner { source: MyErrorInner },
}

#[traced]
#[derive(Debug, Oopsie)]
#[oopsie("Error: {message}")]
pub struct MyErrorInner {
    message: String,
}

/// Initialize a test subscriber with ErrorLayer for spantrace support.
pub fn init_test_subscriber() -> tracing::subscriber::DefaultGuard {
    let subscriber = tracing_subscriber::registry().with(ErrorLayer::json());
    tracing::subscriber::set_default(subscriber)
}

/// Create an `ErrorWithSpanTrace` within instrumented functions to capture spantrace.
#[expect(clippy::items_after_statements)]
pub fn make_error() -> MyError {
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
            (r"\[[0-9a-f]{7,16}\]", "[[PTR]]"),
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
        ] }, $bl }
    };
}

#[cfg(feature = "unstable")]
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {
        concat!($name, "_unstable")
    };
}

#[cfg(not(feature = "unstable"))]
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {
        concat!($name, "_stable")
    };
}
