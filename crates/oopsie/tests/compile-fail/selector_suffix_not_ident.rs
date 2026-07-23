#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false), suffix = "with space")]
#[oopsie("bad suffix")]
struct ConnError {
    info: String,
}

fn main() {}
