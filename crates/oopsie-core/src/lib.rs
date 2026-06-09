//! Core error types and utilities.

#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
// Doctests that use `#[derive(Oopsie)]` will see the macro emit a `provide`
// impl when the unstable feature is active; inject the corresponding language
// feature flag so those doctests compile under `--features unstable`.
#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    doc(test(attr(feature(error_generic_member_access))))
)]

mod backtrace;
mod diagnostic;
pub mod spantrace;
#[cfg(feature = "test-utils")]
pub mod test_utils;
mod traits;
mod welp;

use std::borrow::Cow;
use std::io;
use std::ops::Deref;

pub use backtrace::{
    Backtrace, RustBacktrace, clear_rust_backtrace_override, rust_backtrace, rust_panic_backtrace,
    set_rust_backtrace_override, with_rust_backtrace_override,
};
use color_backtrace::termcolor;
pub use diagnostic::Diagnostic;

/// Private helpers used by macro-generated code. Not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use crate::backtrace::{
        is_backtrace_capture_code, is_internal_frame, is_post_panic_code, is_runtime_init_code,
    };
    /// Autoref probe for capture deduplication.
    ///
    /// When the concrete source type implements `Diagnostic`, the high-priority
    /// `CaptureFromExt` impl is selected and tries to extract existing traces.
    /// For non-`Diagnostic` sources (e.g., `io::Error`), the low-priority
    /// `CaptureFromFallback` impl is selected via autoref and does fresh capture.
    pub struct CaptureProbe<'a, T: ?Sized>(pub &'a T);

    /// High-priority: source implements `Diagnostic` → try extraction.
    pub trait CaptureFromExt {
        fn resolve<C: crate::CaptureExt>(&self) -> C;
    }

    impl<T: crate::Diagnostic> CaptureFromExt for CaptureProbe<'_, T> {
        #[inline]
        #[track_caller]
        fn resolve<C: crate::CaptureExt>(&self) -> C {
            C::capture_or_extract(self.0)
        }
    }

    /// Low-priority: source doesn't implement `Diagnostic` → fresh capture.
    pub trait CaptureFromFallback {
        fn resolve<C: crate::Capturable>(&self) -> C;
    }

    impl<T: ?Sized> CaptureFromFallback for &CaptureProbe<'_, T> {
        #[inline]
        #[track_caller]
        fn resolve<C: crate::Capturable>(&self) -> C {
            C::capture()
        }
    }

    /// Accessor-time autoref probe for forwarding a transparent wrapper's
    /// `Diagnostic` accessors to its source.
    ///
    /// Unlike [`CaptureProbe`] (which runs at construction to decide
    /// extract-vs-capture), this runs when `oopsie_*` is called: a transparent
    /// variant delegates each accessor to its source's concrete type. When that
    /// type implements `Diagnostic` the high-priority [`DiagForwardExt`] impl
    /// forwards the call; otherwise the [`DiagForwardFallback`] impl (selected
    /// via autoref) yields `None`. Works on stable, where the Provider-API
    /// [`source_trace`] path would yield `None`.
    pub struct DiagProbe<'a, T: ?Sized>(pub &'a T);

    /// High-priority: source implements `Diagnostic` → forward the accessor.
    pub trait DiagForwardExt<'a> {
        fn fwd_code(&self) -> Option<crate::ErrorCode>;
        fn fwd_help(&self) -> Option<crate::HelpText>;
        fn fwd_backtrace(&self) -> Option<&'a crate::Backtrace>;
        fn fwd_spantrace(&self) -> Option<&'a crate::SpanTrace>;
    }

    impl<'a, T: crate::Diagnostic + ?Sized> DiagForwardExt<'a> for DiagProbe<'a, T> {
        #[inline]
        fn fwd_code(&self) -> Option<crate::ErrorCode> {
            self.0.oopsie_error_code()
        }
        #[inline]
        fn fwd_help(&self) -> Option<crate::HelpText> {
            self.0.oopsie_help_text()
        }
        #[inline]
        fn fwd_backtrace(&self) -> Option<&'a crate::Backtrace> {
            self.0.oopsie_backtrace()
        }
        #[inline]
        fn fwd_spantrace(&self) -> Option<&'a crate::SpanTrace> {
            self.0.oopsie_spantrace()
        }
    }

    /// Low-priority: source doesn't implement `Diagnostic` → nothing to forward.
    pub trait DiagForwardFallback<'a> {
        #[inline]
        fn fwd_code(&self) -> Option<crate::ErrorCode> {
            None
        }
        #[inline]
        fn fwd_help(&self) -> Option<crate::HelpText> {
            None
        }
        #[inline]
        fn fwd_backtrace(&self) -> Option<&'a crate::Backtrace> {
            None
        }
        #[inline]
        fn fwd_spantrace(&self) -> Option<&'a crate::SpanTrace> {
            None
        }
    }

    impl<'a, T: ?Sized> DiagForwardFallback<'a> for &DiagProbe<'a, T> {}

    /// Pull the deepest `T` reachable from a source error via the Provider API.
    ///
    /// The generated `provide()` forwards to the source before providing its
    /// own trace, and std's `Request` is first-wins, so the deepest provider in
    /// the chain fills the slot — this surfaces the origin-most trace rather
    /// than a wrap-site one. Derived stable accessors call this on their source
    /// so they agree with the provider path.
    ///
    /// Returns `None` without `unstable-error-generic-member-access`: descending
    /// into a type-erased `dyn Error` source is not portable there, and the
    /// caller falls back to the wrapper's own field.
    #[inline]
    #[must_use]
    pub fn source_trace<'a, T: 'static>(
        source: &'a (dyn std::error::Error + 'static),
    ) -> Option<&'a T> {
        #[cfg(feature = "unstable-error-generic-member-access")]
        {
            core::error::request_ref::<T>(source)
        }
        #[cfg(not(feature = "unstable-error-generic-member-access"))]
        {
            let _ = source;
            None
        }
    }
}

pub use spantrace::{OptionalSpanTrace, SpanTrace};
use tracing_error::ErrorLayer;
use tracing_subscriber::fmt::format::JsonFields;
use tracing_subscriber::registry::LookupSpan;
pub use traits::*;
pub use welp::{Welp, WelpOptionExt, WelpResultExt};

/// Install the color backtrace printer.
///
/// Replaces the process-global panic hook via `std::panic::set_hook`, so this
/// is intended to be called once, early in `main`. Callers needing to preserve
/// a previously installed hook must save (`take_hook`) and restore it
/// themselves, as `oopsie::Report::run` does.
pub fn install() {
    color_backtrace::BacktracePrinter::new().install(color_backtrace::default_output_stream());
}

#[derive(
    Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
#[repr(transparent)]
pub struct ErrorCode(Cow<'static, str>);

impl ErrorCode {
    /// Build an `ErrorCode` from a `&'static str` in `const` context.
    #[must_use]
    #[inline]
    pub const fn from_static(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }

    /// Borrow the underlying string.
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the `ErrorCode`, returning the underlying `Cow`.
    #[must_use]
    #[inline]
    pub fn into_inner(self) -> Cow<'static, str> {
        self.0
    }
}

impl Deref for ErrorCode {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&'static str> for ErrorCode {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }
}

impl From<String> for ErrorCode {
    #[inline]
    fn from(s: String) -> Self {
        Self(Cow::Owned(s))
    }
}

impl std::fmt::Display for ErrorCode {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Help text associated with an error, provided via the Provider API.
#[derive(
    Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
#[repr(transparent)]
pub struct HelpText(Cow<'static, str>);

impl HelpText {
    /// Build a `HelpText` from a `&'static str` in `const` context.
    #[must_use]
    #[inline]
    pub const fn from_static(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }

    /// Borrow the underlying string.
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the `HelpText`, returning the underlying `Cow`.
    #[must_use]
    #[inline]
    pub fn into_inner(self) -> Cow<'static, str> {
        self.0
    }
}

impl Deref for HelpText {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&'static str> for HelpText {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }
}

impl From<String> for HelpText {
    #[inline]
    fn from(s: String) -> Self {
        Self(Cow::Owned(s))
    }
}

impl std::fmt::Display for HelpText {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Construct a `tracing_error::ErrorLayer` configured to format span fields
/// as JSON.
///
/// Equivalent to `ErrorLayer::new(JsonFields::default())` but doesn't require
/// the caller to depend on `tracing-subscriber` directly.
#[inline]
#[must_use]
pub fn json_error_layer<S>() -> ErrorLayer<S, JsonFields>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    ErrorLayer::new(JsonFields::default())
}

#[expect(
    dead_code,
    reason = "color helper trait kept complete; not all methods are wired up yet"
)]
pub(crate) trait WriteColorExt {
    fn black(&mut self) -> io::Result<&mut Self>;
    fn blue(&mut self) -> io::Result<&mut Self>;
    fn green(&mut self) -> io::Result<&mut Self>;
    fn red(&mut self) -> io::Result<&mut Self>;
    fn cyan(&mut self) -> io::Result<&mut Self>;
    fn magenta(&mut self) -> io::Result<&mut Self>;
    fn yellow(&mut self) -> io::Result<&mut Self>;
    fn white(&mut self) -> io::Result<&mut Self>;
    fn rgb(&mut self, r: u8, g: u8, b: u8) -> io::Result<&mut Self>;
    fn ansi256(&mut self, code: u8) -> io::Result<&mut Self>;
    fn bold(&mut self) -> io::Result<&mut Self>;
    fn italic(&mut self) -> io::Result<&mut Self>;
    fn underline(&mut self) -> io::Result<&mut Self>;
    fn strike(&mut self) -> io::Result<&mut Self>;
}

impl<W: termcolor::WriteColor> WriteColorExt for W {
    #[inline]
    fn black(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Black)))?;
        Ok(self)
    }
    #[inline]
    fn blue(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Blue)))?;
        Ok(self)
    }
    #[inline]
    fn green(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Green)))?;
        Ok(self)
    }
    #[inline]
    fn red(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Red)))?;
        Ok(self)
    }
    #[inline]
    fn cyan(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Cyan)))?;
        Ok(self)
    }
    #[inline]
    fn magenta(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Magenta)))?;
        Ok(self)
    }
    #[inline]
    fn yellow(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Yellow)))?;
        Ok(self)
    }
    #[inline]
    fn white(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::White)))?;
        Ok(self)
    }
    #[inline]
    fn rgb(&mut self, r: u8, g: u8, b: u8) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Rgb(r, g, b))))?;
        Ok(self)
    }
    #[inline]
    fn ansi256(&mut self, code: u8) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Ansi256(code))))?;
        Ok(self)
    }
    #[inline]
    fn bold(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_bold(true))?;
        Ok(self)
    }
    #[inline]
    fn italic(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_italic(true))?;
        Ok(self)
    }
    #[inline]
    fn underline(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_underline(true))?;
        Ok(self)
    }
    #[inline]
    fn strike(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_strikethrough(true))?;
        Ok(self)
    }
}
