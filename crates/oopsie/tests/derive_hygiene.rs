//! Regression tests for proc-macro identifier-hygiene defects. Each
//! previously-broken combination is exercised here so the generated code is
//! forced to compile and behave:
//!
//! - a renamed `#[oopsie(from)]` source combined with a `#[oopsie(capture)]`
//!   field made `build_error` move `source` and then borrow it (`E0382`).
//! - a user field literally named `f` shadowed the `Formatter` parameter
//!   in the generated `Display::fmt`.
//! - a user field literally named `request` shadowed the `&mut Request`
//!   parameter in the generated `Error::provide` (unstable feature only).
#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use oopsie::{Contextual as _, Oopsie};
use std::error::Error as _;
use std::io;

// ── renamed `#[oopsie(from)]` source + a capture field (enum) ───────────────

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum RenamedFromEnum {
    #[oopsie("wrapped: {inner}")]
    Wrapped {
        #[oopsie(from)]
        inner: io::Error,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
    },
}

#[test]
fn c1_renamed_from_source_plus_capture_enum() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: RenamedFromEnum = Wrapped.build_error(io_err);
    assert!(matches!(err, RenamedFromEnum::Wrapped { .. }));
    assert_eq!(err.source().expect("has source").to_string(), "pipe broke");
}

// ── renamed `#[oopsie(from)]` source + a capture field (struct) ─────────────

#[derive(Debug, Oopsie)]
struct RenamedFromStructError {
    #[oopsie(from)]
    inner: io::Error,
    #[oopsie(capture)]
    bt: Box<oopsie::Backtrace>,
}

#[test]
fn c2_renamed_from_source_plus_capture_struct() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: RenamedFromStructError = RenamedFromStructOopsie.build_error(io_err);
    assert_eq!(err.source().expect("has source").to_string(), "pipe broke");
}

// ── user field literally named `f` (enum Display) ───────────────────────────

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum FieldNamedFEnum {
    #[oopsie("value is {f}")]
    Holds { f: u32 },
}

#[test]
fn c5_field_named_f_display_enum() {
    let err = FieldNamedFEnum::Holds { f: 42 };
    assert_eq!(err.to_string(), "value is 42");
}

// ── user field literally named `f` (struct Display) ─────────────────────────

#[derive(Debug, Oopsie)]
#[oopsie("value is {f}")]
struct FieldNamedFStruct {
    f: u32,
}

#[test]
fn c6_field_named_f_display_struct() {
    let err = FieldNamedFStruct { f: 7 };
    assert_eq!(err.to_string(), "value is 7");
}

// ── user field literally named `request` (provide; unstable only) ───────────

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum FieldNamedRequestEnum {
    #[oopsie("req error")]
    #[oopsie(provide(::oopsie::HelpText => ::oopsie::HelpText::from(request.clone())))]
    V { request: String },
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn c7_field_named_request_in_provide() {
    let err = FieldNamedRequestEnum::V {
        request: "payload".to_owned(),
    };
    let help = core::error::request_value::<oopsie::HelpText>(&err);
    assert_eq!(help.as_deref(), Some("payload"));
}
