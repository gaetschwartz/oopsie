//! Sealing support for the crate's extension traits.
//!
//! Two markers, not one: a shared marker on both a blanket `impl<E: Error>` and a
//! concrete `impl for Result<T, E>` would let rustc treat the impls as potentially overlapping.

#[expect(
    unnameable_types,
    reason = "the sealed-trait pattern: nameable only from inside this crate"
)]
pub trait Chain {}
#[expect(
    unnameable_types,
    reason = "the sealed-trait pattern: nameable only from inside this crate"
)]
pub trait Ctx {}
