#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

#[derive(Oopsie)]
union U {
    a: u32,
    b: u32,
}

fn main() {}
