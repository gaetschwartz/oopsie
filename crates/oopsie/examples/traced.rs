#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

//! Simple example showing `oopsie`'s `traced` attribute in action: capture a `tracing` span trace
//! alongside backtraces.
//!
//! Run with: `RUST_BACKTRACE=1 cargo run --example traced -F fancy`

use oopsie::{Report, oopsie};

#[oopsie(traced)]
pub enum QueryError {
    #[oopsie("unknown user: {user}")]
    UnknownUser { user: String },
}

#[cfg_attr(feature = "tracing", tracing::instrument)]
fn fetch_user(user: &str) -> Result<(), QueryError> {
    query_oopsies::UnknownUser { user }.fail()
}

fn main() -> Report<QueryError> {
    init_tracing();

    Report::run(|| fetch_user("alice"))
}

// const-eligible only without `tracing`, where the body is empty; with the
// feature it calls non-const subscriber setup, so the lint fires in just one cfg.
#[cfg_attr(
    not(feature = "tracing"),
    expect(
        clippy::missing_const_for_fn,
        reason = "empty body only without `tracing`"
    )
)]
fn init_tracing() {
    #[cfg(feature = "tracing")]
    {
        use tracing_subscriber::prelude::*;

        tracing_subscriber::registry()
            .with({
                #[cfg(feature = "serde")]
                {
                    oopsie::tracing::json_error_layer()
                }
                #[cfg(not(feature = "serde"))]
                {
                    oopsie::tracing::default_error_layer()
                }
            })
            .init();
    }
}
