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

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        name: Option<&str>,
        filename: Option<&str>,
        line: Option<u32>,
        column: Option<u32>,
    ) -> ErasedFrame {
        ErasedFrame {
            name: name.map(|s| s.to_owned().into_boxed_str()),
            filename: filename.map(|s| std::path::Path::new(s).into()),
            line,
            column,
        }
    }

    fn single(f: ErasedFrame) -> ErasedBacktrace {
        ErasedBacktrace {
            frames: Box::from([f]),
        }
    }

    #[test]
    fn display_unknown_frame_without_location() {
        // name == None -> `<unknown>`; filename == None -> no `at` line.
        let bt = single(frame(None, None, None, None));
        assert_eq!(bt.to_string(), "  1: <unknown>\n");
    }

    #[test]
    fn display_full_location_with_column() {
        let bt = single(frame(Some("a::b"), Some("/x/y.rs"), Some(12), Some(3)));
        assert_eq!(bt.to_string(), "  1: a::b\n           at /x/y.rs:12:3\n");
    }

    #[test]
    fn display_location_with_line_no_column() {
        let bt = single(frame(Some("a::b"), Some("/x/y.rs"), Some(12), None));
        assert_eq!(bt.to_string(), "  1: a::b\n           at /x/y.rs:12\n");
    }

    #[test]
    fn display_location_with_filename_no_line() {
        // filename present but no line -> bare path, no trailing `:`.
        let bt = single(frame(Some("a::b"), Some("/x/y.rs"), None, None));
        assert_eq!(bt.to_string(), "  1: a::b\n           at /x/y.rs\n");
    }

    #[test]
    fn from_backtrace_filters_internal_frames_unless_full() {
        oopsie_core::set_rust_backtrace_override(oopsie_core::RustBacktrace::Enabled);
        let bt = <oopsie_core::Backtrace as oopsie_core::Capturable>::capture();

        // Enabled: `is_internal_frame` machinery is stripped.
        let filtered = ErasedBacktrace::from_backtrace(bt.clone());
        // Full: the `if full { return true }` short-circuit keeps every symbol.
        oopsie_core::set_rust_backtrace_override(oopsie_core::RustBacktrace::Full);
        let full = ErasedBacktrace::from_backtrace(bt.clone());
        oopsie_core::clear_rust_backtrace_override();

        assert!(
            full.frames().len() >= filtered.frames().len(),
            "full ({}) must keep at least as many frames as filtered ({})",
            full.frames().len(),
            filtered.frames().len()
        );
        // Postcondition of the filtered path: nothing internal survives.
        for fr in filtered.frames() {
            assert!(
                !oopsie_core::is_internal_frame(fr.name.as_deref(), fr.filename.as_deref()),
                "filtered backtrace leaked an internal frame: {:?}",
                fr.name
            );
        }
    }
}
