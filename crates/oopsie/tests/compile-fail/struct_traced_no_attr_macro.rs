#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// `traced` only has an effect through the `#[oopsie::oopsie(traced)]`
// attribute macro; a plain `#[derive(Oopsie)]` struct gets a dedicated
// pointer instead of a bare "Unknown field" error.
#[derive(Oopsie)]
#[oopsie(traced)]
struct E {
    #[oopsie(source)]
    inner: std::io::Error,
}

fn main() {}
