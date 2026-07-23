#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
enum WrapError {
    #[oopsie("x", module)]
    Wrap,
}

fn main() {}
