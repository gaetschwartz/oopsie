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
    /// Create an `ErasedBacktrace` from a live `Backtrace`.
    ///
    /// Frames belonging to the `backtrace` crate's own capture machinery
    /// (`backtrace::backtrace::*` and `<backtrace::capture::*>::*`) are
    /// stripped — those are platform/toolchain-dependent implementation
    /// detail, never user-relevant.
    #[must_use]
    #[inline]
    pub fn from_backtrace_ref(bt: &oopsie_core::Backtrace) -> Self {
        Self::from_backtrace(bt.clone())
    }
    /// Create an `ErasedBacktrace` from a live `Backtrace`.
    ///
    /// Frames belonging to the `backtrace` crate's own capture machinery
    /// (`backtrace::backtrace::*` and `<backtrace::capture::*>::*`) are
    /// stripped — those are platform/toolchain-dependent implementation
    /// detail, never user-relevant.
    #[must_use]
    pub fn from_backtrace(mut bt: oopsie_core::Backtrace) -> Self {
        bt.resolve();
        let frames = bt
            .frames()
            .iter()
            .flat_map(|frame| {
                frame.symbols().iter().map(|sym| ErasedFrame {
                    name: sym.name().map(|n| n.to_string().into_boxed_str()),
                    filename: sym.filename().map(Box::from),
                    line: sym.lineno(),
                    column: sym.colno(),
                })
            })
            .filter(|f| !oopsie_core::is_internal_frame(f.name.as_deref(), f.filename.as_deref()))
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
            write!(f, "{i:>4}: ")?;
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
