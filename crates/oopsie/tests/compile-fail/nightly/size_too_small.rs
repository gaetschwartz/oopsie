#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(9999..))]
enum TooSmallError {
    #[oopsie("small")]
    Small,
}

fn main() {}
