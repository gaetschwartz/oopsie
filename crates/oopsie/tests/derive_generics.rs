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

//! Generic error types: a type param used in a display field, a param used only
//! in the source, lifetimes, const params, where-clauses, generic structs,
//! transparent generic wrappers, traced generic enums, selector `Into`
//! ergonomics with generic targets, and `Diagnostic` accessors on a generic
//! traced error.

use oopsie::{Contextual as _, Diagnostic as _, Oopsie};
use std::error::Error as _;
use std::fmt;
use std::io;

// ─── type param used in a display field ───
// `T` rides the `Wrap` selector (its `payload` field references it); the
// `Other` variant references no param, so its selector stays non-generic.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum PayloadError<T: fmt::Debug> {
    #[oopsie("payload was {payload:?}")]
    Wrap { payload: T },
    #[oopsie("plain: {detail}")]
    Other { detail: String },
}

#[test]
fn type_param_in_display_field() {
    let err: PayloadError<u32> = Wrap { payload: 7u32 }.build();
    assert_eq!(err.to_string(), "payload was 7");
    assert!(matches!(err, PayloadError::Wrap { payload } if payload == 7));
}

#[test]
fn non_generic_selector_on_generic_enum() {
    // `Other` carries no type param even though the enum is generic.
    let err: PayloadError<u32> = Other { detail: "x" }.build();
    assert_eq!(err.to_string(), "plain: x");
}

// ─── param used only in the source ───
// `E` appears only as the source type; the `Db` selector accepts a source of
// type `E` through `Contextual<E>`, the leaf `Annotation` selector is
// non-generic and infers `E` from the build site.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum SourcedError<E: std::error::Error + 'static> {
    #[oopsie("db query failed")]
    Db { source: E, query: String },
    #[oopsie("note: {text}")]
    Annotation { text: String },
}

#[test]
fn param_only_in_source() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
    let err: SourcedError<io::Error> = Db {
        query: "SELECT 1".to_owned(),
    }
    .build_error(io_err);
    assert_eq!(err.to_string(), "db query failed");
    let src = err.source().expect("Db exposes its source");
    assert_eq!(src.to_string(), "missing");
}

// ─── lifetime param: a borrowed field ───
// The selector carries `'a` because `note: &'a str` references it.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum BorrowedError<'a> {
    #[oopsie("borrowed note: {note}")]
    Note { note: &'a str },
}

#[test]
fn lifetime_param_borrowed_field() {
    let text = String::from("transient");
    let err: BorrowedError<'_> = Note {
        note: text.as_str(),
    }
    .build();
    assert_eq!(err.to_string(), "borrowed note: transient");
}

// ─── const param ───
// `N` is referenced by the `data: [u8; N]` field, so the selector carries it.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum ArrayError<const N: usize> {
    #[oopsie("fixed buffer of {} bytes", N)]
    Buffer { data: [u8; N] },
}

#[test]
fn const_param_in_field() {
    let err: ArrayError<4> = Buffer { data: [1, 2, 3, 4] }.build();
    assert_eq!(err.to_string(), "fixed buffer of 4 bytes");
    assert!(matches!(err, ArrayError::Buffer { data } if data == [1, 2, 3, 4]));
}

// ─── where-clause ───
// The where-clause rides the impls, not the selector struct.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum WhereError<T>
where
    T: fmt::Display + fmt::Debug,
{
    #[oopsie("value: {value}")]
    Value { value: T },
}

#[test]
fn where_clause_on_generic_enum() {
    let err: WhereError<i64> = Value { value: -3 }.build();
    assert_eq!(err.to_string(), "value: -3");
}

// ─── generic struct (module-form selector with generics) ───

#[derive(Debug, Oopsie)]
struct ParseFailure<T: fmt::Debug> {
    token: T,
    line: usize,
}

#[test]
fn generic_struct_module_selector() {
    let err: ParseFailure<char> = ParseFailureOopsie {
        token: '{',
        line: 12usize,
    }
    .build();
    assert!(matches!(err, ParseFailure { token, line } if token == '{' && line == 12));
}

// ─── transparent generic wrapper ───
// `From<E>` for `Wrapper<E>` exercises the pre-fixed impl/ty split.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum Wrapper<E: std::error::Error + 'static> {
    #[oopsie(display("wrapped"), transparent)]
    Inner { source: E },
}

#[test]
fn transparent_generic_wrapper() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
    let err: Wrapper<io::Error> = Wrapper::from(io_err);
    // Display is the variant's own string, not the source's.
    assert_eq!(err.to_string(), "wrapped");
    let src = err.source().expect("transparent exposes source");
    assert_eq!(src.to_string(), "denied");
}

// ─── selector field referencing a parameter keeps the parameter's type ───
// A field whose type names a parameter (`Vec<T>`) takes that type directly on
// the selector; the `Into` ergonomics apply only to parameter-free fields (the
// `count` field here). Mixing both in one variant exercises that split.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum BatchError<T: fmt::Debug> {
    #[oopsie("batch of {count} items")]
    Batch { items: Vec<T>, count: usize },
}

#[test]
fn selector_field_split_concrete_and_into() {
    // `items` is supplied as its exact `Vec<u8>`; `count` rides `Into<usize>`,
    // so the `u8` literal converts.
    let err: BatchError<u8> = Batch {
        items: vec![1u8, 2, 3],
        count: 3u8,
    }
    .build();
    assert_eq!(err.to_string(), "batch of 3 items");
}

// ─── traced generic enum + Diagnostic accessors ───
// Trace injection adds concrete `__oopsie_*` fields; they must not interact with
// the user's `T`, and the generics must thread through the re-derive.

#[cfg(feature = "tracing")]
mod traced_generics {
    use super::*;

    #[oopsie::oopsie(traced)]
    #[oopsie(module(false))]
    pub enum TracedError<T: fmt::Debug + 'static> {
        #[oopsie("traced payload {payload:?}")]
        Payload { payload: T },
    }

    #[test]
    fn traced_generic_enum_builds_and_displays() {
        let err: TracedError<String> = Payload {
            payload: "hi".to_owned(),
        }
        .build();
        assert_eq!(err.to_string(), "traced payload \"hi\"");
    }

    #[test]
    fn traced_generic_diagnostic_accessors() {
        let err: TracedError<u8> = Payload { payload: 9u8 }.build();
        // A backtrace is captured into the injected field and surfaced through
        // the Diagnostic accessor regardless of the user's type param.
        let _ = err.oopsie_backtrace();
        // An auto error code is derived from the type path.
        let code = err.oopsie_error_code().expect("auto code present");
        assert!(code.as_str().contains("TracedError::Payload"), "{code:?}");
    }
}

// ─── multi-param enum: sourced variant references every parameter ───
// `Query` carries both params (`E` in its source, `K` in a field), so its
// `Contextual` impl constrains both — the leaf `Decode` variant infers them.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum CacheError<K: fmt::Debug, E: std::error::Error + 'static> {
    #[oopsie("query for {key:?} failed")]
    Query { source: E, key: K },
    #[oopsie("decode error")]
    Decode { detail: String },
}

#[test]
fn multi_param_sourced_variant_references_all_params() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "miss");
    let err: CacheError<&str, io::Error> = Query { key: "users:7" }.build_error(io_err);
    assert_eq!(err.to_string(), "query for \"users:7\" failed");
    assert_eq!(
        err.source().expect("Query exposes source").to_string(),
        "miss"
    );
}

// ─── generic struct with a source field ───

#[derive(Debug, Oopsie)]
struct LoadError<E: std::error::Error + 'static> {
    source: E,
    path: String,
}

#[test]
fn generic_struct_with_source() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "nope");
    let err: LoadError<io::Error> = LoadOopsie {
        path: "/etc/conf".to_owned(),
    }
    .build_error(io_err);
    assert_eq!(err.path, "/etc/conf");
    assert_eq!(err.source().unwrap().to_string(), "nope");
}

// ─── inter-parameter inline bound: leaf references the dependent param only ───
// `U: From<T>` names `T` in its bound, but the convert variant's field uses only
// `U`. The selector carries `U` alone (bound-free struct); `T` rides
// `build`/`fail` as a method generic, where `U: From<T>` is back in scope. The
// seed variant keeps `T` non-degenerate so the enum itself is well-formed.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum InlineBoundError<T: fmt::Debug + Clone, U: From<T> + fmt::Debug> {
    #[oopsie("converted {value:?}")]
    InlineConvert { value: U },
    #[oopsie("seed {seed:?}")]
    InlineSeed { seed: T },
}

#[test]
fn inline_inter_parameter_bound_leaf() {
    // `T = u8`, `U = u16`; `U: From<T>` is satisfied, supplied as method generic.
    let err: InlineBoundError<u8, u16> = InlineConvert { value: 300u16 }.build();
    assert_eq!(err.to_string(), "converted 300");
    assert!(matches!(err, InlineBoundError::InlineConvert { value } if value == 300));
    // `fail` returns the same destination through its `Err` arm.
    let res: Result<(), InlineBoundError<u8, u16>> = InlineConvert { value: 7u16 }.fail();
    assert_eq!(res.unwrap_err().to_string(), "converted 7");
}

// ─── inter-parameter where-clause bound: same shape via a `where` clause ───

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum WhereBoundError<T, U>
where
    T: fmt::Debug + Clone,
    U: From<T> + fmt::Debug,
{
    #[oopsie("converted {value:?}")]
    WhereConvert { value: U },
    #[oopsie("seed {seed:?}")]
    WhereSeed { seed: T },
}

#[test]
fn where_clause_inter_parameter_bound_leaf() {
    let err: WhereBoundError<u8, u16> = WhereConvert { value: 256u16 }.build();
    assert_eq!(err.to_string(), "converted 256");
    let err: WhereBoundError<u8, u16> = WhereSeed { seed: 9u8 }.build();
    assert_eq!(err.to_string(), "seed 9");
}

// ─── inter-parameter bound on a sourced variant that references all params ───
// The sourced `Contextual` impl carries every error parameter, so `U: From<T>`
// is satisfiable there as long as the variant constrains both `T` and `U` (a
// variant that left `T` unreferenced is rejected by the unconstrained-param
// guard, covered by the `sourced_variant_unconstrained_param` trybuild case).

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum SourcedInterBound<
    T: fmt::Debug + Clone,
    U: From<T> + fmt::Debug,
    E: std::error::Error + 'static,
> {
    #[oopsie("converted {value:?} from {seed:?}")]
    SourcedConvert { source: E, value: U, seed: T },
}

#[test]
fn sourced_inter_parameter_bound_all_params() {
    let io_err = io::Error::new(io::ErrorKind::Other, "boom");
    let err: SourcedInterBound<u8, u16, io::Error> = SourcedConvert {
        value: 42u16,
        seed: 1u8,
    }
    .build_error(io_err);
    assert_eq!(err.to_string(), "converted 42 from 1");
    assert_eq!(err.source().expect("exposes source").to_string(), "boom");
}

// ─── leaf that does not reference an outer lifetime ───
// `Detached` references `T` but not `'a`, so `'a` is "free" and rides
// `build`/`fail` as a method generic. `fail` inserts an `Ok`-type param, and a
// free lifetime must stay ahead of it (lifetimes precede type/const params).
// `build`/`fail` share one impl block, so a malformed `fail` list poisons
// `build` too — every prior generic test referenced its param in the leaf, so
// none exercised a free lifetime.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum FreeLifetimeError<'a, T: fmt::Debug> {
    #[oopsie("ref: {note}")]
    RefNote { note: &'a str },
    #[oopsie("detached: {value:?}")]
    Detached { value: T },
}

#[test]
fn leaf_free_of_outer_lifetime() {
    let err: FreeLifetimeError<'_, u32> = Detached { value: 5u32 }.build();
    assert_eq!(err.to_string(), "detached: 5");
    // `fail` is the path that inserts `__T` ahead of the free params.
    let res: Result<(), FreeLifetimeError<'_, u32>> = Detached { value: 7u32 }.fail();
    assert_eq!(res.unwrap_err().to_string(), "detached: 7");
}

// ─── leaf freeing both a lifetime and a const param ───
// `FreeLeaf` references only `T`, freeing `'a` and `N`; `fail`'s list must order
// them `<'a, __T, N>`, not `<__T, 'a, N>`.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum MixedFreeError<'a, T: fmt::Debug, const N: usize> {
    #[oopsie("anchor {note} x{}", N)]
    Anchor { note: &'a str, data: [u8; N] },
    #[oopsie("free {value:?}")]
    FreeLeaf { value: T },
}

#[test]
fn leaf_free_of_lifetime_and_const() {
    let err: MixedFreeError<'_, u32, 4> = FreeLeaf { value: 9u32 }.build();
    assert_eq!(err.to_string(), "free 9");
    let res: Result<(), MixedFreeError<'_, u32, 4>> = FreeLeaf { value: 1u32 }.fail();
    assert_eq!(res.unwrap_err().to_string(), "free 1");
}

// ─── shorthand associated-type projection field ───
// `item: T::Item` names its base param `T` only in the path's leading segment,
// so `T` stays free (no field pins it) and the field rides `Into<T::Item>`. That
// bound must land on `build`/`fail`, which declare `T`, not the selector-scoped
// impl, which does not.

trait HasItem {
    type Item: fmt::Debug;
}

impl HasItem for u32 {
    type Item = u16;
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum ProjectionError<T: HasItem + fmt::Debug> {
    #[oopsie("item {item:?}")]
    Projected { item: T::Item },
}

#[test]
fn shorthand_associated_type_projection_field() {
    let err: ProjectionError<u32> = Projected { item: 5u16 }.build();
    assert_eq!(err.to_string(), "item 5");
    assert!(matches!(err, ProjectionError::Projected { item } if item == 5u16));
    let res: Result<(), ProjectionError<u32>> = Projected { item: 7u16 }.fail();
    assert_eq!(res.unwrap_err().to_string(), "item 7");
}

// ─── defaulted generic params (type and const) ───
// A default (`= u8`, `= 4`) is legal only on the type's own declaration; every
// impl header and method-generic list the derive emits must strip it, or rustc
// rejects them ("defaults for generic parameters are not allowed here"). The
// `Payload` leaf projects `T` onto its selector and frees the defaulted const
// `N`; `Buffer` frees the defaulted type `T`.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum DefaultedError<T: fmt::Debug = u8, const N: usize = 4> {
    #[oopsie("payload {payload:?}")]
    DefPayload { payload: T },
    #[oopsie("buffer of {} bytes", N)]
    DefBuffer { data: [u8; N] },
}

#[test]
fn defaulted_type_and_const_params() {
    let err: DefaultedError = DefPayload { payload: 9u8 }.build();
    assert_eq!(err.to_string(), "payload 9");
    let err: DefaultedError = DefBuffer { data: [1, 2, 3, 4] }.build();
    assert_eq!(err.to_string(), "buffer of 4 bytes");
    assert!(matches!(err, DefaultedError::DefBuffer { data } if data == [1, 2, 3, 4]));
}

// ─── defaulted param on a sourced struct ───
// The sourced `Contextual` impl carries the error's full generics; the default
// `= io::Error` on `E` must be stripped from that impl header.

#[derive(Debug, Oopsie)]
struct DefaultedLoad<E: std::error::Error + 'static = io::Error> {
    source: E,
    path: String,
}

#[test]
fn defaulted_param_sourced_struct() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "gone");
    let err: DefaultedLoad = DefaultedLoadOopsie {
        path: "/x".to_owned(),
    }
    .build_error(io_err);
    assert_eq!(err.path, "/x");
    assert_eq!(err.source().unwrap().to_string(), "gone");
}
