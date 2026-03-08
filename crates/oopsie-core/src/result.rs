#![allow(unused)]
use std::convert::Infallible;
use std::ops::{ControlFlow, FromResidual, Try};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct MayBoxResult<T, E>(Result<T, smallbox::SmallBox<E, smallbox::space::S1>>);

impl<T, E> MayBoxResult<T, E> {
    pub fn new(err: E) -> Self {
        Self(Err(smallbox::SmallBox::new(err)))
    }

    pub fn map<F, U>(self, f: F) -> MayBoxResult<U, E>
    where
        F: FnOnce(T) -> U,
    {
        MayBoxResult(self.0.map(f))
    }

    pub fn map_err<F, U>(self, f: F) -> MayBoxResult<T, U>
    where
        F: FnOnce(E) -> U,
    {
        match self.0 {
            Ok(t) => MayBoxResult(Ok(t)),
            Err(e) => MayBoxResult(Err(smallbox::SmallBox::new(f(e.into_inner())))),
        }
    }

    pub fn and_then<F, U>(self, f: F) -> MayBoxResult<U, E>
    where
        F: FnOnce(T) -> MayBoxResult<U, E>,
    {
        MayBoxResult(self.0.and_then(|t| f(t).0))
    }

    pub fn or_else<F, U>(self, f: F) -> MayBoxResult<T, U>
    where
        F: FnOnce(E) -> MayBoxResult<T, U>,
    {
        match self.0 {
            Ok(t) => MayBoxResult(Ok(t)),
            Err(e) => f(e.into_inner()),
        }
    }
}
impl<T, E: std::fmt::Debug> MayBoxResult<T, E> {
    /// Unwraps the result, yielding the content of an `Ok`.
    ///
    /// # Panics
    ///
    /// Panics if the value is an `Err`.
    pub fn unwrap(self) -> T {
        match self.0 {
            Ok(t) => t,
            Err(e) => panic!("called `MayBoxResult::unwrap()` on an `Err` value: {e:?}"),
        }
    }
}

impl<T, E> From<Result<T, E>> for MayBoxResult<T, E> {
    fn from(result: Result<T, E>) -> Self {
        Self(result.map_err(smallbox::SmallBox::new))
    }
}

impl<T, E> Try for MayBoxResult<T, E> {
    type Output = T;
    type Residual = MayBoxResult<Infallible, E>;

    fn from_output(output: T) -> Self {
        Self(Ok(output))
    }

    fn branch(self) -> ControlFlow<Self::Residual, T> {
        match self.0 {
            Ok(v) => ControlFlow::Continue(v),
            Err(e) => ControlFlow::Break(MayBoxResult(Err(e))),
        }
    }
}

impl<T, E> FromResidual<Result<Infallible, E>> for MayBoxResult<T, E> {
    fn from_residual(residual: Result<Infallible, E>) -> Self {
        match residual {
            Err(e) => Self(Err(smallbox::SmallBox::new(e))),
        }
    }
}

impl<T, E> FromResidual<MayBoxResult<Infallible, E>> for MayBoxResult<T, E> {
    fn from_residual(residual: MayBoxResult<Infallible, E>) -> Self {
        match residual.0 {
            Err(e) => Self(Err(e)),
        }
    }
}
