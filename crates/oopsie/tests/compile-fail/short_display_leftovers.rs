#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
enum ModuleError {
    #[oopsie("x", module)]
    Wrap,
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum ExtraArgError {
    #[oopsie("{} x", n, n + 1)]
    Counted { n: u32 },
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum RedundantArgError {
    #[oopsie("x {a}", a)]
    Captured { a: u32 },
}

fn main() {}
