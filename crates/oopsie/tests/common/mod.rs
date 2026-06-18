//! Shared helpers for oopsie integration tests.
#![allow(
    dead_code,
    unused_imports,
    reason = "each integration-test binary compiles this module fresh and uses only a subset of the helpers"
)]

use oopsie::{ResultExt as _, oopsie};
pub use oopsie_core::test_utils::force_backtrace;
#[cfg(feature = "tracing")]
pub use oopsie_core::test_utils::init_test_subscriber;
#[cfg(feature = "tracing")]
use tracing::instrument;

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

/// Create a `MyError` within instrumented functions to capture spantrace.
#[cfg(feature = "tracing")]
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
