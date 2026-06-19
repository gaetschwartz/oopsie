#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

// The enum blows its size budget because of `Big`; the error must point at that
// variant, not at `Small`/`Medium` or the `size(...)` attribute.
#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(..=8))]
enum BigError {
    #[oopsie("small")]
    Small { byte: u8 },
    #[oopsie("big")]
    Big { buf: [u8; 64] },
    #[oopsie("medium")]
    Medium { half: u16 },
}

fn main() {}
