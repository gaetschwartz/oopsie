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
pub enum BadError {
    #[oopsie(transparent)]
    Bad {
        #[oopsie(forward)]
        source: LeafError,
    },
}

fn main() {}
