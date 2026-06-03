#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    clippy::pedantic,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

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

// ---- GAP 26: exact size(N) on a realistic String-carrying struct ----
// A struct holding a single `String` is 24 bytes on a 64-bit target (ptr + len + cap).
// `size(24)` is an exact match, so the generated `== 24` const assertion compiles.

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(24))]
#[oopsie("realistic: {msg}")]
struct RealisticError {
    msg: String,
}

#[test]
fn size_exact_realistic_passes() {
    assert_eq!(std::mem::size_of::<RealisticError>(), 24);
    let _err = RealisticError {
        msg: "boom".to_owned(),
    };
}

// ---- GAP 27: tight size(..=N) upper bound on a String struct ----
// A single-`String` struct is exactly 24 bytes, so `..=24` is the tightest upper
// bound that still compiles (the generated `<= 24` assertion holds).

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(..=24))]
#[oopsie("tight: {detail}")]
struct TightUpperBound {
    detail: String,
}

#[test]
fn size_at_most_tight_passes() {
    assert!(std::mem::size_of::<TightUpperBound>() <= 24);
    let _err = TightUpperBound {
        detail: "x".to_owned(),
    };
}

// ---- GAP 28: multi-data-variant enum with a size constraint ----
// Two data-carrying variants: a `String` (24 bytes) and a `u32`. The String
// variant dominates and the discriminant fits in the String's niche, so the
// enum is 24 bytes. `..=24` exercises the constraint across multiple variants.

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(..=24))]
enum MultiVariantError {
    #[oopsie("data: {data}")]
    Data { data: String },
    #[oopsie("code: {code}")]
    Code { code: u32 },
}

#[test]
fn size_multi_variant_enum_passes() {
    assert_eq!(std::mem::size_of::<MultiVariantError>(), 24);
    let _a = MultiVariantError::Data {
        data: "hi".to_owned(),
    };
    let _b = MultiVariantError::Code { code: 7 };
}

// ---- GAP 54: lower-bound-only size(N..) with a meaningful N ----
// The String-carrying enum is 24 bytes, satisfying the generated `>= 16` assertion.

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(16..))]
enum LowerBoundError {
    #[oopsie("payload: {payload}")]
    Payload { payload: String },
    #[oopsie("empty")]
    Empty,
}

#[test]
fn size_at_least_meaningful_passes() {
    assert!(std::mem::size_of::<LowerBoundError>() >= 16);
    let _err = LowerBoundError::Payload {
        payload: "p".to_owned(),
    };
    let _empty = LowerBoundError::Empty;
}
