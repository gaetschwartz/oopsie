#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// Two trace-typed backtrace fields would each auto-enable capture; the second
// silently double-captures while only the first is exposed, so the derive
// rejects the ambiguity.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie("two backtrace fields")]
struct E {
    first: oopsie::Backtrace,
    second: oopsie::Backtrace,
}

fn main() {}
