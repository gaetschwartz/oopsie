#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
// The parenthesized-trait-object regression test deliberately writes
// `Box<(dyn Error + ...)>`; the derive re-emits that type in generated code,
// so `unused_parens` must be relaxed crate-wide for this fixture. The lint is
// version-dependent (it doesn't fire on the MSRV), so `#[expect]` would be
// unfulfilled there — `#[allow]` tolerates both. The companion allow opts this
// fixture out of the workspace's allow-over-expect restriction for the same
// reason: expect can't be used for a lint that only fires on some toolchains.
#![allow(
    clippy::allow_attributes,
    reason = "unused_parens is version-dependent, so expect would be unfulfilled on the MSRV"
)]
#![allow(
    unused_parens,
    reason = "parenthesized trait object is the regression under test"
)]

//! Test that `Error::source()` works for three source-field shapes:
//!   1. A concrete `Error` type      — `source: std::io::Error`
//!   2. A `Box<Concrete>`             — `source: Box<std::io::Error>`
//!   3. A `Box<dyn Error + Send + Sync + 'static>`
//!
//! #3 used to fail to compile because rustc couldn't coerce
//! `&Box<dyn Error + Send + Sync + 'static>` to `&(dyn Error + 'static)` — the
//! stdlib's `impl<E: Error> Error for Box<E>` requires `E: Sized` and
//! `dyn Error + Send + Sync` is `?Sized`. `AsErrorSource` papers over this by
//! providing an explicit impl for the boxed trait-object case, with method-
//! call autoderef picking the right impl per field type.

use oopsie::{Contextual as _, Oopsie};
use std::error::Error as StdError;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum Wrapper {
    #[oopsie("concrete source")]
    Concrete { source: std::io::Error },

    #[oopsie("boxed concrete source")]
    BoxedConcrete { source: Box<std::io::Error> },

    #[oopsie("boxed dyn source")]
    BoxedDyn {
        source: Box<dyn StdError + Send + Sync + 'static>,
    },
}

#[test]
fn concrete_source_chain_intact() {
    let inner = std::io::Error::other("disk full");
    let e: Wrapper = Concrete.build_error(inner);
    let s = StdError::source(&e).expect("source missing");
    assert_eq!(s.to_string(), "disk full");
}

#[test]
fn boxed_concrete_source_chain_intact() {
    // The macro auto-boxes the source when the field is `Box<T>` with T:Sized,
    // so the selector takes the inner `io::Error` directly.
    let inner = std::io::Error::other("disk full");
    let e: Wrapper = BoxedConcrete.build_error(inner);
    let s = StdError::source(&e).expect("source missing");
    assert_eq!(s.to_string(), "disk full");
}

#[test]
fn boxed_dyn_source_chain_intact() {
    let inner: Box<dyn StdError + Send + Sync + 'static> =
        Box::new(std::io::Error::other("disk full"));
    let e: Wrapper = BoxedDyn.build_error(inner);
    let s = StdError::source(&e).expect("source missing");
    assert_eq!(s.to_string(), "disk full");
}

// A struct with a boxed-dyn source — covers the gen_struct_error path.
#[derive(Debug, Oopsie)]
#[oopsie("struct-shaped boxed dyn source")]
pub struct StructBoxedDyn {
    source: Box<dyn StdError + Send + Sync + 'static>,
}

#[test]
fn struct_boxed_dyn_source_chain_intact() {
    let inner: Box<dyn StdError + Send + Sync + 'static> =
        Box::new(std::io::Error::other("disk full"));
    let e = StructBoxedDyn { source: inner };
    let s = StdError::source(&e).expect("source missing");
    assert_eq!(s.to_string(), "disk full");
}

// Parenthesized boxed trait object: syn parses the inner as `Type::Paren(...)`,
// which must still be recognized as a trait object (not auto-unboxed into an
// unsized `Source`).
#[derive(Debug, Oopsie)]
#[oopsie("struct-shaped parenthesized boxed dyn source")]
pub struct StructParenBoxedDyn {
    source: Box<(dyn StdError + Send + Sync + 'static)>,
}

#[test]
fn struct_paren_boxed_dyn_source_chain_intact() {
    let inner: Box<(dyn StdError + Send + Sync + 'static)> =
        Box::new(std::io::Error::other("disk full"));
    let e = StructParenBoxedDyn { source: inner };
    let s = StdError::source(&e).expect("source missing");
    assert_eq!(s.to_string(), "disk full");
}
