#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(..24))]
enum BelowBoundary {
    #[oopsie("has data: {data}")]
    HasData { data: String },
}

fn main() {}
