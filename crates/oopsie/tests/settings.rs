//! End-to-end coverage for the `settings` feature, in the style of `trybuild`:
//! fixtures under `tests/settings/<group>/`. Each group becomes ONE generated
//! crate with a `[[bin]]` per `<case>.rs`, sharing that group's
//! `[package.metadata.oopsie]` (the optional `<group>/oopsie.toml`, merged into a
//! default manifest). A case with a sibling `<case>.stderr` must fail to compile
//! and match it; otherwise it must build clean.
//!
//! A group that also carries a `<group>/workspace.toml` sidecar is a *workspace
//! group*: instead of one detached crate it generates a real Cargo workspace (a
//! virtual root manifest carrying the sidecar under `[workspace]`, plus one member
//! crate holding the cases), so the macro's workspace-root discovery and
//! `[workspace.metadata.oopsie]` merge are exercised end-to-end.
//!
//! trybuild itself can't drive this: it regenerates each fixture's `Cargo.toml`
//! and drops `[package.metadata]`, so a fixture would never see custom settings.
//! We instead build each group with one `cargo build --bins --keep-going
//! --message-format=json`, which compiles the shared `oopsie` once and every bin
//! together, and map per-bin diagnostics from the JSON. A failing bin has no
//! cached artifact, so it always re-reports its error; a passing bin emits no
//! error-level message.
//!
//! Gated by `OOPSIE_SETTINGS_E2E` (set on one leg by the Justfile) because each
//! group spawns a `cargo build`. Re-bless `*.stderr` with `OOPSIE_SETTINGS_BLESS=1`
//! (wired into `just test-bless`).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn oopsie_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// `<workspace-root>/target/tests/settings` — standalone groups live in `ws/`
/// (sources rewritten only when changed); `target/` (a sibling) stays warm and is
/// the shared `CARGO_TARGET_DIR` for every group.
fn target_base() -> PathBuf {
    oopsie_dir()
        .ancestors()
        .nth(2)
        .expect("workspace root above crates/oopsie")
        .join("target")
        .join("tests")
        .join("settings")
}

/// Workspace groups generate a real Cargo root, which must sit OUTSIDE the oopsie
/// workspace tree: an excluded member otherwise walks up past the generated root
/// to the oopsie root, which doesn't list it (cargo's "believes it's in a
/// workspace when it's not"). A stable temp path keeps warm caching across runs.
fn wsroot_base() -> PathBuf {
    std::env::temp_dir().join("oopsie-settings-wsroot")
}

/// Deep-merge `overlay` into `base`: tables recurse, everything else is replaced.
fn merge_into(base: &mut toml_edit::Table, mut overlay: toml_edit::Table) {
    for (key, value) in overlay.iter_mut() {
        if let (Some(base_sub), Some(overlay_sub)) = (
            base.get_mut(&key).and_then(toml_edit::Item::as_table_mut),
            value.as_table_mut(),
        ) {
            merge_into(base_sub, std::mem::take(overlay_sub));
        } else {
            base.insert(&key, std::mem::take(value));
        }
    }
}

/// The package manifest for a group's crate: `[package]` + a `[[bin]]` per case +
/// the `oopsie` path dep, then the group's `oopsie.toml` merged in for its
/// `[package.metadata.oopsie]`. `workspace_table` is the standalone empty
/// `[workspace]` that detaches the crate from the parent workspace; a member of a
/// generated workspace passes `false` so it resolves against the generated root.
fn package_manifest(
    group: &str,
    oopsie_path: &str,
    cases: &[String],
    sidecar: Option<&str>,
    workspace_table: bool,
) -> String {
    let workspace_table = if workspace_table {
        "[workspace]\n\n"
    } else {
        ""
    };
    let mut base = format!(
        "[package]\n\
         name = \"settings-{group}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2024\"\n\
         autobins = false\n\
         \n\
         {workspace_table}\
         [dependencies]\n\
         oopsie = {{ path = {oopsie_path:?}, default-features = false, features = [\"settings\"] }}\n",
    );
    for case in cases {
        // infallible into a String
        let _ = write!(base, "\n[[bin]]\nname = \"{case}\"\npath = \"{case}.rs\"\n");
    }
    let Some(sidecar) = sidecar else {
        return base;
    };
    let mut doc = base
        .parse::<toml_edit::DocumentMut>()
        .expect("base manifest parses");
    let overlay = sidecar
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|e| panic!("[{group}] oopsie.toml does not parse: {e}"));
    merge_into(doc.as_table_mut(), overlay.into_table());
    doc.to_string()
}

/// The virtual root manifest for a workspace group: `[workspace] resolver = "2"`
/// with the group's `workspace.toml` deep-merged under `[workspace]`. The sidecar
/// fully controls membership — no default `members` is injected.
fn workspace_root_manifest(group: &str, sidecar: &str) -> String {
    let mut doc = "[workspace]\nresolver = \"2\"\n"
        .parse::<toml_edit::DocumentMut>()
        .expect("root manifest base parses");
    let overlay = sidecar
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|e| panic!("[{group}] workspace.toml does not parse: {e}"));
    let workspace = doc["workspace"]
        .as_table_mut()
        .expect("workspace table present");
    merge_into(workspace, overlay.into_table());
    doc.to_string()
}

/// Write `content` to `path` only if it differs, so an unchanged case keeps its
/// mtime and cargo can reuse the cached build (warm re-runs stay fast).
fn write_if_changed(path: &Path, content: &str) {
    if std::fs::read_to_string(path).ok().as_deref() != Some(content) {
        std::fs::write(path, content).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
}

#[derive(Default)]
struct Outcome {
    has_error: bool,
    rendered: String,
}

/// Sidecars read alongside a group's `*.rs` cases.
struct Sidecars {
    oopsie: Option<String>,
    /// Present iff this is a workspace group (`None` keeps the standalone layout).
    workspace: Option<String>,
}

/// Build one group as a multi-bin crate; returns each case's rendered errors.
///
/// A standalone group (no `workspace.toml`) builds one detached crate under `ws/`.
/// A workspace group generates a virtual root + member crate (out of tree, see
/// [`wsroot_base`]) and builds in the member dir, so the macro walks up to the
/// generated root.
fn build_group(
    group: &str,
    oopsie_path: &str,
    cases: &[(String, String)],
    sidecars: &Sidecars,
) -> BTreeMap<String, Outcome> {
    let base = target_base();
    let stems: Vec<String> = cases.iter().map(|(stem, _)| stem.clone()).collect();

    let build_dir = if let Some(workspace_sidecar) = &sidecars.workspace {
        let root_dir = wsroot_base().join(group);
        let member_dir = root_dir.join("member");
        std::fs::create_dir_all(&member_dir).expect("create workspace member dir");
        write_if_changed(
            &root_dir.join("Cargo.toml"),
            &workspace_root_manifest(group, workspace_sidecar),
        );
        write_if_changed(
            &member_dir.join("Cargo.toml"),
            &package_manifest(
                group,
                oopsie_path,
                &stems,
                sidecars.oopsie.as_deref(),
                false,
            ),
        );
        member_dir
    } else {
        let crate_dir = base.join("ws").join(group);
        std::fs::create_dir_all(&crate_dir).expect("create group crate dir");
        write_if_changed(
            &crate_dir.join("Cargo.toml"),
            &package_manifest(group, oopsie_path, &stems, sidecars.oopsie.as_deref(), true),
        );
        crate_dir
    };

    for (stem, source) in cases {
        write_if_changed(&build_dir.join(format!("{stem}.rs")), source);
    }

    let output = Command::new(cargo())
        .current_dir(&build_dir)
        .arg("build")
        .arg("--bins")
        .arg("--keep-going")
        .arg("--message-format=json")
        .env("CARGO_TARGET_DIR", base.join("target"))
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `{}`: {e}", cargo().to_string_lossy()));

    let mut outcomes: BTreeMap<String, Outcome> = cases
        .iter()
        .map(|(stem, _)| (stem.clone(), Outcome::default()))
        .collect();

    // cargo emits `build-finished` only once it actually compiled the crate. If it
    // errors earlier (e.g. manifest resolution), there are no `compiler-message`s
    // and every case would look error-free — guard against that false pass.
    let mut saw_build_finished = false;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match value.get("reason").and_then(serde_json::Value::as_str) {
            Some("build-finished") => {
                saw_build_finished = true;
                continue;
            }
            Some("compiler-message") => {}
            _ => continue,
        }
        if value
            .pointer("/message/level")
            .and_then(serde_json::Value::as_str)
            != Some("error")
        {
            continue;
        }
        // Map the message to a case by the source file name (exact, so one stem
        // can't match another's suffix).
        let file = value
            .pointer("/target/src_path")
            .and_then(serde_json::Value::as_str)
            .map(|p| Path::new(p).file_name().unwrap_or_default().to_owned());
        let Some(stem) = file.and_then(|f| {
            cases
                .iter()
                .map(|(stem, _)| stem)
                .find(|stem| f == AsRef::<std::ffi::OsStr>::as_ref(&format!("{stem}.rs")))
                .cloned()
        }) else {
            continue;
        };
        let outcome = outcomes.get_mut(&stem).expect("case has an outcome slot");
        outcome.has_error = true;
        if let Some(rendered) = value
            .pointer("/message/rendered")
            .and_then(serde_json::Value::as_str)
        {
            outcome.rendered.push_str(rendered);
        }
    }
    assert!(
        saw_build_finished,
        "cargo build for group `{group}` produced no build-finished (it failed before \
         compiling fixtures). status: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    outcomes
}

/// Scrub machine-specific absolute paths so a `.stderr` is reproducible.
fn normalize(rendered: &str, oopsie_path: &str) -> String {
    format!("{}\n", rendered.replace(oopsie_path, "$OOPSIE").trim_end())
}

/// `(stem, source)` for every `*.rs` in `dir`, sorted.
fn cases_in(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let stem = path
            .file_stem()
            .expect("fixture has a stem")
            .to_string_lossy()
            .into_owned();
        let source = std::fs::read_to_string(&path).expect("read fixture .rs");
        out.push((stem, source));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn settings_fixtures() {
    if std::env::var_os("OOPSIE_SETTINGS_E2E").is_none() {
        return;
    }
    let bless = std::env::var_os("OOPSIE_SETTINGS_BLESS").is_some();
    let root = oopsie_dir().join("tests").join("settings");
    let oopsie_path = oopsie_dir().to_string_lossy().into_owned();

    let mut groups: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("read {}: {e}", root.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.is_dir())
        .collect();
    groups.sort();

    let mut failures = Vec::new();
    for group_dir in &groups {
        let group = group_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let cases = cases_in(group_dir);
        let sidecars = Sidecars {
            oopsie: std::fs::read_to_string(group_dir.join("oopsie.toml")).ok(),
            workspace: std::fs::read_to_string(group_dir.join("workspace.toml")).ok(),
        };
        let outcomes = build_group(&group, &oopsie_path, &cases, &sidecars);

        for (stem, _) in &cases {
            let outcome = &outcomes[stem];
            let stderr_path = group_dir.join(format!("{stem}.stderr"));
            let rendered = normalize(&outcome.rendered, &oopsie_path);

            if bless {
                if outcome.has_error {
                    std::fs::write(&stderr_path, &rendered).expect("write .stderr");
                } else if stderr_path.exists() {
                    std::fs::remove_file(&stderr_path).expect("remove stale .stderr");
                }
                continue;
            }

            match (stderr_path.exists(), outcome.has_error) {
                (true, true) => {
                    let expected = std::fs::read_to_string(&stderr_path).unwrap_or_default();
                    if rendered != expected {
                        failures.push(format!(
                            "{group}/{stem}: diagnostics changed (re-bless with OOPSIE_SETTINGS_BLESS=1)\n\
                             --- expected ---\n{expected}\n--- actual ---\n{rendered}"
                        ));
                    }
                }
                (true, false) => {
                    failures.push(format!("{group}/{stem}: has a .stderr but built clean"));
                }
                (false, true) => failures.push(format!(
                    "{group}/{stem}: expected a clean build, got:\n{rendered}"
                )),
                (false, false) => {}
            }
        }
    }

    assert!(
        failures.is_empty(),
        "settings fixtures failed:\n\n{}",
        failures.join("\n\n"),
    );
}
