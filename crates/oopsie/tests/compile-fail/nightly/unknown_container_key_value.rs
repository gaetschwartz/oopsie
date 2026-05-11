#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(visibility = "pub")]
enum E {
    #[oopsie("`vis`, not `visibility`")]
    A,
}

fn main() {}
