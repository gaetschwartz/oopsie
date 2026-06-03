#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// A packed `traces` field already supplies both traces; a coexisting standalone
// backtrace field would be silently dropped, so the derive rejects it.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie("packed traces alongside a standalone backtrace")]
struct E {
    traces: (oopsie::Backtrace, oopsie::SpanTrace),
    bt: oopsie::Backtrace,
}

fn main() {}
