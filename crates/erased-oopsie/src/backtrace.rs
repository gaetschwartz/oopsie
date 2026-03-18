//! Serializable backtrace representation.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A serializable, type-erased representation of a backtrace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErasedBackTrace {
    frames: Vec<ErasedFrame>,
}

/// A single frame in an erased backtrace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErasedFrame {
    pub name: Option<String>,
    pub filename: Option<PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl ErasedBackTrace {
    /// Create an `ErasedBackTrace` from a live `BackTrace`.
    pub fn from_backtrace(bt: &oopsie_core::BackTrace) -> Self {
        let frames = bt
            .inner()
            .frames()
            .iter()
            .flat_map(|frame| {
                frame.symbols().iter().map(|sym| ErasedFrame {
                    name: sym.name().map(|n| n.to_string()),
                    filename: sym.filename().map(std::borrow::ToOwned::to_owned),
                    line: sym.lineno(),
                    column: sym.colno(),
                })
            })
            .collect();
        Self { frames }
    }

    /// Returns a slice of all frames.
    #[must_use]
    pub fn frames(&self) -> &[ErasedFrame] {
        &self.frames
    }
}

impl From<&oopsie_core::BackTrace> for ErasedBackTrace {
    fn from(bt: &oopsie_core::BackTrace) -> Self {
        Self::from_backtrace(bt)
    }
}

impl fmt::Display for ErasedBackTrace {
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
