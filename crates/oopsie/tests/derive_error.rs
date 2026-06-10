#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

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
    #[oopsie(provide(ref, oopsie::Backtrace => bt.as_ref()))]
    WithBacktrace {
        msg: String,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
    },

    #[oopsie("help error")]
    #[oopsie(provide(::oopsie::HelpText => ::oopsie::HelpText::from_static("try rebooting")))]
    WithHelp { msg: String },

    #[oopsie("coded error")]
    #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from("app::parse")))]
    WithCode { msg: String },

    // Regression: a variant-level `provide` expr that references a bare user
    // field must bind that field in the generated `provide()` match arm.
    #[oopsie("field help error")]
    #[oopsie(provide(::oopsie::HelpText => ::oopsie::HelpText::from(msg.clone())))]
    WithFieldHelp { msg: String },
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn provide_backtrace_ref() {
    let err = WithBacktrace { msg: "boom" }.build();
    let bt = core::error::request_ref::<oopsie::Backtrace>(&err);
    assert!(bt.is_some(), "should provide a Backtrace ref");
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn provide_help_text() {
    let err = WithHelp { msg: "broken" }.build();
    let help = core::error::request_value::<oopsie::HelpText>(&err);
    assert!(help.is_some(), "should provide HelpText");
    assert_eq!(&*help.unwrap(), "try rebooting");
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn provide_error_code() {
    let err = WithCode { msg: "bad input" }.build();
    let code = core::error::request_value::<oopsie::ErrorCode>(&err);
    assert!(code.is_some(), "should provide ErrorCode");
    assert_eq!(&*code.unwrap(), "app::parse");
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn provide_help_text_from_user_field() {
    let err = WithFieldHelp {
        msg: "needs attention",
    }
    .build();
    let help = core::error::request_value::<oopsie::HelpText>(&err);
    assert!(
        help.is_some(),
        "should provide HelpText built from a user field"
    );
    assert_eq!(&*help.unwrap(), "needs attention");
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
    #[oopsie(provide(ref, oopsie::Backtrace => bt.as_ref()))]
    #[oopsie(provide(::oopsie::HelpText => ::oopsie::HelpText::from_static("fix the inner thing")))]
    #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from("inner::code")))]
    Root {
        detail: String,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
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

#[cfg(feature = "unstable-error-generic-member-access")]
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
    let bt_direct = core::error::request_ref::<oopsie::Backtrace>(inner_ref);
    assert!(
        bt_direct.is_some(),
        "inner error should provide backtrace directly"
    );

    // The backtrace should also be accessible from the outer error,
    // because provide() forwards to source.provide() at each level
    let bt_outer = core::error::request_ref::<oopsie::Backtrace>(&outer);
    assert!(
        bt_outer.is_some(),
        "backtrace should propagate through source chain to outer"
    );
}

#[cfg(feature = "unstable-error-generic-member-access")]
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

#[cfg(feature = "unstable-error-generic-member-access")]
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

/// Test that `#[oopsie(traced)]` properly propagates
/// backtrace through nested errors.
#[oopsie::oopsie(traced)]
pub enum AttrInnerError {
    #[oopsie("attr inner: {msg}")]
    Boom { msg: String },
}

#[oopsie::oopsie(traced)]
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

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn nested_attr_macro_backtrace_propagated() {
    use attr_inner_oopsies::Boom;
    use attr_outer_oopsies::Wrapper;

    // Empty traces are withheld from provide(), so capture must succeed.
    oopsie_core::test_utils::force_backtrace();
    let inner = Boom { msg: "kaboom" }.build();
    let outer: AttrOuterError = Wrapper { ctx: "handling" }.build_error(inner);

    // #[oopsie(traced)] injects backtrace with provide(ref, Backtrace => ...)
    // on each variant. The outer error's provide() forwards to source.provide(),
    // so the inner's backtrace should be reachable from the outer.
    let bt = core::error::request_ref::<oopsie::Backtrace>(&outer);
    assert!(
        bt.is_some(),
        "backtrace from #[oopsie(traced)] inner should propagate to outer"
    );
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn stable_accessor_surfaces_deepest_trace() {
    use attr_inner_oopsies::Boom;
    use attr_outer_oopsies::Wrapper;
    use oopsie::Diagnostic as _;
    use tracing::instrument;

    #[instrument(target = "test")]
    fn make_inner() -> AttrInnerError {
        attr_inner_oopsies::Boom { msg: "deep" }.build()
    }

    // Empty traces are withheld from provide(), so both captures must
    // succeed: force backtraces and build inside an instrumented span.
    oopsie_core::test_utils::force_backtrace();
    let _guard = oopsie_core::test_utils::init_test_subscriber();

    let inner = make_inner();
    let outer: AttrOuterError = Wrapper { ctx: "shallow" }.build_error(inner);

    // The unstable provider path walks source-first under std's first-wins
    // `Request`, so it surfaces the deepest (inner, origin-most) trace.
    let bt_provide = core::error::request_ref::<oopsie::Backtrace>(&outer)
        .expect("provider path yields a backtrace");
    let st_provide = core::error::request_ref::<oopsie::SpanTrace>(&outer)
        .expect("provider path yields a span trace");

    // The stable accessor must agree: the same deepest trace, not the outer
    // wrapper's shallow wrap-site one. Pointer identity proves we surfaced the
    // very same `Backtrace`/`SpanTrace` the provider path filled the slot with.
    let bt_stable = outer
        .oopsie_backtrace()
        .expect("stable accessor yields a backtrace");
    let st_stable = outer
        .oopsie_spantrace()
        .expect("stable accessor yields a span trace");

    assert!(
        std::ptr::eq(bt_provide, bt_stable),
        "oopsie_backtrace() must surface the same deepest backtrace as the provider API, \
         not the outer wrapper's wrap-site one"
    );
    assert!(
        std::ptr::eq(st_provide, st_stable),
        "oopsie_spantrace() must surface the same deepest span trace as the provider API"
    );
}

// ---- Bug fix: help/code with bare #[derive(Oopsie)] using = syntax ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum BareHelpError {
    #[oopsie(display("need help"), help = "Try rebooting")]
    NeedHelp { detail: String },
}

#[cfg(feature = "unstable-error-generic-member-access")]
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

#[cfg(feature = "unstable-error-generic-member-access")]
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

#[cfg(feature = "unstable-error-generic-member-access")]
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

#[oopsie::oopsie(traced)]
pub struct AttrStructWithBt {
    msg: String,
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn struct_provide_backtrace() {
    // Empty traces are withheld from provide(), so capture must succeed.
    oopsie_core::test_utils::force_backtrace();
    let err = AttrStructWithBtOopsie { msg: "test" }.build();
    let bt = core::error::request_ref::<oopsie::Backtrace>(&err);
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
    use oopsie::Diagnostic as _;
    let err = WithDynamicHelp {
        suggestion: "try a shorter name",
    }
    .build();
    let help = err.oopsie_help_text();
    assert!(help.is_some(), "should return dynamic help text");
    assert_eq!(&*help.unwrap(), "try a shorter name");
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn dynamic_help_field_provider_matches_accessor() {
    use oopsie::Diagnostic as _;
    let err = WithDynamicHelp {
        suggestion: "try a shorter name",
    }
    .build();
    let provided = core::error::request_value::<oopsie::HelpText>(&err);
    assert!(provided.is_some(), "provide() should yield dynamic help");
    assert_eq!(provided, err.oopsie_help_text());
}

#[test]
fn no_help_field_returns_none() {
    use oopsie::Diagnostic as _;
    let err = NoHelp { msg: "boom" }.build();
    assert!(
        err.oopsie_help_text().is_none(),
        "variant without help should return None"
    );
}

// Struct parity: a struct with only a `#[oopsie(help)]` field must agree
// between `oopsie_help_text()` and `request_value::<HelpText>`.

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
struct StructDynamicHelp {
    #[oopsie(help)]
    suggestion: String,
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn struct_dynamic_help_field_provider_matches_accessor() {
    use oopsie::Diagnostic as _;
    let err = StructDynamicHelpOopsie {
        suggestion: "try a shorter name".to_owned(),
    }
    .build();
    let provided = core::error::request_value::<oopsie::HelpText>(&err);
    assert!(provided.is_some(), "provide() should yield dynamic help");
    assert_eq!(provided, err.oopsie_help_text());
}

// ---- help() format-string interpolation referencing variant fields ----
//
// `help("fmt {}", expr)` parses as a DisplayAttr and renders through `::std::format!`,
// exactly like `display(...)`. The generated `oopsie_help_text()` accessor binds the
// variant's fields (mirroring the display arm), so positional args may reference them.
// (Inline `{field}` capture without an explicit arg is covered separately below.
// `code = "..."` stays a plain string.)

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum HelpInterpError {
    // Single positional arg referencing the `host` field.
    #[oopsie("connect failed")]
    #[oopsie(help("Retry connecting to {}", host))]
    SingleField { host: String },

    // Multiple positional args, each referencing a variant field.
    #[oopsie("ingest failed")]
    #[oopsie(help("got {} errors in {}", count, module))]
    MultiField { count: u32, module: String },
}

#[test]
fn help_single_field_interpolation_renders_value() {
    use oopsie::Diagnostic as _;
    let err = SingleField {
        host: "db.local".to_owned(),
    }
    .build();
    let help = err.oopsie_help_text();
    assert!(help.is_some(), "should render interpolated help text");
    assert_eq!(&*help.unwrap(), "Retry connecting to db.local");
}

#[test]
fn help_multi_field_interpolation_renders_value() {
    use oopsie::Diagnostic as _;
    let err = MultiField {
        count: 3u32,
        module: "ingest".to_owned(),
    }
    .build();
    let help = err.oopsie_help_text();
    assert!(help.is_some(), "should render interpolated help text");
    assert_eq!(&*help.unwrap(), "got 3 errors in ingest");
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn help_field_interpolation_via_provider() {
    let err = SingleField {
        host: "db.local".to_owned(),
    }
    .build();
    let help = core::error::request_value::<oopsie::HelpText>(&err);
    assert!(
        help.is_some(),
        "provider path should yield interpolated help"
    );
    assert_eq!(&*help.unwrap(), "Retry connecting to db.local");
}

// ---- help inline-capture interpolation (`{field}` with no explicit arg) ----
//
// `help = "{field}"` / `help("{field}")` carry no trailing args, but the format
// string references a field by inline capture (like `format!("{x}")`). The macro
// must detect the placeholder and render through `format!`, binding the fields —
// not store the literal verbatim via `from_static`. A brace-free or escaped-only
// string still takes the cheap static path.

#[derive(Debug, Oopsie)]
#[oopsie(suffix, help = "fix the file at {path}")]
struct StructInlineHelp {
    path: String,
}

#[test]
fn struct_inline_capture_help_renders_field() {
    use oopsie::Diagnostic as _;
    let err = StructInlineHelpOopsie {
        path: "/etc/hosts".to_owned(),
    }
    .build();
    let help = err.oopsie_help_text();
    assert!(help.is_some(), "inline-capture help should render");
    assert_eq!(&*help.unwrap(), "fix the file at /etc/hosts");
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum HelpInlineError {
    // List form, inline capture, no explicit arg.
    #[oopsie("connect failed")]
    #[oopsie(help("retry connecting to {host}"))]
    ListForm { host: String },

    // Name-value form, inline capture, no explicit arg.
    #[oopsie("restart needed")]
    #[oopsie(help = "restart {service}")]
    NameValueForm { service: String },

    // Escaped braces only, no real placeholder: stays on the static path but
    // must unescape `{{`/`}}` to match format-string semantics.
    #[oopsie("templating")]
    #[oopsie(help = "wrap names in {{braces}}")]
    EscapedOnly { unused: String },
}

#[test]
fn enum_inline_capture_help_list_form_renders_field() {
    use oopsie::Diagnostic as _;
    let err = ListForm {
        host: "db.local".to_owned(),
    }
    .build();
    let help = err.oopsie_help_text();
    assert!(help.is_some(), "inline-capture help should render");
    assert_eq!(&*help.unwrap(), "retry connecting to db.local");
}

#[test]
fn enum_inline_capture_help_name_value_form_renders_field() {
    use oopsie::Diagnostic as _;
    let err = NameValueForm {
        service: "nginx".to_owned(),
    }
    .build();
    let help = err.oopsie_help_text();
    assert!(help.is_some(), "inline-capture help should render");
    assert_eq!(&*help.unwrap(), "restart nginx");
}

#[test]
fn help_escaped_braces_unescape_on_static_path() {
    use oopsie::Diagnostic as _;
    let err = EscapedOnly {
        unused: "x".to_owned(),
    }
    .build();
    let help = err.oopsie_help_text();
    assert!(help.is_some(), "escaped-brace help should render");
    assert_eq!(&*help.unwrap(), "wrap names in {braces}");
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn help_inline_capture_via_provider() {
    let err = ListForm {
        host: "db.local".to_owned(),
    }
    .build();
    let help = core::error::request_value::<oopsie::HelpText>(&err);
    assert!(
        help.is_some(),
        "provider path should render interpolated help"
    );
    assert_eq!(&*help.unwrap(), "retry connecting to db.local");
}

// ---- oopsie_error_code() stable accessor (static + dynamic code) ----
//
// `code = "..."` is a plain string with no interpolation. The stable
// `oopsie_error_code()` accessor must surface it on stable toolchains; the
// Provider API (`request_value::<ErrorCode>`) reaches parity on nightly.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum CodeAccessorError {
    #[oopsie("static coded")]
    #[oopsie(code = "test::code")]
    StaticCode { msg: String },

    #[oopsie("dynamic coded")]
    #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from("dyn::code")))]
    DynamicCode { msg: String },

    #[oopsie("uncoded")]
    NoCode { msg: String },
}

#[test]
fn error_code_accessor_returns_static_code() {
    use oopsie::Diagnostic as _;
    let err = StaticCode {
        msg: "boom".to_owned(),
    }
    .build();
    let code = err.oopsie_error_code();
    assert!(code.is_some(), "static code should be accessible on stable");
    assert_eq!(code.unwrap().as_str(), "test::code");
}

#[test]
fn error_code_accessor_returns_none_when_absent() {
    use oopsie::Diagnostic as _;
    let err = NoCode {
        msg: "boom".to_owned(),
    }
    .build();
    assert!(
        err.oopsie_error_code().is_none(),
        "variant without code should return None"
    );
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn error_code_accessor_and_provider_agree() {
    use oopsie::Diagnostic as _;
    let err = StaticCode {
        msg: "boom".to_owned(),
    }
    .build();
    let via_accessor = err.oopsie_error_code().expect("accessor yields code");
    let via_provider =
        core::error::request_value::<oopsie::ErrorCode>(&err).expect("provider yields code");
    assert_eq!(via_accessor.as_str(), via_provider.as_str());
    assert_eq!(via_accessor.as_str(), "test::code");
}

#[test]
fn error_code_accessor_surfaces_dynamic_provide() {
    use oopsie::Diagnostic as _;
    let err = DynamicCode {
        msg: "boom".to_owned(),
    }
    .build();
    // A variant-level `provide(ErrorCode => ...)` is also surfaced through the
    // stable accessor (gen_error.rs folds it into the same `code_arms`).
    let code = err.oopsie_error_code();
    assert!(
        code.is_some(),
        "dynamic provide(ErrorCode) reaches accessor"
    );
    assert_eq!(code.unwrap().as_str(), "dyn::code");
}

#[test]
fn error_code_renders_via_display_end_to_end() {
    use oopsie::Diagnostic as _;
    // End-to-end: a built error's code reaches the accessor and renders through
    // `ErrorCode`'s Display, which is the surface `Report` uses for
    // `Error[<code>]:`.
    let err = StaticCode {
        msg: "boom".to_owned(),
    }
    .build();
    let code = err.oopsie_error_code().expect("code present");
    assert_eq!(format!("{code}"), "test::code");
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn error_code_dynamic_accessor_and_provider_agree() {
    use oopsie::Diagnostic as _;
    let err = DynamicCode {
        msg: "boom".to_owned(),
    }
    .build();
    let via_accessor = err
        .oopsie_error_code()
        .expect("accessor yields dynamic code");
    let via_provider =
        core::error::request_value::<oopsie::ErrorCode>(&err).expect("provider yields code");
    assert_eq!(via_accessor.as_str(), "dyn::code");
    assert_eq!(via_accessor.as_str(), via_provider.as_str());
}

// ---- provide(ErrorCode => ...) exprs that reference fields / ref-form ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum FieldCodeError {
    #[oopsie("boom")]
    #[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from(format!("app::{kind}"))))]
    FieldCode { kind: String },

    #[oopsie("ref boom")]
    #[oopsie(provide(ref, ::oopsie::ErrorCode => code))]
    RefCode { code: oopsie::ErrorCode },
}

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
#[oopsie("struct boom")]
#[oopsie(provide(::oopsie::ErrorCode => ::oopsie::ErrorCode::from(format!("app::{kind}"))))]
struct StructFieldCode {
    kind: String,
}

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
#[oopsie("struct ref boom")]
#[oopsie(provide(ref, ::oopsie::ErrorCode => code))]
struct StructRefCode {
    code: oopsie::ErrorCode,
}

#[test]
fn error_code_provide_can_reference_fields() {
    use oopsie::Diagnostic as _;
    let err = FieldCode {
        kind: "db".to_owned(),
    }
    .build();
    let code = err.oopsie_error_code().expect("accessor yields code");
    assert_eq!(code.as_str(), "app::db");
}

#[test]
fn error_code_ref_provide_returns_owned_clone() {
    use oopsie::Diagnostic as _;
    let err = RefCode {
        code: oopsie::ErrorCode::from("ref::code"),
    }
    .build();
    let code = err.oopsie_error_code().expect("accessor yields code");
    assert_eq!(code.as_str(), "ref::code");
}

#[test]
fn struct_error_code_provide_can_reference_fields() {
    use oopsie::Diagnostic as _;
    let err = StructFieldCodeOopsie {
        kind: "fs".to_owned(),
    }
    .build();
    let code = err.oopsie_error_code().expect("accessor yields code");
    assert_eq!(code.as_str(), "app::fs");
}

#[test]
fn struct_error_code_ref_provide_returns_owned_clone() {
    use oopsie::Diagnostic as _;
    let err = StructRefCodeOopsie {
        code: oopsie::ErrorCode::from("ref::struct"),
    }
    .build();
    let code = err.oopsie_error_code().expect("accessor yields code");
    assert_eq!(code.as_str(), "ref::struct");
}
