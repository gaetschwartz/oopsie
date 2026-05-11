#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(modul(false))]
enum E {
    #[oopsie("typo on the container key")]
    A,
}

fn main() {}
