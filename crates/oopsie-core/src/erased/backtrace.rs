//! Serializable backtrace representation.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A serializable, type-erased representation of a backtrace.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ErasedBacktrace {
    frames: Box<[ErasedFrame]>,
}

/// A single frame in an erased backtrace.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ErasedFrame {
    pub name: Option<Box<str>>,
    pub filename: Option<Box<str>>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl ErasedBacktrace {
    /// Create an `ErasedBacktrace` from a live `Backtrace`.
    ///
    /// Captures every resolved symbol verbatim — including capture machinery,
    /// OS/libc entry points, and unresolvable frames. Hiding
    /// implementation/platform detail is a render-time concern; this snapshot
    /// stays raw so a consumer can still render the full stack later.
    #[must_use]
    pub fn from_backtrace(bt: &crate::Backtrace) -> Self {
        bt.resolve();

        let mut frames = Vec::new();
        for frame in bt.frames() {
            let symbols = frame.symbols();
            // A frame that failed to resolve has no symbols; keep a placeholder
            // so the stack shape survives erasure.
            if symbols.is_empty() {
                frames.push(ErasedFrame {
                    name: None,
                    filename: None,
                    line: None,
                    column: None,
                });
            }
            frames.extend(symbols.iter().map(|sym| ErasedFrame {
                name: sym.name().map(|n| n.to_string().into_boxed_str()),
                filename: sym.filename().map(|p| p.to_string_lossy().into()),
                line: sym.lineno(),
                column: sym.colno(),
            }));
        }
        Self {
            frames: frames.into(),
        }
    }

    /// Returns a slice of all frames.
    #[must_use]
    #[inline]
    pub fn frames(&self) -> &[ErasedFrame] {
        &self.frames
    }
}

impl From<&crate::Backtrace> for ErasedBacktrace {
    #[inline]
    fn from(bt: &crate::Backtrace) -> Self {
        Self::from_backtrace(bt)
    }
}
impl From<crate::Backtrace> for ErasedBacktrace {
    #[inline]
    fn from(bt: crate::Backtrace) -> Self {
        Self::from_backtrace(&bt)
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
                write!(f, "           at {filename}")?;
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
            filename: filename.map(Into::into),
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
    fn from_backtrace_retains_raw_internal_frames() {
        crate::set_rust_backtrace_override(crate::RustBacktrace::Enabled);
        let bt = <crate::Backtrace as crate::Capturable>::capture();
        crate::clear_rust_backtrace_override();

        let erased = ErasedBacktrace::from_backtrace(&bt);

        // The capture path itself runs through this crate, so a raw snapshot
        // must retain its frame — proving filtering is deferred to render
        // time rather than applied here.
        assert!(
            erased.frames().iter().any(|fr| {
                fr.filename
                    .as_deref()
                    .is_some_and(|f| f.starts_with(crate::__private::CORE_SRC_PATH))
            }),
            "from_backtrace should retain raw internal frames, not strip them at capture"
        );
    }

    #[test]
    fn from_backtrace_keeps_at_least_one_erased_frame_per_source_frame() {
        crate::set_rust_backtrace_override(crate::RustBacktrace::Enabled);
        let bt = <crate::Backtrace as crate::Capturable>::capture();
        crate::clear_rust_backtrace_override();

        let erased = ErasedBacktrace::from_backtrace(&bt);
        assert!(
            erased.frames().len() >= bt.frames().len(),
            "every source frame must survive erasure (unresolved ones as placeholders): \
             {} erased < {} source",
            erased.frames().len(),
            bt.frames().len()
        );
    }
}
