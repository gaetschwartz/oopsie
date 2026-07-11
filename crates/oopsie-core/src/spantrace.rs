//! SpanTrace wrapper with `Capturable` support.

use core::fmt;

/// A wrapper around `tracing_error::SpanTrace`.
#[cfg(feature = "tracing")]
#[derive(Debug, Clone)]
pub struct SpanTrace {
    inner: tracing_error::SpanTrace,
}

#[cfg(feature = "tracing")]
impl SpanTrace {
    /// Captures the currently active span stack from the installed subscriber.
    ///
    /// Yields an empty trace when no span is active or the subscriber does not
    /// support span traces; check [`is_captured`](Self::is_captured) to tell the
    /// two apart.
    #[must_use]
    #[inline]
    pub fn capture() -> Self {
        Self {
            inner: tracing_error::SpanTrace::capture(),
        }
    }

    /// Wraps an existing `tracing_error::SpanTrace`.
    #[must_use]
    #[inline]
    pub const fn new(inner: tracing_error::SpanTrace) -> Self {
        Self { inner }
    }

    /// Returns whether this trace was captured, empty, or unsupported.
    #[must_use]
    #[inline]
    pub fn status(&self) -> SpanTraceStatus {
        self.inner.status().into()
    }

    /// `true` if a span trace was actually captured (an active span existed and
    /// the subscriber supports `SpanTrace`). An empty/unsupported trace yields
    /// no frames and should not be rendered as a `SPANTRACE` section.
    #[must_use]
    #[inline]
    pub fn is_captured(&self) -> bool {
        matches!(self.status(), SpanTraceStatus::Captured)
    }

    /// Consumes the wrapper and returns the underlying `tracing_error::SpanTrace`.
    #[must_use]
    #[inline]
    pub fn into_span_trace(self) -> tracing_error::SpanTrace {
        self.inner
    }

    /// Borrows the underlying `tracing_error::SpanTrace`.
    #[inline]
    #[must_use]
    pub const fn as_span_trace(&self) -> &tracing_error::SpanTrace {
        &self.inner
    }
}

/// The status of a [`SpanTrace`]: whether it was captured, or why it is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanTraceStatus {
    /// Span traces are unsupported — typically no `ErrorLayer` is installed, or
    /// it comes from an incompatible `tracing-error` version.
    Unsupported,
    /// The span trace is empty, typically because it was captured outside of
    /// any spans.
    Empty,
    /// A span trace was captured and renders meaningful frames.
    Captured,
}

#[cfg(feature = "tracing")]
impl From<tracing_error::SpanTraceStatus> for SpanTraceStatus {
    fn from(status: tracing_error::SpanTraceStatus) -> Self {
        if status == tracing_error::SpanTraceStatus::CAPTURED {
            Self::Captured
        } else if status == tracing_error::SpanTraceStatus::EMPTY {
            Self::Empty
        } else {
            Self::Unsupported
        }
    }
}

// Span fields carry no stable identity, so only debug builds keep them around to
// tighten equality; release builds drop them and compare callsites alone.
#[cfg(all(feature = "tracing", debug_assertions))]
#[inline]
fn capture_fields(fields: &str) -> String {
    fields.to_owned()
}

#[cfg(all(feature = "tracing", not(debug_assertions)))]
#[inline]
fn capture_fields(_fields: &str) {}

#[cfg(all(feature = "tracing", debug_assertions))]
#[inline]
fn fields_eq(stored: &str, current: &str) -> bool {
    stored == current
}

#[cfg(all(feature = "tracing", not(debug_assertions)))]
#[inline]
fn fields_eq(_stored: &(), _current: &str) -> bool {
    true
}

#[cfg(feature = "tracing")]
impl PartialEq for SpanTrace {
    fn eq(&self, other: &Self) -> bool {
        use std::collections::VecDeque;

        let a = &self.inner;
        let b = &other.inner;

        let mut a_frames = VecDeque::with_capacity(2);
        a.with_spans(|a_md, a_fields| {
            a_frames.push_back((a_md.callsite(), capture_fields(a_fields)));
            true
        });
        let mut equal = true;
        b.with_spans(|b_md, b_fields| {
            equal = match a_frames.pop_front() {
                Some((a_callsite, a_fields)) => {
                    a_callsite == b_md.callsite() && fields_eq(&a_fields, b_fields)
                }
                None => false,
            };
            equal
        });
        equal && a_frames.is_empty()
    }
}

#[cfg(feature = "tracing")]
impl fmt::Display for SpanTrace {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

/// A no-op span trace stub: the type stays present without the `tracing`
/// feature so the captured-spantrace surface is unconditional, but capture is
/// inert and the trace is never considered present.
#[cfg(not(feature = "tracing"))]
#[derive(Clone, Debug)]
pub struct SpanTrace;

#[cfg(not(feature = "tracing"))]
impl SpanTrace {
    /// Returns the inert stub; capture is a no-op without the `tracing` feature.
    #[must_use]
    #[inline]
    pub const fn capture() -> Self {
        Self
    }

    /// Always `false`: the stub never holds a captured trace.
    #[must_use]
    #[inline]
    pub const fn is_captured(&self) -> bool {
        false
    }
}

#[cfg(not(feature = "tracing"))]
impl PartialEq for SpanTrace {
    #[inline]
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[cfg(not(feature = "tracing"))]
impl fmt::Display for SpanTrace {
    #[inline]
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

impl crate::Capturable for SpanTrace {
    #[inline]
    fn capture() -> Self {
        Self::capture()
    }

    #[inline]
    fn capture_or_extract(source: &dyn crate::Diagnostic) -> Self {
        match source.oopsie_spantrace() {
            // Keep the source's trace only if capture actually succeeded; an
            // EMPTY/UNSUPPORTED trace carries nothing worth preserving over a
            // fresh capture at the wrap site.
            Some(trace) if trace.is_captured() => trace.clone(),
            _ => Self::capture(),
        }
    }
}

/// A wrapper around `Option<SpanTrace>` that implements [`Capturable`](crate::Capturable).
///
/// [`is_some`](Self::is_some) / [`is_none`](Self::is_none) reflect whether a
/// `SpanTrace` value is present, not whether it was successfully captured: the
/// [`some`](Self::some) and `From` constructors wrap unconditionally, so a
/// present trace may still be empty/unsupported. Only the
/// [`Capturable`](crate::Capturable) construction path enforces
/// `is_some() == captured`.
#[derive(Clone, Debug, Default)]
pub struct OptionalSpanTrace(Option<SpanTrace>);

impl OptionalSpanTrace {
    /// Create an empty `OptionalSpanTrace`.
    #[must_use]
    #[inline]
    pub const fn none() -> Self {
        Self(None)
    }

    /// Create an `OptionalSpanTrace` from an existing `SpanTrace`.
    #[must_use]
    #[inline]
    pub const fn some(trace: SpanTrace) -> Self {
        Self(Some(trace))
    }

    /// Returns the inner `Option<SpanTrace>`.
    // The real `SpanTrace`'s destructor isn't const-evaluable, so only the
    // featureless stub variant can be a `const fn`.
    #[cfg(feature = "tracing")]
    #[must_use]
    #[inline]
    pub fn into_inner(self) -> Option<SpanTrace> {
        self.0
    }

    /// Returns the inner `Option<SpanTrace>`.
    #[cfg(not(feature = "tracing"))]
    #[must_use]
    #[inline]
    pub const fn into_inner(self) -> Option<SpanTrace> {
        self.0
    }

    /// Returns a reference to the inner `SpanTrace` if present.
    #[must_use]
    #[inline]
    pub const fn as_ref(&self) -> Option<&SpanTrace> {
        self.0.as_ref()
    }

    /// Returns true if this contains a span trace.
    #[must_use]
    #[inline]
    pub const fn is_some(&self) -> bool {
        self.0.is_some()
    }

    /// Returns true if this does not contain a span trace.
    #[must_use]
    #[inline]
    pub const fn is_none(&self) -> bool {
        self.0.is_none()
    }
}

impl fmt::Display for OptionalSpanTrace {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(trace) => fmt::Display::fmt(trace, f),
            None => Ok(()),
        }
    }
}

impl crate::Capturable for OptionalSpanTrace {
    #[inline]
    fn capture() -> Self {
        let trace = SpanTrace::capture();
        if trace.is_captured() {
            Self(Some(trace))
        } else {
            Self(None)
        }
    }

    #[inline]
    fn capture_or_extract(source: &dyn crate::Diagnostic) -> Self {
        match source.oopsie_spantrace() {
            // Keep the source's trace only if capture actually succeeded; an
            // Empty/Unsupported trace must not flip `is_some()` to true (the
            // type's documented invariant) — fall back to a fresh capture.
            Some(trace) if trace.is_captured() => Self::some(trace.clone()),
            _ => <Self as crate::Capturable>::capture(),
        }
    }
}

impl From<SpanTrace> for OptionalSpanTrace {
    #[inline]
    fn from(trace: SpanTrace) -> Self {
        Self(Some(trace))
    }
}

impl From<Option<SpanTrace>> for OptionalSpanTrace {
    #[inline]
    fn from(opt: Option<SpanTrace>) -> Self {
        Self(opt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Capturable;

    #[test]
    fn test_span_trace_capture() {
        let trace = SpanTrace::capture();
        let _ = trace.is_captured();
    }

    #[test]
    fn test_generate_implicit_data() {
        let trace: SpanTrace = Capturable::capture();
        let _ = trace.is_captured();
    }

    #[test]
    fn test_optional_span_trace() {
        let trace: OptionalSpanTrace = Capturable::capture();
        let _ = trace;
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn test_into_span_trace_tracing_returns_some() {
        let tracing = SpanTrace::capture();
        let _ = tracing.into_span_trace();
    }

    #[test]
    fn test_optional_span_trace_none_into_inner() {
        let opt = OptionalSpanTrace::none();
        assert!(opt.into_inner().is_none());
    }

    #[test]
    fn test_optional_span_trace_as_ref_none() {
        let opt = OptionalSpanTrace::none();
        assert!(opt.as_ref().is_none());
    }

    #[test]
    fn test_optional_span_trace_is_none() {
        let opt = OptionalSpanTrace::none();
        assert!(opt.is_none());
        assert!(!opt.is_some());
    }

    #[test]
    fn test_optional_span_trace_display_none() {
        let opt = OptionalSpanTrace::none();
        let display = opt.to_string();
        assert!(display.is_empty());
    }

    #[test]
    fn test_optional_span_trace_from_option_none() {
        let opt: OptionalSpanTrace = None::<SpanTrace>.into();
        assert!(opt.is_none());
    }

    #[test]
    fn test_optional_span_trace_generate_without_subscriber() {
        let opt: OptionalSpanTrace = Capturable::capture();
        assert!(opt.is_none());
    }

    // Tests that need a live SpanTrace instance.
    #[test]
    fn test_optional_span_trace_some_into_inner() {
        let trace = SpanTrace::capture();
        let opt = OptionalSpanTrace::some(trace);
        assert!(opt.into_inner().is_some());
    }

    #[test]
    fn test_optional_span_trace_as_ref_some() {
        let trace = SpanTrace::capture();
        let opt = OptionalSpanTrace::some(trace);
        assert!(opt.as_ref().is_some());
    }

    #[test]
    fn test_optional_span_trace_is_some() {
        let trace = SpanTrace::capture();
        let opt = OptionalSpanTrace::some(trace);
        assert!(opt.is_some());
        assert!(!opt.is_none());
    }

    #[test]
    fn test_optional_span_trace_display_some() {
        let trace = SpanTrace::capture();
        let opt = OptionalSpanTrace::some(trace);
        let _ = opt.to_string();
    }

    #[test]
    fn test_optional_span_trace_from_spantrace() {
        let trace = SpanTrace::capture();
        let opt: OptionalSpanTrace = trace.into();
        assert!(opt.is_some());
    }

    #[test]
    fn test_optional_span_trace_from_option_some() {
        let trace = SpanTrace::capture();
        let opt: OptionalSpanTrace = Some(trace).into();
        assert!(opt.is_some());
    }

    #[cfg(feature = "tracing")]
    fn with_error_subscriber<R>(f: impl FnOnce() -> R) -> R {
        use tracing_subscriber::prelude::*;
        let subscriber =
            tracing_subscriber::Registry::default().with(tracing_error::ErrorLayer::default());
        tracing::subscriber::with_default(subscriber, f)
    }

    #[cfg(feature = "tracing")]
    fn without_error_subscriber<R>(f: impl FnOnce() -> R) -> R {
        let subscriber = tracing_subscriber::Registry::default();
        tracing::subscriber::with_default(subscriber, f)
    }

    // Fixed callsites shared across captures: a depth-3 stack leaf -> mid -> root.
    // Same source location => same `&'static Metadata`, so two captures through
    // the same functions yield identical stacks.
    #[cfg(feature = "tracing")]
    fn leaf() -> SpanTrace {
        let _g = tracing::info_span!("leaf").entered();
        SpanTrace::capture()
    }
    #[cfg(feature = "tracing")]
    fn mid() -> SpanTrace {
        let _g = tracing::info_span!("mid").entered();
        leaf()
    }
    #[cfg(feature = "tracing")]
    fn via_root_a() -> SpanTrace {
        let _g = tracing::info_span!("root_a").entered();
        mid()
    }
    #[cfg(feature = "tracing")]
    fn via_root_b() -> SpanTrace {
        let _g = tracing::info_span!("root_b").entered();
        mid()
    }

    // A single fixed callsite that records a field value. Two captures through
    // this function share the same `&'static Metadata` (same source location),
    // so they differ ONLY in the recorded value of `x`.
    #[cfg(feature = "tracing")]
    fn via_field(x: u32) -> SpanTrace {
        let _g = tracing::info_span!("field_span", x).entered();
        SpanTrace::capture()
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn identical_depth3_stacks_are_equal() {
        let (a, b) = with_error_subscriber(|| (via_root_a(), via_root_a()));
        assert_eq!(a.status(), SpanTraceStatus::Captured);
        assert_eq!(a, b);
        assert_eq!(a, a.clone());
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn depth3_stacks_differing_only_at_root_are_unequal() {
        // Shared leaf+mid callsites, divergent root: the case a leaf-only or
        // out-of-order comparison gets wrong.
        let (a, b) = with_error_subscriber(|| (via_root_a(), via_root_b()));
        assert_ne!(a, b);
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn shorter_stack_is_not_equal_to_deeper_one() {
        let (deep, shallow) = with_error_subscriber(|| (via_root_a(), leaf()));
        assert_ne!(deep, shallow);
        // Reverse direction exercises the b-longer-than-a frame-walk branch.
        assert_ne!(shallow, deep);
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn empty_span_traces_are_equal() {
        // Without a subscriber, captures are empty; equality must be
        // reflexive.
        let a = SpanTrace::capture();
        let b = SpanTrace::capture();
        assert_eq!(a.status(), SpanTraceStatus::Empty);
        assert_eq!(b.status(), SpanTraceStatus::Empty);
        assert_eq!(a, b);
        assert_eq!(a, a.clone());
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn unsupported_span_traces() {
        // When a subscriber is present but does not support span traces, captures are
        // unsupported.
        let (a, b) = without_error_subscriber(|| (via_root_a(), via_root_b()));

        assert_eq!(a.status(), SpanTraceStatus::Unsupported);
        assert_eq!(b.status(), SpanTraceStatus::Unsupported);
        assert_eq!(a, b);
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn same_callsite_differing_field_values() {
        // Both captures go through the *same* `info_span!` callsite, so the
        // callsite comparison in `eq` can't tell them apart — only the recorded
        // value of `x` differs. This is the one shape that reaches the
        // field-value branch of `eq` with a non-equal result; every other
        // inequality test diverges by span name (i.e. by callsite).
        let (a, b) = with_error_subscriber(|| (via_field(1), via_field(2)));
        assert_eq!(a.status(), SpanTraceStatus::Captured);

        // Control: identical callsite *and* identical field value compare equal
        // in either build profile, proving the divergence below comes from the
        // field values and nothing else.
        let (c, d) = with_error_subscriber(|| (via_field(1), via_field(1)));
        assert_eq!(c, d);

        // Debug builds compare recorded field values; release builds compare
        // callsites only (see `capture_fields`/`fields_eq`).
        #[cfg(debug_assertions)]
        assert_ne!(a, b, "debug builds must distinguish differing field values");
        #[cfg(not(debug_assertions))]
        assert_eq!(
            a, b,
            "release builds compare callsites only, ignoring fields"
        );
    }

    #[derive(Debug)]
    struct DiagSource;

    impl fmt::Display for DiagSource {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("diag source")
        }
    }

    impl std::error::Error for DiagSource {}
    impl crate::Diagnostic for DiagSource {}

    #[test]
    fn optional_span_trace_captures_from_diagnostic_source() {
        let opt = <OptionalSpanTrace as crate::Capturable>::capture_or_extract(&DiagSource);
        assert!(opt.is_none());
    }

    #[derive(Debug)]
    struct DiagSourceWithEmptyTrace(SpanTrace);

    impl fmt::Display for DiagSourceWithEmptyTrace {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("diag source")
        }
    }

    impl std::error::Error for DiagSourceWithEmptyTrace {}
    impl crate::Diagnostic for DiagSourceWithEmptyTrace {
        fn oopsie_spantrace(&self) -> Option<&SpanTrace> {
            Some(&self.0)
        }
    }

    #[test]
    fn capture_or_extract_drops_empty_source_trace() {
        // With no active subscriber the captured trace is empty/unsupported.
        // Extracting it must not flip `is_some()` to true (the type invariant).
        let src = DiagSourceWithEmptyTrace(SpanTrace::capture());
        assert!(!src.0.is_captured(), "precondition: source trace is empty");
        let opt = <OptionalSpanTrace as crate::Capturable>::capture_or_extract(&src);
        assert!(opt.is_none());
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn span_trace_capture_or_extract_recaptures_over_empty_source_trace() {
        // Source captured with no subscriber → empty trace.
        let src = DiagSourceWithEmptyTrace(SpanTrace::capture());
        assert!(!src.0.is_captured(), "precondition: source trace is empty");
        // Wrap site has a live subscriber inside an active span → fresh capture
        // must win over the source's empty trace.
        let extracted = with_error_subscriber(|| {
            let _g = tracing::info_span!("wrap_site").entered();
            <SpanTrace as crate::Capturable>::capture_or_extract(&src)
        });
        assert!(extracted.is_captured());
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn span_trace_capture_or_extract_keeps_captured_source_trace() {
        let (src_trace, extracted) = with_error_subscriber(|| {
            let src = DiagSourceWithEmptyTrace(leaf());
            let t = <SpanTrace as crate::Capturable>::capture_or_extract(&src);
            (src.0, t)
        });
        assert!(extracted.is_captured());
        assert_eq!(extracted, src_trace);
    }

    #[cfg(not(feature = "tracing"))]
    mod stub {
        use super::*;

        #[test]
        fn capture_is_inert() {
            assert!(!SpanTrace::capture().is_captured());
        }

        #[test]
        fn display_is_empty() {
            assert!(SpanTrace::capture().to_string().is_empty());
        }

        #[test]
        fn equality_is_reflexive() {
            let a = SpanTrace::capture();
            assert_eq!(a, a.clone());
            assert_eq!(SpanTrace::capture(), SpanTrace::capture());
        }

        #[test]
        fn capture_or_extract_yields_stub() {
            let extracted = <SpanTrace as crate::Capturable>::capture_or_extract(
                &DiagSourceWithEmptyTrace(SpanTrace::capture()),
            );
            assert!(!extracted.is_captured());
        }

        #[test]
        fn optional_capture_is_none() {
            let opt: OptionalSpanTrace = Capturable::capture();
            assert!(opt.is_none());
        }
    }
}
