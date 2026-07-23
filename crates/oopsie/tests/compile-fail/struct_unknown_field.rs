#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// An unknown `#[oopsie(...)]` key on a struct must list the available keys,
// same as it does on an enum container — darling's own hint silently drops
// once a type has ten or more addressable keys, which `StructAttrs` does.
#[derive(Oopsie)]
#[oopsie(bogus_key)]
struct E {
    #[oopsie(source)]
    inner: std::io::Error,
}

fn main() {}
