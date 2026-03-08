# Replace snafu with native oopsie infrastructure

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove the `snafu` dependency entirely, replacing it with oopsie's own traits, types, and derive macro.

**Architecture:** The `#[derive(Oopsie)]` derive macro replaces `#[derive(Snafu)]`, generating context selectors, `Display`, `Error` (with optional `provide()`), `IntoError`, and `From` impls. The `#[oopsie]` attribute macro now implicitly adds `#[derive(Oopsie)]` and injects fields as before. New traits (`IntoError`, `GenerateImplicitData`, `ResultExt`, `OptionExt`) in oopsie-core replace snafu's versions.

**Tech Stack:** Rust nightly (edition 2024), syn 2, quote 1, darling 0.23, proc-macro2

**Design doc:** `docs/2026-03-08-replace-snafu-design.md`

---

## Phase 1: Core Traits (oopsie-core)

### Task 1: Add core traits module

Add `IntoError`, `GenerateImplicitData`, `NoneError`, `ResultExt`, `OptionExt` to oopsie-core. These are standalone — they don't touch existing code yet.

**Files:**
- Create: `crates/oopsie-core/src/traits.rs`
- Modify: `crates/oopsie-core/src/lib.rs` (add `mod traits; pub use traits::*;`)

**Step 1: Write the traits module**

```rust
// crates/oopsie-core/src/traits.rs

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
```

**Step 2: Wire up in lib.rs**

Add `mod traits;` and `pub use traits::*;` to `crates/oopsie-core/src/lib.rs`. Remove the line `pub use snafu::{OptionExt, ResultExt};`.

**Step 3: Run `cargo check -p oopsie-core`**

Expected: May fail because `backtrace.rs` and `spantrace.rs` still reference `snafu::GenerateImplicitData`. That's expected — we fix those in Task 2.

**Step 4: Commit**

```
feat(core): add IntoError, GenerateImplicitData, ResultExt, OptionExt traits
```

---

### Task 2: Migrate Backtrace and Spantrace to own GenerateImplicitData

Update `backtrace.rs` and `spantrace.rs` to implement oopsie's `GenerateImplicitData` instead of snafu's.

**Files:**
- Modify: `crates/oopsie-core/src/backtrace.rs`
- Modify: `crates/oopsie-core/src/spantrace.rs`

**Step 1: Update backtrace.rs**

Change `impl snafu::GenerateImplicitData for Backtrace` to `impl GenerateImplicitData for Backtrace`, using `use crate::GenerateImplicitData;`.

**Step 2: Update spantrace.rs**

Change all `impl snafu::GenerateImplicitData for ...` to use `crate::GenerateImplicitData` for both `Spantrace` and `OptionalSpanTrace`. The trait signature is identical so the method bodies stay the same.

**Step 3: Remove snafu from oopsie-core**

- Remove `pub use snafu::{OptionExt, ResultExt};` from lib.rs (done in Task 1)
- Remove `pub mod private { pub use snafu; }` from lib.rs
- Run `cargo rm snafu -p oopsie-core` to remove snafu dependency
- Note: The test at spantrace.rs line 505 uses `#[derive(Snafu)]` and `#[snafu(provide(...))]` — this test must be rewritten to use `#[derive(Oopsie)]` later (Task 8). For now, **comment it out or gate it behind `#[cfg(any())]`** with a TODO.

**Step 4: Run `cargo check -p oopsie-core`**

Expected: PASS. oopsie-core should now be snafu-free.

**Step 5: Run `cargo nextest run -p oopsie-core`**

Expected: All tests pass except the commented-out provide test.

**Step 6: Commit**

```
refactor(core): replace snafu traits with native GenerateImplicitData
```

---

## Phase 2: Derive Macro (oopsie-macros)

This is the largest phase. We build `#[derive(Oopsie)]` which generates context selectors, Display, Error, IntoError, and From impls.

### Task 3: Scaffold the derive macro and attribute parsing

Add the `#[derive(Oopsie)]` proc-macro entry point and parse the new `#[oopsie(...)]` attribute syntax.

**Files:**
- Modify: `crates/oopsie-macros/src/lib.rs` (add derive macro entry point)
- Create: `crates/oopsie-macros/src/derive/mod.rs` (main derive dispatcher)
- Create: `crates/oopsie-macros/src/derive/parse.rs` (attribute parsing)

**Step 1: Add derive macro to lib.rs**

Add `mod derive;` and the `#[proc_macro_derive(Oopsie, attributes(oopsie))]` entry point that routes to `derive::expand()`.

```rust
mod derive;

#[proc_macro_derive(Oopsie, attributes(oopsie))]
pub fn oopsie_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match derive::expand(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
```

**Step 2: Create derive/parse.rs**

Parse the `#[oopsie(...)]` attributes at three levels:

**Container-level** (on enum/struct):
- `module` / `module(name)` / `module(false)` — module wrapping (default for enums: on)
- `vis = <visibility>` — default selector visibility (default: `pub(crate)`)
- `suffix` / `suffix = "X"` / `suffix(false)` — selector name suffix (default: off)
- `path = "..."` — path to oopsie crate (default: `::oopsie`)

**Variant/struct-level** (determines Display + behavior):
- First positional string + args = display (short form)
- `display("format", args...)` — display (long form)
- `transparent` — generate `From` instead of context selector
- `help = "..."` — help text
- `code = "..."` — error code

**Field-level:**
- `from` — marks as source field
- `from(Type, transform)` — source with transformation
- `auto` — implicit data generation
- `provide(ref, Type => expr)` — provider API (behind `unstable`)

Use `syn` for parsing, NOT darling for the derive attributes (darling is for the attribute macro args which use a different format). Define types:

```rust
struct ContainerAttrs {
    module: ModuleSetting,        // On(Option<Ident>) | Off
    visibility: Option<Visibility>,
    suffix: SuffixSetting,        // Off | Default("Oopsie") | Custom(String)
    path: Option<Path>,           // default ::oopsie
}

struct VariantAttrs {
    display: Option<DisplayAttr>, // (format_str, args)
    transparent: bool,
    help: Option<String>,
    code: Option<String>,
}

struct FieldAttrs {
    from: SourceKind,             // No | Yes | Transformed(Type, Expr)
    auto: bool,
    provide: Vec<ProvideAttr>,
}
```

Parse `#[oopsie(...)]` attributes by iterating over `attrs` on DeriveInput, variants, and fields. A field named `source` with an error-like type automatically gets `from: SourceKind::Yes`.

**Step 3: Create derive/mod.rs**

Skeleton `expand()` function that:
1. Parses `DeriveInput`
2. Extracts container attrs
3. Routes to enum vs struct expansion (stubs for now returning empty `TokenStream`)

**Step 4: Run `cargo check -p oopsie-macros`**

Expected: PASS (derive is registered, parsing compiles, expansion stubs return empty tokens).

**Step 5: Commit**

```
feat(macros): scaffold #[derive(Oopsie)] with attribute parsing
```

---

### Task 4: Generate context selectors

For each non-transparent variant, generate the context selector struct with `build()`, `fail()`, and `IntoError` impl.

**Files:**
- Create: `crates/oopsie-macros/src/derive/gen_selectors.rs`
- Modify: `crates/oopsie-macros/src/derive/mod.rs` (call selector generation)

**Step 1: Write selector generation**

For each variant, categorize fields:
- **source field**: detected by name `source` or `#[oopsie(from)]`
- **auto fields**: marked with `#[oopsie(auto)]`
- **user fields**: everything else (these become selector fields)

Generate:
1. Selector struct with generic type params for user fields
2. `IntoError` impl (when source exists)
3. `build()` and `fail()` methods (when no source — leaf error)
4. For transparent variants: `From` impl instead

Use the oopsie crate path (from container attrs, default `::oopsie`) for referencing `IntoError`, `GenerateImplicitData`, `NoneError`.

Source transformation (`#[oopsie(from(Type, transform))]`): In `IntoError`, `type Source = Type` and the body applies `(transform)(source)` before constructing the error.

**Step 2: Wire into mod.rs**

Call selector generation from `expand()`, collect tokens for all variants.

**Step 3: Write a basic compile-test**

Create a test in `crates/oopsie-macros/src/derive/mod.rs` (or a separate test file) that defines a simple error enum with `#[derive(Oopsie)]` and verifies it compiles and the selectors work:

```rust
#[test]
fn test_basic_selectors() {
    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    enum TestError {
        #[oopsie("Not found: {path}")]
        NotFound { path: String },
    }

    let err = NotFound { path: "test" }.build();
    assert!(matches!(err, TestError::NotFound { .. }));
}
```

Note: This test will also need Display and Error generation (Task 5) to work. It's okay to implement Tasks 4 and 5 together and test after both.

**Step 4: Commit**

```
feat(macros): generate context selectors and IntoError impls
```

---

### Task 5: Generate Display and Error impls

**Files:**
- Create: `crates/oopsie-macros/src/derive/gen_display.rs`
- Create: `crates/oopsie-macros/src/derive/gen_error.rs`
- Modify: `crates/oopsie-macros/src/derive/mod.rs`

**Step 1: Generate Display impl**

For each variant, parse the display format string and args from `VariantAttrs::display`. Generate a `match` arm that destructures the variant and calls `write!(f, ...)`.

Handle short form: `#[oopsie("format {field}")]` — extract field references from the format string.
Handle long form: `#[oopsie(display("format {}", expr))]` — use explicit args.

If no display attribute is provided, fall back to the variant name as the display string.

**Step 2: Generate Error impl**

Generate `impl std::error::Error for EnumName`:
- `source()`: for each variant, if it has a source field, return `Some(source)`. Otherwise `None`.
- `provide()` (behind `#[cfg(feature = "unstable")]`): for each variant, forward source's provide, then provide any `#[oopsie(provide(...))]` fields.

The `provide()` method generation should:
- Call `source.provide(request)` for source fields
- Call `request.provide_ref::<Type>(expr)` for `#[oopsie(provide(ref, Type => expr))]`
- Call `request.provide_value::<Type>(expr)` for `#[oopsie(provide(Type => expr))]`

**Step 3: Write integration tests**

```rust
#[test]
fn test_display_and_error() {
    use std::error::Error;

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    enum TestError {
        #[oopsie("Something failed: {msg}")]
        Basic { msg: String },

        #[oopsie(display("IO: {}", source))]
        Io { source: std::io::Error },
    }

    let err = Basic { msg: "test" }.build();
    assert_eq!(err.to_string(), "Something failed: test");
    assert!(err.source().is_none());

    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
    let err = Io.into_error(io_err);
    assert!(err.source().is_some());
}
```

**Step 4: Run `cargo nextest run -p oopsie-macros`**

Expected: PASS

**Step 5: Commit**

```
feat(macros): generate Display and Error trait impls
```

---

### Task 6: Module wrapping and struct support

**Files:**
- Create: `crates/oopsie-macros/src/derive/gen_module.rs`
- Modify: `crates/oopsie-macros/src/derive/mod.rs`

**Step 1: Module wrapping for enums**

When `module` is enabled (default for enums), wrap all generated context selectors in a module:

```rust
mod module_name {
    use super::*;
    // ... selector structs, IntoError impls, build/fail methods
}
```

Module name defaults to snake_case of the enum name. Custom name via `#[oopsie(module(custom_name))]`.

**Step 2: Struct support**

For structs, generate:
- A single context selector struct (same name as the error struct, or with suffix)
- No module wrapping
- Same Display/Error/IntoError generation as enum variants

**Step 3: Suffix support**

When `suffix` is enabled:
- Default suffix: `"Oopsie"` (e.g., `ConnectionOopsie`)
- Custom: `#[oopsie(suffix = "Error")]` → `ConnectionError`
- Off (default): selector name = variant name

**Step 4: Write tests**

Test module wrapping:
```rust
#[derive(Debug, Oopsie)]
enum AppError {
    #[oopsie("Not found")]
    NotFound,
}

// Should work via module:
let err = app_error::NotFound.build();
```

Test struct:
```rust
#[derive(Debug, Oopsie)]
#[oopsie("Something broke: {reason}")]
struct Broken { reason: String }

let err = Broken { reason: "test" }.build();
assert_eq!(err.to_string(), "Something broke: test");
```

**Step 5: Run `cargo nextest run -p oopsie-macros`**

**Step 6: Commit**

```
feat(macros): add module wrapping, struct support, and suffix option
```

---

### Task 7: Update `#[oopsie]` attribute macro

The `#[oopsie]` attribute macro must now:
1. Implicitly add `#[derive(Oopsie)]` (instead of checking for `#[derive(Snafu)]`)
2. Replace `#[snafu(...)]` attribute injection with `#[oopsie(...)]` attributes
3. Parse the new syntax for `help`, `code`, `transparent` etc.
4. Remove all snafu-specific code (snafu_attrs.rs, type_check.rs's check_derive_snafu)

**Files:**
- Modify: `crates/oopsie-macros/src/error/mod.rs`
- Modify: `crates/oopsie-macros/src/error/inject.rs`
- Modify: `crates/oopsie-macros/src/error/config.rs`
- Modify: `crates/oopsie-macros/src/error/expand_enum.rs`
- Modify: `crates/oopsie-macros/src/error/expand_struct.rs`
- Modify: `crates/oopsie-macros/src/error/type_check.rs`
- Delete: `crates/oopsie-macros/src/error/snafu_attrs.rs`
- Modify: `crates/oopsie-macros/src/error/args.rs` (add `help`, `code` support to attribute macro args)

**Step 1: Update attribute macro to add `#[derive(Oopsie)]`**

In `expand_enum.rs` and `expand_struct.rs`, instead of `check_derive_snafu()`, add `#[derive(Oopsie)]` to the derives list if not already present. Remove `check_derive_snafu()` from `type_check.rs`.

**Step 2: Replace snafu attribute injection with oopsie attributes**

In `inject.rs`, change all generated attributes from `#[snafu(implicit)]` to `#[oopsie(auto)]`, from `#[snafu(provide(...))]` to `#[oopsie(provide(...))]`.

In `expand_enum.rs`, replace:
- `#[snafu(module(...))]` → `#[oopsie(module(...))]`
- `#[snafu(context(suffix(false)))]` → `#[oopsie(suffix(false))]` (or just don't add it since off is the default)
- `#[snafu(visibility(...))]` → `#[oopsie(vis = ...)]`

In `expand_struct.rs`, same replacements.

**Step 3: Add help/code to attribute macro args**

Move `#[help("...")]` and `#[code("...")]` parsing into the oopsie attribute. The attribute macro should extract these from variants and convert them to `#[oopsie(help = "...", code = "...")]` attributes (which the derive will then use for provide generation).

Alternatively, the attribute macro can directly generate `#[oopsie(provide(...))]` attributes for help/code as it does today — just with the new attribute name.

**Step 4: Delete snafu_attrs.rs**

This file parsed `#[snafu(module)]`, `#[snafu(visibility)]`, `#[snafu(context)]`. No longer needed.

**Step 5: Update tests in error/mod.rs**

All existing tests check for `#[snafu(...)]` in the output — update them to check for `#[oopsie(...)]` instead. Update assertions that check for `#[derive(Snafu)]` → should now check for or generate `#[derive(Oopsie)]`.

**Step 6: Run `cargo nextest run -p oopsie-macros`**

**Step 7: Commit**

```
refactor(macros): update #[oopsie] to emit #[oopsie(...)] attrs instead of #[snafu(...)]
```

---

## Phase 3: Integration

### Task 8: Remove snafu from oopsie-daisy

**Files:**
- Modify: `crates/oopsie-daisy/src/erased.rs`
- Modify: `crates/oopsie-daisy/src/fancy_report.rs`
- Modify: `crates/oopsie-daisy/Cargo.toml`

**Step 1: Update test error types**

Replace all `#[derive(Debug, Snafu)]` + `#[snafu(...)]` with `#[oopsie]` + `#[oopsie(...)]` syntax in test code:

```rust
// Before:
#[oopsie(path = "crate")]
#[derive(Debug, Snafu)]
#[snafu(display("Something went wrong: {message}"), visibility(pub))]
pub struct ErrorWithHelp { message: String }

// After:
#[oopsie(path = "crate")]
#[derive(Debug)]
#[oopsie(display("Something went wrong: {message}"), vis = pub)]
pub struct ErrorWithHelp { message: String }
```

Note: `#[oopsie]` (attribute macro) will add `#[derive(Oopsie)]` automatically.

**Step 2: Update imports**

- Remove `use snafu::{IntoError as _, Snafu};` → use `use oopsie_core::IntoError as _;`
- Remove `snafu::GenerateImplicitData` → `oopsie_core::GenerateImplicitData`
- Any other snafu references

**Step 3: Remove snafu dependency**

```bash
cargo rm snafu -p oopsie-daisy
```

**Step 4: Run `cargo nextest run -p oopsie-daisy`**

Snapshots may need updating with `cargo insta accept --all`.

**Step 5: Commit**

```
refactor(daisy): remove snafu dependency, use native oopsie derives
```

---

### Task 9: Remove snafu from workspace and update oopsie-core test

**Files:**
- Modify: `Cargo.toml` (workspace root — remove snafu from workspace.dependencies)
- Modify: `crates/oopsie-core/src/spantrace.rs` (rewrite the commented-out provide test)

**Step 1: Rewrite the provide test in spantrace.rs**

The test at ~line 505 that used `#[derive(Snafu)]` with `#[snafu(provide(...))]` should now use:

```rust
#[cfg(feature = "unstable")]
#[test]
fn test_extract_boxed_spantrace_via_provide_ref() {
    #[derive(Debug, Oopsie)]
    #[oopsie("Boxed spantrace error", module(false))]
    struct BoxedSpantraceError {
        #[oopsie(auto)]
        span: Box<Spantrace>,
        #[oopsie(provide(ref, Spantrace => span.as_ref()))]
    }

    let err = BoxedSpantraceError.build();
    let extracted = Spantrace::extract(&err);
    assert!(extracted.is_some());
}
```

Note: oopsie-core tests depend on oopsie-macros for the derive. Add `oopsie-macros` to oopsie-core's dev-dependencies.

**Step 2: Remove snafu from workspace**

Remove `snafu = { ... }` from `[workspace.dependencies]` in root `Cargo.toml`.

**Step 3: Run `cargo check` (full workspace)**

Expected: PASS — no crate should reference snafu anymore.

**Step 4: Run `cargo nextest run` (full workspace)**

Expected: All tests pass.

**Step 5: Commit**

```
refactor: remove snafu from workspace entirely
```

---

### Task 10: Update facade crate and add prelude

**Files:**
- Modify: `crates/oopsie/src/lib.rs`

**Step 1: Update re-exports**

```rust
pub use oopsie_macros::oopsie;
pub use oopsie_macros::Oopsie;  // the derive macro

pub use oopsie_core::*;

#[cfg(feature = "daisy")]
pub use oopsie_daisy::*;
```

**Step 2: Verify `cargo check -p oopsie`**

**Step 3: Commit**

```
feat: re-export Oopsie derive from facade crate
```

---

## Phase 4: Verification

### Task 11: Stable compilation check

**Step 1: Run `cargo +stable check -p oopsie-core --no-default-features`**

Expected: PASS — no nightly features when `unstable` is off.

**Step 2: Run `cargo +stable check -p oopsie-macros`**

Expected: PASS — proc-macro crate should work on stable.

**Step 3: Run `cargo +stable check -p oopsie --no-default-features --features daisy`**

Expected: PASS — full workspace minus unstable features works on stable.

**Step 4: Run `cargo +stable check --no-default-features`**

Expected: PASS — entire workspace compiles on stable.

If any step fails, fix the issue (likely missing `#[cfg(feature = "unstable")]` gates) and re-check.

**Step 5: Run full nightly check with defaults**

```bash
cargo check
cargo nextest run
cargo clippy
```

All must pass.

**Step 6: Commit any fixes**

```
fix: ensure stable rust compilation without unstable feature
```

---

### Task 12: Cleanup and final verification

**Step 1: Search for any remaining snafu references**

```bash
grep -r "snafu" crates/ --include="*.rs" --include="*.toml"
```

Remove any stragglers (comments referencing snafu in non-migration contexts, etc.). Keep the migration table in the design doc.

**Step 2: Run `cargo fmt`**

**Step 3: Run full test suite**

```bash
cargo nextest run
cargo clippy
cargo +stable check --no-default-features
```

**Step 4: Commit**

```
chore: remove remaining snafu references
```

---

## Task Dependency Graph

```
Task 1 (traits) → Task 2 (migrate backtrace/spantrace)
                                    ↓
Task 3 (derive scaffold) → Task 4 (selectors) → Task 5 (Display/Error)
                                                        ↓
                                                Task 6 (module/struct)
                                                        ↓
                                                Task 7 (update #[oopsie] attr macro)
                                                        ↓
                                        Task 8 (remove snafu from daisy)
                                                        ↓
                                        Task 9 (remove snafu from workspace)
                                                        ↓
                                        Task 10 (update facade)
                                                        ↓
                                        Task 11 (stable check)
                                                        ↓
                                        Task 12 (cleanup)
```

Tasks 1-2 and Tasks 3-6 can be done in parallel since they're in different crates. Task 7 merges both streams.

## Key Implementation Notes

- The derive macro (`#[derive(Oopsie)]`) needs to register the `oopsie` helper attribute so field-level `#[oopsie(from)]` etc. don't cause "unknown attribute" errors. This is done via `#[proc_macro_derive(Oopsie, attributes(oopsie))]`.
- The attribute macro `#[oopsie]` and the derive `#[derive(Oopsie)]` both register `oopsie` as an attribute — this works fine because attribute macros consume their attributes before the derive runs.
- The `#[oopsie]` attribute macro runs BEFORE `#[derive(Oopsie)]`. It injects fields and `#[oopsie(...)]` attributes, then the derive processes them.
- For `source` auto-detection: a field named `source` whose type path ends in `Error` or is a known error type should be auto-detected. Conservative approach: detect any field named `source` as a source field.
- `#[track_caller]` on all error construction paths (build, fail, into_error, From) ensures backtraces point to the caller.
- The `provide()` generation must be wrapped in `#[cfg(feature = "unstable")]`. The derive macro uses `cfg!(feature = "unstable")` at macro expansion time (not compile time) — so it should emit `#[cfg(feature = "unstable")]` guards in the generated code, similar to how snafu does it.
