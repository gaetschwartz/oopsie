# Audit #1 — 2026-07-23

Adversarial audit of the oopsie workspace (crates: `oopsie`, `oopsie-core`, `oopsie-macros`) by seven finder agents (correctness-core, correctness-macros-derive, correctness-macros-traced-runtime, soundness-unsafe, performance, dx-api-docs, dx-diagnostics-features), each finding independently re-verified by a dedicated verifier agent with refutation attempts. **46 findings were filed; all 46 survived verification (46 confirmed, 0 uncertain, 0 refuted).** Overall the codebase is in good health: there are no memory-safety holes in the shipping API and only one finding that aborts a process in plausible normal use (the panic hook on broken stderr). The dominant themes are (1) proc-macro diagnostics that degrade into raw rustc errors on generated code for unusual-but-legal inputs (duplicate names, lifetime params, cfg-gated fields, renamed crates), (2) stale documentation around the rc.18 removal of `fancy` from the default feature set — the same stale `lib.rs:323` table row was independently found three times, and the README quick start has been broken for three released versions — and (3) small robustness gaps in `oopsie-core`'s no_std/serde and chain-walking paths. Four findings are high, thirteen medium, twenty-nine low.

## Summary table

| ID | Severity | Category | Title | Location |
|---|---|---|---|---|
| CC-F1 | medium | correctness | no_std + serde: `ErasedError::source()` stubbed to None, silently dropping the transported source chain | crates/oopsie-core/src/erased/mod.rs:88 |
| CC-F2 | low | correctness | `Backtrace::is_captured()` returns true for zero-frame captures | crates/oopsie-core/src/backtrace.rs:253 |
| CMD-F1 | high | correctness | Lifetime bounds on projected type params misrouted onto selector-scoped impl (E0261) | crates/oopsie-macros/src/derive/generics.rs:183 |
| CMD-F2 | medium | correctness | Sourced variant with free lifetime slips past unconstrained-parameter guard → raw E0207 | crates/oopsie-macros/src/derive/gen_selectors.rs:234 |
| CMD-F3 | medium | correctness | Auto error code silently lost via `oopsie_error_code()` when crate is renamed (`path = "..."`) | crates/oopsie-macros/src/derive/gen_error.rs:1206 |
| CMD-F4 | medium | correctness | Selector-name collision checks miss collisions with the error type's own name | crates/oopsie-macros/src/derive/model.rs:84 |
| CMD-F5 | medium | correctness | Field-level `provide(...)` ignores the field's `#[cfg]`, breaking generated code | crates/oopsie-macros/src/derive/gen_error.rs:245 |
| CMD-F6 | low | correctness | Synthetic generic names `__T` / `__T{i}` collide with user parameters (E0403) | crates/oopsie-macros/src/derive/gen_selectors.rs:957 |
| CMD-F7 | low | correctness | Field named `__request` shadows the mangled provide parameter (E0599, nightly) | crates/oopsie-macros/src/derive/gen_error.rs:191 |
| CMD-F8 | low | correctness | Source field typed `Backtrace`/`SpanTrace` dual-classified as source and trace field | crates/oopsie-macros/src/derive/parse.rs:1722 |
| CMD-F9 | medium | correctness | `#[cfg]`-gated field referencing a generic param leaves it dangling on the selector (E0392) | crates/oopsie-macros/src/derive/gen_selectors.rs:79 |
| CMD-F10 | low | correctness | Generated `From` impls for transparent items can collide (E0119), no targeted diagnostic | crates/oopsie-macros/src/derive/gen_selectors.rs:462 |
| CMD-F11 | low | dx | `vis` (and struct `suffix`) silently ignored on transparent items | crates/oopsie-macros/src/derive/gen_selectors.rs:428 |
| CMTR-F1 | high | correctness | Panic hook aborts the process (SIGABRT) when stderr writes fail | crates/oopsie/src/panic_hook.rs:35 |
| CMTR-F2 | medium | dx | lib.rs feature table claims `fancy` is a default feature; it is not | crates/oopsie/src/lib.rs:323 |
| CMTR-F3 | low | correctness | Pre-existing SystemTime/DateTime field suppresses timestamp injection without capture/provide | crates/oopsie-macros/src/traced/inject.rs:45 |
| CMTR-F4 | low | correctness | Bottom-peel frame filter strips user frames of a crate named `test` | crates/oopsie/src/trace_printer.rs:440 |
| CMTR-F5 | low | dx | `Theme` docs claim it is `Copy`; the derive does not include it | crates/oopsie/src/theme.rs:18 |
| CMTR-F6 | low | dx | `Tristate::explicit` doc comment contradicts the code | crates/oopsie-macros/src/traced/args.rs:32 |
| SU-F1 | medium | correctness | Recursive Drop/Clone on `ChainNode`: stack-overflow abort from an untrusted erased payload | crates/oopsie-core/src/erased/mod.rs:96 |
| SU-F2 | low | correctness | `ErrorChainExt::root_cause` never terminates on a cyclic `source()` chain | crates/oopsie-core/src/chain.rs:90 |
| SU-F3 | low | correctness | Test-only unsafe `env::set_var` whose justification does not hold under the default test harness | crates/oopsie-core/src/extras.rs:275 |
| PERF-F1 | low | performance | `Report::new` eagerly symbolicates the backtrace even when never rendered | crates/oopsie/src/report.rs:56 |
| PERF-F2 | low | performance | Every `Report` render re-materializes all backtrace frames | crates/oopsie/src/trace_printer.rs:116 |
| PERF-F3 | low | performance | `SpanTrace::eq` allocates a String per span per comparison in debug builds | crates/oopsie-core/src/spantrace.rs:123 |
| DX-F1 | high | dx | README quick start does not compile with the documented install | README.md:37 |
| DX-F2 | medium | dx | Crate-docs feature table claims `fancy` is on by default | crates/oopsie/src/lib.rs:323 |
| DX-F3 | low | dx | Broken intra-doc links when building docs for no_std | crates/oopsie-core/src/diagnostic.rs:46 |
| DX-F4 | low | dx | README no_std table lists `test-utils`, not a feature of the `oopsie` facade | README.md:92 |
| DX-F5 | medium | dx | Example "Run with:" headers name the wrong feature set (3 of 6 examples) | crates/oopsie/examples/complete.rs:18 |
| DX-F6 | low | dx | CHANGELOG rc.18 claims the default build "renders plain reports" | CHANGELOG.md:37 |
| DX-F7 | low | dx | CHANGELOG reference links defined for only 3 of 20 version headers | CHANGELOG.md:239 |
| DX-F8 | low | dx | no_std crate docs omit `extras` from the std-only list | crates/oopsie/src/lib.rs:339 |
| DF-F1 | high | correctness | `settings` feature leaks across crates via feature unification; builds are non-hermetic | crates/oopsie/Cargo.toml:36 |
| DF-F2 | medium | dx | Invalid per-type `suffix = "..."` string panics the proc macro | crates/oopsie-macros/src/derive/gen_selectors.rs:740 |
| DF-F3 | medium | dx | Crate docs claim `fancy` is a default feature; it is not | crates/oopsie/src/lib.rs:323 |
| DF-F4 | medium | correctness | Manifest `max-size` silently skipped on generic error types while docs claim it caps every error | crates/oopsie-macros/src/derive/mod.rs:84 |
| DF-F5 | medium | dx | `traced(timestamp)` under no_std / `timestamp(chrono = true)` without chrono fails with cryptic `__private` errors | crates/oopsie-macros/src/traced/config.rs:70 |
| DF-F6 | low | dx | Unknown `#[oopsie(...)]` key on a struct loses the Available-values list; struct-level `traced` gets no guidance | crates/oopsie-macros/src/derive/parse.rs:649 |
| DF-F7 | low | dx | User field colliding with an injected trace field name produces 6 cascading rustc errors | crates/oopsie-macros/src/traced/inject.rs:21 |
| DF-F8 | low | dx | Container keywords swallowed as display args on variants yield "cannot find value `module`" | crates/oopsie-macros/src/derive/parse.rs:398 |
| DF-F9 | low | dx | `vis = pub(crate)` bare form errors with "expected an expression" | crates/oopsie-macros/src/derive/parse.rs:288 |
| DF-F10 | low | dx | Packed-boxing conflict error spanned at the item, not the offending toggle | crates/oopsie-macros/src/oopsie_attr/mod.rs:119 |
| DF-F11 | low | dx | SynParse errors leak the literal placeholder `key` to users | crates/oopsie-macros/src/utils/mod.rs:186 |
| DF-F12 | low | dx | Error type in a fn body without `module(false)` gives a misleading resolution error | crates/oopsie/src/lib.rs:201 |
| DF-F13 | low | dx | `CARGO_WORKSPACE_DIR` override honored but undocumented | crates/oopsie-macros/src/utils/settings.rs:69 |

**Cross-references:** CMTR-F2, DX-F2 and DF-F3 are the same defect (stale `fancy | yes` row at `crates/oopsie/src/lib.rs:323`), independently filed by three finders; DX-F1 and DX-F6 share the same root cause (rc.18 removed `fancy` from defaults; docs never updated). CC-F1 and SU-F1 both concern `ErasedError`'s `ChainNode` materialization from an uncapped deserialized `source_chain`. CMD-F6, CMD-F7 and DF-F7 are the same class: unprobed synthetic/mangled identifiers colliding with user names.

## Confirmed findings

### CMTR-F1 — Panic hook aborts the process (SIGABRT) when stderr writes fail — high, correctness

`install_panic_hook` renders the panic report with `eprint!` (crates/oopsie/src/panic_hook.rs:35), which panics when writing to stderr fails. When stderr is a broken pipe (`myapp 2> >(head -5)`, an early-exiting log shipper, `/dev/full`), the write fails *inside the panic hook* — a double panic, which unconditionally aborts. std's default hook discards the write result and unwinding proceeds normally (exit 101). A second path fires via `Termination::report` (crates/oopsie/src/report.rs:333, `eprintln!("{self}")`): it panics on EPIPE, and with oopsie's hook installed that panic re-enters the hook → abort instead of the declared exit code.

**Evidence.** Independent probe crate (/tmp/oopsie-verify-f1) with stderr attached to a pre-closed pipe: plain `eprintln!` at EPIPE → exit 101; panic with std's hook → exit 101; panic with oopsie's hook → SIGABRT. `Termination::report` with declared exit 42 at EPIPE: exit 101 without oopsie's hook (the declared code is lost even in the default `fn main() -> Report<E>` pattern, since `Report::run` restores the prior hook before `Termination::report` runs), SIGABRT with it. Healthy-stderr controls exit 42.

**Verifier's reasoning.** Both cited lines verified verbatim; the workspace has no `panic = "abort"` profile so double-panic-abort semantics apply; no EPIPE/broken-pipe test coverage exists; SIGPIPE is ignored by Rust at startup so writes fail with EPIPE and `eprint!`/`eprintln!` panic on write failure (proven by the plain-eprintln control). The edge is reachable in ordinary shell usage and hits the normal error-exit path, not just panics.

**Suggested fix.** Render through a write that ignores errors: `let _ = ::std::io::Write::write_fmt(&mut ::std::io::stderr().lock(), format_args!("{}", report));` (mirroring std's default hook). Apply the same treatment to the `eprintln!` in `Termination::report`.

Reports: [finder](agents/correctness-macros-traced-runtime/REPORT.md) · [verifier](agents/verify-correctness-macros-traced-runtime-F1/REPORT.md)

### DF-F1 — `settings` feature leaks across crates via feature unification; workspace builds are non-hermetic — high, correctness

`settings = ["oopsie-macros/settings"]` (crates/oopsie/Cargo.toml:36) is a flag on the proc-macro crate. Cargo unifies features per build: if any crate in the graph enables `oopsie/settings`, the single oopsie-macros build has `settings` on for every crate compiled in that invocation. Since the macros read each expanded crate's own manifest and workspace root, a crate that never opted in still gets `[workspace.metadata.oopsie]` defaults (`max-size`, `traced = true`, `default-vis`, `default-suffix`, `module`) applied. Whether a crate compiles depends on which other crates are built alongside it, contradicting the docs' claim that workspace settings apply only to "every member crate that opts into the `settings` feature" (crates/oopsie/src/lib.rs:345-352, keyword_docs/container/size.md:30-33).

**Evidence.** /tmp/oopsie-uni: virtual workspace with `[workspace.metadata.oopsie] max-size = 8`, crate-a with `features = ["settings"]`, crate-b without (24-byte error type). `cargo build -p crate-b` → OK; `cargo build --workspace` → `error[E0080]: evaluation panicked: 'BErr' is 24 bytes, must be ≤ 8 ... set by '[workspace.metadata.oopsie] max-size'` in crate-b. Reproduced by the verifier with identical results.

**Verifier's reasoning.** The settings module is `#![cfg(feature = "settings")]` inside the macro crate and reads the expanding crate's `CARGO_MANIFEST_DIR` plus workspace root unconditionally when compiled in; resolver v2/v3 does not split these host-side features (verified empirically); no per-crate opt-in gate exists and none is possible via a feature on the macro crate; no doc or test acknowledges the behavior. Beyond the compile error, the same leak silently changes generated code (including public API via `default-vis`) in non-opted-in crates. The documented "declare once at the workspace root" pattern is exactly the triggering setup, and gradual adoption (one member opts in) is the natural way to hit it.

**Suggested fix.** The macro cannot detect which crate enabled the feature: drop the `settings` cargo feature and always read manifests, or treat an (even empty) `[package.metadata.oopsie]` table in the expanded crate's own manifest as the opt-in signal, or at minimum document the unification behavior prominently.

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F1/REPORT.md)

### CMD-F1 — Lifetime bounds on a projected type param referencing a free lifetime are misrouted onto the selector-scoped impl (E0261) — high, correctness

`predicate_named_params` (crates/oopsie-macros/src/derive/generics.rs:183) only visits `TypeParamBound::Trait` bounds, ignoring `TypeParamBound::Lifetime`, so `T: 'a` is treated as naming only `{T}`. The predicate router in gen_selectors.rs (`scoped_predicates` / `free_params`) then places `T: 'a` on the selector-scoped impl that declares only `T` (E0261 undeclared lifetime) and never on the `build`/`fail` methods that declare the free `'a`. A legitimate generic enum fails to compile with an error pointing at the user's own bound.

**Evidence.** `enum LtBoundError<'a, T: 'a + Debug> { A { value: T }, B { note: &'a str } }` → `error[E0261]: use of undeclared lifetime name 'a` ("lifetime `'a` is missing in item created through this procedural macro"). Reproduced by the verifier for both inline-bound and where-clause forms. `-Zunpretty=expanded` shows `impl<T> A<T> where T: 'a + fmt::Debug { pub fn build<'a>(self) -> LtBoundError<'a, T> ... }` — the bound is misplaced on an impl that can't see `'a` *and* absent from `build`/`fail` whose return type requires `T: 'a` for well-formedness: the generated code is broken twice.

**Verifier's reasoning.** The neighboring `WherePredicate::Lifetime` arm does record lifetimes, confirming the asymmetry; the routing consequences follow mechanically from `scoped_predicates`/`free_params`; existing tests cover free lifetimes with `T: fmt::Debug` but never `T: 'a` (suite passes 20/20 — a coverage gap, not a handled invariant). The input shape (generic value stored beside borrowed data) is ordinary Rust and the rustc error points at the user's own bound with a misleading `for<'a>` suggestion, so users will hit it.

**Suggested fix.** In `predicate_named_params`, record declared lifetimes appearing in `TypeParamBound::Lifetime` bounds of `WherePredicate::Type` (mirroring the `WherePredicate::Lifetime` arm), so `T: 'a` names `{T, 'a}` and routes to the methods declaring the free lifetime.

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F1/REPORT.md)

### DX-F1 — README quick start does not compile with the documented install — high, dx

The Install section says `cargo add oopsie` (default features: `std` only), but the quick start returns `oopsie::Report<AppError>` from main and calls `oopsie::Report::run`. `Report` is gated behind the non-default `fancy` feature (crates/oopsie/src/lib.rs:529-530, crates/oopsie/Cargo.toml:18,30). The README never mentions `fancy` outside the no_std table, so there is no hint which feature to enable. Commit aacb8a0 (2026-06-29, "remove 'fancy' from default feature set") is contained in tags v0.1.0-rc.18, rc.19 and rc.20 — three released versions ship the broken onboarding path. Cross-reference: DX-F2/CMTR-F2/DF-F3 (same root cause), DX-F6 (same commit's changelog entry).

**Evidence.** Throwaway crate with the verbatim README snippet under default features: `error[E0433] could not find 'Report' in 'oopsie'`; `error[E0425] cannot find type 'Report'`; note: the item is gated behind the `fancy` feature. The same snippet compiles cleanly with `features = ["fancy"]`. The repo's own `examples/quickstart.rs` (no required-features) deliberately avoids `Report`, corroborating that the README snippet cannot work under default features.

**Verifier's reasoning.** Refutation attempts (prelude re-export, `std` implying `fancy`, adequate docs pointer) all failed; the failure is deterministic from the cfg gate. High is appropriate — this is the primary onboarding path, and every new user following the README hits it.

**Suggested fix.** Change the install line to `cargo add oopsie --features fancy` and mention `fancy` in the quick-start prose, or rewrite the quick start to not need `Report` (print `{err}` and `.chain()` like examples/quickstart.rs).

Reports: [finder](agents/dx-api-docs/REPORT.md) · [verifier](agents/verify-dx-api-docs-F1/REPORT.md)

### CC-F1 — no_std + serde: `ErasedError::source()` stubbed to None, silently dropping the transported source chain — medium, correctness

Under `not(feature = "std")`, `ErasedError`'s `Error` impl returns `None` from `source()` (crates/oopsie-core/src/erased/mod.rs:88-91) even when `source_chain` holds transported entries. serde does not require std in the feature graph, so no_std+serde is a supported, compiling combination (justfile:73 explicitly builds it). Consequences: (1) generic `Error::source()` walkers (`ErrorChainExt::chain`, `root_cause`) see a one-element chain despite `source_chain()` reporting the real causes; (2) re-erasing via the documented `from_error_ref` path silently loses the entire source chain, contradicting the unqualified doc comment at erased/mod.rs:39-42 ("Re-erasing an ErasedError ... preserves the message, source chain, code, help, and exit code"). The std-gating exists only because the lazy `source: OnceLock<...>` field needs `std::sync::OnceLock`; `ChainNode` itself is alloc-only. Cross-reference: SU-F1 (same ChainNode materialization, DoS angle).

**Evidence.** Verifier's independent reproduction (/tmp/oopsie-f1-verify, path dep with `default-features = false, features = ["serde"]`): deserialized `{"message":"outer","source_chain":["middle","root"]}` gives `source_chain().len() == 2` but `Error::source(&erased).is_none()`, and `ErasedError::from_error_ref(&erased).source_chain().is_empty()` — chain silently lost on re-erasure.

**Verifier's reasoning.** The combo is reachable and CI-built; the doc is unqualified; the chain data is present (the `source_chain` field survives) so this is a genuine silent drop, not a transport gap; no test covers no_std+serde behavior. Impact is bounded: `render_text`/`format_short` read `source_chain` directly so text reports stay correct — the loss is confined to `source()`-walking and re-erasure in the edge config. No panic or soundness issue.

**Suggested fix.** Re-materialize the chain under no_std too — build the `ChainNode` list eagerly at construction/deserialization instead of lazily via `OnceLock`, since `ChainNode` is alloc-only — or, if the lazy path is deliberately std-only, qualify the doc claim on `ErasedError` and document that `Error::source()` walking and chain-preserving re-erasure are std-only.

Reports: [finder](agents/correctness-core/REPORT.md) · [verifier](agents/verify-correctness-core-F1/REPORT.md)

### SU-F1 — Recursive Drop/Clone on `ChainNode`: stack-overflow abort from an untrusted erased payload — medium, correctness

`ChainNode { message: Box<str>, source: Option<Box<Self>> }` (crates/oopsie-core/src/erased/mod.rs:96-101) has derived `Clone` and compiler-generated drop glue; both recurse one stack frame per link. The chain is built lazily by `ErasedError::source()` from `source_chain`, which on the deserialization path has NO length cap (`MAX_SOURCE_CHAIN_DEPTH = 128` is only enforced by `from_error_ref` at capture time). An attacker-controlled transported JSON payload can carry a `source_chain` of arbitrary length; any consumer calling `.source()` (the standard way to walk a dyn Error) materializes the nested chain, and dropping or cloning the `ErasedError` recurses to a stack overflow — a fatal process abort, not a catchable panic. `ErasedError` is explicitly designed for deserialized transported payloads, so untrusted input is the intended use case. Bonus: the derived `Debug` recurses too, a third trigger.

**Evidence.** End-to-end probe against the real crate (/tmp/oopsie-e2e): `serde_json::from_str::<ErasedError>` with 400k-entry `source_chain`, walk `Error::source()` to the end, then drop or clone → "thread 'main' has overflowed its stack / fatal runtime error: stack overflow" (SIGABRT, exit 134). Threshold on an 8 MiB main-thread stack is between 200k and 500k entries (~1–2 MB JSON payload).

**Verifier's reasoning.** Refutation attempts all failed: serde_json's 128-deep recursion limit only applies to nested JSON structures — `source_chain` is a flat array; no custom `Deserialize` impl exists; the chain IS materialized on first `.source()` call, which any anyhow/eyre-style chain walker (and the repo's own `source_walk_yields_transported_chain_in_order` test) makes; drop glue is not flattened (abort is empirical). Kept at medium: requires a multi-hundred-KB crafted payload plus a source-walking consumer; DoS abort, not memory unsafety — but a genuine uncatchable crash from untrusted input in the documented use case.

**Suggested fix.** Implement `Drop` for `ChainNode` with an iterative unlink loop and a manual iterative `Clone`; consider a non-recursive `Debug`. Defense in depth: cap `source_chain.len()` in a custom `Deserialize` impl (e.g. the same 128) so the wire format and capture path share one bound.

Reports: [finder](agents/soundness-unsafe/REPORT.md) · [verifier](agents/verify-soundness-unsafe-F1/REPORT.md)

### CMD-F3 — Auto-generated error code silently lost through `oopsie_error_code()` when the crate is renamed — medium, correctness

Trace injection emits the auto code provide as `#oopsie_path::ErrorCode` using the resolved crate path (traced/config.rs:99, inject.rs:166), but `is_error_code_provide` (crates/oopsie-macros/src/derive/gen_error.rs:1206) only matches a bare `ErrorCode` or one qualified by literally `oopsie`/`oopsie_core`. With a renamed dependency and `path = "renamed_oopsie"` (or `path = "crate"`), the injected provide doesn't match, so no `oopsie_error_code` override is generated and the stable accessor returns `None` — a silent, stable-only loss (the nightly `provide` path still works).

**Evidence.** End-to-end: `#[renamed_oopsie::oopsie(traced, path = "renamed_oopsie")] enum RenamedError { Boom { msg } }` → `err.oopsie_error_code()` prints `None`; the identical unrenamed crate prints `Some(ErrorCode("normal_code::RenamedError::Boom"))`. Expansion shows the generated `Diagnostic` impl contains backtrace/spantrace/location accessors but no `oopsie_error_code` override.

**Verifier's reasoning.** The doc comment on `is_error_code_provide` ("Matches a bare ErrorCode (the form trace injection emits)") is stale — it only matches today because the default resolved path's qualifier happens to be `oopsie`. No existing test covers renamed-path + traced auto-code. `path = "..."` exists precisely for renamed deps, and in exactly that configuration the auto code silently vanishes while everything else keeps working — inconsistent, hard-to-notice degradation.

**Suggested fix.** Thread the resolved `oopsie_path` into `is_error_code_provide` and compare the provide type's path segments against it instead of hardcoded `oopsie`/`oopsie_core` qualifiers.

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F3/REPORT.md)

### CMD-F5 — Field-level `provide(...)` ignores the field's `#[cfg]`, breaking generated code when the field is stripped — medium, correctness

`field_cfg_attrs` is forwarded onto bindings and trace-field provide statements but not onto statements generated from a field's own `#[oopsie(provide(...))]`: the `provide()` stmt (crates/oopsie-macros/src/derive/gen_error.rs:245-247 enum, :749-751 struct) carries no cfg → E0425 under `unstable-error-generic-member-access` when the expr references the stripped field. The stable `oopsie_error_code()` accessor replaying an `ErrorCode` provide (enum :445-467, struct :987-1009) is gated only on the variant cfg while the field binding drops → E0425 on stable.

**Evidence.** Field with `#[cfg(any())] #[oopsie(provide(oopsie::ErrorCode => oopsie::ErrorCode::from(extra)))]` → `error[E0425]: cannot find value 'extra' in this scope` on stable (enum and struct paths) and in `provide()` on nightly with the feature enabled. All three failure modes reproduced by the verifier.

**Verifier's reasoning.** Adjacent code in the same loops forwards field cfg for trace fields (`trace_field_cfg`) and help fields (`field_cfg_for`), and `collect_provide_field_binds` forwards per-field cfg onto pattern bindings — so this is an inconsistency, not a design constraint. Not user error: the macro deliberately supports cfg-stripped fields everywhere else, leaving the user no workaround. Caveat: the stable-path compile error only occurs for an `ErrorCode` provide whose expr references the stripped field; other field provides break only on nightly with the unstable feature.

**Suggested fix.** Apply `field_cfg_for(categorized, field_ident)` to field-level provide statements (like `trace_field_cfg` does) and to the code-accessor arm/method generated from a field-level provide.

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F5/REPORT.md)

### CMD-F9 — A `#[cfg]`-gated field referencing a generic parameter leaves the parameter dangling on the stripped selector (E0392) — medium (adjusted from low), correctness

`selector_shape` (crates/oopsie-macros/src/derive/gen_selectors.rs:79) runs `referenced.add_type` for cfg-gated user fields too, so the parameter is projected onto the selector struct; when the cfg strips the field, the struct keeps a now-unused parameter → E0392 on the generated selector plus E0282 inference cascades. Field/type references themselves are correctly gated; only the projection dangles. Reachable in realistic code: `#[cfg(feature = "x")] x: T` compiles with the feature on and fails whenever it is off, surfacing only in `--no-default-features`/downstream builds.

**Evidence.** `enum CfgGenericError2<T: Debug> { V { #[cfg(any())] x: T, keep: u32 }, W { y: T } }` → `error[E0392]: type parameter 'T' is never used` (span: the user's enum declaration) + `error[E0282]: type annotations needed` at the build site. Expansion shows `pub struct VOopsie<T, __T1> { pub keep: __T1 }` — the cfg-gated `x: T` was stripped, leaving `T` dangling; the user's enum still uses `T` via variant W.

**Verifier's reasoning.** The only cfg-related rejection anywhere is `reject_cfg_on_source` (source fields only); existing cfg tests use only concrete-typed gated fields; no docs forbid the shape. Severity adjusted low → medium: hard compile error in generated code with a misattributed span (E0392 points at the user's enum, not the selector) and no hint at the real cause.

**Suggested fix.** Either reject cfg-gated parameter-referencing fields with a targeted error, or add a `PhantomData` marker field to the selector when a projected parameter is only referenced by cfg-gated fields.

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F9/REPORT.md)

### CMD-F2 — Sourced variant with a free lifetime parameter slips past the unconstrained-parameter guard → raw E0207 — medium, correctness

`SelectorShape::unconstrained_error_param` (crates/oopsie-macros/src/derive/gen_selectors.rs:234) returns false for `GenericParam::Lifetime`, so the guard that emits a clear targeted error for unconstrained type/const params never fires for lifetimes. `sourced_impl` emits `impl<'a, T> Contextual<T> for ASelector` with `'a` unconstrained (the associated `Destination` doesn't constrain), producing a raw E0207 on generated code instead of the helpful diagnostic the type/const twins get.

**Evidence.** `enum FreeLtSourced<'a, T: Error + 'static> { A { source: T }, B { note: &'a str } }` → `error[E0207]: the lifetime parameter 'a is not constrained by the impl trait, self type, or predicates`. The const-param twin correctly gets the macro's targeted error.

**Verifier's reasoning.** Plain-rustc probes show a predicate-only lifetime is constrained only when absent from associated-type bindings; since the generated impl always names every error param in `type Destination = E<'a, T>`, a sibling-only lifetime is *always* E0207 even with bounds like `T: Error + 'a` — the configuration is genuinely unexpressible, so the macro should reject it with its tailored diagnostic exactly as for type/const params. The fix cannot false-positive: the guard's constrained set matches rustc's rule for this exact impl shape, and `ReferencedParams::names()` already includes lifetime names. Medium (leaning low): compilation correctly fails either way; only diagnostic quality differs, but the codebase clearly invests in this diagnostic class.

**Suggested fix.** Include lifetimes in `unconstrained_error_param`; the `referenced_names`/`from_source` name sets already carry lifetime names.

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F2/REPORT.md)

### CMD-F4 — Selector-name collision checks miss collisions with the error type's own name — medium, correctness

The collision check (crates/oopsie-macros/src/derive/model.rs:84) only compares selector names across variants. (a) A variant whose stripped selector name equals the enum name (`enum Config { Config }`) produces E0428 with `module(false)` and confusing E0223 "ambiguous associated type" with default module wrapping — and this triggers under DEFAULT enum settings. (b) A struct with `suffix(false)` + `module(false)` whose name lacks an Error suffix collides with its own selector (the `super::` handling exists only for module-wrapped selectors). Sub-case (c) of the original filing (cfg-gated collision) was assessed by the verifier as a deliberate, documented trade-off (proc macros cannot evaluate cfg predicates) and is not counted as a defect.

**Evidence.** `enum Config { Config, Other }` → E0428 (module off) / E0223 ambiguous associated type naming `config_oopsies::Config` (module on, no hint a selector is the culprit). `struct Failure` with `module(false), suffix(false)` → E0428 + E0107 cascade. All repros fail identically on develop @ 5c55047.

**Verifier's reasoning.** rustc always rejects the output — nothing incorrect compiles — so this is a missing targeted diagnostic (DX), not accepted-wrong-code. Existing trybuild fixtures cover only variant-vs-variant and keyword collisions. Medium (low end): the default-settings enum case produces a genuinely misleading diagnostic, but the naming pattern is uncommon and failure is always compile-time.

**Suggested fix.** Include the container's own name in the selector collision set (enums); reject `module(false)` + suffix resolving to the bare struct name (structs) with a targeted error.

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F4/REPORT.md)

### CMTR-F2 / DX-F2 / DF-F3 — Crate-docs feature table claims `fancy` is a default feature; it is not — medium, dx (DX-F2 adjusted from high)

The feature table in crate-level rustdoc says `| fancy | yes | stable | Report, colorized output, and the panic hook |` (crates/oopsie/src/lib.rs:323), but Cargo.toml:18 is `default = ["std"]`, and CHANGELOG rc.18 explicitly documents the removal of `fancy` from defaults. Commit aacb8a0 changed only the manifest; the doc table was never updated. docs.rs renders this table as the canonical feature reference. The table also omits the default-on `std` row (partially mitigated by no_std prose). **The same defect was independently filed by three finders** — fix it once. Cross-reference: DX-F1 (README onboarding), DX-F6 (changelog wording).

**Evidence.** Scratch crate with default features referencing `oopsie::Report` fails with E0433/E0425 "the item is gated behind the `fancy` feature". All nine other table rows match the manifest exactly; `fancy` is the lone wrong row. README ("`std` is on by default") agrees with the manifest.

**Verifier's reasoning.** The Default column's meaning is unambiguous; no alternate ungated path to `Report` exists; the failure is loud and the compiler note names the feature, making recovery trivial — hence medium rather than the filed high (DX-F2) for the docs.rs surface. History nuance: `fancy` briefly WAS a default (8f240fc) before aacb8a0 removed it.

**Suggested fix.** Change the `fancy` row's Default cell to `no`; optionally add a `std | yes` row.

Reports: [finder (CMTR)](agents/correctness-macros-traced-runtime/REPORT.md) · [verifier](agents/verify-correctness-macros-traced-runtime-F2/REPORT.md) · [finder (DX)](agents/dx-api-docs/REPORT.md) · [verifier](agents/verify-dx-api-docs-F2/REPORT.md) · [finder (DF)](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F3/REPORT.md)

### DF-F4 — Manifest `max-size` silently skipped on generic error types while docs claim it caps every error — medium, correctness

An explicit per-type `size(...)` on a generic error type is a hard error ("`size(...)` cannot be combined with generic parameters"), but the manifest `max-size` cap is silently skipped for generic types via `cap_info.filter(|_| input.generics.params.is_empty())` (crates/oopsie-macros/src/derive/mod.rs:84 enums, :118 structs). The doc (keyword_docs/container/size.md) says max-size "caps every error derived in that crate, as if each carried `#[oopsie(size(..=N))]`" — false for generic types: per-type form errors, manifest form no-ops. Silent feature-dependent behavior with no warning; a project relying on the cap gets false safety for exactly its generic error types.

**Evidence.** /tmp/oopsie-settings with `[package.metadata.oopsie] max-size = 8`: generic `enum E<T: Debug> { A { v: T } }` compiles warning-free (E<u64> is 16 bytes > cap 8); a non-generic oversized enum in the same crate fails with E0080 naming the cap.

**Verifier's reasoning.** Code, docs and reproduction all confirm the claim; no test pins the skip as intentional. The finder's "warning-style diagnostic" suggestion is not implementable from a stable proc macro, but the alternative fix (documenting the exemption) fully addresses the finding. Medium as filed: a real, silent, doc-contradicting false-safety gap; skipping is the only implementable behavior for generic types.

**Suggested fix.** Document the generic-type exemption in the size keyword doc and the settings table (lib.rs:356).

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F4/REPORT.md)

### DF-F5 — `traced(timestamp)` under no_std / `timestamp(chrono = true)` without chrono fails with cryptic `__private` resolution errors — medium, dx

Generated code references `#oopsie_path::__private::SystemTime` (crates/oopsie-macros/src/traced/config.rs:78) or `__private::chrono` (:75), which only exist under the corresponding cargo features (crates/oopsie-core/src/lib.rs cfg-gates them). The macro cannot know the consumer's feature set, so misuse surfaces as name-resolution errors inside an internal module, with no hint about which feature to enable. Everything else about `traced` works under no_std, and crates/nostd-smoke exercises only the untraced path, so CI has no no_std coverage for `traced` at all.

**Evidence.** no_std crate: `error[E0425]: cannot find type 'SystemTime' in module '::oopsie::__private'`. std crate without chrono, `traced(timestamp(chrono = true))`: `error[E0433]: cannot find 'chrono' in '__private'`. Bare `traced` compiles cleanly under no_std — the failure is isolated to the timestamp option.

**Verifier's reasoning.** Code, gating and both errors reproduced verbatim. keyword_docs/traced/chrono.md and the crate-level docs do state the feature/std requirements, which keeps this at medium/dx — but the compiler error names only a hidden internal module, and timestamp.md does not note that bare `timestamp` (SystemTime) is std-only. `nostd_stubs.rs` stubs backtrace/trace markers but not SystemTime/chrono, so deliberately-named stubs can only improve the diagnostic.

**Suggested fix.** Under `not(feature = "std")` / `not(feature = "chrono")`, export deliberately-named stubs from `__private` so the diagnostic names the missing feature; add a `traced` case to crates/nostd-smoke to lock in the supported no_std surface.

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F5/REPORT.md)

### DF-F2 — Invalid per-type `suffix = "..."` string panics the proc macro — medium (adjusted from high), dx

`selector_name` (crates/oopsie-macros/src/derive/gen_selectors.rs:709-720) concatenates the stripped name with a user `suffix` string and calls `ident_maybe_raw`, which only special-cases `self|Self|super|crate|_` into a real error and calls `Ident::new_raw(name, span)` (:740) for everything else — panicking on punctuation/whitespace. The adjacent comment claims names that cannot be raw are "a real error, not a panic" — only true for those five names. The manifest settings path validates identifier fragments (`is_ident_fragment`); the per-type attribute does not. Adjacent wart: `suffix = " "` silently behaves like `suffix(false)` because `"A "` parses as the ident `A`.

**Evidence.** `#[derive(oopsie::Oopsie)] #[oopsie(suffix = "with space")]` → `error: proc-macro derive panicked` / `= help: message: '"Awith space"' is not a valid identifier`, spanned at the whole derive with no mention of `suffix`. (Verifier note: the finder's `#[oopsie::oopsie(suffix=...)]` repro form is rejected as an unknown field by the attribute macro; the reachable path is the derive form — same panic, claim stands.)

**Verifier's reasoning.** Severity adjusted high → medium: triggering requires a malformed suffix string; valid configurations are unaffected and compilation still fails with an error quoting the invalid identifier — a diagnostics defect on invalid input, not wrong behavior in normal use.

**Suggested fix.** Validate the suffix (or the composed selector name) as an identifier fragment — reuse `is_ident_fragment` — and return a spanned `syn::Error` naming the `suffix` attribute and its invalid value instead of falling through to `Ident::new_raw`.

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F2/REPORT.md)

### DX-F5 — Example "Run with:" headers name the wrong feature set — medium, dx

Three of six examples' doc-header commands fail as written because they don't match the `required-features` in crates/oopsie/Cargo.toml:99-117: complete.rs:18 says `-F fancy,tracing` but requires `fancy,tracing,serde`; welp.rs:4 says plain `cargo run --example welp` but requires `fancy`; panic_hook.rs:8 says plain `cargo run --example panic_hook` but requires `fancy,tracing,serde`. (traced.rs, erased_json.rs and quickstart.rs headers are correct.)

**Evidence.** All three reproduced: `cargo build --example welp` → "target `welp` requires the features: `fancy`"; `--example complete -F fancy,tracing` → requires `fancy, tracing, serde`; `--example panic_hook` → same.

**Verifier's reasoning.** Cargo does not auto-enable required features; the error is self-healing (it suggests the exact flag), so low would be defensible, but 3 of 6 onboarding examples shipping broken copy-paste commands is a meaningful first-run DX drag — medium stands.

**Suggested fix.** Spell the full feature list in each header to match the required-features arrays in Cargo.toml.

Reports: [finder](agents/dx-api-docs/REPORT.md) · [verifier](agents/verify-dx-api-docs-F5/REPORT.md)

### CC-F2 — `Backtrace::is_captured()` returns true for zero-frame captures — low, correctness

`is_captured` (crates/oopsie-core/src/backtrace.rs:255-260) matches only the `Inner` variant while the doc (lines 248-250) promises "capture was enabled and the platform produced a stack". On targets where backtrace 0.3.76 falls back to the noop backend (wasm32-unknown-unknown, emscripten, uwp), `Inner::Captured` with empty frames and `is_captured() == true` is constructible, defeating the documented skip-empty filter contract keyed on `is_captured` (`capture_or_extract`, `__private::source_backtrace`, `Welp`'s fallback).

**Evidence.** Doc vs variant-only check at backtrace.rs:248-260; backtrace-0.3.76 noop backend cfg (src/backtrace/mod.rs:202-204); eager frame capture in `new_unresolved` (an emptiness check at construction is free).

**Verifier's reasoning.** Behavioral blast radius is near zero — same-platform shadowing is vacuous (a fresh capture is equally empty), `Welp` forwards the source first and `Request` is first-wins, `ErasedError` independently filters on `!frames().is_empty()` (erased/mod.rs:276), and `TracePrinter` suppresses empty sections. What remains is a public-API doc/behavior mismatch on an edge platform — polish.

**Suggested fix.** In `Backtrace::capture()`, check `backtrace.frames().is_empty()` after `new_unresolved()` and store `Inner::Disabled` when zero frames were recorded; or weaken the doc to "capture was enabled".

Reports: [finder](agents/correctness-core/REPORT.md) · [verifier](agents/verify-correctness-core-F2/REPORT.md)

### SU-F2 — `ErrorChainExt::root_cause` never terminates on a cyclic `source()` chain — low, correctness

The std Error contract does not forbid `source()` cycles — the crate itself says so in the two places it hardens (crates/oopsie/src/report.rs:189-190 and crates/oopsie-core/src/erased/mod.rs:243-253, both capped at 128). But `root_cause_of` (crates/oopsie-core/src/chain.rs:91-95) loops `while let Some(source) = cause.source()` with no cap, so `err.root_cause()` spins forever on a cyclic chain. The `Chain` iterator shares the property but discloses it in its docs (chain.rs:11-15); `root_cause`'s docs do not.

**Evidence.** Empirical: /tmp/f2-verify with `fn source(&self) -> Option<&(dyn Error + 'static)> { Some(self) }` (compiles via lifetime elision — the same pattern as the crate's own `Cyclic` test) — `err.root_cause()` still spinning after 3 s, SIGKILLed. `ErrorChainExt` is re-exported from oopsie's prelude and used in examples/quickstart.rs:48.

**Verifier's reasoning.** Public user-facing API via a blanket impl; a genuine hang, not theoretical; the hardening inconsistency and doc asymmetry are real. Low: the trigger requires a pathological self-referential error type no normal user writes.

**Suggested fix.** Cap `root_cause_of` at the same 128 and return the node at the cap (documenting the bound), or document non-termination as the `Chain` iterator docs already do.

Reports: [finder](agents/soundness-unsafe/REPORT.md) · [verifier](agents/verify-soundness-unsafe-F2/REPORT.md)

### SU-F3 — Test-only unsafe `env::set_var` whose justification does not hold under the default test harness — low, correctness

Edition 2024 makes `env::set_var` unsafe because POSIX setenv races with concurrent getenv/environ reads. The `#[expect(unsafe_code, reason = "test setup requires setting an env var at runtime")]` at crates/oopsie-core/src/extras.rs:269-277 states why the unsafe exists, not why it is safe. Under the default libtest harness (`cargo test -p oopsie-core`), sibling tests in the same binary read the environment concurrently (backtrace.rs:49,79 read RUST_LIB_BACKTRACE/RUST_BACKTRACE; test_utils.rs:118-121 reads CARGO_HOME/HOME), so the no-concurrent-access invariant is not upheld. It is upheld only under cargo nextest (process-per-test), which the repo's `just test` and CI use — latent rather than actively triggered.

**Evidence.** extras.rs:269-277; concurrent readers at backtrace.rs:49,79 and test_utils.rs:118-121; justfile:41-58 and ci.yml:37-50 show nextest isolation is what currently makes this safe. Contrast with the repo's own correct invariant-based SAFETY comment (benches/vs_ecosystem.rs:74-88) and the re-exec pattern (crates/oopsie/tests/color_env.rs).

**Verifier's reasoning.** All factual assertions verified; glibc setenv can realloc/free environ storage while an unlocked concurrent getenv traverses it — a real violation of set_var's safety contract, and the variable being test-private does not help. Low: test-only, never triggered by the repo's tooling, but a genuine latent soundness violation falling short of the repo's own standard.

**Suggested fix.** Re-exec the env-mutating probe in a child process (the color_env.rs pattern), or gate on a serialized mutex; at minimum state the real no-concurrent-access invariant in the expect reason.

Reports: [finder](agents/soundness-unsafe/REPORT.md) · [verifier](agents/verify-soundness-unsafe-F3/REPORT.md)

### CMD-F6 — Synthetic generic names `__T` / `__T{i}` collide with user-declared parameters (E0403) — low, correctness

`fail`'s synthetic Ok-type param `__T` (crates/oopsie-macros/src/derive/gen_selectors.rs:956-965) and the Into params `__T{i}` (:88) are emitted without probing the user's declared parameter names. A user param named `__T` yields `pub fn fail<__T, __T>` → E0403; a user param named `__T0` collides with the synthesized Into param in the selector struct decl and method generics. Additionally, `sourced_impl` identifies synthetic params via `param_name.starts_with("__T")` (:196), which also matches a user param named `__T0` and duplicates it in impl generics — an independent defect isolated by the verifier. The probing pattern the fix needs already exists: gen_display.rs:135-138 probes `__oopsie_fmt` against declared names. Cross-reference: CMD-F7, DF-F7 (same class).

**Evidence.** `enum DunderT2<T: Debug, __T: Debug> { A { value: T }, B { other: __T } }` → `error[E0403]: the name '__T' is already used for a generic parameter`; same for `__T0`.

**Verifier's reasoning.** All three sub-claims reproduce with concrete compiler output; a control case compiles cleanly, confirming the projection + prefix-match combination is required. Low: deliberately naming a param `__T`/`__T{i}` is legal but unusual; the failure is a loud, comprehensible compile-time error with an immediate workaround (rename).

**Suggested fix.** Choose synthetic names by probing the declared parameter set (skip taken names), as `gen_display::formatter` already does; track synthetic params structurally instead of by name prefix.

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F6/REPORT.md)

### CMD-F7 — A field named `__request` shadows the mangled provide parameter (E0599, nightly only) — low, correctness

The `Error::provide` parameter is mangled to `__request` (crates/oopsie-macros/src/derive/gen_error.rs:191 enum, :723 struct) to survive a field named `request`, but a field named `__request` is bound by the generated destructure and shadows the parameter, so `#req.provide_ref::<Backtrace>(...)` resolves against the user's field. Only reachable with `unstable-error-generic-member-access` (stable emits no provide method). Cross-reference: CMD-F6, DF-F7 (same class).

**Evidence.** Enum variant `{ __request: u32, #[oopsie(backtrace)] bt: oopsie::Backtrace }` on the repo-pinned nightly with the feature → `error[E0599]: no method named 'provide_ref' found for reference '&u32'` at the `#[oopsie::oopsie]` attribute.

**Verifier's reasoning.** Both idents are call-site spanned so hygiene cannot help; all user fields are bound precisely so provide exprs can reference them; no reserved-name guard exists. Low: loud compile error, opt-in nightly feature, pathological field name.

**Suggested fix.** Dedup the mangled parameter name against field names, the same probing approach as `formatter()`.

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F7/REPORT.md)

### CMD-F8 — A source field whose type's last segment is `Backtrace`/`SpanTrace` is dual-classified as source and trace field (E0416/E0025) — low, correctness

`CategorizedFields::from_fields` (crates/oopsie-macros/src/derive/parse.rs:1722) records `backtrace_field`/`spantrace_field` by last-segment type match independent of the later source classification. An error type literally named `Backtrace` used as a source field produces a `Diagnostic` accessor arm whose pattern binds the same field twice, plus a bogus `Borrow::<oopsie::Backtrace>::borrow(source)` body. The post-loop ambiguity checks (parse.rs:1805-1843) have no source-vs-trace-field overlap check.

**Evidence.** `struct Backtrace; impl Error for Backtrace; enum DualError { Wrap { source: Backtrace, n: u32 } }` → `error[E0416]: identifier 'source' is bound more than once in the same pattern`; `error[E0025]: field 'source' bound multiple times`.

**Verifier's reasoning.** `is_backtrace_type`'s own doc comment acknowledges the false-positive trade-off. The suggested fix is safe: `oopsie::Backtrace` does not implement `Error`, so a real backtrace can never legitimately be a source field. Low: niche naming collision of a documented heuristic; compile-time failure with span on the field.

**Suggested fix.** Skip trace-field detection for the field classified as the source (or reject the overlap with a targeted error in the existing ambiguity-rejection style).

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F8/REPORT.md)

### CMD-F10 — Generated `From` impls for transparent items can collide (E0119) with no targeted diagnostic — low, correctness

(a) Two transparent variants with the same source type generate duplicate `From<S>` impls → raw E0119 with both spans pointing at the same `#[derive(Oopsie)]` attribute (thiserror diagnoses duplicate `#[from]` itself). (b) A transparent variant with an auto-boxed self source (`source: Box<Self>`) generates `From<Self> for Self`, conflicting with core's blanket `impl<T> From<T> for T` — genuinely unexpressible, so it needs a macro-side error.

**Evidence.** `enum DupTransparent { A { source: io::Error } (transparent), B { source: io::Error } (transparent) }` → E0119; `enum SelfBox { Wrap { source: Box<SelfBox> } (transparent), Leaf }` → E0119 vs core blanket. Code sites: gen_selectors.rs:462-464 (no dedup), model.rs:67-118 (resolve checks only selector names), parse.rs:930-941 (AutoBoxed unwrap, no Self check).

**Verifier's reasoning.** Both rejected shapes are hard errors today, so the fix breaks nothing; the finding claims only a missing targeted diagnostic, which is accurate. Low: diagnostic-quality gap only.

**Suggested fix.** Detect same-source-type transparent duplicates in `ResolvedEnum::resolve`; reject `AutoBoxed` sources whose inner type is the error type itself, both with targeted errors.

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F10/REPORT.md)

### CMD-F11 — `vis` (and struct `suffix`) are silently ignored on transparent items — low, dx

`VariantAttrs` accepts `vis` and `StructAttrs` accepts `vis`/`suffix`, but the transparent code paths (crates/oopsie-macros/src/derive/gen_selectors.rs:428 variant, :609 struct) emit only a `From` impl and never read them — the attributes are silently dropped, inconsistent with the codebase's otherwise careful inert-attribute errors (e.g. `reject_inert_variant_traced`, and the `forward(...)`-on-transparent rejection). Adjacent observation: `module(...)` on a transparent struct is also silently ignored.

**Evidence.** `#[oopsie(display("x"), transparent, vis(pub))]` on a variant and `#[oopsie(display("x"), transparent, vis(pub), suffix("Blah"))]` on a struct build with zero warnings; referencing the would-be selector fails with E0425, proving the attributes have zero effect.

**Verifier's reasoning.** Exhaustive grep shows no consumer of `.visibility()` reachable from a transparent item; no test pins this as intentional. Low: silent no-op attribute / DX inconsistency, no runtime misbehavior.

**Suggested fix.** Reject `vis`/`suffix` on transparent items in `validate_transparent` with a targeted error (requires passing the parsed attrs in).

Reports: [finder](agents/correctness-macros-derive/REPORT.md) · [verifier](agents/verify-correctness-macros-derive-F11/REPORT.md)

### CMTR-F3 — Pre-existing SystemTime/DateTime field suppresses timestamp injection but is neither auto-captured nor provided — low, correctness

`check_existing_fields` (crates/oopsie-macros/src/traced/inject.rs:45) sets `has_timestamp` for any field whose type's last segment is `SystemTime` or `DateTime`, suppressing injection. But `FieldAttrs::from_field` (derive/parse.rs:919-926) auto-captures only backtrace/spantrace/traces/location-typed fields — not timestamp-typed ones — so the user's field stays an ordinary caller-supplied selector field, and the `provide` attr for `timestamp(provide = true)` is only attached to the *injected* field. Net: `traced(timestamp(provide = true))` with a pre-existing SystemTime/DateTime field is silently inert — unlike a pre-existing Backtrace field, which is auto-captured and surfaced by type. The suppression itself is deliberate (unit-test-pinned); the gap is the missing capture/provide follow-through.

**Evidence.** Probe: `#[oopsie::oopsie(traced(timestamp))] struct TsProbe { pub at: SystemTime, pub msg: String }` — `TsProbeOopsie { msg }.build()` fails E0063 missing field `at` (field is selector-supplied, injection suppressed); on nightly, `request_value::<SystemTime>(&err)` returns `None` despite `provide = true`.

**Verifier's reasoning.** All three load-bearing facts verified; docs never describe the silent no-op; no validation can catch it. Fix nuance: full parity (auto-capturing timestamp-typed fields) would risk silently removing common domain fields (`created_at: SystemTime`) from selectors — a diagnostic on suppression is the safer remedy. Low: silent no-op of an opt-in feature with a niche trigger.

**Suggested fix.** Emit a diagnostic when `traced(timestamp)` is requested but suppressed by an existing field (preferred), or treat timestamp-typed fields like the other trace fields.

Reports: [finder](agents/correctness-macros-traced-runtime/REPORT.md) · [verifier](agents/verify-correctness-macros-traced-runtime-F3/REPORT.md)

### CMTR-F4 — Bottom-peel frame filter strips user frames of a crate named `test` — low, correctness

`is_runtime_tail_code` (crates/oopsie/src/trace_printer.rs:440) classifies every frame whose owning crate is `std | core | alloc | test` as runtime tail plumbing. `test` is the harness crate, but it is also a legal user package name. For an app whose crate is named `test`, the contiguous bottom peel strips `test::main` and every `test::*` frame below the first non-test frame. The verifier's end-to-end repro showed the impact is worse than filed: with a real binary named `test`, ZERO of 23 captured frames survived filtering — the peel stops at the non-internal `oopsie_core::...Capturable>::capture` frame with keep>0, so the peel-to-nothing guard never fires and the top-cutoff hides the rest. A completely frameless backtrace in the common single-crate case.

**Evidence.** Public-API probe: frames `[test::work, foo::helper, test::main, std::rt::lang_start, _main]` → kept `[test::work, foo::helper]`, while `my_crate::main` survives on an identical stack. End-to-end repro with `[package] name = "test"`: empty kept-frame list after `error_backtrace_frame_filter`.

**Verifier's reasoning.** `test` is a legal package name (the repro built and ran); existing tests (`runtime_tail_spares_user_spellings`) never cover a crate named exactly `test`. Fix caveat: dropping `"test"` from `std_owned` alone would regress the harness spellings asserted at trace_printer.rs:1743-1744 — the scoped tables must be extended too. Low: total failure for affected users, but the trigger is rare.

**Suggested fix.** Drop `"test"` from the crate-ownership rule and recognize the harness by its known entry paths instead (extend the scoped RUNTIME_INIT/RUNTIME_TAIL tables with `test::run_test*`; `test::__rust_begin_short_backtrace` is already scoped).

Reports: [finder](agents/correctness-macros-traced-runtime/REPORT.md) · [verifier](agents/verify-correctness-macros-traced-runtime-F4/REPORT.md)

### CMTR-F5 — `Theme` docs claim it is `Copy`; the derive does not include it — low, dx

crates/oopsie/src/theme.rs:16-17 rustdoc on the public `Theme` struct says "Small and `Copy`"; line 21 derives only `Clone, Debug, Eq, PartialEq`. A user relying on the documented contract (`let b = a; let c = a;`) gets E0382. All nine fields are `(u8,u8,u8)` tuples (27 bytes, matching the `size_of <= 32` const assert), so adding `Copy` is trivially valid.

**Evidence.** Doc vs derive at theme.rs:16-21; workspace-wide grep for `impl Copy` found nothing.

**Verifier's reasoning.** Accurate on every point; no runtime impact — purely a misleading rustdoc line on a public type. One-line fix either direction.

**Suggested fix.** Add `Copy` to the derive (matching the doc), or correct the doc comment.

Reports: [finder](agents/correctness-macros-traced-runtime/REPORT.md) · [verifier](agents/verify-correctness-macros-traced-runtime-F5/REPORT.md)

### CMTR-F6 — `Tristate::explicit` doc comment contradicts the code — low, dx

The doc comment at crates/oopsie-macros/src/traced/args.rs:32 says "A settings block (`key(...)`) always counts as explicit-on", but `self.explicit.then(|| self.inner.is_enabled())` yields `Some(false)` for `timestamp(enabled = false)` / `code(enabled = false)` settings blocks — explicit-OFF. Per the repo's own comment rules, a wrong-about-the-code comment is worse than none.

**Evidence.** args.rs:31-35; `traced(timestamp(enabled = false))` parses (FieldSetting::from_meta routes non-LitBool Meta::List to Settings, which has `enabled: Option<bool>`); the `enabled` key is used by the repo's own keyword_docs test. `cargo test -p oopsie-macros --lib`: 233 passed.

**Verifier's reasoning.** Reachable, supported syntax falsifies "always explicit-on". Minor imprecision in the claim: the precedence tests it cites rely on the Flag form (`timestamp = false`), not the settings-block path, which is untested — does not affect the verdict.

**Suggested fix.** Reword to "a settings block counts as explicit, with the state taken from its `enabled` key (default on)".

Reports: [finder](agents/correctness-macros-traced-runtime/REPORT.md) · [verifier](agents/verify-correctness-macros-traced-runtime-F6/REPORT.md)

### PERF-F1 — `Report::new` eagerly symbolicates the backtrace even when the report is never rendered — low, performance

`resolve_backtrace` (crates/oopsie/src/report.rs:56-60, called from new/run/from_residual) unconditionally clones the error's backtrace and calls `backtrace.resolve()` at `Report` construction. The doc comment justifies it as "repeated rendering never re-symbolicates", but that property is already guaranteed by the `LazyLock` inside `oopsie_core::Backtrace` (clones share one `Arc<Lazy>`). The eager call buys nothing for the rendered case and is pure waste when a `Report` is constructed but never displayed.

**Evidence.** Timing harness (release+debuginfo): `Report::new` with backtrace enabled but never rendered costs 17.4 µs vs 8.0 µs for bare error capture (~9 µs of symbolication at construction; finder measured ~7 µs / 293 allocs). `Report::ok()` costs 0.

**Verifier's reasoning.** No correctness reason for eager resolution exists: all render paths force the same shared LazyLock, and symbolication uses captured frame IPs so deferring past stack unwind is safe. The fix must also move the `frames().is_empty()` pre-filter into `write_backtrace`. Low: ~7–17 µs once per constructed-but-unrendered Report, only with backtraces enabled; the dominant `main() -> Report` path always renders anyway.

**Suggested fix.** Defer resolution to the first Display/Debug render (the LazyLock dedupes it); move the empty-frames pre-filter into `write_backtrace`, which already early-returns for `None`.

Reports: [finder](agents/performance/REPORT.md) · [verifier](agents/verify-performance-F1/REPORT.md)

### PERF-F2 — Every `Report` render re-materializes all backtrace frames (per-Display allocation) — low, performance

`BacktraceProvider::frames()` for `Backtrace` (crates/oopsie/src/trace_printer.rs:116-129) clones every symbol name into a `Box<str>` and every filename into a `Box<Path>` on each call; `write_backtrace` (:726-730) then allocates a second `Vec<Option<&BacktraceFrame>>` of the same length, and the all-masked early return (:734) happens only afterwards. Symbol resolution itself is cached, but the materialization is not: displaying the same `Report` twice (log::error! then Termination, or Debug after Display — both ordinary usage) re-does all of it with zero amortization.

**Evidence.** Structurally guaranteed by the code (Vec::collect + per-symbol Box + second Vec); counting-allocator harness: render #1 and render #2 each cost 50 allocations in a tiny binary, scaling with frame count.

**Verifier's reasoning.** Every element confirmed in code; nothing caches the materialized vec anywhere; only inaccuracy is the marginal "re-demangles" wording (platform-dependent). Cold path, genuinely small — the finder's own "polish only" note is right.

**Suggested fix.** Cache the materialized `Vec<BacktraceFrame>` in `Report` at construction, or change `BacktraceProvider` to lend frames (`fn frames(&self, f: &mut dyn FnMut(&BacktraceFrame))`) so a render borrows instead of cloning.

Reports: [finder](agents/performance/REPORT.md) · [verifier](agents/verify-performance-F2/REPORT.md)

### PERF-F3 — `SpanTrace::eq` allocates a String per span per comparison in debug builds — low, performance

In debug builds, every `==` on a `SpanTrace` eagerly walks the whole left span stack and `to_owned()`s each span's formatted fields into a `VecDeque` (crates/oopsie-core/src/spantrace.rs:93-101, :116-140) before any comparison — so O(spans) Strings are allocated per comparison even when the traces diverge on the first frame or the right side is empty. A user-derived `PartialEq` on an error containing a `SpanTrace` field inherits this. Release builds drop the strings entirely (callsite-only comparison).

**Evidence.** `capture_fields` is `fields.to_owned()` under `cfg(debug_assertions)`; vendored tracing-error 0.2.1 `with_spans` signature (`FnMut(&'static Metadata, &str) -> bool`) confirms retaining fields requires allocation. `cargo test -p oopsie-core --features tracing spantrace`: 27 passed.

**Verifier's reasoning.** The eager full-left collection is not load-bearing for the deliberate debug/release semantics — a lazy pairwise walk with identical behavior keeps all tests green and allocates nothing on the common unequal path. Low: debug-only, requires an active ErrorLayer subscriber at comparison time, and the crate itself never calls eq outside tests.

**Suggested fix.** Compare incrementally: collect only callsites (pointers, no allocation) and compare field strings lazily only when callsites match; or pre-check span counts so the common unequal path allocates nothing.

Reports: [finder](agents/performance/REPORT.md) · [verifier](agents/verify-performance-F3/REPORT.md)

### DX-F3 — Broken intra-doc links when building docs for no_std — low, dx

Under `cargo doc --no-default-features`, 7 `broken_intra_doc_links` warnings fire because doc comments link to `std::` paths that don't exist in a no_std build: crates/oopsie-core/src/diagnostic.rs:46-47 (`std::process::Termination`, `std::process::ExitCode`), traits.rs:37 (`std::time::SystemTime`, `std::time::Instant`), welp.rs:422,424 (`std::fmt::Display`, `std::error::Error::source`), and crates/oopsie/src/lib.rs:77 (`start_marker!`, std-gated but referenced unconditionally). docs.rs builds all-features so published docs are unaffected; no CI lane runs rustdoc under true no_std (every CI doc config transitively enables std).

**Evidence.** `cargo doc --workspace --no-deps --no-default-features` → exactly the 7 claimed warnings at the claimed locations.

**Verifier's reasoning.** Reproduces verbatim; no_std is a first-class configuration (dedicated stub module, CI no_std build lanes, nostd-smoke crate); the suggested fix matches existing practice (`cfg_attr(feature, doc)` blocks already used; `core::fmt::Display`/`core::error::Error::source` resolve in both builds).

**Suggested fix.** Link to `core` where the item exists there, use plain code spans for std-only items, or cfg-gate doc text; add a CI/just step running `cargo doc --no-default-features`.

Reports: [finder](agents/dx-api-docs/REPORT.md) · [verifier](agents/verify-dx-api-docs-F3/REPORT.md)

### DX-F4 — README no_std feature table lists `test-utils`, which is not a feature of the `oopsie` facade — low (adjusted from medium), dx

README.md:92 has a row `test-utils | implies std`, but `test-utils` exists only on oopsie-core (crates/oopsie-core/Cargo.toml:24); the facade has no such feature. A user writing `features = ["test-utils"]` on the oopsie dependency gets a hard cargo resolution error. The table also omits `settings`, a genuine facade feature that is proc-macro-only and therefore no_std-compatible, and never says which crate each row belongs to.

**Evidence.** Throwaway crate with `features = ["test-utils"]` on oopsie → "package depends on 'oopsie' with feature 'test-utils' but 'oopsie' does not have that feature". The facade's own doc table (crates/oopsie/src/lib.rs:319-332) correctly includes `settings` and omits `test-utils`.

**Verifier's reasoning.** Severity adjusted medium → low: the failure message lists all valid features, so a user who hits it recovers in seconds — docs polish, not a meaningful DX drag.

**Suggested fix.** Drop the `test-utils` row (or annotate it as oopsie-core-only) and add a `settings` row.

Reports: [finder](agents/dx-api-docs/REPORT.md) · [verifier](agents/verify-dx-api-docs-F4/REPORT.md)

### DX-F6 — CHANGELOG rc.18 claims the default build "renders plain reports" — low, dx

CHANGELOG.md:37 says "The default build is now dependency-light and renders plain reports." There is no report rendering without `fancy`: `Report`, trace_printer, `Theme`, and the panic hook are all `#[cfg(feature = "fancy")]` (crates/oopsie/src/lib.rs:505-548), and this was already true at the rc.18 commit itself — the entire rc.17..rc.18 behavioral change was the one-line Cargo.toml default-features edit. With default features a user has no report type at all, plain or otherwise. The only non-fancy plain-text renderer (`ErasedError::to_text`) is serde-gated, also non-default.

**Evidence.** `git show aacb8a0` (one-line default change) and `git show v0.1.0-rc.18:crates/oopsie/src/lib.rs` (Report already fancy-gated); cfg gates on current develop.

**Verifier's reasoning.** Factually wrong both at the tag and on current develop; the likely intended meaning (reports *would* render plain without fancy's colors) is not what the sentence says. Low: changelog inaccuracy, loud compile error for anyone misled.

**Suggested fix.** Reword to make clear reports require opting into `fancy` (e.g. "the default build no longer includes any report renderer").

Reports: [finder](agents/dx-api-docs/REPORT.md) · [verifier](agents/verify-dx-api-docs-F6/REPORT.md)

### DX-F7 — CHANGELOG reference links defined for only 3 of 20 version headers — low, dx

Link targets are defined only for [0.1.0-rc.1], [rc.2] and [rc.3] (CHANGELOG.md:239-241). Every header from [0.1.0-rc.20] through [rc.4] uses the same bracket syntax with no matching definition, so the three oldest versions render as hyperlinks and the seventeen newest as dead text. All 20 git tags exist, so every `releases/tag/vX` URL would be valid; the file is hand-maintained (no cargo-release changelog hooks), so the asymmetry is manual drift.

**Evidence.** `grep -n '^\[.*\]: ' CHANGELOG.md` → exactly 3 definitions; 20 headers all using reference-style syntax.

**Verifier's reasoning.** The preamble cites Keep a Changelog (which prescribes linked version headers) and the three working definitions prove the syntax is intended as links. Pure docs polish.

**Suggested fix.** Add the 17 missing link definitions (or drop the brackets from headers).

Reports: [finder](agents/dx-api-docs/REPORT.md) · [verifier](agents/verify-dx-api-docs-F7/REPORT.md)

### DX-F8 — no_std crate docs omit `extras` from the std-only list — low, dx

The no_std section (crates/oopsie/src/lib.rs:334-343) says fancy enables std and that "Panic hooks, backtraces, `tracing`, and clock-based timestamps (`chrono` / `jiff`) are also `std`-only; everything else ... works unchanged." But `extras` also forces std (crates/oopsie/Cargo.toml:35: `extras = ["std", ...]`; oopsie-core/Cargo.toml:27 adds `dep:pastey`), as README.md:88 correctly notes. A no_std user enabling extras silently gets std pulled in — a build failure on targets without std — contradicting the "everything else works unchanged" claim.

**Evidence.** Both Cargo.tomls make extras imply std; grep shows no other mention of extras-as-std-only in lib.rs; the README explicitly defers to the crate docs as the feature-flag reference, making lib.rs the outlier.

**Verifier's reasoning.** The "everything else" sentence reads as exhaustive; on the repo's own thumbv7em smoke target, `default-features = false, features = ["extras"]` fails to build. Low: docs-only omission with a clear compile error as the failure mode.

**Suggested fix.** Add `extras` to the std-only enumeration in the no_std paragraph.

Reports: [finder](agents/dx-api-docs/REPORT.md) · [verifier](agents/verify-dx-api-docs-F8/REPORT.md)

### DF-F6 — Unknown `#[oopsie(...)]` key on a struct loses the Available-values list; struct-level `traced` gets no guidance — low, dx

An unknown key on an enum container errors "Unknown field: `bogus_key`. Available values: `exit_code`, `module`, `path`, `size`, `suffix`, `vis`", while the same key on a struct errors bare "Unknown field: `bogus_key`" (crates/oopsie-macros/src/derive/parse.rs:649-668 — darling behavior difference). And `#[oopsie(traced)]` on a derive-struct gives the bare "Unknown field: `traced`" with no pointer to the attribute macro, while enum variants get a dedicated message via `reject_inert_variant_traced`.

**Evidence.** Reproduced verbatim in /tmp/oopsie-scratch for all three cases. StructAttrs declares no `traced` field and passes darling's error through unmodified; VariantAttrsInner declares `traced` so variant-level `traced` gets the dedicated post-parse diagnostic.

**Verifier's reasoning.** No trybuild fixture pins the struct-side output, so the suggested fix is additive and cannot break pinned behavior. Low: compilation still fails with the key named and correctly spanned; only the guidance is degraded.

**Suggested fix.** Intercept the darling error in `StructAttrs::from_attrs` (or add a `traced` pre-pass mirroring `reject_inert_variant_traced`) so struct users get the available-values list and a targeted hint for `traced`.

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F6/REPORT.md)

### DF-F7 — User field colliding with an injected trace field name produces 6 cascading rustc errors — low, dx

`check_existing_fields` (crates/oopsie-macros/src/traced/inject.rs:21-53) suppresses injection by type and by mangled name only for `__oopsie_timestamp`. A user field named `__oopsie_traces` (or `__oopsie_backtrace`/`__oopsie_location`/`__oopsie_spantrace`) of an unrelated type does not suppress injection, so the macro emits a duplicate field and the user gets six call-site-spanned errors (E0416, E0124, E0025 x2, E0308, E0062), all pointing at `#[oopsie::oopsie(traced)]` rather than at their field — including a misleading E0308 "expected Box<(Backtrace, SpanTrace)>, found u8". Cross-reference: CMD-F6, CMD-F7 (same class of synthetic-name collisions).

**Evidence.** `#[oopsie::oopsie(traced)] enum E { A { __oopsie_traces: u8, v: u8 } }` → exactly the 6 claimed errors, each spanned at the attribute line 1:1.

**Verifier's reasoning.** The `__oopsie_timestamp` name-check exists for re-expansion of already-injected fields, not collision protection — the asymmetry is real. The fix does not break the re-expansion guard since injected fields are trace-typed and still suppressed by type. Low: requires colliding with the internal mangled prefix; fails loudly.

**Suggested fix.** Check all injected idents by name and, on collision with a non-trace-typed field, emit one targeted error at the user field: "`__oopsie_traces` conflicts with a field injected by `traced`; rename it".

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F7/REPORT.md)

### DF-F8 — Container keywords swallowed as display args on variants yield "cannot find value `module`" — low, dx

`reject_keyword_args` catches `#[oopsie("fmt", transparent)]`-style mistakes, but on an enum variant it only flags VARIANT_KEYWORDS (crates/oopsie-macros/src/derive/parse.rs:421-429). A container keyword in the same position is parsed as a format argument and fails later in generated code with a resolution error, even though container keywords are never legal on a variant.

**Evidence.** `#[oopsie("x", module)]` on a variant → `error[E0425]: cannot find value 'module' in this scope` (+ "argument never used"); `#[oopsie("x {}", module(false))]` → E0425 "cannot find function `module`". `#[oopsie("x", transparent)]` → friendly targeted error, as claimed.

**Verifier's reasoning.** Legitimate uses (a variant field named `module` used as display arg) are preserved by the suggested fix because the field-name check runs first; the only new rejection would be an in-scope const sharing a container-keyword name — the same tradeoff already accepted for variant keywords.

**Suggested fix.** In the `DisplayScope::Variant` arm of `reject_keyword_args`, also reject CONTAINER_KEYWORDS when the variant has no field of that name, with a message pointing at the enum-level placement.

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F8/REPORT.md)

### DF-F9 — `vis = pub(crate)` bare form errors with "expected an expression" — low, dx

The bare `vis = pub(crate)` form fails with "expected an expression" spanned at `pub`; nothing tells the user the two accepted spellings (`vis(pub(crate))` or `vis = "pub(crate)"`). syn parses `Meta::NameValue` values as `syn::Expr`, `pub` is not an expression, so the failure occurs before darling's `SynParse<Visibility>::from_meta` ever runs; the doc comment at parse.rs:288-294 documents this as a known limitation.

**Evidence.** `#[oopsie(vis = pub(crate))]` → `error: expected an expression` at the `pub` token, no mention of `vis` or the accepted forms. Reproduced byte-for-byte.

**Verifier's reasoning.** The suggested token pre-pass cannot false-positive (bare `vis = pub` is always invalid; the quoted form is a string literal). Framing nuance: sibling bare-value forms (e.g. `suffix = Ctx`) produce equally generic darling errors, so `vis` is not a unique outlier — a general darling bare-value DX weakness.

**Suggested fix.** Pre-scan `#[oopsie(...)]` attribute args for `vis = pub` and error with "`vis` takes `vis(pub(crate))` or `vis = \"pub(crate)\"`, not a bare expression".

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F9/REPORT.md)

### DF-F10 — Packed-boxing conflict error is spanned at the item, not the offending toggle — low, dx

`resolved.validate(args_span)` receives `item.span()` (crates/oopsie-macros/src/oopsie_attr/mod.rs:119, :175) even though the parameter is named `args_span`. The packed/boxed coherence error therefore highlights the item's leading keyword (`pub`/`enum`) instead of the conflicting `boxed` toggle inside `traced(...)`. The message itself is good; the span is not actionable. The pinned compile-fail snapshots (packed_incoherent_boxing.stderr) encode the current span — they would need updating, not refutation.

**Evidence.** `#[oopsie::oopsie(traced(spantrace(boxed = false)))]` → coherence error spanned at `pub enum E` with the caret under `pub` (live repro).

**Verifier's reasoning.** The same file already spans other errors into the attribute args (mod.rs:46-51, 61-67 via `new_spanned`), so threading the traced(...) meta span is consistent with existing practice. Minor detail correction: the caret lands on the item's first token, not the type name as the finder said.

**Suggested fix.** Thread the traced(...) meta span (available during `OopsieAttrArgs::from_list`) into expand_enum/expand_struct and pass it to `validate`.

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F10/REPORT.md)

### DF-F11 — SynParse errors leak the literal placeholder `key` to users — low, dx

`SynParse::from_meta`'s error strings (crates/oopsie-macros/src/utils/mod.rs:186-194) literally say "expected a value, e.g. `key(...)` or `key = \"...\"`" — the word `key` is a template placeholder never substituted with the actual attribute name. Spans are correct; only the text is off. Scope correction: only `vis` uses SynParse; `path` is `Option<syn::Path>` via darling's native FromMeta and produces different errors.

**Evidence.** `#[oopsie(vis)]` → "error: expected a value, e.g. `key(...)` or `key = \"...\"`"; `#[oopsie(vis = 42)]` → "error: expected `key(...)` or `key = \"...\"`". Reproduced verbatim.

**Verifier's reasoning.** Darling does not rewrite `Error::custom` text with the field name; no test pins these messages, so the fix is safe. Diagnostic-wording wart only.

**Suggested fix.** Hand-roll the SynParse use for `vis` with the real key in the message, or post-process darling errors at the `from_attrs` level to rewrite `key` to the actual field name.

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F11/REPORT.md)

### DF-F12 — Error type defined in a fn body without `module(false)` gives a misleading resolution error — low, dx

A fn-local enum without `#[oopsie(module(false))]` produces "cannot find type `E` in this scope" (E0425/E0433) with a nonsense suggestion to rename the type to a variant name (`A` is a generated selector struct; renaming fixes nothing), because the generated module cannot see fn-local items. The docs call out the requirement in two places (crates/oopsie/src/lib.rs:201-203 and keyword_docs/container/module.md:8-10) and `module(false)` fixes it, but the emitted error sends users toward renaming. On stable the macro cannot detect fn-local expansion, so a better in-macro diagnostic is not possible — flagged as a known sharp edge.

**Evidence.** `fn f() { #[oopsie::oopsie] pub enum E { #[oopsie("x")] A } }` → E0425 "cannot find type `E`", help "a struct with a similar name exists — `A`", plus a second E0433 suggesting `Eq`. With `module(false)`: compiles cleanly.

**Verifier's reasoning.** Every factual element reproduces exactly; not a code defect — a documented, unavoidable-on-stable DX papercut where rustc's diagnostic for a known limitation points the wrong way. The only actionable residue is the optional doc tweak of naming the E0425/E0433 symptom in the module keyword doc.

**Suggested fix.** None cheap on stable; keep the docs callout and consider naming the exact E0425/E0433 symptom in the `module` keyword doc.

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F12/REPORT.md)

### DF-F13 — `CARGO_WORKSPACE_DIR` override is honored but undocumented — low, dx

Workspace-root discovery honors a `CARGO_WORKSPACE_DIR` env override (crates/oopsie-macros/src/utils/settings.rs:69, 199-203 — deliberate: documented in the `find_root_from` doc comment and pinned by the `discovery_workspace_override_wins` test), but it appears nowhere in user-facing docs. Cargo never sets this variable, so it only acts when a user deliberately sets it — and no user can discover that from the docs. Setting it silently changes which `[workspace.metadata.oopsie]` table is read.

**Evidence.** Repo-wide grep finds `CARGO_WORKSPACE_DIR` only in the two source locations; the "Project-wide settings" doc section (lib.rs:345-413) documents every key and precedence rule but not the override.

**Verifier's reasoning.** Not dead code, not internal-only; low is right — the affected scenario (hermetic/vendored builds with unusual layouts) is niche.

**Suggested fix.** One line in the "Project-wide settings" section of crates/oopsie/src/lib.rs.

Reports: [finder](agents/dx-diagnostics-features/REPORT.md) · [verifier](agents/verify-dx-diagnostics-features-F13/REPORT.md)

## Uncertain

None. Every filed finding was either confirmed or refuted by its verifier; none landed in needs-investigation.

## Refuted

None. All 46 filed findings survived adversarial verification. (Within CMD-F4, sub-case (c) — a cfg-gated selector collision — was assessed by the verifier as a deliberate, documented design trade-off rather than a defect; the finding stands on sub-cases (a) and (b). Several findings had minor factual corrections noted in their sections — e.g. DX-F1 understated the affected releases, DF-F11's scope narrows to `vis` only, CMTR-F4's real-world impact is worse than filed — but none changed a verdict.)

## Recommended priority order

1. **CMTR-F1** (panic hook / `Termination::report` abort on EPIPE) — the only finding that aborts a process in plausible normal use; small, well-specified fix at two sites.
2. **DF-F1** (`settings` feature unification leak) — build-selection-dependent compile failures and silent cross-crate codegen changes; needs a design decision (drop the feature gate vs. per-manifest opt-in signal), so start early.
3. **CMD-F1** (lifetime bound `T: 'a` misrouted, E0261) — legitimate ordinary Rust fails to compile with a misleading error; fix is localized to `predicate_named_params`.
4. **DX-F1** (README quick start broken) — primary onboarding path broken for three released versions; one-line install fix or quick-start rewrite.
5. **CMTR-F2 / DX-F2 / DF-F3** (stale `fancy | yes` doc row) — one-cell fix, filed three times independently; do together with DX-F1, DX-F4, DX-F6, DX-F7, DX-F8 as a single docs sweep.
6. **SU-F1** (ChainNode recursive Drop/Clone stack overflow) — uncatchable crash from untrusted input in the documented use case; iterative Drop/Clone + Deserialize cap. Pair with **CC-F1** (same type, no_std chain drop) since both touch ChainNode materialization.
7. **CMD-F5, CMD-F9** (cfg-gated fields breaking generated code) — hard compile errors within the macro's advertised cfg support.
8. **CMD-F3** (renamed-crate auto error code silently lost) — silent stable-only feature loss in the exact configuration `path = "..."` exists for.
9. **DF-F4** (manifest max-size silently skipped on generics) — false-safety gap; doc fix suffices.
10. **DF-F5** (cryptic `__private` errors for timestamp feature misuse) — stub-based diagnostics + nostd-smoke coverage.
11. **CMD-F2, CMD-F4, DF-F2** (missing targeted diagnostics: E0207 lifetimes, container-name collisions, suffix panic) — diagnostic-quality batch in gen_selectors/model.
12. **DX-F5** (example headers) — trivial; fold into the docs sweep.
13. Remaining lows in thematic batches: synthetic-name collisions (**CMD-F6, CMD-F7, DF-F7**), macro diagnostics polish (**CMD-F8, CMD-F10, CMD-F11, DF-F6..DF-F13**), core robustness (**CC-F2, SU-F2, SU-F3**), performance polish (**PERF-F1..F3**), doc/comment truthfulness (**CMTR-F3..F6**).
