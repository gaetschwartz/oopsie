#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie("wrong-typed explicit backtrace")]
struct E {
    #[oopsie(backtrace)]
    backtrace: String,
}

fn main() {}
