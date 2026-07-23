# Adversarial Audit #2 — oopsie workspace

**Date:** 2026-07-23
**Scope:** `oopsie` (facade), `oopsie-core`, `oopsie-macros`, `nostd-smoke`; README, CI, tooling
**Method:** Five parallel audit agents (core correctness, macro codegen, facade runtime, performance, DX/tests). Every finding below was verified against the code; most carry a runtime or compile-time repro performed in a scratch crate outside the repo.

---

## Executive summary

The codebase is in strong shape: the success path is ~free (1.4 ns vs 1.1 ns baseline), symbolication is lazy and paid once, there is zero `unsafe`, cyclic-chain rendering is capped, `RUST_BACKTRACE`/`RUST_LIB_BACKTRACE` precedence matches std exactly, trybuild diagnostics are extensive and actionable, the full feature matrix compiles, and MSRV gating is credible. No macro ICEs were found.

The real defects cluster in four themes:

1. **Onboarding landmine** — the flagship README quick-start does not compile with the documented install, because `fancy` left the default feature set in rc.18 and neither the README nor the rustdoc feature table was updated. CI structurally cannot catch this: no CI job builds the default feature set.
2. **The crash reporter can crash the process** — the panic hook uses `eprint!`, turning a benign stderr EPIPE into a panic-while-panicking abort (verified: SIGABRT/134 where std's default hook exits 101).
3. **`Welp` is the weak spot of the error path** — every wrap layer pays ~14 µs + 4 allocations for a backtrace that is usually shadowed, and its `Diagnostic` impl silently drops the wrapped error's error code and help text.
4. **Inconsistent defensiveness** — cycles are capped in the serde path but not in `root_cause()`; selector-name shadowing is handled for structs but not enums; `cfg_attr` is blindly forwarded on normal fields and blanket-rejected on source fields; the injected-field collision guard covers only `__oopsie_timestamp`.

---

## High severity

### H1. README quick-start does not compile with the documented install

- **Where:** `README.md:14-40`, `crates/oopsie/src/lib.rs:323` (rustdoc feature table), `.github/workflows/ci.yml:42-45`
- **What:** `cargo add oopsie` yields `default = ["std"]`, but the quick-start's `fn main() -> oopsie::Report<AppError>` / `Report::run` requires `fancy` (`Report` is `#[cfg(feature = "fancy")]`, `crates/oopsie/src/lib.rs:530`). Verified: the exact README snippet fails with `error[E0433]: could not find Report in oopsie … gated behind the fancy feature`. With `features = ["fancy"]` the same snippet compiles and runs correctly — the macro naming (`app_oopsies::Connect`), the `&str` → `String` `Into` conversion, and `Report` as a `main` return type are all real. The example is right; the install instructions are wrong.
- **Since:** rc.18, when `fancy` left the default set (CHANGELOG). Neither the README nor the hand-written rustdoc feature table (which still claims `fancy` is default "yes") was updated. README explicitly defers to the crate docs for feature flags, so both places a user would check mislead about the same thing.
- **Why CI can't catch it:** every CI matrix row uses `--no-default-features`; the configuration a user actually gets from `cargo add oopsie` (`std` alone) is never built in CI.
- **Fix:** document `cargo add oopsie --features fancy` (or make the quick-start feature-complete as written), correct the rustdoc table, and add a CI row that builds/tests the default feature set.

### H2. Panic hook aborts (SIGABRT) when writing the report to stderr fails

- **Where:** `crates/oopsie/src/panic_hook.rs:35` (`eprint!("{}", PanicReport::new(info))`); same class at `crates/oopsie/src/report.rs:333` (`eprintln!("{self}")` in `Termination::report`)
- **What:** `eprint!` panics when the write fails. std swallows EBADF via `handle_ebadf` but **not EPIPE** (stderr piped to a reader that exited). A panic inside a panic hook is panic-while-panicking → the process aborts instead of unwinding.
- **Verified:** scratch binary with `install_panic_hook()` + `panic!()` + stderr piped to an immediately-exiting reader → exit status **134 (SIGABRT)** on every run; std's default hook under identical conditions → **101**. Realistic trigger: `./app 2>&1 | head -c0`.
- **Fix:** write to `io::stderr().lock()` with `let _ = write!(...)`, ignoring errors, as std's default hook does.

### H3. `Welp` captures a full backtrace at every wrap layer, usually wasted

- **Where:** `crates/oopsie-core/src/welp.rs:135,161,196` (all constructors call `Backtrace::capture()` unconditionally); shadowing at `welp.rs:371` (`source_backtrace(&**source).or(Some(&traces.0))`)
- **What:** each `.welp()` / `.welp_context()` pays `backtrace::Backtrace::new_unresolved()` — measured **~13.9 µs** — plus ~4 heap allocations. When the source already carries a trace, the wrap-site capture is discarded at accessor time (source wins). N wrap layers → N stack walks, N−1 wasted. A 3-layer `welp_context` chain pays ~42 µs of stack walking for one usable trace. The typed `#[oopsie]` path avoids this via `CaptureProbe`/`capture_or_extract` (Arc refcount bump); `Welp::wrap` is generic and cannot use the probe.
- **Fix:** on nightly, use `core::error::request_ref::<Backtrace>(&source)` at construction and skip capture when provided; on stable, a `source.downcast_ref::<Welp>()` fast path covers Welp-on-Welp layering.
- **Related data-loss bug:** `impl Diagnostic for Welp` (`welp.rs:367-404`) forwards `oopsie_exit_code` from the source via the Provider API but implements no `oopsie_error_code` / `oopsie_help_text` — wrapping a coded error gives `oopsie_exit_code() == Some(42)` but `oopsie_error_code() == None`, `oopsie_help_text() == None` (verified with `--features unstable-error-generic-member-access`). `ErasedError::from_error_ref(&welp).diagnostics()` comes back empty. Unconditional and undocumented. Fix: add `source_error_code`/`source_help` Provider helpers mirroring `source_exit_code`.

---

## Medium severity

### M1. `root_cause()` hangs forever on a cyclic source chain

- **Where:** `crates/oopsie-core/src/chain.rs:90-96`
- **What:** `while let Some(source) = cause.source() { cause = source; }` — no cap, no cycle warning in its doc. A foreign error with `source() -> Some(self)` (permitted by the `Error` contract) pins the thread at 100% CPU forever (verified: still spinning after 2s). The crate caps the same walk at 128 in `ErasedError::from_error_ref` (`erased/mod.rs:243-255`) and in report rendering (`report.rs:190`), and the `Chain` iterator documents the hazard — `root_cause` is the inconsistent outlier.
- **Fix:** depth cap or visited-set, matching the crate's existing 128-entry convention.

### M2. Panic backtraces silently honor the thread-local override, contradicting the hook's docs — then print a misleading hint

- **Where:** `crates/oopsie/src/panic_hook.rs:94` (calls `oopsie_core::rust_panic_backtrace()`, which checks the thread-local override first, `oopsie-core/src/backtrace.rs:148-153`) vs docs at `panic_hook.rs:25-28`, field comment `panic_hook.rs:83-85`, and `lib.rs:443` ("honor `RUST_BACKTRACE` only")
- **What (verified):** `RUST_BACKTRACE=1` + `backtrace::set_override(Disabled)` + panic → no backtrace rendered, and the hook prints `note: run with RUST_BACKTRACE=1 to display a backtrace` — even though `RUST_BACKTRACE=1` *is* set. Either the docs or the behavior is wrong; if intentional, the hint must not fire when the suppressor is the override.

### M3. Selector name can shadow the enum itself inside the generated module

- **Where:** `crates/oopsie-macros/src/derive/gen_selectors.rs:530-531`, `gen_module.rs:41`; collision check at `model.rs:84-110`
- **What:** the generated module does `use super::*;` and names the error type unqualified (`#enum_ident::#variant_ident`). A selector struct shadows the glob-imported enum when they share a name: `enum Config { Config }`, or `enum Read { ReadError }` → selector `Read` after `Error`-stripping. Verified repros produce E0223 (ambiguous associated type) pointing into generated code, including via the `transparent` `From` impl; with `module(false)` it's E0428 + E0119. The struct path already solved this with `super::`-qualification (`gen_selectors.rs:588-601`); the enum path did not. The collision check only compares selector names against each other, never against the enum name.
- **Fix:** emit `super::#enum_ident::#variant_ident` when wrapped; targeted error when `module(false)` and names coincide.

### M4. `cfg_attr` with derive-helper attributes on a field breaks generated selectors when active

- **Where:** `crates/oopsie-macros/src/derive/parse.rs:1631-1638` (collects `cfg_attr` verbatim), forwarded at `gen_selectors.rs:95`, `model.rs:328-343`; blanket rejection for source fields at `model.rs:207-218`
- **What:** `#[cfg_attr(all(), serde(skip))] v: u32` on an error field → `error: cannot find attribute serde in this scope` on the selector field (which has no `derive(Serialize)`). `cfg_attr(feature = "serde", serde(...))` on error fields is a realistic pattern. Asymmetry: source fields reject *all* `cfg_attr` with a message that names `#[cfg(...)]` — which the user never wrote; other fields forward it blindly.
- **Fix:** forward only `cfg_attr`s whose metas are themselves `cfg(...)`; correct the source-field error message.

### M5. Every `Report` render re-demangles and re-allocates every frame string

- **Where:** `crates/oopsie/src/trace_printer.rs:115-130` (per render: fresh demangle + `into_boxed_str()` per symbol, `Box<Path>` per frame), `:729` (second `Vec` alloc), `:746-750` (`std::env::current_dir()` syscall + alloc on **every** render)
- **What:** a 30-frame trace with inline expansion costs ~60–100 allocations per `Display`, repeated on each render of the same `Report`; `format!("{report}")` twice pays twice.
- **Related:** `Report::new` eagerly symbolicates (`report.rs:56-60`) even when the report is never rendered — symbolication is the most expensive operation in the library, and the `Lazy` capture cell already guarantees laziness.
- **Fix:** materialize rendered frames once (in `Report::resolve_backtrace` or the capture cell), hoist `current_dir()`, drop the eager `resolve()`.

### M6. First-time contributor setup is undocumented and incomplete

- **Where:** `README.md:101-113`, `rust-toolchain.toml`, `Justfile`
- **What:** `just test`/`just clippy` require stable + pinned nightly + `cargo nextest` + `cargo-hack` — none mentioned in the Development section (verified: `cargo-hack` absent on a fresh machine → all `*-full` recipes fail). `just nostd` needs the `thumbv7em-none-eabihf` target, which `rust-toolchain.toml` does not declare (`components` only, no `targets` key), so rustup does not auto-install it and the recipe fails out of the box; CI installs it explicitly.
- **Fix:** document prerequisites; add `targets = ["thumbv7em-none-eabihf"]` to `rust-toolchain.toml`.

### M7. README no_std table lists a feature the facade doesn't have

- **Where:** `README.md:92`
- **What:** the `test-utils` row refers to an `oopsie-core` feature; `oopsie` itself has no such feature — `cargo add oopsie --features test-utils` errors. The table also silently omits `std` and `settings`.

---

## Low severity

### Correctness — oopsie-core

- **L1.** no_std `ErasedError::source()` returns `None` unconditionally, even when `source_chain` is populated — undocumented std/no_std behavioral divergence in a core API (`erased/mod.rs:88-91`).
- **L2.** `Backtrace::is_captured()` is a variant check, not a frame check — returns `true` for a zero-frame capture despite its doc claiming "the platform produced a stack"; lets an empty trace shadow a real outer one on unwinder-less platforms (`backtrace.rs:255-260` vs capture at 191-207).
- **L3.** no_std `Backtrace` stub lacks `frames()`/`resolve()`/`marker_hidden_frames()` despite the "API keeps its shape" doc claim; `start_marker!` is a compile error under no_std rather than a degrading no-op (`lib.rs:34-35`, `nostd_stubs.rs:51-53`, `marker.rs:110-115`).
- **L4.** `EnvVarOpt` docs say "`None` if unset"; it is also `None` for non-Unicode values (`env::var` errors) (`extras.rs:148-173`).
- **L5.** Test-only `unsafe { env::set_var }` in a multithreaded test binary (`extras.rs:271-277`) — the exact pattern edition 2024's unsafe marking targets; the crate already has a re-exec pattern that avoids it.

### Correctness — oopsie-macros

- **L6.** fn-local error enum with default module wrapping cannot compile (module inside a fn body cannot see fn-local types) — structural (snafu shares it), but the E0425 error is confusing and the `module(false)` workaround is undocumented (`gen_module.rs:41`).
- **L7.** `vis(pub(in self::path))` lifted to invalid `pub(in super::self::path)` → E0433 + E0742; only the single-segment `self` form is special-cased (`gen_selectors.rs:400-406`).
- **L8.** User generic params named `__T`/`__T0` collide with generated `__T{i}` params → E0403 (`gen_selectors.rs:88,957`). Mint collision-free names the way `formatter()` already does for `__oopsie_fmt`.
- **L9.** Mangled injected-field names (`__oopsie_backtrace`, `__oopsie_traces`, …) are only guarded for `__oopsie_timestamp`; a user field with a mangled name produces a six-error cascade (`traced/inject.rs:26-29`).
- **L10.** A field named `__request` shadows the generated `provide` parameter (nightly feature) → E0599; same class for `__bt`/`__st` locals vs variant-level provide exprs (`gen_error.rs:191,723,274-296`).
- **L11.** Variant-level `traced(spantrace(false))` settings form fails with bare "expected literal" — no hint that variants accept only a bool toggle (`parse.rs:351` via `utils/mod.rs:140-143`).
- **L12.** Struct selector name == struct name (`suffix(false)`, `module(off)`) → raw E0428 + eight follow-on errors; the macro computes both names and could emit a targeted error, as it already does for the wrapped variant (`gen_selectors.rs:655`, `model.rs:286-288`).
- **L13.** `transparent` + explicit `display(...)` silently prefers display; thiserror hard-errors on the combination (`gen_display.rs:27-32`).
- **L14.** Manifest `max-size` cap silently skipped for generic error types; explicit `size(...)` on a generic type is rejected — visible inconsistency (`derive/mod.rs:84,118`).

### Correctness — oopsie facade

- **L15.** Default-hook window during `Report::run` acquire/release: `take_hook()` installs std's default before `install_panic_hook()`; a panic on another thread in that window renders with std's output. Tiny, inherent to take/set, partially documented (`panic_hook.rs:56-75`).
- **L16.** Env-based color detection is cached forever on first use (`cached_env_supports_color`, `color.rs:36-47`); long-running processes that redirect stderr get stale results; undocumented.
- **L17.** No `TERM=dumb` handling in color detection (`color.rs:15-34`). (`NO_COLOR` beating `FORCE_COLOR` is deliberate and tested — informational.)

### Performance

- **L18.** Boxed `dyn FnOnce` per captured backtrace — 1 extra alloc per capture; documented, blocked on TAIT (`backtrace.rs:374-381`).
- **L19.** `String::into_boxed_str()` shrink-realloc per Welp message — 1 extra alloc + copy per message (`welp.rs:100,133,159`).
- **L20.** Linear `GENERATED_SITES` scan per frame per render — O(frames × sites) with a `to_string_lossy()` per pair; fine for small apps, noticeable with hundreds of error types (`trace_printer.rs:359-369,391`).
- **L21.** `marker_hidden_frames()` recomputed per render — two O(frames) passes on every `Display` (`report.rs:305`, `backtrace.rs:280-299`).
- **L22.** `theme.rs:17` comment claims `Theme` is "Small and `Copy`" but it only derives `Clone`; `get_theme()` clones behind an `RwLock` per render (`report.rs:166`). Harmless, but the comment lies.

### DX / tests / tooling

- **L23.** CHANGELOG reference-style links are broken for rc.4–rc.20 — only rc.1–rc.3 have URL definitions, so 17 release headings render as literal bracketed text on GitHub (`CHANGELOG.md:239-241`).
- **L24.** The two primary macros' doc examples are all ` ```ignore `d — never compiled, can rot silently; the `Oopsie` derive example already relies on an unshown `module(false)` (`oopsie-macros/src/lib.rs:46,82,103`).
- **L25.** crates.io package for `oopsie` ships 249 files, 190 under `tests/` and `benches/` (~530 KB of insta snapshots and trybuild fixtures) — no `include`/`exclude` in `crates/oopsie/Cargo.toml`.
- **L26.** `criterion.toml` and `mutants.toml` have no entry points — no `just bench`/`just mutants` recipes, no README mention; `criterion.toml` is only read by the external `cargo-criterion` tool.
- **L27.** `nostd-smoke`'s `#[cfg(test)]` suite never runs — the crate is workspace-excluded and both `just nostd` and CI only `cargo build` it; the chain-rendering/`Welp`/selector assertions are dead code unless run manually (`nostd-smoke/src/lib.rs:59-80`).
- **L28.** Backtrace-snapshot parity gap: `just test` runs four snapshot combos with `OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1`; CI sets that env var only on one row — no-serde backtrace snapshots are blessable locally but never verified in CI (`Justfile:43-44` vs `ci.yml:47`).
- **L29.** Compile-fail coverage gaps (trybuild suite is otherwise extensive): selector-name == enum-name collision (M3), `cfg_attr`-on-field cases (M4), fn-local error types (L6), `vis(pub(in self::…))` (L7).
- **L30.** Bench coverage gaps: no `Welp` bench (the path with the worst per-layer cost, H3), no re-render bench (would expose M5), `propagate/context_err` runs only with backtraces `Disabled` (never measures traced attach end-to-end), no chain-depth-scaling or spantrace render benches.

---

## Verified performance numbers (Apple Silicon, `--quick` benches)

| Operation | Cost |
|---|---|
| Success path `.context()` on `Ok` | ~1.4 ns (baseline 1.1 ns) — effectively free |
| Untraced `.context()` on `Err` | ~77 ns |
| Unresolved backtrace capture | ~13.9 µs |
| `traced` error construction | ~14.6 µs |
| Full `Report` render | ~33 µs |

---

## Checked and cleared

The audits explicitly verified the following as **sound** (not merely unexamined):

- **No `unsafe`** in library code anywhere (workspace denies it; single `#[expect(unsafe_code)]` is test-only).
- Success path: selectors hold raw `impl Into<T>` values and convert only in `build_error`; `welp_context(&str)` allocates nothing on `Ok`; env/backtrace/settings detection all cached atomics/`OnceLock`s.
- Capture architecture: unresolved capture with deferred symbolication, `Arc`-shared across clones, `Clone` is a refcount bump.
- Panic-hook refcounting: `acquire_hook`/`release_hook` save/restore a prior user hook correctly, including when `Report::run` itself panics (verified empirically); `release_hook` is correctly not in a `Drop` guard.
- TLS teardown in the hook handled via `try_with`; `set_hook`-during-unwind abort guarded.
- Exit codes: `oopsie_exit_code` is `NonZeroU8` end-to-end — no truncation, no zero.
- Unicode safety: `split_function_hash` guards char boundaries; `frame_file_matches` uses checked byte indexing.
- `RUST_LIB_BACKTRACE`/`RUST_BACKTRACE` precedence, empty-string, `=0`, and `full` semantics match std exactly (verified against std source).
- Chain/erased logic: `ChainNode::build` order, 128-entry truncation sentinel, `Chain` iterator fusion, arrow rendering — all off-by-one-free.
- Serde: `Box<[Box<OsStr>]>` round-trips non-UTF8 losslessly; `TracingLevel` tolerant deserialization degrades gracefully.
- Macro hygiene core: `::core::…` absolute paths, `__private::alloc`, generic-parameter projection (lifetimes, free params, `T::Item` projections, cfg-gated fields) compiled correctly in every case tried; all failures surface as rustc/`syn::Error` diagnostics — no ICEs or macro panics found.
- Generated `Display` writes straight to the formatter; chain rendering is iterative and depth-capped; no quadratic chain or spantrace-dedup loops.
- Publish metadata: all three crates at `0.1.0-rc.20` with consistent pins; descriptions/license/repository/readme/keywords/categories set; sub-crate READMEs accurate.
- `cargo doc -D warnings` on the full stable feature surface: zero warnings, no broken intra-doc links.
- MSRV 1.89 credible: `rust-version` set workspace-wide, CI MSRV job derives from `cargo metadata`, all dev-dependency MSRVs ≤ 1.88.
- Full cfg matrix (`--no-default-features`, ±serde, tracing, `--all-features`) compiles warning-free; all test suites green (176 core unit tests, 233 macro tests, full `oopsie` suite).

---

## Suggested fix order

1. **H1** — README install line + rustdoc feature table + a CI row for default features. ~15 minutes, biggest user impact (every evaluator's first 60 seconds).
2. **H2** — panic-hook stderr write with errors ignored (mirror std's default hook).
3. **H3** — `Welp` construction-time trace reuse (nightly Provider API + stable `downcast_ref::<Welp>` fast path) and `source_error_code`/`source_help` Provider helpers.
4. **M1–M4** — `root_cause` depth cap; resolve the panic-backtrace override doc/behavior split and fix the misleading hint; `super::`-qualify enum paths in the selector module; sane `cfg_attr` forwarding.
5. **M5** — cache rendered frames in `Report`; drop eager symbolication in `Report::new`; hoist `current_dir()`.
6. **M6–M7 + L23–L30** — doc/tooling sweep: contributor prerequisites, `rust-toolchain.toml` targets, feature tables, CHANGELOG links, un-`ignore` macro examples, package excludes, wire up bench/mutant/nostd-test entry points, close the CI snapshot parity gap.
