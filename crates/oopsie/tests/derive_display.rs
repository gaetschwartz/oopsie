#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(unused, clippy::all)]

use oopsie::Oopsie;

// ---- Enum with display attributes ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum NetError {
    #[oopsie("connection failed: {addr}")]
    ConnFailed {
        addr: String,
    },

    #[oopsie("error at {file}:{line}")]
    Location {
        file: String,
        line: u32,
    },

    PlainVariant {
        code: u16,
    },
}

// ---- Structs with display attributes ----

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
#[oopsie("parse error: {msg}")]
struct ParseErr {
    msg: String,
}

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
struct BareStruct {
    value: i32,
}

// ---- Tests ----

#[test]
fn enum_field_interpolation() {
    let err = ConnFailed {
        addr: "10.0.0.1:443",
    }
    .build();
    assert_eq!(format!("{err}"), "connection failed: 10.0.0.1:443");
}

#[test]
fn enum_multiple_fields() {
    let err = Location {
        file: "main.rs",
        line: 42u32,
    }
    .build();
    assert_eq!(format!("{err}"), "error at main.rs:42");
}

#[test]
fn enum_no_display_fallback() {
    // No #[oopsie("...")] on PlainVariant, so Display uses the variant name.
    let err = PlainVariant { code: 500u16 }.build();
    assert_eq!(format!("{err}"), "PlainVariant");
}

#[test]
fn struct_field_interpolation() {
    let err = ParseErrOopsie {
        msg: "unexpected EOF",
    }
    .build();
    assert_eq!(format!("{err}"), "parse error: unexpected EOF");
}

#[test]
fn struct_no_display_fallback() {
    let err = BareStructOopsie { value: 99i32 }.build();
    assert_eq!(format!("{err}"), "BareStruct");
}

// ---- Bug fix: short form display with positional format args ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum PositionalError {
    #[oopsie("Got {} errors", count)]
    Multiple { count: u32 },
}

#[test]
fn short_form_with_positional_args() {
    let err = Multiple { count: 5u32 }.build();
    assert_eq!(err.to_string(), "Got 5 errors");
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum LongFormError {
    #[oopsie(display("Got {} errors from {}", count, host))]
    Stats { count: u32, host: String },
}

#[test]
fn long_form_display_with_args() {
    let err = Stats {
        count: 3u32,
        host: "db",
    }
    .build();
    assert_eq!(err.to_string(), "Got 3 errors from db");
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum ShortFormArgsError {
    #[oopsie("code {} at {}", code, location)]
    WithArgs { code: u32, location: String },
}

#[test]
fn short_form_with_args_and_other_attrs() {
    let err = WithArgs {
        code: 42u32,
        location: "main.rs",
    }
    .build();
    assert_eq!(err.to_string(), "code 42 at main.rs");
}
