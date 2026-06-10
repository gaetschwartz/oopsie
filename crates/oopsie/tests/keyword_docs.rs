//! Compile-pass coverage for the hover-documentation `use` blocks the derive
//! and the `#[oopsie]` attribute macro emit: the fixtures below exercise
//! every `#[oopsie(...)]` keyword at its scope, so a keyword that parses but
//! has no entry in `oopsie::__private::documented` fails this build with an
//! unresolved import.

#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use std::io;

use oopsie::{Backtrace, Contextual as _, Oopsie, SpanTrace};

// ---- Enum: container keywords (module, suffix, size, path, vis) plus every
// ---- variant- and field-level keyword across its variants.

#[derive(Debug, Oopsie)]
#[oopsie(module(kitchen_sink))]
#[oopsie(suffix = "Ctx")]
#[oopsie(size(..=64))]
#[oopsie(path = "::oopsie")]
#[oopsie(vis(pub(crate)))]
enum KitchenSinkError {
    #[oopsie(
        display("formatted: {detail}"),
        help = "variant help",
        code = "kw::variant"
    )]
    #[oopsie(provide(::oopsie::HelpText => ::oopsie::HelpText::from_static("variant provide")))]
    #[oopsie(vis(pub(crate)))]
    Formatted { detail: String },

    #[oopsie(transparent)]
    Wrapped { source: io::Error },

    #[oopsie("from keyword: {inner}")]
    Marked {
        #[oopsie(from)]
        inner: io::Error,
    },

    #[oopsie("capture keyword")]
    Captured {
        #[oopsie(capture)]
        at: std::time::SystemTime,
    },

    #[oopsie("field provide keyword")]
    FieldProvide {
        #[oopsie(provide(::oopsie::HelpText => ::oopsie::HelpText::from(hint.clone())))]
        hint: String,
    },

    #[oopsie("split trace keywords")]
    SplitTraces {
        #[oopsie(backtrace)]
        bt: Box<Backtrace>,
        #[oopsie(spantrace)]
        st: Box<SpanTrace>,
    },

    #[oopsie("packed traces keyword")]
    Packed {
        #[oopsie(traces)]
        traces: Box<(Backtrace, SpanTrace)>,
    },

    #[oopsie("dynamic help keyword: {hint}")]
    DynamicHelp {
        #[oopsie(help)]
        hint: String,
    },
}

// ---- Struct: container and variant keywords mixed in one `#[oopsie(...)]`
// ---- scope, exercising the container-vs-variant routing, plus
// ---- `from(Type, transform)`.

#[derive(Debug, Oopsie)]
#[oopsie(module(struct_sink), suffix(false), size(..=64), path = "::oopsie", vis(pub(crate)))]
#[oopsie(display("struct: {name}"), help("try {name}"), code = "kw::struct")]
#[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from("kw::struct")))]
struct StructSinkError {
    name: String,
    #[oopsie(from(io::Error, Box::new))]
    cause: Box<io::Error>,
}

// ---- Attribute macro: every top-level argument (traced, code, path) and
// ---- every nested key (backtrace, spantrace, timestamp, packed, boxed,
// ---- chrono, provide, r#type, enabled) in their settings-list forms.

#[oopsie::oopsie(
    traced(
        backtrace(r#type = ::oopsie::Backtrace, boxed = true, enabled = true),
        spantrace(enabled = true),
        timestamp(chrono = false, provide = true),
        packed = false,
        boxed = true
    ),
    code(r#type = ::oopsie::ErrorCode),
    path = "::oopsie"
)]
enum AttrSinkError {
    #[oopsie("attr sink: {info}")]
    Boom { info: String },
}

// ---- Attribute macro: the bool forms of the top-level arguments.

#[oopsie::oopsie(traced, code = false)]
enum AttrFlagError {
    #[oopsie("flag form")]
    Flagged,
}

#[test]
fn fixtures_construct() {
    let err = kitchen_sink::FormattedCtx {
        detail: "d".to_owned(),
    }
    .build();
    assert_eq!(err.to_string(), "formatted: d");

    let err: KitchenSinkError = io::Error::other("io").into();
    assert!(matches!(err, KitchenSinkError::Wrapped { .. }));

    let err = struct_sink::StructSink {
        name: "n".to_owned(),
    }
    .build_error(io::Error::other("io"));
    assert_eq!(err.to_string(), "struct: n");

    let err = attr_sink_oopsies::Boom {
        info: "i".to_owned(),
    }
    .build();
    assert_eq!(err.to_string(), "attr sink: i");

    let err = attr_flag_oopsies::Flagged.build();
    assert_eq!(err.to_string(), "flag form");
}
