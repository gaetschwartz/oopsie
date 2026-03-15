/// Converts a context selector and its source error into the target error type.
///
/// Implemented by generated context selector structs.
pub trait Contextual<E: std::error::Error> {
    /// The source error type (or `NoSource` for leaf errors).
    type Source;

    /// Build the target error from this context selector and the source error.
    #[track_caller]
    fn build_error(self, source: Self::Source) -> E;
}

/// Generates data to be implicitly included in an error.
///
/// Types like `Backtrace` and `Spantrace` implement this trait
/// so they can be auto-filled when an error is constructed.
pub trait Capturable {
    #[track_caller]
    fn capture() -> Self;
}

impl<T: Capturable> Capturable for Box<T> {
    #[track_caller]
    fn capture() -> Self {
        Box::new(T::capture())
    }
}

/// Hidden trait for extracting existing traces from [`ErrorExt`](crate::ErrorExt) sources.
///
/// Implemented for `BackTrace` and `SpanTrace` (and their `Box` wrappers)
/// to try extraction before falling back to fresh capture.
#[doc(hidden)]
pub trait CaptureExt: Capturable {
    #[track_caller]
    fn capture_or_extract(source: &dyn crate::ErrorExt) -> Self
    where
        Self: Sized;
}

impl<T: CaptureExt> CaptureExt for Box<T> {
    #[track_caller]
    fn capture_or_extract(source: &dyn crate::ErrorExt) -> Self {
        Box::new(T::capture_or_extract(source))
    }
}

/// Unit type used as `Contextual::Source` for errors without a source.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct NoSource;

/// Extension trait on `Result` for ergonomic error context.
pub trait ResultExt<T, E> {
    /// Wrap the error with additional context.
    fn context<C, E2>(self, context: C) -> Result<T, E2>
    where
        C: Contextual<E2, Source = E>,
        E2: std::error::Error;

    /// Wrap the error with lazily-evaluated context.
    fn with_context<F, C, E2>(self, context: F) -> Result<T, E2>
    where
        F: FnOnce(&mut E) -> C,
        C: Contextual<E2, Source = E>,
        E2: std::error::Error;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    #[track_caller]
    fn context<C, E2>(self, context: C) -> Result<T, E2>
    where
        C: Contextual<E2, Source = E>,
        E2: std::error::Error,
    {
        self.map_err(|error| context.build_error(error))
    }

    #[track_caller]
    fn with_context<F, C, E2>(self, context: F) -> Result<T, E2>
    where
        F: FnOnce(&mut E) -> C,
        C: Contextual<E2, Source = E>,
        E2: std::error::Error,
    {
        self.map_err(|mut error| context(&mut error).build_error(error))
    }
}

/// Extension trait on `Option` for converting `None` into errors.
pub trait OptionExt<T> {
    /// Convert `None` into an error with the given context.
    fn context<C, E>(self, context: C) -> Result<T, E>
    where
        C: Contextual<E, Source = NoSource>,
        E: std::error::Error;

    /// Convert `None` into an error with lazily-evaluated context.
    fn with_context<F, C, E>(self, context: F) -> Result<T, E>
    where
        F: FnOnce() -> C,
        C: Contextual<E, Source = NoSource>,
        E: std::error::Error;
}

impl<T> OptionExt<T> for Option<T> {
    #[track_caller]
    fn context<C, E>(self, context: C) -> Result<T, E>
    where
        C: Contextual<E, Source = NoSource>,
        E: std::error::Error,
    {
        self.ok_or_else(|| context.build_error(NoSource))
    }

    #[track_caller]
    fn with_context<F, C, E>(self, context: F) -> Result<T, E>
    where
        F: FnOnce() -> C,
        C: Contextual<E, Source = NoSource>,
        E: std::error::Error,
    {
        self.ok_or_else(|| context().build_error(NoSource))
    }
}
