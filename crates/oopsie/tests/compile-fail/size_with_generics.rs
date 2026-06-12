#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// `size(...)` asserts a fixed byte count, which a generic type cannot have.
#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(..=32))]
enum E<T: std::fmt::Debug> {
    #[oopsie("payload was {payload:?}")]
    Wrap { payload: T },
}

fn main() {}
