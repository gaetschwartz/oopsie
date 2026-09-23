#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum ExitError {
    #[oopsie("exited", exit_code = 3)]
    Exited,
}

#[derive(Debug, Oopsie)]
#[oopsie("config broke", suffix = "Ctx")]
struct ConfigError {
    path: String,
}

fn main() {}
