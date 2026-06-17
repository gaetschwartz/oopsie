#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::oopsie;

#[oopsie(traced)]
pub enum LeafError {
    #[oopsie("leaf")]
    Boom { msg: String },
}

#[oopsie(traced)]
pub struct BadError {
    #[oopsie(forward(timestamp = true))]
    source: LeafError,
}

fn main() {}
