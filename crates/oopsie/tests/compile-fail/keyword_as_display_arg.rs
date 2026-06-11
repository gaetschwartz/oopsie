#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum WrapError {
    #[oopsie("wrapped: {source}", transparent)]
    Wrap { source: std::io::Error },
}

fn main() {}
