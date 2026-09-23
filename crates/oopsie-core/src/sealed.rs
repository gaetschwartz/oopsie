//! Sealing support for the crate's extension traits.
//!
//! Kept as two markers rather than one: a shared marker across a blanket
//! `impl<E: Error> _ for E` and a concrete `impl _ for Result<T, E>` (or
//! `Option<T>`) is rejected by coherence — a downstream crate could always add
//! `impl Error for Result<T, E>`, so rustc must treat the two impls as
//! potentially overlapping.

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
