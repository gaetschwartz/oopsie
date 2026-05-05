#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(unused, clippy::all, clippy::pedantic)]

use oopsie::Oopsie;

// ---- AtMost: should compile since size <= 128 ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(..=128))]
enum SmallError {
    #[oopsie("small")]
    Small,
}

#[test]
fn size_at_most_passes() {
    // If it compiles, the assertion passed
    let _err = SmallError::Small;
}

// ---- AtLeast with 0: always passes ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(0..))]
enum AnySize {
    #[oopsie("any")]
    Any,
}

#[test]
fn size_at_least_passes() {
    let _err = AnySize::Any;
}

// ---- Exact: unit enum is typically 0 bytes ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(0))]
enum ZeroSize {
    #[oopsie("zero")]
    Zero,
}

#[test]
fn size_exact_passes() {
    let _err = ZeroSize::Zero;
}

// ---- Range: should compile since 0 <= size <= 256 ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(0..=256))]
enum RangeSize {
    #[oopsie("range")]
    Range,
}

#[test]
fn size_range_passes() {
    let _err = RangeSize::Range;
}

// ---- Struct with size constraint ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(..=256))]
#[oopsie("struct error: {message}")]
struct StructWithSize {
    message: String,
}

#[test]
fn size_struct_passes() {
    let _ = std::mem::size_of::<StructWithSize>();
}
