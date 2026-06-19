#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

// Two variants tie for the largest payload; the blame goes to the first in
// source order. The leading unit variant is filtered out of the computation.
#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(..=8))]
enum TieError {
    #[oopsie("unit")]
    Unit,
    #[oopsie("first")]
    First { a: [u8; 64] },
    #[oopsie("second")]
    Second { b: [u8; 64] },
}

fn main() {}
