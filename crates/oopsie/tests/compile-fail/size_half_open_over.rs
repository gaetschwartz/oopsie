#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(8..24))]
enum HalfOpenOver {
    #[oopsie("has data: {data}")]
    HasData { data: String },
}

fn main() {}
