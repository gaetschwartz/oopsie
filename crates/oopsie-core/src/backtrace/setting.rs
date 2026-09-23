use std::env;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering::Relaxed;

/// Whether backtrace capture is enabled, and how verbosely it should render.
///
/// Resolved once from the environment (see [`current`](crate::backtrace::current))
/// following the same convention as `std`: `RUST_LIB_BACKTRACE` is consulted
/// first and, if unset, `RUST_BACKTRACE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RustBacktrace {
    /// Capture disabled — no frames are recorded.
    Disabled = 1,
    /// Capture enabled, rendered as the trimmed "short" view.
    Enabled = 2,
    /// Capture enabled, rendered untrimmed — every frame is shown.
    Full = 3,
}

impl RustBacktrace {
    /// The environment-derived setting, or `None` when neither
    /// `RUST_LIB_BACKTRACE` nor `RUST_BACKTRACE` is set. The environment is
    /// read once and cached for the life of the process.
    pub fn detect_opt() -> Option<Self> {
        const NONE: u8 = u8::MAX;
        const NOT_SET: u8 = 0;

        static ENABLED: AtomicU8 = AtomicU8::new(NOT_SET);
        if let Some(cached) = match ENABLED.load(Relaxed) {
            1 => Some(Some(Self::Disabled)),
            2 => Some(Some(Self::Enabled)),
            3 => Some(Some(Self::Full)),
            NONE => Some(None),
            NOT_SET => None,
            _ => unreachable!(),
        } {
            return cached;
        }

        // `RUST_LIB_BACKTRACE` wins when set; otherwise fall back to
        // `RUST_BACKTRACE`. `or_else` only fires on `Err` (var unset or
        // non-unicode), so an explicit `RUST_LIB_BACKTRACE=0` disables even
        // when `RUST_BACKTRACE=1`.
        let raw = env::var("RUST_LIB_BACKTRACE").or_else(|_| env::var("RUST_BACKTRACE"));
        let enabled = match raw.as_deref() {
            Ok("0") => Some(Self::Disabled),
            Ok("full") => Some(Self::Full),
            Ok(_) => Some(Self::Enabled),
            Err(_) => None,
        };
        ENABLED.store(enabled.map_or(NONE, |f| f as u8), Relaxed);
        enabled
    }

    /// Panic-path detection: consults only `RUST_BACKTRACE`, mirroring std's
    /// panic handler. `RUST_LIB_BACKTRACE` deliberately has no effect here —
    /// it exists to control library error capture independently of panics.
    pub fn detect_panic_opt() -> Option<Self> {
        const NONE: u8 = u8::MAX;
        const NOT_SET: u8 = 0;

        static ENABLED: AtomicU8 = AtomicU8::new(NOT_SET);
        if let Some(cached) = match ENABLED.load(Relaxed) {
            1 => Some(Some(Self::Disabled)),
            2 => Some(Some(Self::Enabled)),
            3 => Some(Some(Self::Full)),
            NONE => Some(None),
            NOT_SET => None,
            _ => unreachable!(),
        } {
            return cached;
        }

        let enabled = match env::var("RUST_BACKTRACE").as_deref() {
            Ok("0") => Some(Self::Disabled),
            Ok("full") => Some(Self::Full),
            Ok(_) => Some(Self::Enabled),
            Err(_) => None,
        };
        ENABLED.store(enabled.map_or(NONE, |f| f as u8), Relaxed);
        enabled
    }

    /// Whether frames should be captured at all.
    #[inline]
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Full | Self::Enabled)
    }

    /// Whether rendering should show every frame, skipping the trimming that
    /// hides capture machinery and runtime setup/teardown frames.
    #[inline]
    #[must_use]
    pub const fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }
}
