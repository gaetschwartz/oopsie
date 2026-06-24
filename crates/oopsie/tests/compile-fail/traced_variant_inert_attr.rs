#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::oopsie;

// A variant `traced` toggle is read only by trace injection, so it is inert
// unless the enum itself is traced. The macro rejects it instead of silently
// doing nothing.
#[oopsie]
enum E {
    #[oopsie(traced)]
    #[oopsie("boom")]
    Boom,
}

fn main() {}
