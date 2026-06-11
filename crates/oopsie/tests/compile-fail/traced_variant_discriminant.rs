#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::oopsie;

// `traced` rewrites a unit variant into a named one to add trace fields, but a
// variant with an explicit discriminant must stay fieldless (E0732). The macro
// rejects this up front with a discriminant-spanning error.
#[oopsie(traced)]
#[oopsie(module(false))]
enum E {
    #[oopsie("boom")]
    Boom = 1,
}

fn main() {}
