#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// `K` lives only on the `Missing` variant, so the `Write` selector's
// `Contextual` impl would carry `K` without constraining it (E0207).
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum StoreError<K: std::fmt::Debug, E: std::error::Error + 'static> {
    #[oopsie("write failed")]
    Write { source: E },
    #[oopsie("missing key: {key:?}")]
    Missing { key: K },
}

fn main() {}
