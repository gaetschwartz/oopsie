//! SpanTrace wrapper with `Capturable` support.

use std::fmt;

/// A wrapper around `tracing_error::SpanTrace`.
#[derive(Debug, Clone)]
pub struct SpanTrace {
    inner: tracing_error::SpanTrace,
}

impl SpanTrace {
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn capture() -> Self {
        Self {
            inner: tracing_error::SpanTrace::capture(),
        }
    }

    #[must_use]
    #[inline]
    pub const fn new(inner: tracing_error::SpanTrace) -> Self {
        Self { inner }
    }

    #[must_use]
    #[inline]
    pub fn status(&self) -> tracing_error::SpanTraceStatus {
        self.inner.status()
    }

    #[must_use]
    #[inline]
    pub fn into_span_trace(self) -> tracing_error::SpanTrace {
        self.inner
    }

    #[inline]
    #[must_use]
    pub const fn as_span_trace(&self) -> &tracing_error::SpanTrace {
        &self.inner
    }
}

impl PartialEq for SpanTrace {
    fn eq(&self, other: &Self) -> bool {
        // `with_spans` iterates until the closure returns `false`; returning
        // `true` walks the whole trace. Span metadata is `&'static`, so a
        // callsite always yields the same address across captures — identity
        // comparison is both correct and cheaper than formatting.
        fn spans(inner: &tracing_error::SpanTrace) -> Vec<(*const (), String)> {
            let mut spans = Vec::new();
            inner.with_spans(|metadata, fields| {
                spans.push((std::ptr::from_ref(metadata).cast::<()>(), fields.to_owned()));
                true
            });
            spans
        }
        spans(&self.inner) == spans(&other.inner)
    }
}

impl fmt::Display for SpanTrace {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl crate::Capturable for SpanTrace {
    #[track_caller]
    #[inline]
    fn capture() -> Self {
        Self::capture()
    }
}

impl crate::CaptureExt for SpanTrace {
    #[inline]
    fn capture_or_extract(source: &dyn crate::Diagnostic) -> Self {
        source
            .oopsie_spantrace()
            .cloned()
            .unwrap_or_else(Self::capture)
    }
}

/// A wrapper around `Option<SpanTrace>` that implements [`Capturable`].
///
/// This type only captures a span trace if the capture was successful
/// (i.e., there was an active span and the subscriber supports it).
/// Use this when you want to optionally include span traces.
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
    #[must_use]
    #[inline]
    pub fn into_inner(self) -> Option<SpanTrace> {
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
    #[track_caller]
    #[inline]
    fn capture() -> Self {
        let trace = SpanTrace::capture();
        match trace.status() {
            tracing_error::SpanTraceStatus::CAPTURED => Self(Some(trace)),
            _ => Self(None),
        }
    }
}

impl crate::CaptureExt for OptionalSpanTrace {
    #[inline]
    fn capture_or_extract(source: &dyn crate::Diagnostic) -> Self {
        match source.oopsie_spantrace().cloned() {
            Some(trace) => Self::some(trace),
            None => <Self as crate::Capturable>::capture(),
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
        let _ = trace.status();
    }

    #[test]
    fn test_generate_implicit_data() {
        let trace: SpanTrace = Capturable::capture();
        let _ = trace.status();
    }

    #[test]
    fn test_optional_span_trace() {
        let trace: OptionalSpanTrace = Capturable::capture();
        let _ = trace;
    }

    #[test]
    fn test_into_span_trace_tracing_returns_some() {
        let tracing = SpanTrace::capture();
        let _ = tracing.into_span_trace();
    }

    // Keep ALL the OptionalSpanTrace tests that don't use serde/fallback
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

    // Tests that need a SpanTrace instance (use capture() instead of serde)
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

    #[test]
    fn empty_span_traces_are_equal() {
        // Without a subscriber, captures are uncaptured/empty; equality must be
        // reflexive rather than reporting two empty traces as unequal.
        let a = SpanTrace::capture();
        let b = SpanTrace::capture();
        assert_eq!(a, b);
        assert_eq!(a, a.clone());
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
        // Regression: `OptionalSpanTrace` must implement `CaptureExt` so the
        // macro's `resolve::<OptionalSpanTrace>()` path compiles for variants
        // whose source implements `Diagnostic`.
        let opt = <OptionalSpanTrace as crate::CaptureExt>::capture_or_extract(&DiagSource);
        assert!(opt.is_none());
    }
}
