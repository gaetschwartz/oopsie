//! Serializable backtrace representation.

use std::fmt;
use std::path;

use serde::{Deserialize, Serialize};

/// A serializable, type-erased representation of a backtrace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErasedBacktrace {
    frames: Box<[ErasedFrame]>,
}

/// A single frame in an erased backtrace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErasedFrame {
    pub name: Option<Box<str>>,
    pub filename: Option<Box<path::Path>>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl ErasedBacktrace {
    /// Create an `ErasedBacktrace` from a live `Backtrace`. See
    /// [`from_backtrace`](Self::from_backtrace).
    #[must_use]
    #[inline]
    pub fn from_backtrace_ref(bt: &oopsie_core::Backtrace) -> Self {
        Self::from_backtrace(bt.clone())
    }
    /// Create an `ErasedBacktrace` from a live `Backtrace`.
    ///
    /// Frames that [`oopsie_core::is_internal_frame`] considers
    /// implementation/platform detail (capture machinery, OS/libc entry
    /// points, unresolvable frames) are stripped, unless `RUST_BACKTRACE=full`
    /// is set. The keep/drop decision is made per frame on its primary symbol,
    /// matching the non-erased `Backtrace` `Debug` rendering.
    #[must_use]
    pub fn from_backtrace(mut bt: oopsie_core::Backtrace) -> Self {
        bt.resolve();
        let full = oopsie_core::rust_backtrace().is_full();
        let frames = bt
            .frames()
            .iter()
            .filter(|frame| {
                if full {
                    return true;
                }
                let primary = frame.symbols().first();
                let name = primary
                    .and_then(backtrace::BacktraceSymbol::name)
                    .and_then(|n| n.as_str());
                let filename = primary.and_then(|s| s.filename());
                !oopsie_core::is_internal_frame(name, filename)
            })
            .flat_map(|frame| {
                frame.symbols().iter().map(|sym| ErasedFrame {
                    name: sym.name().map(|n| n.to_string().into_boxed_str()),
                    filename: sym.filename().map(Box::from),
                    line: sym.lineno(),
                    column: sym.colno(),
                })
            })
            .collect();
        Self { frames }
    }

    /// Returns a slice of all frames.
    #[must_use]
    #[inline]
    pub fn frames(&self) -> &[ErasedFrame] {
        &self.frames
    }
}

impl From<&oopsie_core::Backtrace> for ErasedBacktrace {
    #[inline]
    fn from(bt: &oopsie_core::Backtrace) -> Self {
        Self::from_backtrace_ref(bt)
    }
}
impl From<oopsie_core::Backtrace> for ErasedBacktrace {
    #[inline]
    fn from(bt: oopsie_core::Backtrace) -> Self {
        Self::from_backtrace(bt)
    }
}

impl fmt::Display for ErasedBacktrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, frame) in self.frames.iter().enumerate() {
            write!(f, "{:>3}: ", i + 1)?;
            if let Some(name) = &frame.name {
                writeln!(f, "{name}")?;
            } else {
                writeln!(f, "<unknown>")?;
            }
            if let Some(filename) = &frame.filename {
                write!(f, "           at {}", filename.display())?;
                if let Some(line) = frame.line {
                    write!(f, ":{line}")?;
                    if let Some(col) = frame.column {
                        write!(f, ":{col}")?;
                    }
                }
                writeln!(f)?;
            }
        }
        Ok(())
    }
}
