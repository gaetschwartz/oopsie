//! Shared helpers for erased-oopsie integration tests.

use std::sync::LazyLock;

use oopsie::{Oopsie, ResultExt as _, traced};
use tracing::instrument;
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
    let subscriber = tracing_subscriber::registry().with(oopsie::tracing::json_error_layer());
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
    .to_owned()
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

/// `::h<16 hex>` mangled symbol-hash suffix (stable rust default demangling).
/// Stripped entirely so identical symbols match across stable / nightly /
/// platform variants.
pub static FN_HASH_SUFFIX_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"::h[0-9a-f]{16}\b").unwrap());

/// Normalizes `Box<concrete::Type>` (nightly demangling, with the
/// monomorphized concrete type) back to `Box<T>` (stable's form using the
/// type-param name) so the same snapshot matches both channels.
pub static BOX_GENERIC_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"Box<[^,>]+>").unwrap());

/// Synthetic type-parameter names emitted by stable demangling
/// (`<__T0>`, `<__T1>`, ...) — normalize to `<T>`.
pub static SYNTHETIC_PARAM_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"<__T\d+>").unwrap());

/// Outer self-type wrap for monomorphized methods (nightly form:
/// `<MyType<X>>::method`). Strip the wrap and normalize generics to
/// `MyType<T>::method`, matching stable's preferred form.
pub static MONOMORPHIZED_WRAP_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"<(\w+(?:::\w+)*)<[^<>]+>>::").unwrap());

/// Empty-return turbofish (`::<()>`) appears on nightly demangling but not
/// stable. Strip entirely.
pub static EMPTY_TURBOFISH_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"::<\(\)>").unwrap());

pub static PATH_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(&format!(
        r"(?:{}|{}|/rustc/[0-9a-f]+)/",
        regex::escape(CARGO_WORKSPACE_ROOT),
        regex::escape(&SYS_ROOT),
    ))
    .unwrap()
});

/// Local toolchains store stdlib sources at `<sysroot>/lib/rustlib/src/rust/
/// library/...`; CI distributes stdlib via `/rustc/<commit>/library/...`.
/// After `PATH_REGEX` normalizes the prefix, the local form still carries
/// the `lib/rustlib/src/rust/` middle segment. Strip it (or the bare empty
/// equivalent on CI) so both produce `[STDLIB]/library/...`.
pub static STDLIB_PATH_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\[PATH\]/(?:lib/rustlib/src/rust/)?library/").unwrap());

pub static REGISTRY_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r".*/index\.crates\.io-[a-f0-9]+/(\w+)-[^/]+/").unwrap());

#[macro_export]
macro_rules! redact {
    (backtrace, $bl:block) => {
        insta::with_settings! {
          { filters => [
            // Crate-hash markers — `[abc1234]` brackets (nightly) and
            // `::h<16 hex>` suffixes (stable). Strip BOTH to empty so the
            // demangled names align across toolchains.
            (r"\[[0-9a-f]{7,16}\]", ""),
            (r"::h[0-9a-f]{16}\b", ""),
            // Demangling differences for monomorphized generic methods.
            // Stable shows `MyType<__T0>::method` (synthetic param name);
            // nightly shows `<MyType<alloc::string::String>>::method` (concrete
            // type plus an outer self-type wrap). Normalize both to
            // `MyType<T>::method`.
            (r"Box<[^,>]+>", "Box<T>"),
            (r"<__T\d+>", "<T>"),
            (r"<(\w+(?:::\w+)*)<[^<>]+>>::", "$1<T>::"),
            // Empty-return-type turbofish on nightly (`fail::<()>` vs `fail`).
            (r"::<\(\)>", ""),
            (r"rs:\d+(:\d+)?", "rs:[LOC]"),
            (r"\/[a-f0-9]+\/", "/[HASH]/"),
            ($crate::common::CARGO_WORKSPACE_ROOT, "[WORKSPACE_ROOT]"),
            (&*$crate::common::SYS_ROOT, "[SYS_ROOT]"),
            (concat!(env!("HOME"), "/.cargo/registry/src/"), "[CARGO_REGISTRY]/"),
            // Normalize stdlib paths after the above substitutions:
            //   local: `[SYS_ROOT]/lib/rustlib/src/rust/library/...`
            //   CI:    `/rustc/[HASH]/library/...`
            // Both → `[STDLIB]/library/...`.
            (r"\[SYS_ROOT\]/lib/rustlib/src/rust/library/", "[STDLIB]/library/"),
            (r"/rustc/\[HASH\]/library/", "[STDLIB]/library/"),
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
                    }
                    panic!("Expected a string value for name redaction but got: {value:?}");
                };
                // Strip / normalize toolchain-specific demangling forms
                // (see backtrace arm for the same chain).
                let s = $crate::common::CRATE_HASH_REGEX.replace_all(s, "");
                let s = $crate::common::FN_HASH_SUFFIX_REGEX.replace_all(&s, "");
                let s = $crate::common::BOX_GENERIC_REGEX.replace_all(&s, "Box<T>");
                let s = $crate::common::MONOMORPHIZED_WRAP_REGEX.replace_all(&s, "$1<T>::");
                let s = $crate::common::SYNTHETIC_PARAM_REGEX.replace_all(&s, "<T>");
                let s = $crate::common::EMPTY_TURBOFISH_REGEX.replace_all(&s, "");
                s.into_owned().into()
            }),
        );
        settings.add_redaction(
            ".backtrace.frames[].filename",
            insta::dynamic_redaction::<insta::internals::Content, _>(move |value, _path| {
                let Some(s) = value.as_str() else {
                    if value.is_nil() {
                        return ().into();
                    }
                    panic!("Expected a string value for filename redaction but got: {value:?}");
                };
                let s = $crate::common::REGISTRY_REGEX.replace(s, "[REGISTRY]/$1-[VERSION]/");
                let s = $crate::common::PATH_REGEX.replace(&s, "[PATH]/");
                let s = $crate::common::STDLIB_PATH_REGEX.replace(&s, "[STDLIB]/library/");
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

#[cfg(feature = "unstable-error-generic-member-access")]
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {
        concat!($name, "_unstable")
    };
}

#[cfg(not(feature = "unstable-error-generic-member-access"))]
#[macro_export]
macro_rules! snap_name {
    ($name:literal) => {
        concat!($name, "_stable")
    };
}
