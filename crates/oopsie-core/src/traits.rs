/// Converts a context selector and its source error into the target error type.
///
/// Implemented by generated context selector structs.
pub trait IntoError<E: std::error::Error> {
    /// The source error type (or `NoneError` for leaf errors).
    type Source;

    /// Build the target error from this context selector and the source error.
    #[track_caller]
    fn into_error(self, source: Self::Source) -> E;
}

/// Generates data to be implicitly included in an error.
///
/// Types like `Backtrace` and `Spantrace` implement this trait
/// so they can be auto-filled when an error is constructed.
pub trait GenerateImplicitData {
    #[track_caller]
    fn generate() -> Self;

    #[track_caller]
    fn generate_with_source(source: &dyn std::error::Error) -> Self
    where
        Self: Sized,
    {
        let _ = source;
        Self::generate()
    }
}

/// Unit type used as `IntoError::Source` for errors without a source.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct NoneError;

/// Extension trait on `Result` for ergonomic error context.
pub trait ResultExt<T, E> {
    /// Wrap the error with additional context.
    fn context<C, E2>(self, context: C) -> Result<T, E2>
    where
        C: IntoError<E2, Source = E>,
        E2: std::error::Error;

    /// Wrap the error with lazily-evaluated context.
    fn with_context<F, C, E2>(self, context: F) -> Result<T, E2>
    where
        F: FnOnce(&mut E) -> C,
        C: IntoError<E2, Source = E>,
        E2: std::error::Error;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    #[track_caller]
    fn context<C, E2>(self, context: C) -> Result<T, E2>
    where
        C: IntoError<E2, Source = E>,
        E2: std::error::Error,
    {
        self.map_err(|error| context.into_error(error))
    }

    #[track_caller]
    fn with_context<F, C, E2>(self, context: F) -> Result<T, E2>
    where
        F: FnOnce(&mut E) -> C,
        C: IntoError<E2, Source = E>,
        E2: std::error::Error,
    {
        self.map_err(|mut error| context(&mut error).into_error(error))
    }
}

/// Extension trait on `Option` for converting `None` into errors.
pub trait OptionExt<T> {
    /// Convert `None` into an error with the given context.
    fn context<C, E>(self, context: C) -> Result<T, E>
    where
        C: IntoError<E, Source = NoneError>,
        E: std::error::Error;

    /// Convert `None` into an error with lazily-evaluated context.
    fn with_context<F, C, E>(self, context: F) -> Result<T, E>
    where
        F: FnOnce() -> C,
        C: IntoError<E, Source = NoneError>,
        E: std::error::Error;
}

impl<T> OptionExt<T> for Option<T> {
    #[track_caller]
    fn context<C, E>(self, context: C) -> Result<T, E>
    where
        C: IntoError<E, Source = NoneError>,
        E: std::error::Error,
    {
        self.ok_or_else(|| context.into_error(NoneError))
    }

    #[track_caller]
    fn with_context<F, C, E>(self, context: F) -> Result<T, E>
    where
        F: FnOnce() -> C,
        C: IntoError<E, Source = NoneError>,
        E: std::error::Error,
    {
        self.ok_or_else(|| context().into_error(NoneError))
    }
}
