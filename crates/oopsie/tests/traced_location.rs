//! `#[oopsie(traced)]` caller-location capture (F2).
//!
//! Location capture is on by default in `traced` mode and surfaces through
//! `Diagnostic::oopsie_location`. The captured value is the literal call site of
//! the construction call (`.build()` / `.context(...)` / `From::from`), threaded
//! via `#[track_caller]`, so the asserted line is the one that call sits on.

#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::all,
    reason = "integration test fixtures intentionally trip style lints"
)]

use oopsie::{Contextual as _, Diagnostic as _, Oopsie, ResultExt as _, oopsie};

#[oopsie(traced)]
#[oopsie("leaf failed: {what}")]
pub struct LeafError {
    what: String,
}

#[oopsie(traced)]
#[oopsie("wrap failed")]
pub struct WrapError {
    source: LeafError,
}

#[oopsie(traced)]
#[oopsie("io wrap failed")]
pub struct IoWrapError {
    source: std::io::Error,
}

#[oopsie(traced(location = false))]
#[oopsie("no location: {what}")]
pub struct NoLocationError {
    what: String,
}

#[oopsie(traced)]
pub enum AppError {
    #[oopsie("leaf variant: {what}")]
    Leaf { what: String },
    #[oopsie("wrap variant")]
    Wrap { source: std::io::Error },
}

#[test]
fn location_captured_at_build_call_site() {
    let line = line!() + 1;
    let err = leaf_oopsies::Leaf { what: "disk" }.build();

    let loc = err.oopsie_location().expect("location captured");
    assert!(
        loc.file().ends_with("traced_location.rs"),
        "file should be this test file, got {}",
        loc.file()
    );
    // `#[track_caller]` reports the literal line of the `.build()` call.
    assert_eq!(loc.line(), line, "line should be the .build() call site");
}

#[test]
fn location_captured_at_context_call_site() {
    // The source is a foreign `io::Error` (no captured location to inherit), so
    // the wrap layer's own location is the `.context(...)` call site — proving
    // `#[track_caller]` threads through `.context` to `build_error` (not a
    // closure inside the method).
    let result: Result<(), std::io::Error> = Err(std::io::Error::other("io"));
    let line = line!() + 1;
    let err = result.context(io_wrap_oopsies::IoWrap).unwrap_err();

    let loc = err.oopsie_location().expect("location captured");
    assert!(loc.file().ends_with("traced_location.rs"));
    assert_eq!(
        loc.line(),
        line,
        "line should be the .context(...) call site"
    );
}

#[test]
fn context_on_diagnostic_source_surfaces_origin_location() {
    // When `.context(...)` wraps a `Diagnostic` source that already carries a
    // location, the origin-most (the source's) wins — extracted at construction
    // via `CaptureProbe`.
    let leaf_line = line!() + 1;
    let leaf = leaf_oopsies::Leaf { what: "x" }.build();
    let result: Result<(), LeafError> = Err(leaf);
    let err = result.context(wrap_oopsies::Wrap).unwrap_err();

    let loc = err.oopsie_location().expect("location captured");
    assert_eq!(
        loc.line(),
        leaf_line,
        "origin-most location (the leaf's) must win over the .context site"
    );
}

#[test]
fn origin_most_location_wins_through_wrap_chain() {
    // The leaf is built on its own line; the wrap is built on a later one. The
    // origin-most location (the leaf's) must win — it is captured at the wrap's
    // construction via `CaptureProbe`/`capture_or_extract`.
    let leaf_line = line!() + 1;
    let leaf = leaf_oopsies::Leaf { what: "disk" }.build();
    let wrapped: WrapError = wrap_oopsies::Wrap.build_error(leaf);

    let loc = wrapped.oopsie_location().expect("location captured");
    assert_eq!(
        loc.line(),
        leaf_line,
        "the wrap layer must surface the leaf's (origin-most) location"
    );
}

#[test]
fn location_disabled_yields_none() {
    let err = no_location_oopsies::NoLocation { what: "x" }.build();
    assert!(
        err.oopsie_location().is_none(),
        "traced(location = false) must not capture a location"
    );
}

// A hand-written `&'static Location` field is auto-detected (like a `Backtrace`
// field), auto-captured, and surfaced — no `traced` attribute needed.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie("manual location")]
pub struct ManualLocationError {
    #[oopsie(location)]
    at: &'static std::panic::Location<'static>,
}

#[test]
fn manually_marked_location_field_is_captured() {
    let line = line!() + 1;
    let err = ManualLocation.build();
    let loc = err.oopsie_location().expect("location captured");
    assert_eq!(loc.line(), line);
}

// ─── Transparent forwarding ───
//
// A transparent wrapper has no own location field's value to prefer over the
// source; on stable it forwards the source's location via the `DiagProbe`.

#[oopsie(traced)]
#[oopsie("inner: {what}")]
pub struct InnerError {
    what: String,
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie(transparent)]
pub struct TransparentError {
    source: InnerError,
}

#[test]
fn transparent_forwards_source_location() {
    let inner_line = line!() + 1;
    let inner = inner_oopsies::Inner { what: "x" }.build();
    let outer: TransparentError = TransparentError::from(inner);

    let loc = outer.oopsie_location().expect("location forwarded");
    assert_eq!(
        loc.line(),
        inner_line,
        "transparent wrapper must forward the source's location"
    );
}

// ─── Welp ───

#[test]
fn welp_captures_location() {
    use oopsie::Welp;

    let line = line!() + 1;
    let err = Welp::new("boom");
    let loc = oopsie::Diagnostic::oopsie_location(&err).expect("welp location");
    assert!(loc.file().ends_with("traced_location.rs"));
    assert_eq!(loc.line(), line);
}

#[test]
fn welp_context_captures_location() {
    use oopsie::WelpResultExt as _;

    let result: Result<(), std::io::Error> = Err(std::io::Error::other("io"));
    let line = line!() + 1;
    let err = result.welp_context("wrap").unwrap_err();
    let loc = oopsie::Diagnostic::oopsie_location(&err).expect("welp location");
    assert_eq!(loc.line(), line);
}

#[test]
fn welp_captures_location_at_call_site() {
    use oopsie::WelpResultExt as _;

    let result: Result<(), std::io::Error> = Err(std::io::Error::other("io"));
    let line = line!() + 1;
    let err = result.welp().unwrap_err();
    let loc = oopsie::Diagnostic::oopsie_location(&err).expect("welp location");
    assert!(loc.file().ends_with("traced_location.rs"));
    assert_eq!(loc.line(), line);
}

#[test]
fn welp_result_with_context_captures_call_site() {
    use oopsie::WelpResultExt as _;

    let result: Result<(), std::io::Error> = Err(std::io::Error::other("io"));
    let f = |e: &std::io::Error| format!("ctx: {e}");
    let line = line!() + 1;
    let err = result.with_welp_context(f).unwrap_err();
    let loc = oopsie::Diagnostic::oopsie_location(&err).expect("welp location");
    assert!(loc.file().ends_with("traced_location.rs"));
    assert_eq!(loc.line(), line);
}

#[test]
fn welp_option_context_captures_call_site() {
    use oopsie::WelpOptionExt as _;

    let opt: Option<()> = None;
    let line = line!() + 1;
    let err = opt.welp_context("missing").unwrap_err();
    let loc = oopsie::Diagnostic::oopsie_location(&err).expect("welp location");
    assert!(loc.file().ends_with("traced_location.rs"));
    assert_eq!(loc.line(), line);
}

#[test]
fn welp_option_with_context_captures_call_site() {
    use oopsie::WelpOptionExt as _;

    let opt: Option<()> = None;
    let line = line!() + 1;
    let err = opt.with_welp_context(|| "missing".to_owned()).unwrap_err();
    let loc = oopsie::Diagnostic::oopsie_location(&err).expect("welp location");
    assert!(loc.file().ends_with("traced_location.rs"));
    assert_eq!(loc.line(), line);
}

// ─── enum-variant selectors ───

#[test]
fn enum_leaf_selector_captures_build_call_site() {
    let line = line!() + 1;
    let err = app_oopsies::Leaf { what: "x" }.build();
    let loc = err.oopsie_location().expect("location captured");
    assert!(loc.file().ends_with("traced_location.rs"));
    assert_eq!(loc.line(), line, "line should be the .build() call site");
}

#[test]
fn enum_sourced_selector_captures_context_call_site() {
    let result: Result<(), std::io::Error> = Err(std::io::Error::other("io"));
    let line = line!() + 1;
    let err = result.context(app_oopsies::Wrap).unwrap_err();
    let loc = err.oopsie_location().expect("location captured");
    assert_eq!(
        loc.line(),
        line,
        "line should be the .context(...) call site"
    );
}

// ─── remaining context ext methods ───

#[test]
fn result_with_context_captures_call_site() {
    let result: Result<(), std::io::Error> = Err(std::io::Error::other("io"));
    let mk = |_: &std::io::Error| io_wrap_oopsies::IoWrap;
    let line = line!() + 1;
    let err = result.with_context(mk).unwrap_err();
    let loc = err.oopsie_location().expect("location captured");
    assert_eq!(loc.line(), line, "line should be the .with_context site");
}

#[test]
fn option_context_captures_call_site() {
    use oopsie::OptionExt as _;

    let opt: Option<()> = None;
    let line = line!() + 1;
    let err = opt.context(leaf_oopsies::Leaf { what: "x" }).unwrap_err();
    let loc = err.oopsie_location().expect("location captured");
    assert_eq!(loc.line(), line, "line should be the .context site");
}

#[test]
fn option_with_context_captures_call_site() {
    use oopsie::OptionExt as _;

    let opt: Option<()> = None;
    let mk = || leaf_oopsies::Leaf { what: "x" };
    let line = line!() + 1;
    let err = opt.with_context(mk).unwrap_err();
    let loc = err.oopsie_location().expect("location captured");
    assert_eq!(loc.line(), line, "line should be the .with_context site");
}

// ─── ErasedError round-trip ───

#[cfg(feature = "serde")]
#[test]
fn erased_error_preserves_location() {
    use oopsie_core::erased::ErasedError;

    let err = leaf_oopsies::Leaf { what: "disk" }.build();
    let captured = err.oopsie_location().expect("location captured");

    let erased = ErasedError::from_error(err);
    let loc = erased.location().expect("erased location preserved");
    assert_eq!(loc.file(), captured.file());
    assert_eq!(loc.line(), captured.line());
    assert_eq!(loc.column(), captured.column());

    // Survives a serialize/deserialize round-trip.
    let json = serde_json::to_string(&erased).unwrap();
    let restored: ErasedError = serde_json::from_str(&json).unwrap();
    let restored_loc = restored.location().expect("location survives round-trip");
    assert_eq!(restored_loc.file(), captured.file());
    assert_eq!(restored_loc.line(), captured.line());
    assert_eq!(restored_loc.column(), captured.column());
}
