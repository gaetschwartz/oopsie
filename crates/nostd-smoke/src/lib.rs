#![no_std]

extern crate alloc;

use alloc::string::{String, ToString};
use core::fmt;

use oopsie::Oopsie;
use oopsie::oopsie;
use oopsie::prelude::*;

/// A minimal no_std source error: no allocator-backed message, just a fixed
/// `Display`/`core::error::Error` impl.
#[derive(Debug)]
pub struct SensorFault;

impl fmt::Display for SensorFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("sensor fault")
    }
}

impl core::error::Error for SensorFault {}

/// A derived error — proves `#[derive(Oopsie)]` output (Display via the
/// `::alloc::format!` facade, `core::error::Error`, `#[track_caller]`
/// location capture) is no_std-clean.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
pub enum SmokeError {
    #[oopsie("reading {sensor} failed: {source}")]
    Read { sensor: String, source: SensorFault },
    #[oopsie("code {code} out of range")]
    OutOfRange { code: u32 },
}

/// Proves construct + `.context(...)` + chain walking + `Display` rendering
/// works with zero std, using the derive-generated selector.
pub fn render_read_failure() -> String {
    let err: Result<(), SensorFault> = Err(SensorFault);
    let err = err.context(Read { sensor: "temp-1" }).unwrap_err();

    let messages: alloc::vec::Vec<String> = err.chain().map(|e| e.to_string()).collect();
    messages.join(" | ")
}

/// Proves the leaf (no-source) selector's `.build()` / `.fail()` path.
pub fn render_out_of_range() -> String {
    let err: Result<(), SmokeError> = OutOfRange { code: 7u32 }.fail();
    err.unwrap_err().to_string()
}

/// Proves `Welp` construction, wrapping, and chain rendering work no_std.
pub fn render_welp() -> String {
    let err: Result<(), SensorFault> = Err(SensorFault);
    let err = err.welp_context("could not read sensor").unwrap_err();
    err.to_string()
}

/// Proves `#[oopsie(traced)]` (backtrace + spantrace, no timestamp) — the
/// supported no_std surface — builds no_std; `timestamp` stays std/chrono-only.
#[oopsie(traced)]
pub enum TracedError {
    #[oopsie("overheated")]
    Overheated,
}

/// Proves the traced selector builds and captures its trace fields no_std.
pub fn render_traced() -> String {
    traced_oopsies::Overheated.build().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_failure_renders_chain() {
        assert_eq!(
            render_read_failure(),
            "reading temp-1 failed: sensor fault | sensor fault"
        );
    }

    #[test]
    fn out_of_range_renders() {
        assert_eq!(render_out_of_range(), "code 7 out of range");
    }

    #[test]
    fn welp_renders() {
        assert_eq!(render_welp(), "could not read sensor");
    }
}
