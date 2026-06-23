//! Core error types and utilities.

#![warn(missing_docs)]
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
mod chain;
mod diagnostic;
#[cfg(feature = "serde")]
pub mod erased;
#[cfg(feature = "extras")]
pub mod extras;
mod marker;
mod spantrace;
#[cfg(feature = "test-utils")]
pub mod test_utils;
mod traits;
mod welp;

use std::borrow::{Borrow, Cow};
use std::ops::Deref;

pub use backtrace::{
    Backtrace, RustBacktrace, clear_rust_backtrace_override, rust_backtrace, rust_panic_backtrace,
    set_rust_backtrace_override, with_rust_backtrace_override,
};
pub use chain::{Chain, ErrorChainExt};
pub use diagnostic::Diagnostic;
pub use spantrace::{OptionalSpanTrace, SpanTrace};
#[cfg(feature = "tracing")]
pub mod tracing;

pub use traits::*;
pub use welp::{Welp, WelpOptionExt, WelpResultExt};
/// Private helpers used by macro-generated code. Not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use crate::backtrace::CORE_SRC_PATH;
    /// Autoref probe for capture deduplication.
    ///
    /// When the concrete source type implements `Diagnostic`, the high-priority
    /// `CaptureFromExt` impl is selected and tries to extract existing traces.
    /// For non-`Diagnostic` sources (e.g., `io::Error`), the low-priority
    /// `CaptureFromFallback` impl is selected via autoref and does fresh capture.
    pub struct CaptureProbe<'a, T: ?Sized>(pub &'a T);

    /// High-priority: source implements `Diagnostic` → try extraction.
    pub trait CaptureFromExt {
        fn resolve<C: crate::Capturable>(&self) -> C;
    }

    impl<T: crate::Diagnostic> CaptureFromExt for CaptureProbe<'_, T> {
        #[inline]
        #[track_caller]
        fn resolve<C: crate::Capturable>(&self) -> C {
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
        fn fwd_location(&self) -> Option<&'static std::panic::Location<'static>>;
        fn fwd_exit_code(&self) -> Option<core::num::NonZeroU8>;
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
        #[inline]
        fn fwd_location(&self) -> Option<&'static std::panic::Location<'static>> {
            self.0.oopsie_location()
        }
        #[inline]
        fn fwd_exit_code(&self) -> Option<core::num::NonZeroU8> {
            self.0.oopsie_exit_code()
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
        #[inline]
        fn fwd_location(&self) -> Option<&'static std::panic::Location<'static>> {
            None
        }
        #[inline]
        fn fwd_exit_code(&self) -> Option<core::num::NonZeroU8> {
            None
        }
    }

    impl<'a, T: ?Sized> DiagForwardFallback<'a> for &DiagProbe<'a, T> {}

    /// Pull the deepest `T` reachable from a source error via the Provider API.
    ///
    /// The generated `provide()` forwards to the source before providing its
    /// own trace, and std's `Request` is first-wins, so the deepest provider in
    /// the chain fills the slot — this surfaces the origin-most trace rather
    /// than a wrap-site one. Trace accessors go through the typed
    /// [`source_backtrace`] / `source_spantrace` wrappers, which add the
    /// skip-empty filter on top of this lookup.
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

    /// [`source_trace`] for `Backtrace`, treating an empty trace as absent:
    /// a trace that captured no frames is not "available" and must not shadow
    /// a captured one further out.
    #[inline]
    #[must_use]
    pub fn source_backtrace<'a>(
        source: &'a (dyn std::error::Error + 'static),
    ) -> Option<&'a crate::Backtrace> {
        source_trace::<crate::Backtrace>(source).filter(|bt| bt.is_captured())
    }

    /// [`source_trace`] for `SpanTrace`, treating an empty trace as absent.
    #[inline]
    #[must_use]
    pub fn source_spantrace<'a>(
        source: &'a (dyn std::error::Error + 'static),
    ) -> Option<&'a crate::SpanTrace> {
        source_trace::<crate::SpanTrace>(source).filter(|st| st.is_captured())
    }

    pub use crate::marker::{TraceMarker, restore_marker, set_marker};

    /// The exit code a type-erased source declares, reached through the Provider
    /// API. Returns `None` on stable (descending into a `dyn Error` is not
    /// portable there).
    #[inline]
    #[must_use]
    pub fn source_exit_code(
        source: &(dyn std::error::Error + 'static),
    ) -> Option<core::num::NonZeroU8> {
        #[cfg(feature = "unstable-error-generic-member-access")]
        {
            core::error::request_value::<core::num::NonZeroU8>(source)
        }
        #[cfg(not(feature = "unstable-error-generic-member-access"))]
        {
            let _ = source;
            None
        }
    }

    /// Build a `NonZeroU8` from a value the macro already validated to be in
    /// `1..=255`. Panics at const-eval if the invariant is ever broken, so the
    /// generated code stays `unsafe`-free.
    #[inline]
    #[must_use]
    pub const fn nonzero_u8_unchecked(value: u8) -> core::num::NonZeroU8 {
        match core::num::NonZeroU8::new(value) {
            Some(n) => n,
            None => panic!("exit code must be non-zero"),
        }
    }

    /// Fixed-capacity const string builder: concatenate string slices and base-10
    /// `usize`s in a `const` context, then borrow the result as `&str` — e.g. to
    /// feed a const `panic!("{}", …)` with a computed message. `N` is the byte
    /// capacity; callers size it from the pieces (`Σ part.len() + 20·#usize`) so a
    /// push never runs past the buffer.
    pub struct ConstStr<const N: usize> {
        buf: [u8; N],
        len: usize,
    }

    impl<const N: usize> ConstStr<N> {
        #[inline]
        #[must_use]
        pub const fn new() -> Self {
            Self {
                buf: [0; N],
                len: 0,
            }
        }

        /// Append a string slice (its UTF-8 bytes verbatim).
        #[inline]
        #[must_use]
        pub const fn str(mut self, s: &str) -> Self {
            let b = s.as_bytes();
            let mut i = 0;
            while i < b.len() {
                self.buf[self.len] = b[i];
                self.len += 1;
                i += 1;
            }
            self
        }

        /// Append a `usize` in base 10.
        #[inline]
        #[must_use]
        pub const fn usize(mut self, mut n: usize) -> Self {
            let mut digits = [0u8; 20];
            let mut count = if n == 0 {
                digits[0] = b'0';
                1
            } else {
                0
            };
            while n > 0 {
                digits[count] = b'0' + (n % 10) as u8;
                n /= 10;
                count += 1;
            }
            while count > 0 {
                count -= 1;
                self.buf[self.len] = digits[count];
                self.len += 1;
            }
            self
        }

        /// Borrow the accumulated bytes as `&str`. Only valid UTF-8 and ASCII digits
        /// are ever written, so the error branch is unreachable in correct use.
        #[inline]
        #[must_use]
        pub const fn as_str(&self) -> &str {
            match core::str::from_utf8(self.buf.split_at(self.len).0) {
                Ok(s) => s,
                Err(_) => panic!("ConstStr: built a non-utf8 message"),
            }
        }
    }

    impl<const N: usize> Default for ConstStr<N> {
        fn default() -> Self {
            Self::new()
        }
    }

    #[cfg(feature = "chrono")]
    pub use chrono;
}

macro_rules! impl_string_newtypes {
    ($($(#[$meta:meta])* $ident:ident,)* $(,)?) => { $(
        #[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        #[repr(transparent)]
        $(#[$meta])*
        pub struct $ident(Cow<'static, str>);

        impl $ident {
            #[doc = concat!("Build a new `", stringify!($ident), "` from a `&'static str`.")]
            #[must_use]
            #[inline]
            pub const fn from_static(s: &'static str) -> Self {
                Self(Cow::Borrowed(s))
            }

            #[doc = concat!("Build a new `", stringify!($ident), "` from a `String`.")]
            #[must_use]
            #[inline]
            pub const fn from_string(s: String) -> Self {
                Self(Cow::Owned(s))
            }

            /// Borrow the underlying string.
            #[must_use]
            #[inline]
            pub const fn as_str(&self) -> &str {
                match &self.0 {
                    Cow::Borrowed(s) => s,
                    Cow::Owned(s) => s.as_str(),
                }
            }

            #[doc = concat!("Consume the `", stringify!($ident), "` and return the underlying `Cow`.")]
            #[must_use]
            #[inline]
            pub fn into_inner(self) -> Cow<'static, str> {
                self.0
            }
        }

        impl Deref for $ident {
            type Target = str;

            #[inline]
            fn deref(&self) -> &Self::Target {
                self.as_str()
            }
        }

        impl Borrow<str> for $ident {
            #[inline]
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl From<&'static str> for $ident {
            #[inline]
            fn from(s: &'static str) -> Self {
                Self::from_static(s)
            }
        }

        impl From<String> for $ident {
            #[inline]
            fn from(s: String) -> Self {
                Self::from_string(s)
            }
        }

        impl std::fmt::Display for $ident {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

    )* };
}

impl_string_newtypes!(
    /// An opaque error code, used for programmatic handling and matching.
    ErrorCode,
    /// User-facing help text, intended to be shown in diagnostics.
    HelpText,
);

#[cfg(test)]
mod const_str_tests {
    use crate::__private::ConstStr;

    #[test]
    fn builds_interleaved_string() {
        let s = ConstStr::<64>::new()
            .str("`E` is ")
            .usize(80)
            .str(" bytes, must be ≤ 64");
        assert_eq!(s.as_str(), "`E` is 80 bytes, must be ≤ 64");
    }

    #[test]
    fn formats_zero_and_large() {
        assert_eq!(ConstStr::<8>::new().as_str(), "");
        assert_eq!(ConstStr::<32>::new().usize(0).as_str(), "0");
        assert_eq!(
            ConstStr::<32>::new().usize(18446744073709551615).as_str(),
            "18446744073709551615"
        );
    }

    #[test]
    fn usable_in_const_context() {
        const M: &str = {
            const C: ConstStr<16> = ConstStr::new().str("n=").usize(42);
            C.as_str()
        };
        assert_eq!(M, "n=42");
    }
}
