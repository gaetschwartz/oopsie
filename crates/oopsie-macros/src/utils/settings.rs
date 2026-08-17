#![cfg(feature = "experimental-settings")]

use std::path::{Path, PathBuf};
use std::str::FromStr as _;
use std::sync::OnceLock;

use __serde::Deserialize as _;
use __serde::de::IntoDeserializer as _;
use quote::quote;
use toml_edit::DocumentMut;

#[derive(Debug, Clone, Default, __serde::Deserialize)]
#[serde(crate = "__serde", rename_all = "kebab-case", deny_unknown_fields)]
pub struct Settings {
    max_size: Option<usize>,
    default_suffix: Option<RawSuffix>,
    default_vis: Option<String>,
    module: Option<ModuleSetting>,
    traced: Option<TracedSetting>,
}

/// `default-suffix` accepts a name string or `false` (disable). `true` is
/// rejected as ambiguous (it would force the struct-style suffix onto enums).
#[derive(Debug, Clone, __serde::Deserialize)]
#[serde(crate = "__serde", untagged)]
enum RawSuffix {
    Toggle(bool),
    Name(String),
}

/// `module` is either a bool (wrap on/off) or a table of options.
#[derive(Debug, Clone, __serde::Deserialize)]
#[serde(crate = "__serde", untagged)]
enum ModuleSetting {
    Toggle(bool),
    Table(ModuleTable),
}

#[derive(Debug, Clone, __serde::Deserialize)]
#[serde(crate = "__serde", rename_all = "kebab-case", deny_unknown_fields)]
struct ModuleTable {
    enabled: Option<bool>,
    suffix: Option<String>,
}

/// `traced` is either a bool (trace-by-default on/off) or a table whose keys
/// set the defaults for the matching `traced(...)` sub-toggles. `enabled`
/// controls trace-by-default; the sub-toggle defaults apply whenever tracing
/// is active (globally or per type).
#[derive(Debug, Clone, __serde::Deserialize)]
#[serde(crate = "__serde", untagged)]
enum TracedSetting {
    Toggle(bool),
    Table(TracedTable),
}

#[derive(Debug, Clone, __serde::Deserialize)]
#[serde(crate = "__serde", rename_all = "kebab-case", deny_unknown_fields)]
struct TracedTable {
    enabled: Option<bool>,
    location: Option<bool>,
    timestamp: Option<bool>,
    packed: Option<bool>,
    boxed: Option<bool>,
    code: Option<bool>,
}

const MANIFEST_ENV_VAR: &str = "CARGO_MANIFEST_DIR";
const WORKSPACE_ENV_VAR: &str = "CARGO_WORKSPACE_DIR";
const CARGO_HOME_ENV_VAR: &str = "CARGO_HOME";

fn compile_error(msg: &str) -> proc_macro2::TokenStream {
    quote! { ::core::compile_error!(#msg); }
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

/// The member crate's manifest directory, canonicalized for symlink-safe
/// path comparison.
fn member_dir() -> Result<PathBuf, String> {
    let raw = env_path(MANIFEST_ENV_VAR).ok_or_else(|| format!("{MANIFEST_ENV_VAR} is not set"))?;
    std::fs::canonicalize(&raw)
        .map_err(|e| format!("failed to canonicalize {}: {e}", raw.display()))
}

/// Lenient read of `<dir>/Cargo.toml` for ancestor probing: any IO or parse
/// error yields `None` so an unreadable or oddly-shaped manifest on the walk
/// is skipped rather than aborting discovery.
fn read_doc(dir: &Path) -> Option<DocumentMut> {
    let content = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    DocumentMut::from_str(&content).ok()
}

fn has_workspace(doc: &DocumentMut) -> bool {
    doc.as_table().contains_key("workspace")
}

fn workspace_pointer(doc: &DocumentMut) -> Option<String> {
    doc.as_table()
        .get("package")
        .and_then(|item| item.as_table())
        .and_then(|table| table.get("workspace"))
        .and_then(|item| item.as_str())
        .map(ToOwned::to_owned)
}

fn workspace_string_array(doc: &DocumentMut, key: &str) -> Vec<String> {
    doc.as_table()
        .get("workspace")
        .and_then(|item| item.as_table())
        .and_then(|table| table.get(key))
        .and_then(|item| item.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A `members`/`exclude` entry covers, literally, the directory it names and
/// everything beneath it.
fn covers_any(root_dir: &Path, entries: &[String], member_dir: &Path) -> bool {
    entries
        .iter()
        .any(|entry| member_dir.starts_with(root_dir.join(entry)))
}

/// As [`covers_any`], plus glob entries, which match case-sensitively and
/// treat `/` literally (so `crates/*` does not reach `crates/a/b`). Match the
/// member or any ancestor so a nested member is covered by a parent entry.
///
/// The pattern is built from the entry alone and tested against the member's
/// path relative to the root, so glob metacharacters in the root's own path
/// stay literal.
fn matches_any(root_dir: &Path, entries: &[String], member_dir: &Path) -> bool {
    if covers_any(root_dir, entries, member_dir) {
        return true;
    }
    // `MatchOptions::default()` is case-insensitive; `new()` is not.
    let options = glob::MatchOptions {
        require_literal_separator: true,
        ..glob::MatchOptions::new()
    };
    let Ok(relative) = member_dir.strip_prefix(root_dir) else {
        return false;
    };
    entries.iter().any(|entry| {
        glob::Pattern::new(entry).is_ok_and(|pattern| {
            relative
                .ancestors()
                .any(|ancestor| pattern.matches_path_with(ancestor, options))
        })
    })
}

/// Cargo membership: an excluded path is not a member unless a *literal*
/// `members` entry also covers it — Cargo prefix-matches the raw member
/// strings, so a glob such as `crates/*` never beats `exclude`. A member
/// outside `root_dir` (only reachable via a `package.workspace` pointer)
/// can't be glob-matched, so it counts as a member.
fn member_excluded(root_dir: &Path, root_doc: &DocumentMut, member_dir: &Path) -> bool {
    if !member_dir.starts_with(root_dir) {
        return false;
    }
    let excluded = matches_any(
        root_dir,
        &workspace_string_array(root_doc, "exclude"),
        member_dir,
    );
    let explicit_member = covers_any(
        root_dir,
        &workspace_string_array(root_doc, "members"),
        member_dir,
    );
    excluded && !explicit_member
}

fn is_workspace_at(dir: &Path) -> bool {
    read_doc(dir).is_some_and(|doc| has_workspace(&doc))
}

/// The applicable workspace root manifest for the member crate, or `None`
/// when the member is standalone (no workspace, or excluded from the one it
/// sits under). Reads the environment, then delegates to [`find_root_from`].
fn find_workspace_root() -> Result<Option<PathBuf>, String> {
    let member_dir = member_dir()?;
    let workspace_override = env_path(WORKSPACE_ENV_VAR);
    let cargo_home = env_path(CARGO_HOME_ENV_VAR).and_then(|home| std::fs::canonicalize(home).ok());
    find_root_from(
        &member_dir,
        workspace_override.as_deref(),
        cargo_home.as_deref(),
    )
}

/// Workspace-root discovery, with the environment hoisted out so it can be
/// driven over a temp tree: honor an explicit `CARGO_WORKSPACE_DIR`, then the
/// member's own `[workspace]`, then a `package.workspace` pointer, then the
/// first ancestor `Cargo.toml` with a `[workspace]`. The walk stops at
/// `$CARGO_HOME` (both canonicalized) and at a `target/package` staging dir,
/// matching Cargo so a packaged or registry-sourced crate finds no workspace.
fn find_root_from(
    member_dir: &Path,
    workspace_override: Option<&Path>,
    cargo_home: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    if let Some(dir) = workspace_override
        && is_workspace_at(dir)
    {
        return Ok(Some(dir.join("Cargo.toml")));
    }

    let member_doc = {
        let path = member_dir.join("Cargo.toml");
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        DocumentMut::from_str(&content).map_err(|e| e.to_string())?
    };

    if has_workspace(&member_doc) {
        return Ok(Some(member_dir.join("Cargo.toml")));
    }

    if let Some(rel) = workspace_pointer(&member_doc) {
        let root_dir = std::fs::canonicalize(member_dir.join(&rel))
            .map_err(|e| format!("failed to resolve workspace pointer {rel:?}: {e}"))?;
        if !is_workspace_at(&root_dir) {
            return Err(format!(
                "package.workspace points at {}, which has no [workspace] table",
                root_dir.display()
            ));
        }
        return Ok(Some(root_dir.join("Cargo.toml")));
    }

    let mut current = member_dir.parent();
    while let Some(dir) = current {
        if cargo_home == Some(dir) || dir.ends_with("target/package") {
            break;
        }
        if let Some(doc) = read_doc(dir)
            && has_workspace(&doc)
        {
            if member_excluded(dir, &doc, member_dir) {
                return Ok(None);
            }
            return Ok(Some(dir.join("Cargo.toml")));
        }
        current = dir.parent();
    }

    Ok(None)
}

fn cached_workspace_root() -> &'static Result<Option<PathBuf>, String> {
    static CACHE: OnceLock<Result<Option<PathBuf>, String>> = OnceLock::new();
    CACHE.get_or_init(find_workspace_root)
}

/// Navigate `path` (e.g. `["package", "metadata", "oopsie"]`) and deserialize
/// the table found there into [`Settings`]; a missing section yields default
/// (empty) settings. Every table spelling is accepted — a header, a dotted
/// key, or an inline table.
fn take_oopsie_table(
    doc: &DocumentMut,
    path: &[&str],
    section_label: &str,
) -> Result<Settings, String> {
    let mut item = doc.as_item();
    for key in path {
        let Some(next) = item.as_table_like().and_then(|table| table.get(key)) else {
            return Ok(Settings::default());
        };
        item = next;
    }
    let Ok(value) = item.clone().into_value() else {
        return Ok(Settings::default());
    };
    Settings::deserialize(value.into_deserializer())
        .map_err(|e| format!("invalid {section_label} section: {e}"))
}

/// Parse `[package.metadata.oopsie]` out of a Cargo manifest. A manifest with
/// no such section yields default (empty) settings; only a malformed section
/// or an unknown / invalid key is an error.
fn parse_settings(manifest_toml: &str) -> Result<Settings, String> {
    let manifest = DocumentMut::from_str(manifest_toml).map_err(|e| e.to_string())?;
    take_oopsie_table(
        &manifest,
        &["package", "metadata", "oopsie"],
        "[package.metadata.oopsie]",
    )
}

/// Parse `[workspace.metadata.oopsie]` out of a workspace root manifest; a
/// missing section yields default (empty) settings.
fn parse_workspace_settings(manifest_toml: &str) -> Result<Settings, String> {
    let manifest = DocumentMut::from_str(manifest_toml).map_err(|e| e.to_string())?;
    take_oopsie_table(
        &manifest,
        &["workspace", "metadata", "oopsie"],
        "[workspace.metadata.oopsie]",
    )
}

fn read_settings() -> Result<Settings, String> {
    let manifest_dir =
        env_path(MANIFEST_ENV_VAR).ok_or_else(|| format!("{MANIFEST_ENV_VAR} is not set"))?;
    let cargo_toml_path = manifest_dir.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml_path)
        .map_err(|e| format!("failed to read {}: {e}", cargo_toml_path.display()))?;
    parse_settings(&content)
}

fn cached_settings() -> &'static Result<Settings, String> {
    static CACHE: OnceLock<Result<Settings, String>> = OnceLock::new();
    CACHE.get_or_init(read_settings)
}

fn read_workspace_settings() -> Result<Settings, String> {
    let root = match cached_workspace_root() {
        Ok(Some(root)) => root,
        Ok(None) => return Ok(Settings::default()),
        Err(e) => return Err(e.clone()),
    };
    let content = std::fs::read_to_string(root)
        .map_err(|e| format!("failed to read {}: {e}", root.display()))?;
    parse_workspace_settings(&content)
}

fn cached_workspace_settings() -> &'static Result<Settings, String> {
    static CACHE: OnceLock<Result<Settings, String>> = OnceLock::new();
    CACHE.get_or_init(read_workspace_settings)
}

/// Which manifest section a resolved knob came from.
#[derive(Clone, Copy)]
enum CapSource {
    Package,
    Workspace,
}

impl CapSource {
    const fn section(self) -> &'static str {
        match self {
            Self::Package => "[package.metadata.oopsie] max-size",
            Self::Workspace => "[workspace.metadata.oopsie] max-size",
        }
    }
}

/// Reject `Some(0)` — a zero cap can never be satisfied, so it's a config
/// mistake rather than a meaningful limit.
fn validate_cap(
    max_size: Option<(usize, CapSource)>,
) -> Result<Option<(usize, &'static str)>, String> {
    match max_size {
        Some((0, source)) => Err(format!(
            "{} is 0, which is not a valid size limit; remove the key to \
                 disable the cap instead",
            source.section()
        )),
        Some((n, source)) => Ok(Some((n, source.section()))),
        None => Ok(None),
    }
}

/// Pick the cap and its origin: the package value wins, else the workspace's.
fn select_cap(package: &Settings, workspace: &Settings) -> Option<(usize, CapSource)> {
    package
        .max_size
        .map(|n| (n, CapSource::Package))
        .or_else(|| workspace.max_size.map(|n| (n, CapSource::Workspace)))
}

/// The validated project-wide size cap, taken from the package manifest if it
/// sets `max-size`, else the workspace root. A deserialize parse error from
/// *either* manifest is surfaced here (this is the single accessor that
/// reports it).
pub fn cap() -> Result<Option<(usize, &'static str)>, String> {
    let package = match cached_settings() {
        Ok(settings) => settings,
        Err(e) => return Err(e.clone()),
    };
    let workspace = match cached_workspace_settings() {
        Ok(settings) => settings,
        Err(e) => return Err(e.clone()),
    };
    validate_cap(select_cap(package, workspace))
}

/// Resolve and validate the naming/visibility defaults, folding the package
/// manifest over the workspace root leaf-by-leaf. A manifest *parse* error is
/// reported once via [`cap`]; defer to it here (returning defaults + no token)
/// so a malformed manifest doesn't emit the same error per accessor.
pub fn naming_defaults() -> (super::NamingDefaults, proc_macro2::TokenStream) {
    let (Ok(package), Ok(workspace)) = (cached_settings(), cached_workspace_settings()) else {
        return (super::NamingDefaults::default(), quote! {});
    };
    match (
        resolve_naming(package, "[package.metadata.oopsie]"),
        resolve_naming(workspace, "[workspace.metadata.oopsie]"),
    ) {
        (Ok(package_naming), Ok(workspace_naming)) => {
            (package_naming.merge_over(workspace_naming), quote! {})
        }
        (Err(e), _) | (_, Err(e)) => (super::NamingDefaults::default(), compile_error(&e)),
    }
}

fn resolve_naming(settings: &Settings, section: &str) -> Result<super::NamingDefaults, String> {
    let (module, module_suffix) = match &settings.module {
        None => (None, None),
        Some(ModuleSetting::Toggle(b)) => (Some(*b), None),
        Some(ModuleSetting::Table(t)) => {
            let suffix = match &t.suffix {
                Some(s) if !super::is_ident(s) => {
                    return Err(format!(
                        "{section} module.suffix {s:?} is not a valid identifier"
                    ));
                }
                other => other.clone(),
            };
            (t.enabled, suffix)
        }
    };
    let suffix = match &settings.default_suffix {
        None => None,
        Some(RawSuffix::Toggle(false)) => Some(super::SuffixDefault::Off),
        Some(RawSuffix::Toggle(true)) => {
            return Err(format!(
                "{section} default-suffix = true is ambiguous; \
                     use a suffix name or `false` to disable"
            ));
        }
        Some(RawSuffix::Name(s)) if !super::is_ident_fragment(s) => {
            return Err(format!(
                "{section} default-suffix {s:?} is not a valid identifier fragment"
            ));
        }
        Some(RawSuffix::Name(s)) => Some(super::SuffixDefault::Name(s.clone())),
    };
    let vis =
        match &settings.default_vis {
            None => None,
            Some(v) => Some(syn::parse_str::<syn::Visibility>(v).map_err(|e| {
                format!("{section} default-vis {v:?} is not a valid visibility: {e}")
            })?),
        };
    Ok(super::NamingDefaults {
        module,
        module_suffix,
        suffix,
        vis,
    })
}

/// Resolve the `traced` defaults (no validation needed — all booleans),
/// folding the package manifest over the workspace root leaf-by-leaf. A
/// manifest parse error is reported once via [`cap`]; defer to it here.
pub fn traced_defaults() -> (super::TracedDefaults, proc_macro2::TokenStream) {
    let (Ok(package), Ok(workspace)) = (cached_settings(), cached_workspace_settings()) else {
        return (super::TracedDefaults::default(), quote! {});
    };
    (
        resolve_traced(package).merge_over(resolve_traced(workspace)),
        quote! {},
    )
}

fn resolve_traced(settings: &Settings) -> super::TracedDefaults {
    match &settings.traced {
        None => super::TracedDefaults::default(),
        Some(TracedSetting::Toggle(b)) => super::TracedDefaults {
            traced: Some(*b),
            ..super::TracedDefaults::default()
        },
        Some(TracedSetting::Table(t)) => super::TracedDefaults {
            traced: t.enabled,
            location: t.location,
            timestamp: t.timestamp,
            packed: t.packed,
            boxed: t.boxed,
            code: t.code,
        },
    }
}

/// A token tying generated code to the consumer's manifests, so editing
/// `[package.metadata.oopsie]` (member) or `[workspace.metadata.oopsie]`
/// (root) re-runs the macro. The workspace token is emitted whenever an
/// applicable root exists and is a distinct file from the member manifest,
/// even with no oopsie section yet, so adding one later triggers a rebuild.
pub fn manifest_dep_token() -> proc_macro2::TokenStream {
    let member = quote! {
        const _: &[u8] = include_bytes!(concat!(env!(#MANIFEST_ENV_VAR), "/Cargo.toml"));
    };
    let workspace = match cached_workspace_root() {
        Ok(Some(root)) => {
            let differs = member_dir().map_or(true, |dir| dir.join("Cargo.toml") != *root);
            if differs {
                workspace_dep_token(root).unwrap_or_else(|e| compile_error(&e))
            } else {
                quote! {}
            }
        }
        Ok(None) | Err(_) => quote! {},
    };
    quote! { #member #workspace }
}

/// `include_bytes!` takes a string literal, so a workspace root whose path is
/// not valid UTF-8 cannot be tracked at all; say so rather than emitting a
/// lossy path that names no file.
fn workspace_dep_token(root: &Path) -> Result<proc_macro2::TokenStream, String> {
    let root_path = root.to_str().ok_or_else(|| {
        format!(
            "workspace root {} is not a valid UTF-8 path, so oopsie cannot track it \
             for rebuilds; edits to [workspace.metadata.oopsie] would go unnoticed",
            root.display()
        )
    })?;
    Ok(quote! {
        const _: &[u8] = include_bytes!(#root_path);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_section_is_default() {
        assert_eq!(
            parse_settings("[package]\nname = \"x\"\n")
                .unwrap()
                .max_size,
            None
        );
    }

    #[test]
    fn reads_max_size() {
        let settings = parse_settings("[package.metadata.oopsie]\nmax-size = 64\n").unwrap();
        assert_eq!(settings.max_size, Some(64));
    }

    #[test]
    fn reads_max_size_from_every_table_spelling() {
        for toml in [
            "[package.metadata.oopsie]\nmax-size = 64\n",
            "[package.metadata]\noopsie = { max-size = 64 }\n",
            "[package]\nmetadata.oopsie = { max-size = 64 }\n",
            "[package]\nmetadata.oopsie.max-size = 64\n",
            "[package]\nmetadata = { oopsie = { max-size = 64 } }\n",
        ] {
            assert_eq!(
                parse_settings(toml).unwrap().max_size,
                Some(64),
                "spelling: {toml}"
            );
        }
    }

    #[test]
    fn workspace_reads_max_size_from_inline_table() {
        assert_eq!(
            parse_workspace_settings("[workspace.metadata]\noopsie = { max-size = 16 }\n")
                .unwrap()
                .max_size,
            Some(16)
        );
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(parse_settings("[package.metadata.oopsie]\nfoo = 1\n").is_err());
    }

    #[test]
    fn rejects_non_integer() {
        assert!(parse_settings("[package.metadata.oopsie]\nmax-size = \"big\"\n").is_err());
    }

    #[test]
    fn validate_cap_rejects_zero() {
        assert!(validate_cap(Some((0, CapSource::Package))).is_err());
        assert_eq!(
            validate_cap(Some((8, CapSource::Package))).unwrap(),
            Some((8, "[package.metadata.oopsie] max-size"))
        );
        assert_eq!(
            validate_cap(Some((8, CapSource::Workspace))).unwrap(),
            Some((8, "[workspace.metadata.oopsie] max-size"))
        );
        assert_eq!(validate_cap(None).unwrap(), None);
    }

    fn naming(toml: &str) -> Result<super::super::NamingDefaults, String> {
        resolve_naming(
            &parse_settings(toml).expect("valid toml"),
            "[package.metadata.oopsie]",
        )
    }

    #[test]
    fn module_table_suffix() {
        let n = naming("[package.metadata.oopsie]\nmodule = { suffix = \"errors\" }\n").unwrap();
        assert_eq!(n.module_suffix.as_deref(), Some("errors"));
        // A table without `enabled` leaves the per-kind default in place.
        assert_eq!(n.module, None);
    }

    #[test]
    fn module_bool_and_enabled_forms() {
        assert_eq!(
            naming("[package.metadata.oopsie]\nmodule = false\n")
                .unwrap()
                .module,
            Some(false)
        );
        let t = naming("[package.metadata.oopsie]\nmodule = { enabled = true, suffix = \"e\" }\n")
            .unwrap();
        assert_eq!(t.module, Some(true));
        assert_eq!(t.module_suffix.as_deref(), Some("e"));
    }

    #[test]
    fn rejects_bad_module_suffix() {
        assert!(
            naming("[package.metadata.oopsie]\nmodule = { suffix = \"has space\" }\n").is_err()
        );
    }

    /// A type named exactly `Error` strips to an empty stem, leaving the
    /// suffix as the whole module name.
    #[test]
    fn rejects_module_suffix_that_cannot_stand_alone() {
        for suffix in ["2024", "_"] {
            assert!(
                naming(&format!(
                    "[package.metadata.oopsie]\nmodule = {{ suffix = \"{suffix}\" }}\n"
                ))
                .is_err(),
                "suffix: {suffix}"
            );
        }
        for suffix in ["_e", "e2024"] {
            assert!(
                naming(&format!(
                    "[package.metadata.oopsie]\nmodule = {{ suffix = \"{suffix}\" }}\n"
                ))
                .is_ok(),
                "suffix: {suffix}"
            );
        }
    }

    #[test]
    fn rejects_unknown_module_key() {
        assert!(parse_settings("[package.metadata.oopsie]\nmodule = { bogus = 1 }\n").is_err());
    }

    #[test]
    fn default_suffix_forms() {
        use super::super::SuffixDefault;
        let off = naming("[package.metadata.oopsie]\ndefault-suffix = false\n").unwrap();
        assert!(matches!(off.suffix, Some(SuffixDefault::Off)));
        let named = naming("[package.metadata.oopsie]\ndefault-suffix = \"Ctx\"\n").unwrap();
        assert!(matches!(named.suffix, Some(SuffixDefault::Name(s)) if s == "Ctx"));
        // `true` is ambiguous and rejected.
        assert!(naming("[package.metadata.oopsie]\ndefault-suffix = true\n").is_err());
    }

    #[test]
    fn parses_default_vis() {
        let ok = naming("[package.metadata.oopsie]\ndefault-vis = \"pub(crate)\"\n").unwrap();
        assert!(ok.vis.is_some());
        assert!(naming("[package.metadata.oopsie]\ndefault-vis = \"bogus\"\n").is_err());
    }

    #[test]
    fn traced_bool_and_table_forms() {
        assert!(matches!(
            parse_settings("[package.metadata.oopsie]\ntraced = true\n")
                .unwrap()
                .traced,
            Some(TracedSetting::Toggle(true))
        ));
        let table = parse_settings(
            "[package.metadata.oopsie]\n[package.metadata.oopsie.traced]\nlocation = false\n",
        )
        .unwrap()
        .traced;
        assert!(
            matches!(table, Some(TracedSetting::Table(t)) if t.location == Some(false) && t.enabled.is_none())
        );
    }

    #[test]
    fn rejects_unknown_traced_key() {
        assert!(
            parse_settings(
                "[package.metadata.oopsie]\n[package.metadata.oopsie.traced]\nlocaiton = false\n"
            )
            .is_err()
        );
    }

    // ── workspace settings parsing ────────────────────────────────────

    #[test]
    fn workspace_missing_section_is_default() {
        let settings = parse_workspace_settings("[workspace]\nmembers = [\"a\"]\n").unwrap();
        assert_eq!(settings.max_size, None);
    }

    #[test]
    fn workspace_reads_max_size() {
        let settings =
            parse_workspace_settings("[workspace.metadata.oopsie]\nmax-size = 16\n").unwrap();
        assert_eq!(settings.max_size, Some(16));
    }

    #[test]
    fn workspace_rejects_unknown_key() {
        assert!(parse_workspace_settings("[workspace.metadata.oopsie]\nfoo = 1\n").is_err());
    }

    // ── membership / exclude globs (path strings only, no FS) ──────────

    fn doc(toml: &str) -> DocumentMut {
        DocumentMut::from_str(toml).expect("valid toml")
    }

    #[test]
    fn member_excluded_plain_member_is_not_excluded() {
        let root = Path::new("/ws");
        let root_doc = doc("[workspace]\nmembers = [\"crates/*\"]\n");
        assert!(!member_excluded(root, &root_doc, Path::new("/ws/crates/a")));
    }

    #[test]
    fn member_excluded_matches_exclude() {
        let root = Path::new("/ws");
        let root_doc = doc("[workspace]\nexclude = [\"vendor\"]\n");
        assert!(member_excluded(root, &root_doc, Path::new("/ws/vendor")));
    }

    #[test]
    fn member_excluded_explicit_member_beats_exclude() {
        let root = Path::new("/ws");
        let root_doc = doc("[workspace]\nexclude = [\"crates/*\"]\nmembers = [\"crates/keep\"]\n");
        assert!(!member_excluded(
            root,
            &root_doc,
            Path::new("/ws/crates/keep")
        ));
        assert!(member_excluded(
            root,
            &root_doc,
            Path::new("/ws/crates/drop")
        ));
    }

    #[test]
    fn member_excluded_glob_member_does_not_beat_exclude() {
        let root = Path::new("/ws");
        let root_doc =
            doc("[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/legacy\"]\n");
        assert!(member_excluded(
            root,
            &root_doc,
            Path::new("/ws/crates/legacy")
        ));
        assert!(!member_excluded(root, &root_doc, Path::new("/ws/crates/a")));
    }

    #[test]
    fn member_excluded_glob_exclude() {
        let root = Path::new("/ws");
        let root_doc = doc("[workspace]\nexclude = [\"crates/*\"]\n");
        assert!(member_excluded(root, &root_doc, Path::new("/ws/crates/a")));
    }

    #[test]
    fn member_excluded_glob_survives_metachars_in_root_path() {
        let root = Path::new("/dev/[archive]/ws");
        let root_doc = doc("[workspace]\nexclude = [\"vend*\"]\n");
        assert!(member_excluded(
            root,
            &root_doc,
            Path::new("/dev/[archive]/ws/vendor")
        ));
        assert!(!member_excluded(
            root,
            &root_doc,
            Path::new("/dev/[archive]/ws/crates/a")
        ));
    }

    #[test]
    fn member_excluded_glob_is_case_sensitive() {
        let root = Path::new("/ws");
        let root_doc = doc("[workspace]\nexclude = [\"Vendor/*\"]\n");
        assert!(member_excluded(root, &root_doc, Path::new("/ws/Vendor/a")));
        assert!(!member_excluded(root, &root_doc, Path::new("/ws/vendor/a")));
    }

    #[test]
    fn member_excluded_outside_root_is_member() {
        let root = Path::new("/ws");
        let root_doc = doc("[workspace]\nexclude = [\"a\"]\n");
        assert!(!member_excluded(root, &root_doc, Path::new("/elsewhere/a")));
    }

    // ── cap origin selection ──────────────────────────────────────────

    #[test]
    fn cap_origin_package_wins() {
        let package = parse_settings("[package.metadata.oopsie]\nmax-size = 16\n").unwrap();
        let workspace =
            parse_workspace_settings("[workspace.metadata.oopsie]\nmax-size = 64\n").unwrap();
        let (n, source) = select_cap(&package, &workspace).unwrap();
        assert_eq!(n, 16);
        assert_eq!(source.section(), "[package.metadata.oopsie] max-size");
    }

    #[test]
    fn cap_origin_falls_back_to_workspace() {
        let package = parse_settings("[package]\nname = \"x\"\n").unwrap();
        let workspace =
            parse_workspace_settings("[workspace.metadata.oopsie]\nmax-size = 64\n").unwrap();
        let (n, source) = select_cap(&package, &workspace).unwrap();
        assert_eq!(n, 64);
        assert_eq!(source.section(), "[workspace.metadata.oopsie] max-size");
    }

    // ── leaf-by-leaf merge ────────────────────────────────────────────

    #[test]
    fn traced_merge_workspace_fills_unset() {
        // Spec example: workspace `traced = true` + package
        // `traced = { timestamp = true }` resolves to both on.
        let package = resolve_traced(
            &parse_settings("[package.metadata.oopsie]\ntraced = { timestamp = true }\n").unwrap(),
        );
        let workspace = resolve_traced(
            &parse_workspace_settings("[workspace.metadata.oopsie]\ntraced = true\n").unwrap(),
        );
        let merged = package.merge_over(workspace);
        assert_eq!(merged.traced, Some(true));
        assert_eq!(merged.timestamp, Some(true));
    }

    #[test]
    fn traced_merge_package_leaf_wins() {
        let package = super::super::TracedDefaults {
            location: Some(true),
            ..super::super::TracedDefaults::default()
        };
        let workspace = super::super::TracedDefaults {
            location: Some(false),
            timestamp: Some(true),
            ..super::super::TracedDefaults::default()
        };
        let merged = package.merge_over(workspace);
        assert_eq!(merged.location, Some(true));
        assert_eq!(merged.timestamp, Some(true));
    }

    #[test]
    fn naming_merge_workspace_fills_unset() {
        use super::super::SuffixDefault;
        let package = resolve_naming(
            &parse_settings("[package.metadata.oopsie]\nmodule = false\n").unwrap(),
            "[package.metadata.oopsie]",
        )
        .unwrap();
        let workspace = resolve_naming(
            &parse_workspace_settings("[workspace.metadata.oopsie]\ndefault-suffix = \"Ctx\"\n")
                .unwrap(),
            "[workspace.metadata.oopsie]",
        )
        .unwrap();
        let merged = package.merge_over(workspace);
        assert_eq!(merged.module, Some(false));
        assert!(matches!(merged.suffix, Some(SuffixDefault::Name(s)) if s == "Ctx"));
    }

    // ── rebuild-tracking token ────────────────────────────────────────

    #[test]
    fn workspace_dep_token_tracks_utf8_root() {
        assert!(workspace_dep_token(Path::new("/ws/Cargo.toml")).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn workspace_dep_token_reports_non_utf8_root() {
        use std::os::unix::ffi::OsStrExt as _;

        let root = PathBuf::from(std::ffi::OsStr::from_bytes(b"/ws/\xffbad/Cargo.toml"));
        assert!(workspace_dep_token(&root).is_err());
    }

    // ── workspace-root discovery over a temp FS tree ──────────────────

    const VIRTUAL_ROOT: &str = "[workspace]\nresolver = \"2\"\n";
    const PACKAGE: &str = "[package]\nname = \"member\"\nversion = \"0.0.0\"\nedition = \"2024\"\n";

    fn write_manifest(dir: &Path, contents: &str) {
        std::fs::create_dir_all(dir).expect("create dir tree");
        std::fs::write(dir.join("Cargo.toml"), contents).expect("write Cargo.toml");
    }

    // macOS `tempfile` dirs live under a `/var → /private/var` symlink, and
    // `find_root_from`'s path comparisons (and the production caller) assume
    // canonical paths, so derive every test path from the canonical root.
    fn canonical_root(tmp: &tempfile::TempDir) -> PathBuf {
        std::fs::canonicalize(tmp.path()).expect("canonicalize temp root")
    }

    #[test]
    fn discovery_virtual_root_found() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        write_manifest(&root, "[workspace]\nmembers = [\"member\"]\n");
        let member = root.join("member");
        write_manifest(&member, PACKAGE);
        assert_eq!(
            find_root_from(&member, None, None).unwrap(),
            Some(root.join("Cargo.toml"))
        );
    }

    #[test]
    fn discovery_root_package_self_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        let member = root.join("member");
        write_manifest(&member, &format!("{PACKAGE}{VIRTUAL_ROOT}"));
        assert_eq!(
            find_root_from(&member, None, None).unwrap(),
            Some(member.join("Cargo.toml"))
        );
    }

    #[test]
    fn discovery_follows_workspace_pointer() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        write_manifest(&root.join("root"), VIRTUAL_ROOT);
        let member = root.join("member");
        write_manifest(&member, &format!("{PACKAGE}workspace = \"../root\"\n"));
        let expected = std::fs::canonicalize(root.join("root").join("Cargo.toml")).unwrap();
        assert_eq!(find_root_from(&member, None, None).unwrap(), Some(expected));
    }

    #[test]
    fn discovery_pointer_to_non_workspace_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        write_manifest(&root.join("root"), PACKAGE);
        let member = root.join("member");
        write_manifest(&member, &format!("{PACKAGE}workspace = \"../root\"\n"));
        assert!(find_root_from(&member, None, None).is_err());
    }

    #[test]
    fn discovery_standalone_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        let member = root.join("member");
        write_manifest(&member, PACKAGE);
        // Bound the walk just above the member so no real ancestor manifest leaks in.
        assert_eq!(find_root_from(&member, None, Some(&root)).unwrap(), None);
    }

    #[test]
    fn discovery_excluded_member_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        write_manifest(&root, "[workspace]\nexclude = [\"member\"]\n");
        let member = root.join("member");
        write_manifest(&member, PACKAGE);
        assert_eq!(find_root_from(&member, None, None).unwrap(), None);
    }

    #[test]
    fn discovery_nested_excluded_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        write_manifest(&root, "[workspace]\nexclude = [\"vendor\"]\n");
        let member = root.join("vendor").join("inner");
        write_manifest(&member, PACKAGE);
        assert_eq!(find_root_from(&member, None, None).unwrap(), None);
    }

    #[test]
    fn discovery_explicit_member_beats_exclude() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        write_manifest(
            &root,
            "[workspace]\nmembers = [\"crates/foo\"]\nexclude = [\"crates/foo\"]\n",
        );
        let member = root.join("crates").join("foo");
        write_manifest(&member, PACKAGE);
        assert_eq!(
            find_root_from(&member, None, None).unwrap(),
            Some(root.join("Cargo.toml"))
        );
    }

    #[test]
    fn discovery_target_package_stops_walk() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        write_manifest(&root, "[workspace]\nmembers = [\"crates/*\"]\n");
        let member = root.join("target").join("package").join("staged-0.0.0");
        write_manifest(&member, PACKAGE);
        assert_eq!(find_root_from(&member, None, None).unwrap(), None);
    }

    #[test]
    fn discovery_cargo_home_stops_walk() {
        let tmp = tempfile::tempdir().unwrap();
        let home = canonical_root(&tmp);
        write_manifest(&home, VIRTUAL_ROOT);
        let member = home.join("registry").join("src").join("x").join("crate");
        write_manifest(&member, PACKAGE);
        assert_eq!(find_root_from(&member, None, Some(&home)).unwrap(), None);
    }

    #[test]
    fn discovery_workspace_override_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let root = canonical_root(&tmp);
        let override_dir = root.join("forced");
        write_manifest(&override_dir, VIRTUAL_ROOT);
        let member = root.join("member");
        write_manifest(&member, PACKAGE);
        assert_eq!(
            find_root_from(&member, Some(&override_dir), Some(&root)).unwrap(),
            Some(override_dir.join("Cargo.toml"))
        );
    }
}
