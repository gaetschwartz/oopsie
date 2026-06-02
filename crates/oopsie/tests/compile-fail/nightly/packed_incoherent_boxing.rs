#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::oopsie;

#[oopsie(traced(backtrace, spantrace(boxed = false)))]
enum Bad {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

fn main() {}
