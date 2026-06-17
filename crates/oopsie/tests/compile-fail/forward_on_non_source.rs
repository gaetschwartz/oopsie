#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::oopsie;

#[oopsie(traced)]
pub struct BadError {
    #[oopsie(forward)]
    not_a_source: String,
}

fn main() {}
