#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::oopsie;

#[oopsie(traced(timestamp(chrono = true)))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {}
