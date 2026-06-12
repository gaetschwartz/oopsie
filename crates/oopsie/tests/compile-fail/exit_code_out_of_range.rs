#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AppError {
    #[oopsie("boom")]
    #[oopsie(exit_code = 300)]
    Boom { detail: String },
}

fn main() {}
