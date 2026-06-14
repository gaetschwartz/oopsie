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
    let err: ParseFailure<char> = parse_failure_oopsies::ParseFailure {
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
    let err: LoadError<io::Error> = load_oopsies::Load {
        path: "/etc/conf".to_owned(),
    }
    .build_error(io_err);
    assert_eq!(err.path, "/etc/conf");
    assert_eq!(err.source().unwrap().to_string(), "nope");
}
