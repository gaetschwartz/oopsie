//! Core error types and utilities.

#![cfg_attr(
    feature = "unstable",
    feature(error_generic_member_access, try_trait_v2)
)]

mod backtrace;
mod color;
#[cfg(feature = "unstable")]
mod result;
pub mod spantrace;
mod tracing_level;

use std::borrow::Cow;
use std::io;
use std::ops::Deref;

pub use backtrace::Backtrace;
pub use color::{ColorConfig, get_color_mode, set_color_mode};
use color_backtrace::termcolor;
#[cfg(feature = "unstable")]
pub use result::MayBoxResult;
pub use snafu::{OptionExt, ResultExt};
pub use spantrace::{ErasedMetadata, OptionalSpanTrace, Spantrace};
pub use tracing_error::ErrorLayer;
pub use tracing_level::TracingLevel;
use tracing_subscriber::fmt::format::JsonFields;
use tracing_subscriber::registry::LookupSpan;

/// Install the color backtrace printer.
pub fn install() {
    color_backtrace::BacktracePrinter::new().install(color_backtrace::default_output_stream());
}

/// Install the color-eyre panic and error hooks globally.
///
/// This should be called once, early in main, before spawning any threads.
/// It provides colored backtraces and span traces on panics.
#[inline]
pub fn install_panic_hook() -> color_eyre::Result<()> {
    color_eyre::install()
}

#[derive(
    Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ErrorCode(Cow<'static, str>);

impl Deref for ErrorCode {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&'static str> for ErrorCode {
    fn from(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }
}

impl From<String> for ErrorCode {
    fn from(s: String) -> Self {
        Self(Cow::Owned(s))
    }
}

/// Help text associated with an error, provided via the Provider API.
#[derive(Clone, Debug)]
pub struct HelpText(pub &'static str);

impl Deref for HelpText {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

pub trait NewJsonErrorLayer<S> {
    fn json() -> ErrorLayer<S, JsonFields>;
}

impl<S> NewJsonErrorLayer<S> for ErrorLayer<S, JsonFields>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    fn json() -> Self {
        ErrorLayer::new(JsonFields::default())
    }
}

#[doc(hidden)]
pub mod private {
    pub use snafu;
}

#[expect(dead_code)]
pub(crate) trait WriteColorExt {
    fn black(&mut self) -> io::Result<&mut Self>;
    fn blue(&mut self) -> io::Result<&mut Self>;
    fn green(&mut self) -> io::Result<&mut Self>;
    fn red(&mut self) -> io::Result<&mut Self>;
    fn cyan(&mut self) -> io::Result<&mut Self>;
    fn magenta(&mut self) -> io::Result<&mut Self>;
    fn yellow(&mut self) -> io::Result<&mut Self>;
    fn white(&mut self) -> io::Result<&mut Self>;
    fn rgb(&mut self, r: u8, g: u8, b: u8) -> io::Result<&mut Self>;
    fn ansi256(&mut self, code: u8) -> io::Result<&mut Self>;
    fn bold(&mut self) -> io::Result<&mut Self>;
    fn italic(&mut self) -> io::Result<&mut Self>;
    fn underline(&mut self) -> io::Result<&mut Self>;
    fn strike(&mut self) -> io::Result<&mut Self>;
}

impl<W: termcolor::WriteColor> WriteColorExt for W {
    fn black(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Black)))?;
        Ok(self)
    }
    fn blue(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Blue)))?;
        Ok(self)
    }
    fn green(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Green)))?;
        Ok(self)
    }
    fn red(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Red)))?;
        Ok(self)
    }
    fn cyan(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Cyan)))?;
        Ok(self)
    }
    fn magenta(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Magenta)))?;
        Ok(self)
    }
    fn yellow(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Yellow)))?;
        Ok(self)
    }
    fn white(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::White)))?;
        Ok(self)
    }
    fn rgb(&mut self, r: u8, g: u8, b: u8) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Rgb(r, g, b))))?;
        Ok(self)
    }
    fn ansi256(&mut self, code: u8) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Ansi256(code))))?;
        Ok(self)
    }
    fn bold(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_bold(true))?;
        Ok(self)
    }
    fn italic(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_italic(true))?;
        Ok(self)
    }
    fn underline(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_underline(true))?;
        Ok(self)
    }
    fn strike(&mut self) -> io::Result<&mut Self> {
        self.set_color(termcolor::ColorSpec::new().set_strikethrough(true))?;
        Ok(self)
    }
}
