#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(unused, clippy::all)]

use oopsie::{Contextual as _, Oopsie, ResultExt as _};
use std::error::Error as _;
use std::io;

// ---- Enum for source tests ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum MyError {
    WithSource { source: io::Error, info: String },

    Leaf { message: String },
}

// ---- Structs for source tests ----

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
struct StructWithSource {
    source: io::Error,
    detail: String,
}

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
struct StructNoSource {
    reason: String,
}

// ---- Source tests ----

#[test]
fn source_returns_some() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: MyError = WithSource { info: "ctx" }.build_error(io_err);
    let src = err.source();
    assert!(src.is_some());
    assert_eq!(src.unwrap().to_string(), "pipe broke");
}

#[test]
fn source_returns_none_leaf() {
    let err = Leaf { message: "oops" }.build();
    assert!(err.source().is_none());
}

#[test]
fn struct_source_some() {
    let io_err = io::Error::new(io::ErrorKind::AddrInUse, "in use");
    let err: StructWithSource = StructWithSourceOopsie { detail: "binding" }.build_error(io_err);
    let src = err.source();
    assert!(src.is_some());
    assert_eq!(src.unwrap().to_string(), "in use");
}

#[test]
fn struct_source_none() {
    let err = StructNoSourceOopsie {
        reason: "just because",
    }
    .build();
    assert!(err.source().is_none());
}

#[test]
fn error_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MyError>();
    assert_send_sync::<StructWithSource>();
    assert_send_sync::<StructNoSource>();
}

// ---- Unstable provide tests ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum ProvideError {
    #[oopsie("bt error")]
    #[oopsie(provide(ref, oopsie::BackTrace => bt.as_ref()))]
    WithBacktrace {
        msg: String,
        #[oopsie(capture)]
        bt: Box<oopsie::BackTrace>,
    },

    #[oopsie("help error")]
    #[oopsie(provide(::oopsie::HelpText => ::oopsie::HelpText(::std::borrow::Cow::Borrowed("try rebooting"))))]
    WithHelp { msg: String },

    #[oopsie("coded error")]
    #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from("app::parse")))]
    WithCode { msg: String },
}

#[cfg(feature = "unstable")]
#[test]
fn provide_backtrace_ref() {
    let err = WithBacktrace { msg: "boom" }.build();
    let bt = core::error::request_ref::<oopsie::BackTrace>(&err);
    assert!(bt.is_some(), "should provide a Backtrace ref");
}

#[cfg(feature = "unstable")]
#[test]
fn provide_help_text() {
    let err = WithHelp { msg: "broken" }.build();
    let help = core::error::request_value::<oopsie::HelpText>(&err);
    assert!(help.is_some(), "should provide HelpText");
    assert_eq!(&*help.unwrap(), "try rebooting");
}

#[cfg(feature = "unstable")]
#[test]
fn provide_error_code() {
    let err = WithCode { msg: "bad input" }.build();
    let code = core::error::request_value::<oopsie::ErrorCode>(&err);
    assert!(code.is_some(), "should provide ErrorCode");
    assert_eq!(&*code.unwrap(), "app::parse");
}

// ---- Nested error propagation tests ----
//
// These test that backtrace, spantrace, help text, and error codes from inner
// errors are accessible through outer errors via the provide() chain.

/// Inner error that carries a backtrace and help text.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum InnerError {
    #[oopsie("inner: {detail}")]
    #[oopsie(provide(ref, oopsie::BackTrace => bt.as_ref()))]
    #[oopsie(provide(::oopsie::HelpText => ::oopsie::HelpText(::std::borrow::Cow::Borrowed("fix the inner thing"))))]
    #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from("inner::code")))]
    Root {
        detail: String,
        #[oopsie(capture)]
        bt: Box<oopsie::BackTrace>,
    },
}

/// Mid-level error that wraps InnerError.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum MiddleError {
    #[oopsie("middle: {context}")]
    Wrapped { source: InnerError, context: String },
}

/// Outer error that wraps MiddleError.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum OuterError {
    #[oopsie("outer: {label}")]
    Top { source: MiddleError, label: String },
}

#[test]
fn nested_source_chain_two_levels() {
    let inner = Root {
        detail: "db failed",
    }
    .build();
    let middle: MiddleError = Wrapped { context: "query" }.build_error(inner);
    let outer: OuterError = Top { label: "request" }.build_error(middle);

    // Display
    assert_eq!(outer.to_string(), "outer: request");

    // source() chain: outer -> middle -> inner
    let mid = outer.source().expect("outer should have source");
    assert_eq!(mid.to_string(), "middle: query");

    let inn = mid.source().expect("middle should have source");
    assert_eq!(inn.to_string(), "inner: db failed");

    assert!(inn.source().is_none(), "inner is a leaf error");
}

#[test]
fn nested_source_chain_via_context() {
    // Build inner error, then wrap using ResultExt::context()
    let inner = Root { detail: "timeout" }.build();
    let result: Result<(), InnerError> = Err(inner);
    let middle: Result<(), MiddleError> = result.context(Wrapped {
        context: "connecting",
    });
    let result2: Result<(), OuterError> = middle.context(Top { label: "service" });

    let outer = result2.unwrap_err();
    assert_eq!(outer.to_string(), "outer: service");

    let mid = outer.source().unwrap();
    assert_eq!(mid.to_string(), "middle: connecting");

    let inn = mid.source().unwrap();
    assert_eq!(inn.to_string(), "inner: timeout");
}

#[cfg(feature = "unstable")]
#[test]
fn nested_backtrace_propagated_through_source_chain() {
    // Inner error has a backtrace. Wrap it two levels deep.
    let inner = Root { detail: "crash" }.build();
    let middle: MiddleError = Wrapped {
        context: "processing",
    }
    .build_error(inner);
    let outer: OuterError = Top { label: "handler" }.build_error(middle);

    // The backtrace should be accessible from the inner error directly
    let inner_ref = outer.source().unwrap().source().unwrap();
    let bt_direct = core::error::request_ref::<oopsie::BackTrace>(inner_ref);
    assert!(
        bt_direct.is_some(),
        "inner error should provide backtrace directly"
    );

    // The backtrace should also be accessible from the outer error,
    // because provide() forwards to source.provide() at each level
    let bt_outer = core::error::request_ref::<oopsie::BackTrace>(&outer);
    assert!(
        bt_outer.is_some(),
        "backtrace should propagate through source chain to outer"
    );
}

#[cfg(feature = "unstable")]
#[test]
fn nested_help_text_propagated_through_source_chain() {
    let inner = Root { detail: "issue" }.build();
    let middle: MiddleError = Wrapped {
        context: "handling",
    }
    .build_error(inner);
    let outer: OuterError = Top { label: "api" }.build_error(middle);

    // HelpText from inner should be accessible from outer via provide chain
    let help = core::error::request_value::<oopsie::HelpText>(&outer);
    assert!(
        help.is_some(),
        "HelpText should propagate through source chain"
    );
    assert_eq!(&*help.unwrap(), "fix the inner thing");
}

#[cfg(feature = "unstable")]
#[test]
fn nested_error_code_propagated_through_source_chain() {
    let inner = Root {
        detail: "parse fail",
    }
    .build();
    let middle: MiddleError = Wrapped {
        context: "decoding",
    }
    .build_error(inner);
    let outer: OuterError = Top { label: "ingest" }.build_error(middle);

    // ErrorCode from inner should be accessible from outer via provide chain
    let code = core::error::request_value::<oopsie::ErrorCode>(&outer);
    assert!(
        code.is_some(),
        "ErrorCode should propagate through source chain"
    );
    assert_eq!(&*code.unwrap(), "inner::code");
}

/// Test that `#[traced]` + `#[derive(Oopsie)]` properly propagates
/// backtrace through nested errors.
#[oopsie::traced]
#[derive(Debug, oopsie::Oopsie)]
pub enum AttrInnerError {
    #[oopsie("attr inner: {msg}")]
    Boom { msg: String },
}

#[oopsie::traced]
#[derive(Debug, oopsie::Oopsie)]
pub enum AttrOuterError {
    #[oopsie("attr outer: {ctx}")]
    Wrapper { source: AttrInnerError, ctx: String },
}

#[test]
fn nested_attr_macro_source_chain() {
    use attr_inner_oopsies::Boom;
    use attr_outer_oopsies::Wrapper;

    let inner = Boom { msg: "exploded" }.build();
    let outer: AttrOuterError = Wrapper { ctx: "defusing" }.build_error(inner);

    assert_eq!(outer.to_string(), "attr outer: defusing");
    let src = outer.source().unwrap();
    assert_eq!(src.to_string(), "attr inner: exploded");
}

#[cfg(feature = "unstable")]
#[test]
fn nested_attr_macro_backtrace_propagated() {
    use attr_inner_oopsies::Boom;
    use attr_outer_oopsies::Wrapper;

    let inner = Boom { msg: "kaboom" }.build();
    let outer: AttrOuterError = Wrapper { ctx: "handling" }.build_error(inner);

    // #[traced] injects backtrace with provide(ref, Backtrace => ...)
    // on each variant. The outer error's provide() forwards to source.provide(),
    // so the inner's backtrace should be reachable from the outer.
    let bt = core::error::request_ref::<oopsie::BackTrace>(&outer);
    assert!(
        bt.is_some(),
        "backtrace from #[traced] inner should propagate to outer"
    );
}

// ---- Bug fix: help/code with bare #[derive(Oopsie)] using = syntax ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum BareHelpError {
    #[oopsie(display("need help"), help = "Try rebooting")]
    NeedHelp { detail: String },
}

#[cfg(feature = "unstable")]
#[test]
fn provide_help_text_bare_derive() {
    let err = NeedHelp { detail: "stuck" }.build();
    let help = core::error::request_value::<oopsie::HelpText>(&err);
    assert!(help.is_some(), "should provide HelpText");
    assert_eq!(&*help.unwrap(), "Try rebooting");
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum BareCodeError {
    #[oopsie(display("coded error"), code = "bare::code")]
    Coded { msg: String },
}

#[cfg(feature = "unstable")]
#[test]
fn provide_error_code_bare_derive() {
    let err = Coded { msg: "fail" }.build();
    let code = core::error::request_value::<oopsie::ErrorCode>(&err);
    assert!(code.is_some(), "should provide ErrorCode");
    assert_eq!(&*code.unwrap(), "bare::code");
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum BareHelpCodeError {
    #[oopsie(display("both"), help = "help text", code = "app::both")]
    Both { info: String },
}

#[cfg(feature = "unstable")]
#[test]
fn provide_help_and_code_combined() {
    let err = Both { info: "combo" }.build();
    let help = core::error::request_value::<oopsie::HelpText>(&err);
    assert!(help.is_some(), "should provide HelpText");
    assert_eq!(&*help.unwrap(), "help text");
    let code = core::error::request_value::<oopsie::ErrorCode>(&err);
    assert!(code.is_some(), "should provide ErrorCode");
    assert_eq!(&*code.unwrap(), "app::both");
}

// ---- Bug fix: struct provide() works correctly ----

#[oopsie::traced]
#[derive(Debug, oopsie::Oopsie)]
pub struct AttrStructWithBt {
    msg: String,
}

#[cfg(feature = "unstable")]
#[test]
fn struct_provide_backtrace() {
    let err = AttrStructWithBtOopsie { msg: "test" }.build();
    let bt = core::error::request_ref::<oopsie::BackTrace>(&err);
    assert!(
        bt.is_some(),
        "struct provide() should correctly provide backtrace"
    );
}

// ---- Dynamic help field annotation ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum DynamicHelpError {
    #[oopsie("validation failed")]
    WithDynamicHelp {
        #[oopsie(help)]
        suggestion: String,
    },

    #[oopsie("other error")]
    NoHelp { msg: String },
}

#[test]
fn dynamic_help_field_returns_value() {
    use oopsie::ErrorExt as _;
    let err = WithDynamicHelp {
        suggestion: "try a shorter name",
    }
    .build();
    let help = err.oopsie_help_text();
    assert!(help.is_some(), "should return dynamic help text");
    assert_eq!(&*help.unwrap(), "try a shorter name");
}

#[test]
fn no_help_field_returns_none() {
    use oopsie::ErrorExt as _;
    let err = NoHelp { msg: "boom" }.build();
    assert!(
        err.oopsie_help_text().is_none(),
        "variant without help should return None"
    );
}
