//! Color configuration and control utilities.
//!
//! This module provides utilities for controlling colorized output in error
//! reporting. It follows the [`NO_COLOR`](https://no-color.org/) standard and
//! [`FORCE_COLOR`](https://force-color.org/) for explicit enablement (with
//! `FORCE_COLOR=0`/`false` as an explicit disable).

use std::sync::atomic::{AtomicU8, Ordering::SeqCst};
use std::{env, io};

/// Global color mode setting.
static COLOR_MODE: AtomicU8 = AtomicU8::new(ColorConfig::Auto as u8);

/// Detect if colors should be used based on environment variables.
fn detect_env_color_support() -> bool {
    // no-color.org: NO_COLOR disables only "when present and not an empty
    // string"; an empty value must be ignored, not treated as set.
    if let Some(no_color) = env::var_os("NO_COLOR")
        && !no_color.is_empty()
    {
        return false;
    }

    // FORCE_COLOR follows the same present-and-non-empty rule, except that
    // `0`/`false` is the conventional explicit *disable* (force-color.org /
    // Node semantics), not a force-enable.
    if let Some(force) = env::var_os("FORCE_COLOR")
        && !force.is_empty()
    {
        return force != "0" && !force.eq_ignore_ascii_case("false");
    }

    io::IsTerminal::is_terminal(&io::stderr())
}

fn cached_env_supports_color() -> bool {
    static ENV_SUPPORTS_COLOR: AtomicU8 = AtomicU8::new(u8::MAX);

    match ENV_SUPPORTS_COLOR.load(SeqCst) {
        u8::MAX => {
            let detected = detect_env_color_support();
            ENV_SUPPORTS_COLOR.store(u8::from(detected), SeqCst);
            detected
        }
        v => v == u8::from(true),
    }
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
    #[inline]
    const fn from_u8(value: u8) -> Option<Self> {
        const fn is(value: u8, variant: ColorConfig) -> bool {
            value == variant as u8
        }
        match value {
            x if is(x, Self::Auto) => Some(Self::Auto),
            x if is(x, Self::Always) => Some(Self::Always),
            x if is(x, Self::Never) => Some(Self::Never),
            _ => None,
        }
    }

    /// Determine if colors should be used based on this configuration.
    ///
    /// `Auto` resolves through the global mode set by [`set_color_mode`] first,
    /// then environment/terminal detection when the global is also `Auto`.
    #[must_use]
    #[inline]
    pub fn should_colorize(self) -> bool {
        match self {
            Self::Auto => match get_color_mode() {
                Self::Always => true,
                Self::Never => false,
                Self::Auto => cached_env_supports_color(),
            },
            Self::Always => true,
            Self::Never => false,
        }
    }
}

const _: () = {
    const fn assert_roundtrip(value: ColorConfig) {
        assert!(ColorConfig::from_u8(value as u8).unwrap() as u8 == value as u8);
    }
    assert_roundtrip(ColorConfig::Auto);
    assert_roundtrip(ColorConfig::Always);
    assert_roundtrip(ColorConfig::Never);
};

/// Set the global color mode for all `Report` instances using `Auto`.
///
/// This affects the default behavior when no explicit color configuration is
/// provided to `Report`.
#[inline]
pub fn set_color_mode(mode: ColorConfig) {
    COLOR_MODE.store(mode as u8, SeqCst);
}

/// Get the current global color mode.
#[must_use]
#[inline]
pub fn get_color_mode() -> ColorConfig {
    match ColorConfig::from_u8(COLOR_MODE.load(SeqCst)) {
        Some(mode) => mode,
        None => ColorConfig::Auto,
    }
}

macro_rules! style {
    ($expr:expr, $style:expr, $colorize:expr) => {{
        use $crate::color::S;
        _ = S;

        ::owo_colors::OwoColorize::style(&$expr, if $colorize { $style } else { S })
    }};
}

#[doc(hidden)]
pub const S: owo_colors::Style = owo_colors::Style::new();
pub(crate) use style;

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

    #[test]
    fn test_always_should_colorize() {
        assert!(ColorConfig::Always.should_colorize());
    }

    #[test]
    fn test_never_should_not_colorize() {
        assert!(!ColorConfig::Never.should_colorize());
    }

    #[test]
    fn test_auto_roundtrip_through_global() {
        let original = get_color_mode();

        set_color_mode(ColorConfig::Auto);
        assert_eq!(get_color_mode(), ColorConfig::Auto);

        set_color_mode(original);
    }

    #[test]
    fn global_never_disables_auto_colorize() {
        let original = get_color_mode();
        set_color_mode(ColorConfig::Never);
        assert!(!ColorConfig::Auto.should_colorize());
        set_color_mode(ColorConfig::Always);
        assert!(ColorConfig::Auto.should_colorize());
        set_color_mode(original);
    }
}
