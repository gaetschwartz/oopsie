#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

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

// ---- Format specifiers passed verbatim to write! ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum DebugSpecError {
    #[oopsie("value {val:?}")]
    Value { val: i32 },
    #[oopsie("text {text:?}")]
    Text { text: String },
}

#[test]
fn debug_format_specifier() {
    let err = Value { val: 42i32 }.build();
    assert_eq!(format!("{err}"), "value 42");

    let err = Text {
        text: "hi".to_owned(),
    }
    .build();
    assert_eq!(format!("{err}"), "text \"hi\"");
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum IntSpecError {
    #[oopsie("code: {code:x}, mask: {mask:b}, perms: {perms:o}")]
    Bits { code: u32, mask: u8, perms: u16 },
}

#[test]
fn integer_format_specifiers() {
    let err = Bits {
        code: 0x2au32,
        mask: 0b11001001u8,
        perms: 0o755u16,
    }
    .build();
    assert_eq!(format!("{err}"), "code: 2a, mask: 11001001, perms: 755");
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum BraceEscapeError {
    #[oopsie("msg: {{x}}")]
    Literal {},
    #[oopsie("{{ {val} }}")]
    Mixed { val: u32 },
}

#[test]
fn brace_escaping() {
    let err = Literal {}.build();
    assert_eq!(format!("{err}"), "msg: {x}");

    let err = Mixed { val: 7u32 }.build();
    assert_eq!(format!("{err}"), "{ 7 }");
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum PadSpecError {
    #[oopsie("Value:[{value:>10}]")]
    RightAlign { value: u32 },
    #[oopsie("[{value:0<5}]")]
    FillLeft { value: u32 },
}

#[test]
fn width_align_padding_specifiers() {
    let err = RightAlign { value: 42u32 }.build();
    assert_eq!(format!("{err}"), "Value:[        42]");

    let err = FillLeft { value: 42u32 }.build();
    assert_eq!(format!("{err}"), "[42000]");
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum RepeatFieldError {
    #[oopsie("prefix {value} middle {value} suffix")]
    Repeat { value: String },
}

#[test]
fn same_field_used_twice() {
    let err = Repeat {
        value: "X".to_owned(),
    }
    .build();
    assert_eq!(err.to_string(), "prefix X middle X suffix");
}
