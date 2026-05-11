#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum E<T: std::fmt::Debug> {
    #[oopsie("payload was {payload:?}")]
    Wrap { payload: T },
}

fn main() {}
