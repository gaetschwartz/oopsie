#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
pub enum ValueError {
    #[oopsie("expected string")]
    String,
    #[oopsie("bad key {key}")]
    Key { key: String },
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
pub enum ParseError {
    #[oopsie("empty")]
    Vec,
    #[oopsie("bad items")]
    Items { items: std::collections::HashMap<u8, &'static [Vec<u8>]> },
}

fn main() {}
