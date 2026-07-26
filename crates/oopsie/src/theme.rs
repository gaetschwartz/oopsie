//! Color themes for the whole crash report — error chain, span trace,
//! backtrace, and panic output.
//!
//! A [`Theme`] is a small named palette; its `const fn` role accessors
//! ([`Theme::function_name`], [`Theme::line_number`], …) map each part of a
//! report onto a palette color. The shipped themes are curated presets, exposed
//! as associated constants ([`Theme::CATPPUCCIN_MOCHA`], [`Theme::DRACULA`],
//! [`Theme::NORD`], …); pick one. The active default is read from a
//! process-global slot via [`get_theme`] and replaced with [`set_theme`].

use std::sync::{PoisonError, RwLock};

use crate::style::{Style, parse_rgb as hex};
use crate::trace_printer::TraceTheme;

/// A color theme: nine named colors plus the role accessors that paint a
/// report from them. Small and `Copy`.
///
/// Choose one of the shipped presets (the associated constants) and install it
/// with [`set_theme`]; the palette colors are intentionally opaque.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    red: (u8, u8, u8),
    peach: (u8, u8, u8),
    yellow: (u8, u8, u8),
    teal: (u8, u8, u8),
    sky: (u8, u8, u8),
    blue: (u8, u8, u8),
    mauve: (u8, u8, u8),
    overlay1: (u8, u8, u8),
    overlay0: (u8, u8, u8),
}

const _: () = assert!(
    std::mem::size_of::<Theme>() <= 32,
    "Theme is copied on every get_theme(); keep it a small palette",
);

impl Theme {
    /// Catppuccin Mocha — the dark default. <https://catppuccin.com/palette>
    pub const CATPPUCCIN_MOCHA: Self = Self {
        red: hex("#f38ba8"),
        peach: hex("#fab387"),
        yellow: hex("#f9e2af"),
        teal: hex("#94e2d5"),
        sky: hex("#89dceb"),
        blue: hex("#89b4fa"),
        mauve: hex("#cba6f7"),
        overlay1: hex("#7f849c"),
        overlay0: hex("#6c7086"),
    };

    /// Catppuccin Macchiato — a softer, warmer dark flavor.
    pub const CATPPUCCIN_MACCHIATO: Self = Self {
        red: hex("#ed8796"),
        peach: hex("#f5a97f"),
        yellow: hex("#eed49f"),
        teal: hex("#8bd5ca"),
        sky: hex("#91d7e3"),
        blue: hex("#8aadf4"),
        mauve: hex("#c6a0f6"),
        overlay1: hex("#8087a2"),
        overlay0: hex("#6e738d"),
    };

    /// Catppuccin Frappé — the lightest of the dark flavors.
    pub const CATPPUCCIN_FRAPPE: Self = Self {
        red: hex("#e78284"),
        peach: hex("#ef9f76"),
        yellow: hex("#e5c890"),
        teal: hex("#81c8be"),
        sky: hex("#99d1db"),
        blue: hex("#8caaee"),
        mauve: hex("#ca9ee6"),
        overlay1: hex("#838ba7"),
        overlay0: hex("#737994"),
    };

    /// Catppuccin Latte — the light flavor, for light-background terminals.
    pub const CATPPUCCIN_LATTE: Self = Self {
        red: hex("#d20f39"),
        peach: hex("#fe640b"),
        yellow: hex("#df8e1d"),
        teal: hex("#179299"),
        sky: hex("#04a5e5"),
        blue: hex("#1e66f5"),
        mauve: hex("#8839ef"),
        overlay1: hex("#8c8fa1"),
        overlay0: hex("#9ca0b0"),
    };

    /// Dracula. <https://draculatheme.com>
    pub const DRACULA: Self = Self {
        red: hex("#ff5555"),
        peach: hex("#ffb86c"),
        yellow: hex("#f1fa8c"),
        teal: hex("#8be9fd"),
        sky: hex("#8be9fd"),
        blue: hex("#bd93f9"),
        mauve: hex("#bd93f9"),
        overlay1: hex("#6272a4"),
        overlay0: hex("#6272a4"),
    };

    /// Nord. <https://www.nordtheme.com>
    pub const NORD: Self = Self {
        red: hex("#bf616a"),
        peach: hex("#d08770"),
        yellow: hex("#ebcb8b"),
        teal: hex("#8fbcbb"),
        sky: hex("#88c0d0"),
        blue: hex("#81a1c1"),
        mauve: hex("#b48ead"),
        overlay1: hex("#4c566a"),
        overlay0: hex("#4c566a"),
    };

    /// The active default — Catppuccin Mocha.
    pub const DEFAULT: Self = Self::CATPPUCCIN_MOCHA;

    // ── Trace roles ─────────────────────────────────────────────────────────

    /// Backtrace/span-trace frame index.
    #[must_use]
    pub const fn frame_number(&self) -> Style {
        Style::from_rgb(self.overlay1)
    }
    /// Function / span name.
    #[must_use]
    pub const fn function_name(&self) -> Style {
        Style::from_rgb(self.red)
    }
    /// The `::h…` hash suffix on a function name.
    #[must_use]
    pub const fn function_hash(&self) -> Style {
        Style::from_rgb(self.overlay0)
    }
    /// Source file path.
    #[must_use]
    pub const fn file_path(&self) -> Style {
        Style::from_rgb(self.mauve)
    }
    /// `:line:col` location.
    #[must_use]
    pub const fn line_number(&self) -> Style {
        Style::from_rgb(self.peach)
    }
    /// Inline separators (`at`, `:`, `with`).
    #[must_use]
    pub const fn separator(&self) -> Style {
        Style::from_rgb(self.overlay0)
    }
    /// Span field list.
    #[must_use]
    pub const fn fields(&self) -> Style {
        Style::from_rgb(self.sky)
    }
    /// The `━ BACKTRACE ━` / `━ SPANTRACE ━` banner.
    #[must_use]
    pub const fn header(&self) -> Style {
        Style::from_rgb(self.mauve)
    }
    /// The `… N frames hidden …` notice.
    #[must_use]
    pub const fn frames_hidden(&self) -> Style {
        Style::from_rgb(self.teal)
    }

    // ── Error-chain / panic roles ───────────────────────────────────────────

    /// The `Error` headline, and the panic header line.
    #[must_use]
    pub const fn error_title(&self) -> Style {
        Style::from_rgb(self.red).bold()
    }
    /// The `[error_code]` token next to the headline.
    #[must_use]
    pub const fn error_code(&self) -> Style {
        Style::from_rgb(self.blue)
    }
    /// The brackets around the error code.
    #[must_use]
    pub const fn delimiter(&self) -> Style {
        Style::from_rgb(self.overlay0)
    }
    /// The `├─▶` / `╰─▶` arrows joining the error chain.
    #[must_use]
    pub const fn cause(&self) -> Style {
        Style::from_rgb(self.yellow)
    }
    /// The `help` label.
    #[must_use]
    pub const fn help(&self) -> Style {
        Style::from_rgb(self.teal)
    }
    /// The panic message body.
    #[must_use]
    pub const fn panic_message(&self) -> Style {
        Style::from_rgb(self.sky)
    }
    /// The panic location (`file:line:col`).
    #[must_use]
    pub const fn panic_location(&self) -> Style {
        Style::from_rgb(self.mauve)
    }
    /// Dimmed hints, e.g. the `RUST_BACKTRACE` note.
    #[must_use]
    pub const fn hint(&self) -> Style {
        Style::from_rgb(self.overlay0)
    }

    /// Bundle the trace roles into a [`TraceTheme`] for [`TracePrinter`].
    ///
    /// [`TracePrinter`]: crate::trace_printer::TracePrinter
    #[must_use]
    pub const fn trace(&self) -> TraceTheme {
        TraceTheme {
            frame_number: self.frame_number(),
            function_name: self.function_name(),
            function_hash: self.function_hash(),
            file_path: self.file_path(),
            line_number: self.line_number(),
            separator: self.separator(),
            fields: self.fields(),
            header: self.header(),
            frames_hidden: self.frames_hidden(),
        }
    }
}

static THEME: RwLock<Theme> = RwLock::new(Theme::DEFAULT);

/// Replace the process-global theme used by [`Report`](crate::Report) and the
/// panic hook. Affects every report that doesn't carry a per-instance override.
#[inline]
pub fn set_theme(theme: Theme) {
    *THEME.write().unwrap_or_else(PoisonError::into_inner) = theme;
}

/// The current process-global theme.
#[must_use]
#[inline]
pub fn get_theme() -> Theme {
    *THEME.read().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_theme_roundtrips() {
        let original = get_theme();
        set_theme(Theme::DRACULA);
        assert_eq!(get_theme(), Theme::DRACULA);
        set_theme(Theme::NORD);
        assert_eq!(get_theme(), Theme::NORD);
        set_theme(original);
    }
}
