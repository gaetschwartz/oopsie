#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

#[oopsie::oopsie]
#[oopsie(module(false), suffix(false))]
#[oopsie("it failed")]
pub struct Failure {
    n: u32,
}

fn main() {}
