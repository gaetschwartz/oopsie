/// An opaque text style (foreground color plus modifiers) used by report themes.
///
/// Construct with [`Style::new`] and chain the builder methods to add color or
/// modifiers. All methods are `const fn`, so styles can appear in `const`
/// contexts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style(owo_colors::Style);

impl Style {
    /// An empty style that emits no ANSI codes.
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self(owo_colors::Style::new())
    }

    /// Construct a style from a 24-bit RGB color tuple.
    #[must_use]
    #[inline]
    pub const fn from_rgb((r, g, b): (u8, u8, u8)) -> Self {
        Self(owo_colors::Style::new().truecolor(r, g, b))
    }

    /// Set the foreground to a 24-bit truecolor.
    #[must_use]
    #[inline]
    pub const fn truecolor(self, r: u8, g: u8, b: u8) -> Self {
        Self(self.0.truecolor(r, g, b))
    }

    /// Set the foreground to a 24-bit truecolor from a hex literal, e.g. `#ff00aa`.
    #[must_use]
    #[inline]
    pub const fn hex(self, literal: &str) -> Self {
        let (r, g, b) = parse_rgb(literal);
        self.truecolor(r, g, b)
    }

    /// Make the text bold.
    #[must_use]
    #[inline]
    pub const fn bold(self) -> Self {
        Self(self.0.bold())
    }

    /// Make the text dimmed.
    #[must_use]
    #[inline]
    pub const fn dimmed(self) -> Self {
        Self(self.0.dimmed())
    }

    /// Convert to the underlying owo-colors style for rendering.
    #[must_use]
    #[inline]
    pub(crate) const fn into_owo(self) -> owo_colors::Style {
        self.0
    }
}

pub trait Colorize: Sized {
    /// Apply a style to this text, returning a styled wrapper that implements
    /// [`Display`].
    #[must_use]
    fn style(&self, style: Style) -> owo_colors::Styled<&Self>;
}

impl<T: owo_colors::OwoColorize> Colorize for T {
    #[inline]
    fn style(&self, style: Style) -> owo_colors::Styled<&Self> {
        owo_colors::OwoColorize::style(self, style.into_owo())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    MissingHash,
    InvalidLength,
    InvalidHex,
}

/// Fallible core of [`parse_rgb`]: parse a `#rrggbb` literal into its RGB bytes.
pub const fn try_parse_rgb(literal: &str) -> Result<(u8, u8, u8), ParseError> {
    if literal.is_empty() || literal.as_bytes()[0] != b'#' {
        return Err(ParseError::MissingHash);
    }
    if literal.len() != 7 {
        return Err(ParseError::InvalidLength);
    }
    let (_, digits) = literal.split_at(1);
    let Ok(value) = u32::from_str_radix(digits, 16) else {
        return Err(ParseError::InvalidHex);
    };
    Ok((
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    ))
}

/// Parse a `#rrggbb` literal at compile time. Written as a `&str` so editors
/// render the color swatch inline next to each preset.
pub const fn parse_rgb(literal: &str) -> (u8, u8, u8) {
    match try_parse_rgb(literal) {
        Ok(rgb) => rgb,
        Err(ParseError::MissingHash) => panic!("missing hash in hex literal"),
        Err(ParseError::InvalidLength) => panic!("invalid hex literal length"),
        Err(ParseError::InvalidHex) => panic!("invalid hex literal"),
    }
}

impl Default for Style {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // const tests for the hex parser
    const _: () = {
        // Sanity check that the hex parser works at compile time.
        const EXPECTED: (u8, u8, u8) = (0x12, 0x3a, 0xbc);
        const PARSED: (u8, u8, u8) = parse_rgb("#123abc");
        match PARSED {
            EXPECTED => (),
            _ => panic!("hex parser failed at compile time"),
        }
    };

    #[rstest::rstest]
    #[case("#000000", Ok((0, 0, 0)))]
    #[case("#ffffff", Ok((255, 255, 255)))]
    #[case("#123abc", Ok((18, 58, 188)))]
    #[case("#12", Err(ParseError::InvalidLength))]
    #[case("#12345", Err(ParseError::InvalidLength))]
    #[case("#1234567", Err(ParseError::InvalidLength))]
    #[case("123456", Err(ParseError::MissingHash))]
    #[case("", Err(ParseError::MissingHash))]
    #[case("#12g456", Err(ParseError::InvalidHex))]
    fn parse_rgb_valid(#[case] literal: &str, #[case] expected: Result<(u8, u8, u8), ParseError>) {
        assert_eq!(try_parse_rgb(literal), expected);
    }
}
