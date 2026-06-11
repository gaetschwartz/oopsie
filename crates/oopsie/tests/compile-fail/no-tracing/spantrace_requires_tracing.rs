#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

#[oopsie::oopsie(traced(spantrace))]
pub struct Boom {
    msg: String,
}

fn main() {}
