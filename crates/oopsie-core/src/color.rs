//! Color configuration and control utilities.
//!
//! This module provides utilities for controlling colorized output in error
//! reporting. It follows the [`NO_COLOR`](https://no-color.org/) standard and
//! also respects `FORCE_COLOR` for explicit enablement.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// Global color mode setting.
static COLOR_MODE: AtomicU8 = AtomicU8::new(ColorConfig::Auto as u8);

/// Cached environment color detection result.
static ENV_SUPPORTS_COLOR: OnceLock<bool> = OnceLock::new();

/// Detect if colors should be used based on environment variables.
fn detect_env_color_support() -> bool {
    // Check NO_COLOR first (https://no-color.org/)
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }

    // Check FORCE_COLOR
    if std::env::var_os("FORCE_COLOR").is_some() {
        return true;
    }

    // Check if stderr is a terminal
    std::io::IsTerminal::is_terminal(&std::io::stderr())
}

/// Color output configuration.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum ColorConfig {
    /// Automatically detect based on environment and terminal.
    #[default]
    Auto = 0,
    /// Always use colors.
    Always = 1,
    /// Never use colors.
    Never = 2,
}

impl ColorConfig {
    /// Create a `ColorConfig` that auto-detects based on environment.
    #[must_use]
    pub const fn auto() -> Self {
        Self::Auto
    }

    /// Create a `ColorConfig` that always uses colors.
    #[must_use]
    pub const fn always() -> Self {
        Self::Always
    }

    /// Create a `ColorConfig` that never uses colors.
    #[must_use]
    pub const fn never() -> Self {
        Self::Never
    }

    /// Determine if colors should be used based on this configuration.
    #[must_use]
    pub fn should_colorize(self) -> bool {
        match self {
            Self::Auto => *ENV_SUPPORTS_COLOR.get_or_init(detect_env_color_support),
            Self::Always => true,
            Self::Never => false,
        }
    }
}

/// Set the global color mode for all `FancyReport` instances using `Auto`.
///
/// This affects the default behavior when no explicit color configuration is
/// provided to `FancyReport`.
pub fn set_color_mode(mode: ColorConfig) {
    COLOR_MODE.store(mode as u8, Ordering::SeqCst);
}

/// Get the current global color mode.
#[must_use]
pub fn get_color_mode() -> ColorConfig {
    match COLOR_MODE.load(Ordering::SeqCst) {
        0 => ColorConfig::Auto,
        1 => ColorConfig::Always,
        2 => ColorConfig::Never,
        _ => ColorConfig::Auto,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_color_mode() {
        // Save original
        let original = get_color_mode();

        set_color_mode(ColorConfig::Never);
        assert_eq!(get_color_mode(), ColorConfig::Never);

        set_color_mode(ColorConfig::Always);
        assert_eq!(get_color_mode(), ColorConfig::Always);

        // Restore
        set_color_mode(original);
    }
}
