#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// oopsie adds no implicit `Display` bound (thiserror-style inference is out of
// scope): interpolating a generic field whose parameter is not `Display` is a
// normal rustc bound error pointing at the format string's use of the value.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum E<T: std::fmt::Debug> {
    #[oopsie("value is {value}")]
    Show { value: T },
}

fn main() {}
