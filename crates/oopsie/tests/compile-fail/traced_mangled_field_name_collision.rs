#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::oopsie;

#[oopsie(traced)]
pub enum E {
    #[oopsie("boom")]
    A { __oopsie_traces: u8, v: u8 },
}

fn main() {}
