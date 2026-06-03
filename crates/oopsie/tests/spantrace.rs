#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::all,
    reason = "integration test fixtures intentionally trip style lints"
)]

mod common;

use oopsie::{Contextual as _, Oopsie, SpanTrace, oopsie};
use std::io;
use tracing::instrument;

#[derive(Debug, Oopsie)]
#[oopsie("Boxed spantrace error")]
#[oopsie(suffix, module(false))]
struct BoxedSpantraceError {
    #[oopsie(spantrace)]
    span: Box<SpanTrace>,
}

#[test]
fn test_extract_boxed_spantrace_via_provide_ref() {
    use oopsie::Diagnostic as _;
    let err = BoxedSpantraceOopsie.build();
    let extracted = err.oopsie_spantrace();
    assert!(extracted.is_some());
}

// ════════════════════════════════════════════════════════════════════════
// Captured trace CONTENT (not just presence) — gap 30
// ════════════════════════════════════════════════════════════════════════

#[oopsie(traced)]
#[oopsie("captured content")]
pub struct CapturedContentError {
    info: String,
}

#[test]
fn captured_backtrace_has_real_frames() {
    use oopsie::Diagnostic as _;
    common::force_backtrace();
    let err = CapturedContentOopsie { info: "x" }.build();

    let bt = err
        .oopsie_backtrace()
        .expect("traced error must expose a backtrace");
    // Content, not just presence: a forced capture records real stack frames.
    assert!(
        !bt.frames().is_empty(),
        "forced backtrace capture must record frames, got 0"
    );
    // This very test function must appear somewhere in the captured frames,
    // proving the backtrace was captured here (real content) rather than empty.
    let has_this_fn = bt
        .frames()
        .iter()
        .flat_map(backtrace::BacktraceFrame::symbols)
        .filter_map(backtrace::BacktraceSymbol::name)
        .any(|n| format!("{n}").contains("captured_backtrace_has_real_frames"));
    assert!(
        has_this_fn,
        "captured backtrace should contain this test's frame"
    );
}

#[test]
fn captured_spantrace_status_is_captured_within_span() {
    use oopsie::Diagnostic as _;

    #[instrument(target = "test")]
    fn make() -> CapturedContentError {
        CapturedContentOopsie { info: "x" }.build()
    }

    let _guard = common::init_test_subscriber();

    let err = make();
    let st = err
        .oopsie_spantrace()
        .expect("traced error must expose a spantrace");
    // Content: with an active ErrorLayer subscriber and an instrumented frame,
    // the captured span trace is non-empty and its Display names the
    // instrumented span — not just a presence check.
    let rendered = st.to_string();
    assert!(
        !rendered.is_empty(),
        "spantrace captured inside an instrumented span must render non-empty content"
    );
    assert!(
        rendered.contains("make"),
        "spantrace content should name the instrumented span, got: {rendered}"
    );
}

// ════════════════════════════════════════════════════════════════════════
// Extraction path vs fresh-capture path — gap 29
//
// When the wrapped source implements `Diagnostic`, the generated capture code
// routes through `CaptureExt::capture_or_extract` and REUSES the source's
// backtrace (preserving the original call site). When the source does NOT
// implement `Diagnostic` (e.g. `io::Error`), it falls back to a FRESH capture
// at the wrap site. These two paths are observably different in the captured
// frames.
// ════════════════════════════════════════════════════════════════════════

// NOTE: these use `#[oopsie(traced(backtrace))]` (not bare `#[oopsie(capture)]`)
// so the backtrace is auto-injected AND exposed via the `oopsie_backtrace()`
// accessor. A plain `#[oopsie(capture)] bt: Box<Backtrace>` field is captured
// but is NOT surfaced by the stable Diagnostic accessor (that requires the
// field to carry `#[oopsie(backtrace)]`, which `traced` adds for us).
#[oopsie(traced(backtrace))]
enum ExtractSrcError {
    #[oopsie("extract src")]
    Src { info: String },
}

#[oopsie(traced(backtrace))]
enum ExtractWrapDiagError {
    #[oopsie("wrap diag source")]
    WrapDiag { source: ExtractSrcError },
}

#[oopsie(traced(backtrace))]
enum ExtractWrapIoError {
    #[oopsie("wrap io source")]
    WrapIo { source: io::Error },
}

fn symbol_names(bt: &oopsie::Backtrace) -> Vec<String> {
    bt.frames()
        .iter()
        .flat_map(backtrace::BacktraceFrame::symbols)
        .filter_map(|s| s.name().map(|n| format!("{n}")))
        .collect()
}

#[inline(never)]
fn build_extract_src() -> ExtractSrcError {
    extract_src_oopsies::Src { info: "x" }.build()
}

#[test]
fn diagnostic_source_extracts_backtrace_from_source() {
    use oopsie::Diagnostic as _;
    common::force_backtrace();

    let src = build_extract_src();
    let src_names = symbol_names(src.oopsie_backtrace().expect("src backtrace"));

    let wrapped: ExtractWrapDiagError = extract_wrap_diag_oopsies::WrapDiag.build_error(src);
    let wrap_names = symbol_names(wrapped.oopsie_backtrace().expect("wrap backtrace"));

    // Extraction: the wrapping error reuses the source's backtrace verbatim, so
    // the full symbol-name list is identical. A fresh capture at the wrap site
    // would begin at a different frame and could not match exactly.
    assert_eq!(
        wrap_names, src_names,
        "Diagnostic source must extract (reuse) the source's backtrace content"
    );
}

#[test]
fn non_diagnostic_source_captures_fresh_backtrace() {
    use oopsie::Diagnostic as _;
    common::force_backtrace();

    let src = build_extract_src();
    let src_top_user = symbol_names(src.oopsie_backtrace().expect("src backtrace"))
        .into_iter()
        .find(|n| n.contains("build_extract_src"));
    assert!(
        src_top_user.is_some(),
        "sanity: source backtrace should name its builder fn"
    );

    let wrapped: ExtractWrapIoError =
        extract_wrap_io_oopsies::WrapIo.build_error(io::Error::other("disk full"));
    let wrap_names = symbol_names(wrapped.oopsie_backtrace().expect("wrap backtrace"));

    // Fallback / fresh capture: a non-Diagnostic source carries no backtrace, so
    // the wrap site captures a NEW backtrace. It must NOT contain the source's
    // builder frame (that call already returned) — proving this is not the
    // extraction path.
    assert!(
        !wrap_names.iter().any(|n| n.contains("build_extract_src")),
        "fresh capture must not contain the already-returned source builder frame"
    );
    // It must instead contain this test's frame (the fresh capture site).
    assert!(
        wrap_names
            .iter()
            .any(|n| n.contains("non_diagnostic_source_captures_fresh_backtrace")),
        "fresh capture must contain the wrap-site frame"
    );
}

#[cfg(feature = "unstable-error-generic-member-access")]
mod provide_test {
    use super::*;

    #[derive(Debug, Oopsie)]
    #[oopsie("Boxed spantrace error via provide")]
    #[oopsie(suffix, module(false))]
    #[oopsie(provide(ref, oopsie::SpanTrace => prov_span.as_ref()))]
    struct BoxedSpantraceProvideError {
        #[oopsie(spantrace)]
        prov_span: Box<SpanTrace>,
    }

    #[test]
    fn test_extract_boxed_spantrace_via_nightly_provide() {
        let err = BoxedSpantraceProvideOopsie.build();
        let extracted = core::error::request_ref::<SpanTrace>(&err);
        assert!(extracted.is_some());
    }
}
