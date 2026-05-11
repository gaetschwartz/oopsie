#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum E {
    #[oopsie("tuple variants aren't supported")]
    Foo(String),
}

fn main() {}
