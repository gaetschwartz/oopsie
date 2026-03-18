//! Unified colored trace rendering for backtraces and span traces.
//!
//! This module provides [`TracePrinter`], which renders backtraces and span
//! traces with configurable colors via [`TraceTheme`]. The rendering follows
//! the style of `color-backtrace` and `color-spantrace` but uses `owo_colors`
//! directly, eliminating the need for those dependencies.

use std::fmt;
use std::path::PathBuf;

use owo_colors::{OwoColorize as _, Style};

// ─────────────────────────────────────────────────────────────────────────────
// Data types
// ─────────────────────────────────────────────────────────────────────────────

/// A single frame from a backtrace.
pub struct BacktraceFrame {
    pub n: usize,
    pub name: Option<String>,
    pub filename: Option<PathBuf>,
    pub lineno: Option<u32>,
    pub colno: Option<u32>,
}

/// Metadata for a single span in a span trace.
pub struct SpanMetadata {
    pub name: String,
    pub target: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider traits
// ─────────────────────────────────────────────────────────────────────────────

/// Trait for types that can provide backtrace frames.
pub trait BacktraceProvider {
    fn frames(&self) -> Vec<BacktraceFrame>;
}

/// Trait for types that can provide span trace information.
pub trait SpanTraceProvider {
    fn with_spans(&self, f: &mut dyn FnMut(&SpanMetadata, &str) -> bool);
}

// ─────────────────────────────────────────────────────────────────────────────
// Implementations for core types
// ─────────────────────────────────────────────────────────────────────────────

impl BacktraceProvider for crate::BackTrace {
    fn frames(&self) -> Vec<BacktraceFrame> {
        self.inner()
            .frames()
            .iter()
            .enumerate()
            .flat_map(|(n, frame)| {
                frame.symbols().iter().map(move |sym| BacktraceFrame {
                    n,
                    name: sym.name().map(|n| n.to_string()),
                    filename: sym.filename().map(std::borrow::ToOwned::to_owned),
                    lineno: sym.lineno(),
                    colno: sym.colno(),
                })
            })
            .collect()
    }
}

impl SpanTraceProvider for crate::SpanTrace {
    fn with_spans(&self, f: &mut dyn FnMut(&SpanMetadata, &str) -> bool) {
        crate::SpanTrace::with_spans(self, |md, fields| {
            let meta = SpanMetadata {
                name: md.name().to_owned(),
                target: md.target().to_owned(),
                file: md.file().map(std::borrow::ToOwned::to_owned),
                line: md.line(),
            };
            f(&meta, fields)
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Theme
// ─────────────────────────────────────────────────────────────────────────────

/// Color theme for trace rendering.
pub struct TraceTheme {
    pub frame_number: Style,
    pub function_name: Style,
    pub function_hash: Style,
    pub file_path: Style,
    pub line_number: Style,
    pub separator: Style,
    pub fields: Style,
    pub header: Style,
    pub frames_hidden: Style,
}

impl Default for TraceTheme {
    fn default() -> Self {
        Self {
            frame_number: Style::new().dimmed(),
            function_name: Style::new().bright_red(),
            function_hash: Style::new().bright_black(),
            file_path: Style::new().purple(),
            line_number: Style::new().purple(),
            separator: Style::new().dimmed(),
            fields: Style::new().bright_cyan(),
            header: Style::new().red(),
            frames_hidden: Style::new().cyan(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame filtering
// ─────────────────────────────────────────────────────────────────────────────

/// Prefixes for backtrace capture frames that should be skipped.
const BACKTRACE_CAPTURE_PREFIXES: &[&str] = &[
    "std::backtrace_rs::backtrace::",
    "<std::backtrace::Backtrace>::create",
    "<std::backtrace::Backtrace as oopsie_core::Capturable>::",
    "<alloc::boxed::Box<oopsie_core::backtrace::Backtrace> as oopsie_core::Capturable>::",
];

/// Prefixes for runtime initialization frames that should be skipped.
const RUNTIME_INIT_PREFIXES: &[&str] = &[
    "std::rt::lang_start::",
    "std::rt::lang_start_internal::",
    "std::panicking::catch_unwind::",
    "std::panic::catch_unwind::",
    "__rustc",
    "_main",
    "main",
    "__libc_start",
    "__scrt_common_main",
];

/// Check if a frame name matches backtrace capture code.
fn is_backtrace_capture_code(name: &str) -> bool {
    BACKTRACE_CAPTURE_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Check if a frame name matches runtime initialization code.
fn is_runtime_init_code(name: &str) -> bool {
    RUNTIME_INIT_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Default frame filter for error backtraces.
///
/// This filter:
/// 1. Skips frames from the top that are backtrace capture machinery
/// 2. Removes runtime initialization frames from the bottom
pub fn error_backtrace_frame_filter(frames: &mut Vec<&BacktraceFrame>) {
    // Find the index of the last backtrace capture frame
    let top_cutoff_idx = frames
        .iter()
        .rposition(|frame| {
            frame
                .name
                .as_ref()
                .is_some_and(|name| is_backtrace_capture_code(name))
        })
        .map_or(0, |idx| idx + 1);

    // Find the index of runtime init code at the bottom
    let bottom_cutoff_idx = frames
        .iter()
        .position(|frame| {
            frame
                .name
                .as_ref()
                .is_some_and(|name| is_runtime_init_code(name))
        })
        .unwrap_or(frames.len());

    // Keep only frames within the valid range
    let frames_to_keep: Vec<usize> = frames[top_cutoff_idx..bottom_cutoff_idx]
        .iter()
        .map(|f| f.n)
        .collect();

    frames.retain(|frame| frames_to_keep.contains(&frame.n));
}

// ─────────────────────────────────────────────────────────────────────────────
// Hash stripping
// ─────────────────────────────────────────────────────────────────────────────

/// Split a function name into (base, hash_suffix).
/// The hash suffix is `::h` followed by exactly 16 hex characters at the end.
fn split_function_hash(name: &str) -> (&str, Option<&str>) {
    // Look for ::h followed by exactly 16 hex chars at end
    if name.len() >= 20 {
        let suffix_start = name.len() - 19; // "::h" (3) + 16 hex chars
        let candidate = &name[suffix_start..];
        if candidate.starts_with("::h")
            && candidate[3..].len() == 16
            && candidate[3..].chars().all(|c| c.is_ascii_hexdigit())
        {
            return (&name[..suffix_start], Some(candidate));
        }
    }
    (name, None)
}

// ─────────────────────────────────────────────────────────────────────────────
// TracePrinter
// ─────────────────────────────────────────────────────────────────────────────

/// A closure that filters backtrace frames in-place.
pub type FrameFilter = Box<dyn Fn(&mut Vec<&BacktraceFrame>)>;

/// Renders backtraces and span traces with colors.
pub struct TracePrinter {
    theme: TraceTheme,
    frame_filters: Vec<FrameFilter>,
}

impl TracePrinter {
    /// Create a new `TracePrinter` with the default theme and default frame filter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            theme: TraceTheme::default(),
            frame_filters: vec![Box::new(error_backtrace_frame_filter)],
        }
    }

    /// Create a new `TracePrinter` with a custom theme and default frame filter.
    #[must_use]
    pub fn with_theme(theme: TraceTheme) -> Self {
        Self {
            theme,
            frame_filters: vec![Box::new(error_backtrace_frame_filter)],
        }
    }

    /// Add a frame filter for backtrace rendering.
    #[must_use]
    pub fn add_frame_filter(mut self, filter: FrameFilter) -> Self {
        self.frame_filters.push(filter);
        self
    }

    /// Render a colored backtrace.
    pub fn write_backtrace(
        &self,
        f: &mut fmt::Formatter<'_>,
        bt: &impl BacktraceProvider,
    ) -> fmt::Result {
        let all_frames = bt.frames();
        let total_count = all_frames.len();

        // Apply frame filters
        let mut filtered: Vec<&BacktraceFrame> = all_frames.iter().collect();
        for filter in &self.frame_filters {
            filter(&mut filtered);
        }

        let hidden_count = total_count - filtered.len();

        // Header
        writeln!(
            f,
            "{}",
            format_args!("{:━^80}", " BACKTRACE ").style(self.theme.header)
        )?;

        // Hidden frames notice (top)
        if hidden_count > 0 {
            writeln!(
                f,
                "{}",
                format_args!("   ... {hidden_count} frames hidden ...")
                    .style(self.theme.frames_hidden)
            )?;
        }

        // Render each frame
        for (i, frame) in filtered.iter().enumerate() {
            self.write_backtrace_frame(f, i, frame)?;
        }

        Ok(())
    }

    /// Render a single backtrace frame.
    fn write_backtrace_frame(
        &self,
        f: &mut fmt::Formatter<'_>,
        index: usize,
        frame: &BacktraceFrame,
    ) -> fmt::Result {
        // Frame number: right-aligned in 3 chars
        write!(
            f,
            "{}",
            format_args!("{index:>3}").style(self.theme.frame_number)
        )?;
        write!(f, "{}", ": ".style(self.theme.separator))?;

        // Function name
        if let Some(name) = &frame.name {
            let (base, hash) = split_function_hash(name);
            write!(f, "{}", base.style(self.theme.function_name))?;
            if let Some(h) = hash {
                write!(f, "{}", h.style(self.theme.function_hash))?;
            }
        } else {
            write!(f, "{}", "<unknown>".style(self.theme.function_name))?;
        }
        writeln!(f)?;

        // File location
        if let Some(filename) = &frame.filename {
            write!(f, "           ")?; // 11 spaces
            write!(f, "{}", "at ".style(self.theme.separator))?;
            write!(f, "{}", filename.display().style(self.theme.file_path))?;
            if let Some(lineno) = frame.lineno {
                write!(
                    f,
                    "{}",
                    format_args!(":{lineno}").style(self.theme.line_number)
                )?;
                if let Some(colno) = frame.colno {
                    write!(
                        f,
                        "{}",
                        format_args!(":{colno}").style(self.theme.line_number)
                    )?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }

    /// Render a colored span trace.
    pub fn write_spantrace(
        &self,
        f: &mut fmt::Formatter<'_>,
        st: &impl SpanTraceProvider,
    ) -> fmt::Result {
        // Header
        writeln!(
            f,
            "{}",
            format_args!("{:━^80}", " SPANTRACE ").style(self.theme.header)
        )?;

        let mut index = 0usize;
        let mut err = Ok(());

        st.with_spans(&mut |meta, fields| {
            if let Err(e) = self.write_span_frame(f, index, meta, fields) {
                err = Err(e);
                return false;
            }
            index += 1;
            true
        });

        err
    }

    /// Render a single span trace frame.
    fn write_span_frame(
        &self,
        f: &mut fmt::Formatter<'_>,
        index: usize,
        meta: &SpanMetadata,
        fields: &str,
    ) -> fmt::Result {
        // Frame number
        write!(
            f,
            "{}",
            format_args!("{index:>3}").style(self.theme.frame_number)
        )?;
        write!(f, "{}", ": ".style(self.theme.separator))?;

        // target::name
        write!(
            f,
            "{}",
            format_args!("{}::{}", meta.target, meta.name).style(self.theme.function_name)
        )?;
        writeln!(f)?;

        // Fields line (only if non-empty)
        if !fields.is_empty() {
            write!(f, "           ")?; // 11 spaces
            write!(f, "{}", "with ".style(self.theme.separator))?;
            writeln!(f, "{}", fields.style(self.theme.fields))?;
        }

        // File location
        if let Some(file) = &meta.file {
            write!(f, "           ")?; // 11 spaces
            write!(f, "{}", "at ".style(self.theme.separator))?;
            write!(f, "{}", file.style(self.theme.file_path))?;
            if let Some(line) = meta.line {
                write!(
                    f,
                    "{}",
                    format_args!(":{line}").style(self.theme.line_number)
                )?;
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

impl Default for TracePrinter {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(n: usize, name: Option<String>, lineno: Option<u32>) -> BacktraceFrame {
        BacktraceFrame {
            n,
            name,
            filename: None,
            lineno,
            colno: None,
        }
    }

    #[test]
    fn test_is_backtrace_capture_code() {
        assert!(is_backtrace_capture_code(
            "std::backtrace_rs::backtrace::libunwind::trace"
        ));
        assert!(!is_backtrace_capture_code("my_crate::do_stuff"));
    }

    #[test]
    fn test_is_runtime_init_code() {
        assert!(is_runtime_init_code(
            "std::rt::lang_start_internal::something"
        ));
        assert!(!is_runtime_init_code("my_crate::main_logic"));
    }

    #[test]
    fn test_error_backtrace_frame_filter() {
        let capture = make_frame(
            0,
            Some("std::backtrace_rs::backtrace::libunwind::trace".into()),
            None,
        );
        let app1 = make_frame(1, Some("my_crate::function_a".into()), None);
        let app2 = make_frame(2, Some("my_crate::function_b".into()), None);
        let runtime = make_frame(3, Some("std::rt::lang_start_internal::invoke".into()), None);

        let mut frames: Vec<&BacktraceFrame> = vec![&capture, &app1, &app2, &runtime];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].n, 1);
        assert_eq!(frames[1].n, 2);
    }

    #[test]
    fn test_error_backtrace_frame_filter_no_capture_no_runtime() {
        let app1 = make_frame(0, Some("my_crate::function_a".into()), None);
        let app2 = make_frame(1, Some("my_crate::function_b".into()), None);

        let mut frames: Vec<&BacktraceFrame> = vec![&app1, &app2];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].n, 0);
        assert_eq!(frames[1].n, 1);
    }

    #[test]
    fn test_error_backtrace_frame_filter_multiple_capture_frames() {
        let capture1 = make_frame(
            0,
            Some("<std::backtrace::Backtrace>::create::something".into()),
            None,
        );
        let capture2 = make_frame(
            1,
            Some("std::backtrace_rs::backtrace::libunwind::trace".into()),
            None,
        );
        let app = make_frame(2, Some("my_crate::function_a".into()), None);

        let mut frames: Vec<&BacktraceFrame> = vec![&capture1, &capture2, &app];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].n, 2);
    }

    #[test]
    fn test_split_function_hash() {
        let (base, hash) = split_function_hash("my_crate::foo::hab1234567890abcd");
        assert_eq!(base, "my_crate::foo");
        assert_eq!(hash, Some("::hab1234567890abcd"));

        let (base, hash) = split_function_hash("my_crate::foo");
        assert_eq!(base, "my_crate::foo");
        assert_eq!(hash, None);

        // Not exactly 16 hex chars
        let (base, hash) = split_function_hash("my_crate::foo::habcd");
        assert_eq!(base, "my_crate::foo::habcd");
        assert_eq!(hash, None);
    }
}
