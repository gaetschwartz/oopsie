#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use oopsie::{Diagnostic as _, Oopsie};
use std::error::Error as _;
use std::io;

// ─── Enum transparent: From impl + custom display (Tests 1 & 2) ───

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum TransparentError {
    #[oopsie(display("io error"), transparent)]
    Io { source: io::Error },
}

#[test]
fn transparent_generates_from() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: TransparentError = TransparentError::from(io_err);
    assert!(matches!(err, TransparentError::Io { .. }));
}

#[test]
fn transparent_display() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: TransparentError = TransparentError::from(io_err);
    // Display uses the format string "io error", not the source's display.
    assert_eq!(err.to_string(), "io error");
}

#[test]
fn transparent_source_delegates_to_inner() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: TransparentError = TransparentError::from(io_err);
    // `Error::source()` exposes the wrapped inner error.
    let src = err
        .source()
        .expect("transparent variant exposes its source");
    assert_eq!(src.to_string(), "pipe broke");
}

// ─── GAP 31: transparent variant + capture + source, auto field asserted ───

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum TracedError {
    #[oopsie(display("traced io error"), transparent)]
    TracedIo {
        source: io::Error,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
    },
}

#[test]
fn transparent_with_auto_fields() {
    oopsie::set_rust_backtrace_override(oopsie::RustBacktrace::Enabled);
    let io_err = io::Error::new(io::ErrorKind::Other, "something");
    // From impl auto-generates the captured backtrace field.
    let err: TracedError = TracedError::from(io_err);
    assert_eq!(err.to_string(), "traced io error");

    // GAP 31: the capture field is initialized and reachable via destructuring.
    let TracedError::TracedIo { bt, .. } = &err;
    assert!(
        !bt.frames().is_empty(),
        "auto-captured backtrace must be populated"
    );
    // `oopsie_backtrace()` is intentionally NOT asserted here: a transparent
    // variant delegates the accessor to its source, and a trace-less io::Error
    // source yields None. The own capture field's population (asserted above) is
    // the gap-31 guarantee; accessor delegation is covered by `traced_transparent`.
}

// ─── GAP 19: transparent variant with a user (non-source, non-auto) field ───

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum UserFieldError {
    #[oopsie(display("wrapped with user field"), transparent)]
    WithUserField { source: io::Error, msg: String },
}

#[test]
fn transparent_variant_with_user_field_defaults() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
    // The From impl defaults the user field via `Default::default()`.
    let err: UserFieldError = UserFieldError::from(io_err);
    let UserFieldError::WithUserField { msg, .. } = &err;
    assert_eq!(msg, "", "transparent From defaults user fields to Default");
    assert_eq!(err.to_string(), "wrapped with user field");
}

// ─── GAP 18: transparent variant + from(Type, transform) ───

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum TransformError {
    #[oopsie(display("transformed io"), transparent)]
    Wrapped {
        #[oopsie(from(io::Error, |e| Box::new(e)))]
        source: Box<io::Error>,
    },
}

#[test]
fn transparent_variant_with_transform() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "nope");
    // From accepts the *pre-transform* `io::Error`; the closure boxes it.
    let err: TransformError = TransformError::from(io_err);
    let TransformError::Wrapped { source } = &err;
    assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(err.to_string(), "transformed io");
    // source() exposes the boxed inner.
    assert_eq!(err.source().expect("has source").to_string(), "nope");
}

// ─── GAP 11: transparent variant + from(Type, transform) + auto field ───

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum TransformCaptureError {
    #[oopsie(display("transform+capture"), transparent)]
    Wrapped {
        #[oopsie(from(io::Error, |e| Box::new(e)))]
        source: Box<io::Error>,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
    },
}

#[test]
fn transparent_transform_with_auto_field() {
    oopsie::set_rust_backtrace_override(oopsie::RustBacktrace::Enabled);
    let io_err = io::Error::new(io::ErrorKind::TimedOut, "slow");
    let err: TransformCaptureError = TransformCaptureError::from(io_err);
    let TransformCaptureError::Wrapped { source, bt } = &err;
    assert_eq!(source.kind(), io::ErrorKind::TimedOut);
    assert!(
        !bt.frames().is_empty(),
        "auto field captured despite transform"
    );
}

// ─── GAP 21: transparent variant with auto-boxed Box<T> source ───

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum BoxedSourceError {
    #[oopsie(display("boxed io"), transparent)]
    Io { source: Box<io::Error> },
}

#[test]
fn transparent_auto_boxed_source() {
    let io_err = io::Error::new(io::ErrorKind::ConnectionReset, "reset");
    // Auto-boxing: From accepts the *unboxed* io::Error; the macro boxes it.
    let err: BoxedSourceError = BoxedSourceError::from(io_err);
    let BoxedSourceError::Io { source } = &err;
    assert_eq!(source.kind(), io::ErrorKind::ConnectionReset);
    assert_eq!(err.to_string(), "boxed io");
    assert_eq!(err.source().expect("has source").to_string(), "reset");
}

// ─── Mixed transparent and regular variants (Test 4) ───

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum MixedError {
    #[oopsie(display("wrapped io"), transparent)]
    Wrapped { source: io::Error },

    #[oopsie("custom: {msg}")]
    Custom { msg: String },
}

#[test]
fn mixed_transparent_and_regular() {
    // Transparent variant via From.
    let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
    let err: MixedError = MixedError::from(io_err);
    assert!(matches!(err, MixedError::Wrapped { .. }));
    assert_eq!(err.to_string(), "wrapped io");

    // Regular leaf variant via selector build().
    let err = Custom { msg: "bad thing" }.build();
    assert!(matches!(err, MixedError::Custom { .. }));
    assert_eq!(err.to_string(), "custom: bad thing");
}

// ─── GAP 52: transparent variant WITHOUT a source field ───
//
// When `transparent` is set but the variant/struct has no `source` field, the
// macro generates an EMPTY token stream for that variant: no `From` impl, no
// selector. The enum still compiles and the variant is constructible by hand,
// but there is no generated conversion. This characterizes that behavior; it
// is NOT a compile error (the compile-fail agent owns nothing here).

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum NoSourceTransparent {
    #[oopsie(display("no source"), transparent)]
    NoOp,

    #[oopsie("real: {msg}")]
    Real { msg: String },
}

#[test]
fn transparent_without_source_generates_nothing() {
    // The variant is constructible directly (no selector/From was generated).
    let err = NoSourceTransparent::NoOp;
    assert_eq!(err.to_string(), "no source");
    // No `source` is exposed.
    assert!(err.source().is_none());
    // The sibling non-transparent variant still works normally.
    let err = Real { msg: "x" }.build();
    assert_eq!(err.to_string(), "real: x");
}

// ─── STRUCT cases ───
//
// For transparent STRUCTS the macro generates `impl From<Inner> for Struct`
// producing `Self { source, <auto fields> }`. Note: unlike enum variants, the
// struct From impl does NOT emit user-field defaults, so a transparent struct
// may only carry a source plus auto (capture) fields.

// GAP 0 + GAP 16: transparent struct with a source field.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie(display("wrapped io"), transparent)]
struct TransparentStruct {
    source: io::Error,
}

#[test]
fn transparent_struct_from_and_display() {
    let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
    let err: TransparentStruct = TransparentStruct::from(io_err);
    // Display uses the override, not the source's display.
    assert_eq!(err.to_string(), "wrapped io");
    // source() delegates to the inner error.
    assert_eq!(
        err.source().expect("struct exposes source").to_string(),
        "pipe broke"
    );
}

// GAP 17: transparent struct with from(Type, transform) in the From impl.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie(display("transformed struct"), transparent)]
struct TransformStruct {
    #[oopsie(from(io::Error, |e| Box::new(e)))]
    source: Box<io::Error>,
}

#[test]
fn transparent_struct_with_transform() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "nope");
    // From accepts the pre-transform io::Error; the closure boxes it.
    let err: TransformStruct = TransformStruct::from(io_err);
    assert_eq!(err.source.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(err.to_string(), "transformed struct");
}

// GAP 20: transparent struct with an auto (capture) field.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie(display("traced struct"), transparent)]
struct TracedStruct {
    source: io::Error,
    #[oopsie(capture)]
    bt: Box<oopsie::Backtrace>,
}

#[test]
fn transparent_struct_with_auto_field() {
    oopsie::set_rust_backtrace_override(oopsie::RustBacktrace::Enabled);
    let io_err = io::Error::new(io::ErrorKind::Other, "boom");
    let err: TracedStruct = TracedStruct::from(io_err);
    assert_eq!(err.to_string(), "traced struct");
    // GAP 20: the capture field is initialized and reachable.
    assert!(
        !err.bt.frames().is_empty(),
        "struct auto backtrace must be captured"
    );
}

// ─── Provider-API parity for a transparent traced wrapper (nightly only) ───
//
// Real-macro coverage for the stable diagnostic accessors on a transparent,
// trace-injected wrapper. Uses the actual `#[oopsie(traced)]` attribute macro
// (inject → derive) rather than a hand-written post-injection shape, so it
// cannot drift from what injection emits.
#[cfg(feature = "unstable-error-generic-member-access")]
mod traced_transparent {
    use oopsie::Diagnostic as _;

    #[oopsie::oopsie(traced)]
    pub enum Leaf {
        #[oopsie("leaf boom: {detail}")]
        Boom { detail: String },
    }

    #[oopsie::oopsie(traced)]
    pub enum Wrapper {
        #[oopsie(display("wrapper around leaf"), transparent)]
        Around { source: Leaf },
    }

    // A transparent traced wrapper forwards to its source first in `provide()`,
    // so the deepest (leaf) trace fills the slot. The stable accessor must agree.
    #[test]
    fn transparent_traced_stable_matches_provider_deepest() {
        use leaf_oopsies::Boom;

        let leaf = Boom { detail: "x" }.build();
        let outer: Wrapper = Wrapper::from(leaf);

        let bt_provide = core::error::request_ref::<oopsie::Backtrace>(&outer)
            .expect("provider path yields a backtrace");
        let st_provide = core::error::request_ref::<oopsie::SpanTrace>(&outer)
            .expect("provider path yields a span trace");

        let bt_stable = outer
            .oopsie_backtrace()
            .expect("stable accessor yields a backtrace");
        let st_stable = outer
            .oopsie_spantrace()
            .expect("stable accessor yields a span trace");

        assert!(
            std::ptr::eq(bt_provide, bt_stable),
            "transparent traced wrapper: stable oopsie_backtrace() must surface the same \
             deepest backtrace as the provider API"
        );
        assert!(
            std::ptr::eq(st_provide, st_stable),
            "transparent traced wrapper: stable oopsie_spantrace() must surface the same \
             deepest span trace as the provider API"
        );
    }
}
