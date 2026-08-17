//! End-to-end check that macro-generated code compiles in a `#![no_std]`
//! consumer crate: every alloc-dependent construct (notably a dynamic
//! `#[oopsie(help)]` field's `to_string()`) must route through the
//! `__private::alloc` facade rather than rely on the std prelude.
//!
//! Generates fixture crates out-of-tree and `cargo check`s them: one against
//! a std-linked oopsie (default features), one against a pure no_std oopsie
//! (`default-features = false`). Gated by `OOPSIE_NOSTD_E2E` (set on one leg
//! by the Justfile) because each fixture spawns a `cargo check`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn oopsie_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// `<workspace-root>/target/tests/nostd-consumer` — `ws/` holds the generated
/// fixture crates (rewritten only when changed); `target/` (a sibling) stays
/// warm and is the shared `CARGO_TARGET_DIR`.
fn target_base() -> PathBuf {
    oopsie_dir()
        .ancestors()
        .nth(2)
        .expect("workspace root above crates/oopsie")
        .join("target")
        .join("tests")
        .join("nostd-consumer")
}

/// Write `content` to `path` only if it differs, so an unchanged fixture keeps
/// its mtime and cargo can reuse the cached check (warm re-runs stay fast).
fn write_if_changed(path: &Path, content: &str) {
    if std::fs::read_to_string(path).ok().as_deref() != Some(content) {
        std::fs::write(path, content).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
}

/// Both macro entry points (derive and attribute) on both shapes (enum and
/// struct), each carrying a dynamic `#[oopsie(help)]` field.
const LIB_RS: &str = r#"#![no_std]

extern crate alloc;

use alloc::string::String;

#[derive(Debug, oopsie::Oopsie)]
#[oopsie(module(false))]
pub enum DerivedEnum {
    #[oopsie("leaf failed")]
    DerivedLeaf {
        #[oopsie(help)]
        hint: String,
    },
}

#[derive(Debug, oopsie::Oopsie)]
#[oopsie(module(false))]
pub struct DerivedStruct {
    #[oopsie(help)]
    hint: String,
}

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AttrEnum {
    #[oopsie("leaf failed")]
    AttrLeaf {
        #[oopsie(help)]
        hint: String,
    },
}

#[oopsie::oopsie]
#[oopsie(module(false))]
pub struct AttrStruct {
    #[oopsie(help)]
    hint: String,
}
"#;

fn manifest(name: &str, oopsie_path: &str, dep_spec: &str) -> String {
    format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2024\"\n\
         \n\
         [workspace]\n\
         \n\
         [dependencies]\n\
         oopsie = {{ path = {oopsie_path:?}{dep_spec} }}\n"
    )
}

fn check_fixture(base: &Path, name: &str, manifest: &str) {
    let crate_dir = base.join("ws").join(name);
    std::fs::create_dir_all(crate_dir.join("src")).expect("create fixture crate dir");
    write_if_changed(&crate_dir.join("Cargo.toml"), manifest);
    write_if_changed(&crate_dir.join("src").join("lib.rs"), LIB_RS);

    let output = Command::new(cargo())
        .current_dir(&crate_dir)
        .arg("check")
        .env("CARGO_TARGET_DIR", base.join("target"))
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `{}`: {e}", cargo().to_string_lossy()));
    assert!(
        output.status.success(),
        "no_std consumer `{name}` failed to compile:\n{}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn nostd_consumer_compiles() {
    if std::env::var_os("OOPSIE_NOSTD_E2E").is_none() {
        return;
    }
    let base = target_base();
    let oopsie_path = oopsie_dir().to_string_lossy().into_owned();

    check_fixture(
        &base,
        "nostd-consumer-std",
        &manifest("nostd-consumer-std", &oopsie_path, ""),
    );
    check_fixture(
        &base,
        "nostd-consumer-pure",
        &manifest(
            "nostd-consumer-pure",
            &oopsie_path,
            ", default-features = false",
        ),
    );
}
