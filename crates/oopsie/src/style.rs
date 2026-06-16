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
    pub const fn new() -> Self {
        Self(owo_colors::Style::new())
    }

    /// Set the foreground to a 24-bit truecolor.
    #[must_use]
    pub const fn truecolor(self, r: u8, g: u8, b: u8) -> Self {
        Self(self.0.truecolor(r, g, b))
    }

    /// Make the text bold.
    #[must_use]
    pub const fn bold(self) -> Self {
        Self(self.0.bold())
    }

    /// Make the text dimmed.
    #[must_use]
    pub const fn dimmed(self) -> Self {
        Self(self.0.dimmed())
    }

    /// Convert to the underlying owo-colors style for rendering.
    #[must_use]
    pub(crate) const fn into_owo(self) -> owo_colors::Style {
        self.0
    }
}

impl Default for Style {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
