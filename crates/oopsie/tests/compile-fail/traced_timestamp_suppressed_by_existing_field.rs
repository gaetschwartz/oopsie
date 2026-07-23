#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::oopsie;

#[oopsie(traced(timestamp))]
pub struct E {
    pub at: std::time::SystemTime,
    pub msg: String,
}

fn main() {}
