#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum StrayBraceError {
    #[oopsie("stray")]
    #[oopsie(help = "close it }")]
    Variant { data: String },
}

fn main() {}
