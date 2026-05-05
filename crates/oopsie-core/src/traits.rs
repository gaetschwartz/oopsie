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

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;
    use std::fmt;

    // Test error types for trait implementations
    #[derive(Debug)]
    struct SimpleError {
        message: String,
    }

    impl fmt::Display for SimpleError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for SimpleError {}

    #[derive(Debug)]
    struct SourceError {
        message: String,
    }

    impl fmt::Display for SourceError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for SourceError {}

    #[derive(Debug)]
    struct ChainError {
        message: String,
        source: Box<SourceError>,
    }

    impl fmt::Display for ChainError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for ChainError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&*self.source)
        }
    }

    // Test Contextual selector implementations
    struct SimpleSelector;

    impl Contextual<SimpleError> for SimpleSelector {
        type Source = NoSource;

        fn build_error(self, _source: Self::Source) -> SimpleError {
            SimpleError {
                message: "simple error".to_string(),
            }
        }
    }

    struct ChainSelector;

    impl Contextual<ChainError> for ChainSelector {
        type Source = SourceError;

        fn build_error(self, source: Self::Source) -> ChainError {
            ChainError {
                message: "chain error".to_string(),
                source: Box::new(source),
            }
        }
    }

    #[test]
    fn test_result_ext_context_ok() {
        let result: Result<i32, SourceError> = Ok(42);
        let chained: Result<i32, ChainError> = result.context(ChainSelector);
        assert!(chained.is_ok());
        assert_eq!(chained.unwrap(), 42);
    }

    #[test]
    fn test_result_ext_context_err() {
        let result: Result<i32, SourceError> = Err(SourceError {
            message: "source failed".to_string(),
        });
        let chained: Result<i32, ChainError> = result.context(ChainSelector);
        assert!(chained.is_err());
        let err = chained.unwrap_err();
        assert_eq!(err.message, "chain error");
        assert_eq!(StdError::source(&err).unwrap().to_string(), "source failed");
    }

    #[test]
    fn test_result_ext_with_context_ok() {
        let result: Result<i32, SourceError> = Ok(42);
        let chained: Result<i32, ChainError> = result.with_context(|_| ChainSelector);
        assert!(chained.is_ok());
        assert_eq!(chained.unwrap(), 42);
    }

    #[test]
    fn test_result_ext_with_context_err_closure_called() {
        let mut closure_called = false;
        let result: Result<i32, SourceError> = Err(SourceError {
            message: "source failed".to_string(),
        });
        let chained: Result<i32, ChainError> = result.with_context(|source| {
            closure_called = true;
            // Verify we can access the source in the closure
            assert_eq!(source.message, "source failed");
            ChainSelector
        });
        assert!(closure_called);
        assert!(chained.is_err());
    }

    #[test]
    fn test_option_ext_context_some() {
        let option: Option<i32> = Some(42);
        let result: Result<i32, SimpleError> = option.context(SimpleSelector);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_option_ext_context_none() {
        let option: Option<i32> = None;
        let result: Result<i32, SimpleError> = option.context(SimpleSelector);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().message, "simple error");
    }

    #[test]
    fn test_option_ext_with_context_some() {
        let option: Option<i32> = Some(42);
        let result: Result<i32, SimpleError> = option.with_context(|| SimpleSelector);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_option_ext_with_context_none_closure_called() {
        let mut closure_called = false;
        let option: Option<i32> = None;
        let result: Result<i32, SimpleError> = option.with_context(|| {
            closure_called = true;
            SimpleSelector
        });
        assert!(closure_called);
        assert!(result.is_err());
    }

    const _: () = {
        // Verify that Box<T> implements Capturable when T: Capturable
        const fn is_capturable<T: Capturable>() {}
        is_capturable::<Box<crate::BackTrace>>();
    };
}
