#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum SizedError {
    #[oopsie("too big", size = 16)]
    TooBig,
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum SuffixError {
    #[oopsie("x", suffix = "Y")]
    Renamed,
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum UnknownKeyError {
    #[oopsie("{} x", n, bogus = 1)]
    Counted { n: u32 },
}

fn main() {}
