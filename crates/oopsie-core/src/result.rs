#![allow(unused)]
use std::convert::Infallible;
use std::ops::{ControlFlow, FromResidual, Residual, Try};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct MayBoxResult<T, E>(Result<T, smallbox::SmallBox<E, smallbox::space::S1>>);

impl<T, E> MayBoxResult<T, E> {
    #[inline]
    pub fn new(err: E) -> Self {
        Self(Err(smallbox::SmallBox::new(err)))
    }

    #[inline]
    pub fn map<F, U>(self, f: F) -> MayBoxResult<U, E>
    where
        F: FnOnce(T) -> U,
    {
        MayBoxResult(self.0.map(f))
    }

    #[inline]
    pub fn map_err<F, U>(self, f: F) -> MayBoxResult<T, U>
    where
        F: FnOnce(E) -> U,
    {
        match self.0 {
            Ok(t) => MayBoxResult(Ok(t)),
            Err(e) => MayBoxResult(Err(smallbox::SmallBox::new(f(e.into_inner())))),
        }
    }

    #[inline]
    pub fn and_then<F, U>(self, f: F) -> MayBoxResult<U, E>
    where
        F: FnOnce(T) -> MayBoxResult<U, E>,
    {
        MayBoxResult(self.0.and_then(|t| f(t).0))
    }

    #[inline]
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
    #[inline]
    fn from(result: Result<T, E>) -> Self {
        Self(result.map_err(smallbox::SmallBox::new))
    }
}

impl<T, E> Try for MayBoxResult<T, E> {
    type Output = T;
    type Residual = MayBoxResult<Infallible, E>;

    #[inline]
    fn from_output(output: T) -> Self {
        Self(Ok(output))
    }

    #[inline]
    fn branch(self) -> ControlFlow<Self::Residual, T> {
        match self.0 {
            Ok(v) => ControlFlow::Continue(v),
            Err(e) => ControlFlow::Break(MayBoxResult(Err(e))),
        }
    }
}

impl<T, E> FromResidual<Result<Infallible, E>> for MayBoxResult<T, E> {
    #[inline]
    fn from_residual(residual: Result<Infallible, E>) -> Self {
        match residual {
            Err(e) => Self(Err(smallbox::SmallBox::new(e))),
        }
    }
}

impl<T, E> FromResidual<MayBoxResult<Infallible, E>> for MayBoxResult<T, E> {
    #[inline]
    fn from_residual(residual: MayBoxResult<Infallible, E>) -> Self {
        match residual.0 {
            Err(e) => Self(Err(e)),
        }
    }
}

impl<T, E> Residual<T> for MayBoxResult<Infallible, E> {
    type TryType = MayBoxResult<T, E>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(v: i32) -> MayBoxResult<i32, String> {
        MayBoxResult::from(Ok(v))
    }

    fn err(s: &str) -> MayBoxResult<i32, String> {
        MayBoxResult::new(s.to_owned())
    }

    // --- map ---

    #[test]
    fn map_ok() {
        let r = ok(2).map(|v| v * 3);
        assert_eq!(r, MayBoxResult::from(Ok(6)));
    }

    #[test]
    fn map_err() {
        let r = err("bad").map(|v: i32| v * 3);
        assert_eq!(r, MayBoxResult::new("bad".to_owned()));
    }

    // --- map_err ---

    #[test]
    fn map_err_on_ok() {
        let r = ok(5).map_err(|e: String| e.len());
        assert_eq!(r, MayBoxResult::from(Ok(5)));
    }

    #[test]
    fn map_err_on_err() {
        let r = err("hello").map_err(|e| e.len());
        assert_eq!(r, MayBoxResult::new(5usize));
    }

    // --- and_then ---

    #[test]
    fn and_then_ok() {
        let r = ok(3).and_then(|v| MayBoxResult::from(Ok(v + 10)));
        assert_eq!(r, MayBoxResult::from(Ok(13)));
    }

    #[test]
    fn and_then_err() {
        let r = err("fail").and_then(|v| MayBoxResult::from(Ok(v + 10)));
        assert_eq!(r, MayBoxResult::new("fail".to_owned()));
    }

    // --- or_else ---

    #[test]
    fn or_else_ok() {
        let r = ok(7).or_else(|_| MayBoxResult::<i32, i32>::from(Ok(99)));
        assert_eq!(r, MayBoxResult::from(Ok(7)));
    }

    #[test]
    fn or_else_err() {
        let r = err("oops").or_else(|e| MayBoxResult::new(i32::try_from(e.len()).unwrap()));
        assert_eq!(r, MayBoxResult::new(4i32));
    }

    // --- unwrap ---

    #[test]
    fn unwrap_ok() {
        assert_eq!(ok(42).unwrap(), 42);
    }

    #[test]
    #[should_panic(expected = "called `MayBoxResult::unwrap()` on an `Err` value")]
    fn unwrap_err() {
        err("boom").unwrap();
    }

    // --- From<Result<T, E>> ---

    #[test]
    fn from_result_ok() {
        let r: MayBoxResult<i32, String> = MayBoxResult::from(Ok(10));
        assert_eq!(r.unwrap(), 10);
    }

    #[test]
    fn from_result_err() {
        let r: MayBoxResult<i32, String> = MayBoxResult::from(Err("e".to_owned()));
        assert_eq!(r, MayBoxResult::new("e".to_owned()));
    }

    // --- Try::from_output ---

    #[test]
    fn try_from_output() {
        let r = MayBoxResult::<i32, String>::from_output(99);
        assert_eq!(r.unwrap(), 99);
    }

    // --- Try::branch ---

    #[test]
    fn branch_ok() {
        let cf = ok(5).branch();
        assert_eq!(cf, ControlFlow::Continue(5));
    }

    #[test]
    fn branch_err() {
        let cf = err("x").branch();
        match cf {
            ControlFlow::Break(residual) => {
                assert_eq!(residual, MayBoxResult::new("x".to_owned()));
            }
            ControlFlow::Continue(_) => panic!("expected Break"),
        }
    }

    // --- FromResidual<Result<Infallible, E>> ---

    #[test]
    fn from_residual_result() {
        fn inner() -> MayBoxResult<i32, String> {
            let _: i32 = Err::<i32, String>("err".to_owned())?;
            MayBoxResult::from_output(0)
        }
        assert_eq!(inner(), MayBoxResult::new("err".to_owned()));
    }

    // --- FromResidual<MayBoxResult<Infallible, E>> ---

    #[test]
    fn from_residual_mayboxresult() {
        fn inner() -> MayBoxResult<i32, String> {
            let _: i32 = MayBoxResult::new("nested".to_owned())?;
            MayBoxResult::from_output(0)
        }
        assert_eq!(inner(), MayBoxResult::new("nested".to_owned()));
    }

    // Also test the happy path for ? operator to ensure from_output works in context
    #[test]
    fn try_operator_happy_path() {
        fn inner() -> MayBoxResult<i32, String> {
            let a: i32 = MayBoxResult::from(Ok::<_, String>(10))?;
            let b: i32 = MayBoxResult::from(Ok::<_, String>(20))?;
            MayBoxResult::from_output(a + b)
        }
        assert_eq!(inner().unwrap(), 30);
    }
}
