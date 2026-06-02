//! Core error types and utilities.

#![cfg_attr(
    feature = "unstable-try-trait-v2",
    feature(try_trait_v2, try_trait_v2_residual)
)]
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
#[cfg(feature = "unstable-try-trait-v2")]
mod result;
pub mod spantrace;
mod traits;
mod welp;

use std::borrow::Cow;
use std::io;
use std::ops::Deref;

pub use backtrace::{
    Backtrace, RustBacktrace, clear_rust_backtrace_override, is_internal_frame, rust_backtrace,
    set_rust_backtrace_override,
};
use color_backtrace::termcolor;
pub use diagnostic::Diagnostic;

/// Private helpers used by macro-generated code. Not part of the public API.
#[doc(hidden)]
pub mod __private {
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

/// Install the color-eyre panic and error hooks globally.
///
/// This should be called once, early in main, before spawning any threads.
/// It provides colored backtraces and span traces on panics.
#[inline]
pub fn install_panic_hook() -> color_eyre::Result<()> {
    color_eyre::install()
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

#[expect(dead_code)]
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
