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
    #[oopsie("bad")]
    Bad {
        #[oopsie(forward)]
        source: LeafError,
        bt: oopsie::Backtrace,
    },
}

fn main() {}
