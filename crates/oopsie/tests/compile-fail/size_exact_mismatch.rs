#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(1))]
enum WrongSizeError {
    #[oopsie("has data: {data}")]
    HasData { data: String },
}

fn main() {}
