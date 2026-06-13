//! Ready-made [`Capturable`] types for ambient process and environment state.
//!
//! Each type snapshots a piece of context — the working directory, home directory, command-line
//! arguments, or a named environment variable — into an error through a `#[oopsie(capture)]`
//! field. Capturing never fails: an unavailable directory or variable is recorded as `None`.
//!
//! Requires the `extras` feature.

use std::{env, ffi::OsStr, ops::Deref};

use crate::Capturable;

/// The current working directory at the time the error was built, or `None` if it could not
/// be read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct CurrentDirOpt(Option<std::path::PathBuf>);

impl std::fmt::Display for CurrentDirOpt {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(path) => path.display().fmt(f),
            None => write!(f, "<current dir error>"),
        }
    }
}

impl Deref for CurrentDirOpt {
    type Target = Option<std::path::PathBuf>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl CurrentDirOpt {
    /// Consumes this value, yielding the inner `Option`.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Option<std::path::PathBuf> {
        self.0
    }
}

impl Capturable for CurrentDirOpt {
    #[inline]
    fn capture() -> Self {
        Self(env::current_dir().ok())
    }
}

/// The user's home directory at the time the error was built, or `None` if it could not be
/// determined.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct HomeDirOpt(Option<std::path::PathBuf>);

impl std::fmt::Display for HomeDirOpt {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(path) => path.display().fmt(f),
            None => write!(f, "<home dir error>"),
        }
    }
}

impl Deref for HomeDirOpt {
    type Target = Option<std::path::PathBuf>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl HomeDirOpt {
    /// Consumes this value, yielding the inner `Option`.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Option<std::path::PathBuf> {
        self.0
    }
}

impl Capturable for HomeDirOpt {
    #[inline]
    fn capture() -> Self {
        Self(env::home_dir())
    }
}

/// The process's command-line arguments at the time the error was built.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct EnvArgs(Box<[Box<OsStr>]>);

impl std::fmt::Display for EnvArgs {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl Deref for EnvArgs {
    type Target = [Box<OsStr>];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl EnvArgs {
    /// Consumes this value, yielding the inner `Box<[Box<OsStr>]>`.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Box<[Box<OsStr>]> {
        self.0
    }
}

impl Capturable for EnvArgs {
    #[inline]
    fn capture() -> Self {
        Self(
            env::args_os()
                .map(Into::into)
                .collect::<Box<[Box<OsStr>]>>(),
        )
    }
}

/// The value of the environment variable named by `E` at the time the error was built, or
/// `None` if it was unset.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct EnvVarOpt<E: EnvVarName>(Option<Box<str>>, std::marker::PhantomData<E>);

/// Names the environment variable that an [`EnvVarOpt`] reads.
///
/// Declare implementors with [`make_env_var!`](env_vars::make_env_var) rather than by hand.
pub trait EnvVarName {
    /// The environment variable name.
    const NAME: &'static str;
}

impl<E: EnvVarName> Capturable for EnvVarOpt<E> {
    #[inline]
    fn capture() -> Self {
        Self(
            env::var(E::NAME).ok().map(Into::into),
            std::marker::PhantomData,
        )
    }
}

impl<E: EnvVarName> std::fmt::Display for EnvVarOpt<E> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(value) => value.fmt(f),
            None => write!(f, "<env var {} error>", E::NAME),
        }
    }
}

impl<E: EnvVarName> Deref for EnvVarOpt<E> {
    type Target = Option<Box<str>>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<E: EnvVarName> EnvVarOpt<E> {
    /// Consumes this value, yielding the inner `Option`.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Option<Box<str>> {
        self.0
    }
}

/// Ready-made [`EnvVarOpt`] aliases for common variables, plus the
/// [`make_env_var!`](env_vars::make_env_var) macro for declaring your own.
pub mod env_vars {
    #[doc(hidden)]
    pub mod __private {
        pub use pastey;
    }

    #[doc(hidden)]
    #[macro_export]
    macro_rules! __make_env_var {
        ($($name:ident),+ $(,)?) => { $crate::extras::env_vars::__private::pastey::paste! { $(
            #[doc = "Marker type for the `" $name "` environment variable."]
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct [< $name:camel VarName>];

            impl $crate::extras::EnvVarName for [< $name:camel VarName>] {
                const NAME: &'static str = stringify!($name);
            }

            #[doc = "Captures the `" $name "` environment variable."]
            pub type [< $name:camel Opt >] = $crate::extras::EnvVarOpt<[< $name:camel VarName>]>;
        )+ }};
    }
    /// Declare [`EnvVarName`](super::EnvVarName) markers and [`EnvVarOpt`](super::EnvVarOpt)
    /// aliases for named variables.
    ///
    /// `make_env_var!(PATH, RUST_LOG)` generates `PathOpt` and `RustLogOpt` capturable types;
    /// each name becomes an `UpperCamelCase` alias with an `Opt` suffix.
    pub use crate::__make_env_var as make_env_var;

    make_env_var!(PATH, HOME, USER, RUST_BACKTRACE, RUST_LOG,);
}

#[cfg(test)]
mod tests {
    use super::env_vars::make_env_var;

    use super::*;

    #[test]
    fn current_dir_opt_captures_current_dir() {
        let captured = CurrentDirOpt::capture();

        assert_eq!(
            captured.as_ref().unwrap(),
            &std::env::current_dir().unwrap(),
            "captured current dir should match actual current dir"
        );
    }

    #[test]
    fn env_args_opt_captures_args() {
        let captured = EnvArgs::capture();
        let actual_args = std::env::args_os()
            .map(Into::into)
            .collect::<Vec<Box<OsStr>>>();

        assert_eq!(
            captured.as_ref(),
            &actual_args,
            "captured env args should match actual env args"
        );
    }

    #[test]
    fn env_var_opt_captures_env_var() {
        #[expect(
            unsafe_code,
            reason = "test setup requires setting an env var at runtime"
        )]
        unsafe {
            std::env::set_var("__OOPSIE_TEST_ENV_VAR", std::process::id().to_string());
        };
        make_env_var!(__OOPSIE_TEST_ENV_VAR);

        let captured = OopsieTestEnvVarOpt::capture();
        assert_eq!(
            captured.as_deref().unwrap(),
            &std::process::id().to_string(),
            "captured env var should match actual env var value"
        );
    }
}
