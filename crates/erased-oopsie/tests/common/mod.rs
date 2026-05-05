//! Shared helpers for erased-oopsie integration tests.

use std::sync::LazyLock;

use oopsie::{NewJsonErrorLayer as _, Oopsie, ResultExt as _, traced};
use tracing::instrument;
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;

#[traced]
#[derive(Debug, Oopsie)]
pub enum MyError {
    #[oopsie(display("Inner error happened"))]
    Inner { source: MyErrorInner },
}

#[traced]
#[derive(Debug, Oopsie)]
#[oopsie("Error: {message}")]
pub struct MyErrorInner {
    message: String,
}

/// Initialize a test subscriber with ErrorLayer for spantrace support.
pub fn init_test_subscriber() -> tracing::subscriber::DefaultGuard {
    let subscriber = tracing_subscriber::registry().with(ErrorLayer::json());
    tracing::subscriber::set_default(subscriber)
}

/// Create an `ErrorWithSpanTrace` within instrumented functions to capture spantrace.
#[expect(clippy::items_after_statements)]
pub fn make_error() -> MyError {
    let _guard = init_test_subscriber();

    #[instrument(target = "sys", fields(id = 42))]
    fn inner_function(foo: bool, name: &str) -> Result<(), MyErrorInner> {
        MyErrorInnerOopsie {
            message: format!("Inner function failed (foo={foo}, name={name})"),
        }
        .fail()
    }

    #[instrument(target = "controller")]
    fn outer_function(foo: bool, name: &str) -> Result<(), MyError> {
        Ok(inner_function(foo, name).context(my_oopsies::Inner)?)
    }

    outer_function(true, "Alice").expect_err("Should produce an error")
}

pub static SYS_ROOT: LazyLock<String> = LazyLock::new(|| {
    String::from_utf8(
        std::process::Command::new("rustc")
            .arg("--print")
            .arg("sysroot")
            .output()
            .expect("failed to run rustc")
            .stdout,
    )
    .expect("invalid UTF-8 in rustc sysroot")
    .trim()
    .to_string()
});
pub const CARGO_WORKSPACE_ROOT: &str = konst::string::rsplit_once(
    konst::string::rsplit_once(env!("CARGO_MANIFEST_DIR"), "/")
        .unwrap()
        .0,
    "/",
)
.unwrap()
.0;
pub static CRATE_HASH_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\[[0-9a-f]{7,16}\]").unwrap());

pub static PATH_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(&format!(
        r"(?:{}|{}|/rustc/[0-9a-f]+)/",
        regex::escape(CARGO_WORKSPACE_ROOT),
        regex::escape(&SYS_ROOT),
    ))
    .unwrap()
});

pub static REGISTRY_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r".*/index\.crates\.io-[a-f0-9]+/(\w+)-[^/]+/").unwrap());

#[macro_export]
macro_rules! redact {
    (backtrace, $bl:block) => {
        insta::with_settings! {
          { filters => [
            (r"\[[0-9a-f]{7,16}\]", "[PTR]"),
            (r"rs:\d+(:\d+)?", "rs:[LOC]"),
            (r"\/[a-f0-9]+\/", "/[HASH]/"),
            ($crate::common::CARGO_WORKSPACE_ROOT, "[WORKSPACE_ROOT]"),
            (&*$crate::common::SYS_ROOT, "[SYS_ROOT]"),
            (concat!(env!("HOME"), "/.cargo/registry/src/"), "[CARGO_REGISTRY]/"),
        ] }, $bl }
    };
    (json, $bl:block) => {{
        let mut settings = insta::Settings::clone_current();

        // Redact crate hashes [hex7-16] in frame names
        settings.add_redaction(
            ".backtrace.frames[].name",
            insta::dynamic_redaction::<insta::internals::Content, _>(|value, _path| {
                let Some(s) = value.as_str() else {
                    if value.is_nil() {
                        return ().into();
                    } else {
                        panic!("Expected a string value for name redaction but got: {value:?}");
                    }
                };
                $crate::common::CRATE_HASH_REGEX
                    .replace_all(s, "[HASH]")
                    .into_owned()
                    .into()
            }),
        );
        settings.add_redaction(
            ".backtrace.frames[].filename",
            insta::dynamic_redaction::<insta::internals::Content, _>(move |value, _path| {
                let Some(s) = value.as_str() else {
                    if value.is_nil() {
                        return ().into();
                    } else {
                        panic!("Expected a string value for filename redaction but got: {value:?}");
                    }
                };
                let s = $crate::common::REGISTRY_REGEX.replace(s, "[REGISTRY]/$1-[VERSION]/");
                let s = $crate::common::PATH_REGEX.replace(&s, "[PATH]/");
                s.into_owned().into()
            }),
        );

        // Redact volatile line/column numbers
        settings.add_redaction(".backtrace.frames[].line", -1);
        settings.add_redaction(".backtrace.frames[].column", 0);
        settings.add_redaction(".spantrace.spans[].metadata.line", -1);

        settings.bind(|| $bl);
    }};
}

#[cfg(feature = "unstable")]
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {
        concat!($name, "_unstable")
    };
}

#[cfg(not(feature = "unstable"))]
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {
        concat!($name, "_stable")
    };
}
