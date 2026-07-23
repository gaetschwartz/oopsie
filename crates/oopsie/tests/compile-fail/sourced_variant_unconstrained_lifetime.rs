#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// `'a` lives only on the `B` variant, so the `A` selector's `Contextual` impl
// would carry `'a` without constraining it (E0207).
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum FreeLtSourced<'a, T: std::error::Error + 'static> {
    #[oopsie("a failed")]
    A { source: T },
    #[oopsie("note: {note}")]
    B { note: &'a str },
}

fn main() {}
