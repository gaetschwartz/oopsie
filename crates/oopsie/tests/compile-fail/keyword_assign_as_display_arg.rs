#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum CodedError {
    #[oopsie("failed [{code}]", code = "E001")]
    Failed,
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum SizedError {
    #[oopsie("too big", size = 16)]
    TooBig,
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum PathCodedError {
    #[oopsie("failed [{code:>8}]", code = codes::E001)]
    Failed,
}

fn main() {}
