# Formal Verification Report

## Metadata

- **Target**: `.` (oopsie Rust workspace — crates `oopsie-core`, `oopsie-macros`, `oopsie`, `erased-oopsie`)
- **Mode**: logic
- **Knobs**: run_mode=fast segments_max=unset reviewers=1 verifiers=1 skip_verify=false cheap_models=false
- **Segments**: 6 natural → 4 final (fast consolidation)
- **Blind Verification**: enabled
- **User Instructions**: focus on correctness with regard to error trees (sources), backtraces / spantraces being correctly provided, etc
- **Verification Date**: 2026-06-03
- **Run**: 1 of 1
- **Segmentation Strategy**: by processing stage / by user-focus area (source-chain+provider, backtrace, spantrace+render, derive/inject)
- **Agents Launched**: Phase 1 ×1, Phase 2 ×4, Phase 3 ×1, Phase 4A ×1, Phase 4B ×1 (8 total)
- **Models**: coordinator=opus, phase1=sonnet, phase2=opus, phase3=opus, phase4a=opus, phase4b=sonnet
- **Issues Created**: None — `--issues` not set

## Executive Summary

12 locations were flagged, **all 12 confirmed** (none refuted). Notably, the single blind verifier initially passed the two HIGH findings as "looks fine"; the Phase 4B reconciliation re-instantiated the reviewer's exact adversarial input and reversed both to **MISSED** (i.e. the reviewer was right and the blind pass had only traced the happy path). The findings cluster into three root causes, all squarely on the user's stated focus (error trees / backtraces / spantraces correctly provided):

1. **Name-only trace-field classification with no type corroboration** (RA-001/002/003/009) — the derive keys backtrace/spantrace/traces detection on the field *name* (`traces`, `backtrace`, `spantrace`) with the type predicate OR-ed in rather than required. A user field literally named `traces`/`backtrace`/`spantrace` of the wrong type produces **uncompilable generated code** and/or **silently drops the real captured traces**. This is the highest-severity cluster and directly causes the "traces not provided" symptom.
2. **`provide()` source-before-self ordering** (RA-004/005) — under the first-wins std Provider API, a wrapper that wraps a trace-providing source surfaces the *source's older* backtrace/spantrace instead of its own wrap-site one, and disagrees with the stable `Diagnostic` accessor path.
3. **`Debug` renders an unresolved backtrace as empty** (RA-006/007) — `Backtrace`'s `Debug::fmt` never calls `resolve()`, so the internal-frame filter drops every (unsymbolicated) frame and prints nothing, while `Report`/`ErasedBacktrace` (which resolve first) print the real trace.

Plus contract/cosmetic items: spantrace status-gate asymmetry (RA-008), parenthesized-`Box` guard bypass (RA-010), empty-spantrace header (RA-011), and erased-backtrace inlined-symbol leak (RA-012).

## Verification Flow

```
Phase 1: 1 discovery agent
↓
Phase 2: 4 deep-dive agents (parallel) — from Phase 1's Final Segments list (6 natural, consolidated to 4 under RUN_MODE=fast)
↓
Phase 3: 1 reviewer → 12 locations flagged (2 HIGH, 7 MEDIUM, 3 LOW)
↓
Phase 4A: 1 blind verifier (confirmed RA-006; passed the rest on first read)
↓
Phase 4B: 1 comparison pass — re-adjudicated disagreements, reversed RA-001/002/003 to MISSED, RA-008 to PARTIAL
↓
Phase 5: skipped — --issues not set
```

## Confirmed Issues

### Issue 1: Field named `traces` of a non-tuple type → uncompilable codegen + silent trace drop

- **Severity**: HIGH
- **Location**: `crates/oopsie-macros/src/derive/parse.rs:1006-1011` (gen pass) and `crates/oopsie-macros/src/traced/inject.rs:34-45` (injection pass)
- **Symbol**: `CategorizedFields::from_fields` / `check_existing_fields`
- **Description**: Trace classification keys on the field name with no type corroboration: `is_traces = attrs.traces || ident_str == "traces" || is_traces_type(&field.ty)`. The `ident_str == "traces"` disjunct sets `traces_field = Some(..)` even when the type is not `(Backtrace, SpanTrace)`. Downstream, `gen_error.rs` emits tuple indexing (`&self.traces.0`, `&self.traces.1`, enum-arm `&tf.0`/`&tf.1`, `provide_ref::<Backtrace>(&traces.0)`), which is invalid for a non-tuple type. Simultaneously, `check_existing_fields` sets `has_traces = has_backtrace = has_spantrace = true` on the same name match, suppressing injection of the real backtrace/spantrace fields.
- **Evidence**: Input `#[oopsie(traced)] #[derive(Oopsie)] struct S { traces: Vec<String> }` → generated `Some(&self.traces.0)` is a compile error (`Vec<String>` has no field `.0`); and even setting that aside, no real `Backtrace`/`SpanTrace` is injected, so `oopsie_backtrace()`/`oopsie_spantrace()` would surface nothing. The correct type predicate `is_traces_type` already exists but the name shortcut OR-s past it.
- **Suggested Fix**: Require the type predicate for name-based detection — gate the `ident_str == "traces"` disjunct behind `is_traces_type(&field.ty)` (and mirror the same gate in `check_existing_fields`), or emit a clear compile_error! when a `traces`-named field has the wrong type.
- **Consensus**: Reviewer RA (RA-001 + RA-002, the gen and injection halves of one root cause). Single verifier: blindly "looks fine" → **MISSED** after Phase 4B re-instantiated the adversarial input and confirmed both the compile failure and the trace suppression.

### Issue 2: Field named `backtrace`/`spantrace` of a non-trace type → uncompilable `Borrow::<Backtrace>::borrow`

- **Severity**: MEDIUM
- **Location**: `crates/oopsie-macros/src/derive/parse.rs:1000-1004`
- **Symbol**: `CategorizedFields::from_fields`
- **Description**: Same root cause as Issue 1 for the singular trace fields: `attrs.backtrace || ident_str == "backtrace" || ident_str == "back_trace"` sets `backtrace_field` with no type guard (likewise `spantrace`). `gen_error.rs` then emits `::core::borrow::Borrow::<Backtrace>::borrow(field)`, which fails to compile when the field type does not implement `Borrow<Backtrace>`. The injection pass also suppresses the synthetic backtrace via `is_backtrace_name`.
- **Evidence**: Input `#[oopsie(traced)] #[derive(Oopsie)] struct S { backtrace: String }` (e.g. a textual backtrace from an external system) → generated `Borrow::<Backtrace>::borrow(&self.backtrace)` does not compile; if the name was a mistyped intent, the real trace is also dropped.
- **Suggested Fix**: Same as Issue 1 — require a matching type predicate alongside the name match.
- **Consensus**: Reviewer RA (RA-003). Single verifier: blindly noted only the shadowing edge → **MISSED** the non-trace-type compile path; Phase 4B confirmed.

### Issue 3: `provide()` forwards the source before the wrapper's own backtrace/spantrace → first-wins shadowing

- **Severity**: MEDIUM
- **Location**: `crates/oopsie-macros/src/derive/gen_error.rs:58-93` (enum) and `:331-366` (struct)
- **Symbol**: `gen_enum_error` / `gen_struct_error` (provide_stmts)
- **Description**: The generated `provide()` pushes `Error::provide(source.as_error_source(), request)` first, before the wrapper's own `request.provide_ref::<Backtrace>(&tf.0)` / `<SpanTrace>(&tf.1)`. The std `Request`/`request_ref` API is first-wins (a slot is filled only if still empty), so when both the wrapper and its source provide the same type, the **source's older** trace is surfaced, not the wrapper's freshly-captured wrap-site trace.
- **Evidence**: A `traced` enum variant / struct with a `source` field whose source value itself provides a `Backtrace`/`SpanTrace` (e.g. wrapping another oopsie `traced` error): `core::error::request_ref::<Backtrace>(&wrapper)` returns the inner origin's backtrace. This also makes the unstable Provider-API view disagree with the stable `Diagnostic::oopsie_backtrace`/`oopsie_spantrace` accessors, which read the wrapper's own field (gen_error.rs:135-147, 407-411) — so the two trace-surfacing paths return different traces for the same error. Inconsistent with hand-written `Welp`, whose `Sourced` carries no traces of its own (welp.rs:176).
- **Suggested Fix**: Provide the wrapper's own `Backtrace`/`SpanTrace` *before* forwarding `source.provide(request)`, so the wrap-site trace wins and matches the stable accessor path. (Or document "deepest origin wins" as intentional and make the stable accessors agree.)
- **Consensus**: Reviewer RA (RA-004 enum, RA-005 struct). Single verifier: blindly "looks fine" → **CONFIRMED** after Phase 4B re-read confirmed the first-wins semantics. Note: severity bounded — only triggers when both layers provide the same type, and stable rendering via `Report`/`ErasedError` is unaffected. Gated behind the `unstable-error-generic-member-access` feature.

### Issue 4: `Backtrace::Debug::fmt` never resolves → unresolved backtrace prints empty

- **Severity**: MEDIUM
- **Location**: `crates/oopsie-core/src/backtrace.rs:182-216` (`Debug::fmt`), amplified by `:148-150` (`is_internal_frame`)
- **Symbol**: `Backtrace::Debug::fmt` / `is_internal_frame`
- **Description**: The non-`full` `Debug` path filters frames using `frame.symbols().first()` to read name/filename but never calls `resolve()`. `capture()` builds via `new_unresolved()`, so frames have no symbols until resolved; on an unresolved backtrace both `primary_name` and `primary_filename` are `None`, `is_internal_frame(None, None)` returns `true`, every frame is dropped, and an empty backtrace is printed. `Report::resolve_backtrace` and `ErasedBacktrace::from_backtrace` both `resolve()` first and render the real frames — so the same value renders non-empty via the supported paths and empty via bare `{:?}`.
- **Evidence**: `RUST_BACKTRACE=1` (not `full`), a `Backtrace` captured and never resolved, formatted with `{:?}` → prints nothing. Reachable via `#[derive(Debug)]` on a `traced` error: `format!("{:?}", err)` hits this path and shows an empty backtrace while `Report::from_std(err)` shows the full one.
- **Suggested Fix**: Call `self.0.clone().resolve()` (or resolve a local copy) before filtering in `Debug::fmt`, mirroring `ErasedBacktrace::from_backtrace`. `RA-007` (the `(None,None) → drop` early return) is correct in isolation and needs no change once `Debug` resolves first.
- **Consensus**: Reviewer RA (RA-006 + RA-007 amplifier). Single verifier: **independently flagged this blind** (the one finding the blind pass caught on its own) → CONFIRMED.

### Issue 5: `OptionalSpanTrace::capture_or_extract` Some-arm has no status gate (contract break)

- **Severity**: MEDIUM (contract) / LOW (user-visible render)
- **Location**: `crates/oopsie-core/src/spantrace.rs:179-187`
- **Symbol**: `OptionalSpanTrace::capture_or_extract`
- **Description**: The `None` arm routes through `Capturable::capture`, which keeps the trace only if `status() == CAPTURED`; the `Some` arm wraps the source's `SpanTrace` verbatim with no status re-check. A source holding an Empty/Unsupported `SpanTrace` (constructed with no active `ErrorLayer` subscriber) still returns `Some(&trace)` from `oopsie_spantrace()`, so a wrapper that extracts it becomes `is_some() == true` for a trace the type's documented invariant ("only contains a span trace if capture was successful") says should be `None`.
- **Evidence**: Wrapper B doing `OptionalSpanTrace::capture_or_extract(&A)` where A's stored trace has status Empty → B.is_some() is wrongly `true`. Render impact bounded (the empty trace yields zero frames; see Issue 8).
- **Suggested Fix**: Re-check `trace.status() == CAPTURED` in the `Some` arm before wrapping, mirroring the `None`/`capture` arm.
- **Consensus**: Reviewer RA (RA-008). Single verifier: blindly "looks fine" → **PARTIAL** after Phase 4B confirmed the status-gate asymmetry; agreed the contract break is real but render impact is bounded.

### Issue 6: `traces` field silently shadows a separately-provided `backtrace`/`spantrace` field

- **Severity**: MEDIUM
- **Location**: `crates/oopsie-macros/src/derive/gen_error.rs:135-162` (enum arms) / `:407-441` (struct methods), provide at `:71-93`/`:344-366`
- **Symbol**: `gen_enum_error` / `gen_struct_error`
- **Description**: All emit sites are structured `if traces_field { … } else if backtrace_field { … }`. For a type carrying both a packed `traces: (Backtrace, SpanTrace)` and a separate `backtrace: Backtrace`, `from_fields` sets both `traces_field` and `backtrace_field`; the `else if` precedence means only `traces.0` is ever surfaced and the user's hand-written `backtrace` field is silently ignored by `Diagnostic`/`provide`. (The `else if` correctly prevents a duplicate-arm compile error — so this is information-loss, not a compile failure.)
- **Evidence**: `struct S { traces: (Backtrace, SpanTrace), backtrace: Backtrace }` → `oopsie_backtrace()` returns `&traces.0`, never the separate `backtrace` field.
- **Suggested Fix**: Emit a `compile_error!` (or a warning) when a packed `traces` field coexists with a standalone `backtrace`/`spantrace` field, rather than silently preferring the tuple.
- **Consensus**: Reviewer RA (RA-009). Single verifier: blindly noted this edge under RA-003 → CONFIRMED.

### Issue 7: Parenthesized `Box<(dyn Error + Send)>` bypasses the auto-box TraitObject guard → unsized `Source`

- **Severity**: LOW
- **Location**: `crates/oopsie-macros/src/derive/parse.rs:552-560`
- **Symbol**: `FieldAttrs::from_field` (auto-box guard)
- **Description**: The guard is `!matches!(inner, syn::Type::TraitObject(_))`. For `Box<(dyn Error + Send)>`, syn parses the inner as `Type::Paren(Type::TraitObject)`, which is not `Type::TraitObject(_)`, so the guard does not fire and the field is rewritten to `Transformed { source_type: (dyn Error + Send), transform: Box::new }`, giving the selector an unsized `type Source = (dyn Error + Send)` → compile error.
- **Evidence**: An explicit source field written as `Box<(dyn Error + Send)>` (parenthesized inner). The mainstream unparenthesized `Box<dyn Error + Send + Sync + 'static>` is correctly classified and skipped.
- **Suggested Fix**: Peel `Type::Paren` (and group) wrappers before the `matches!(inner, Type::TraitObject(_))` check.
- **Consensus**: Reviewer RA (RA-010). Single verifier: blindly analyzed only the common case → CONFIRMED on the parenthesized variant in Phase 4B.

### Issue 8: Spantrace render gated on `is_some()` only → empty `SPANTRACE` header

- **Severity**: LOW (cosmetic)
- **Location**: `crates/oopsie/src/report.rs:192-206` (and erased `lib.rs:147-151`)
- **Symbol**: `Report::write_span_trace`
- **Description**: The render gate is `let Some(span_trace) = … oopsie_spantrace() else { return }`, with no status/emptiness check. An empty `Some` trace (reachable via Issue 5, or any field holding a raw empty `SpanTrace`) passes the gate; the printer emits the `SPANTRACE` header then yields zero frames → a lone header with no body. The backtrace path by contrast guards emptiness (`resolve_backtrace` returns `None` for empty).
- **Evidence**: An empty-but-`Some` spantrace renders a header with no frames.
- **Suggested Fix**: Gate the spantrace header on non-empty/`status() == CAPTURED`, mirroring `resolve_backtrace`'s emptiness guard.
- **Consensus**: Reviewer RA (RA-011). Single verifier: agreed cosmetic/LOW → CONFIRMED. Downstream of Issue 5.

### Issue 9: `ErasedBacktrace::from_backtrace` filters on the primary symbol but emits all symbols → inlined internal-symbol leak

- **Severity**: LOW
- **Location**: `crates/erased-oopsie/src/backtrace.rs:42-64`
- **Symbol**: `ErasedBacktrace::from_backtrace`
- **Description**: `.filter` decides keep/drop on `frame.symbols().first()` (the primary symbol), then `.flat_map` emits *all* symbols of kept frames. A frame whose primary symbol is user code but whose later inlined symbol is internal (e.g. `backtrace::*`) survives the filter, and the internal inlined symbol is materialized into the erased output — so the per-emitted-frame "nothing internal survives" postcondition is not logically guaranteed (only happens to hold for typical stacks). Resolves first (line 40), so this is not an empty-render bug.
- **Evidence**: A captured stack with an internal symbol inlined as a non-primary symbol behind a user-code primary (stack-shape dependent).
- **Suggested Fix**: Apply `is_internal_frame` per emitted symbol inside the `flat_map`, not just to the primary in `.filter`.
- **Consensus**: Reviewer RA (RA-012). Single verifier: confirmed resolution-correctness blind, agreed LOW on the leak → CONFIRMED.

## Disputed Issues

_No disputed issues — single-verifier mode._

## Refuted / Inconclusive Issues

_None — all 12 flagged locations were confirmed (the two HIGH and one MEDIUM that the blind pass initially passed were reversed to MISSED in Phase 4B after re-instantiating the reviewer's adversarial inputs)._

## Low-Confidence Flags (Not Verified)

_None held back — the three LOW-confidence findings (RA-010/011/012) were carried through Phase 4 alongside the HIGH/MEDIUM items and appear as confirmed Issues 7-9 above._

## Notable non-findings (examined and dropped)

These were considered and deliberately **not** flagged, to document coverage:

- **`source()` correctness** — never drops a real cause nor invents a phantom one; the `unreachable!()` on `SourceKind::No` with a present source is genuinely unreachable; enum source match is exhaustive with cfg-propagated arms.
- **`AsErrorSource` autoderef** — correctly resolves `Box<dyn Error + Send + Sync>` to `&(dyn Error + 'static)`.
- **`transparent` + no-source** — emits no orphan impls; `source()`/`provide()`/`Diagnostic` stay complete.
- **Auto-box `TraitObject` guard (mainstream)** — correctly classifies unparenthesized `Box<dyn Error + ...>` and skips the extra box wrapper.
- **`SpanTrace::PartialEq`** — correct across equal / a-longer / b-longer / empty shapes; `with_spans` stops on the first `false`, so `equal` cannot reset; no false-equal between unequal traces.
- **Error-chain walk / `successors` head-exclusion** — no infinite loop, no skipped/duplicated link, no first/last off-by-one, no empty-chain underflow.
- **`code`/`help` read from top error only** — verified consistent across `Report` and `ErasedError`; a deliberate, uniformly-applied product narrowing, not a wrong-output-per-input defect.
- **Backtrace env state machine** (`RUST_BACKTRACE`/`RUST_LIB_BACKTRACE`/override) — no inversion across all combinations; `resolve()` idempotent at frame and backtrace level.
- **`ResolvedTraceArgs::validate`** — rejects exactly the incoherent packed-boxing combo and accepts all coherent ones; struct/enum injection symmetric; injection order can't perturb size assertions.

## Files Generated

- phase1/main-flow-analysis.md
- phase2/fs1-source-provider-flow.md
- phase2/fs2-backtrace-flow.md
- phase2/fs3-spantrace-render-flow.md
- phase2/fs4-derive-inject-flow.md
- phase3/reviewer-a-locations.md
- phase3/reviewer-a-analysis.md
- phase3/all-locations.md
- phase4/verifier-a-blind.md
- phase4/verifier-a-final.md

## Conclusion

The audit converges cleanly on the user's focus areas. The **single highest-value fix** is the name-only trace-field classification (Issues 1, 2, 6): requiring the existing type predicates (`is_traces_type`/`is_backtrace_type`/`is_spantrace_type`) to corroborate every name-based match would close the two HIGH compile-and-drop defects and the MEDIUM shadowing case in one change. The **`provide()` ordering** (Issue 3) is the most directly on-topic correctness issue for "backtraces/spantraces correctly provided" and is worth a deliberate decision: provide-self-first (recommended, matches the stable accessors) vs. document deepest-origin-wins. The **`Debug` no-resolve** (Issue 4) is a small, contained fix with a clear repro and a ready-made reference (`ErasedBacktrace::from_backtrace`). The remaining contract/cosmetic items (5, 7, 8, 9) are low-risk hardening.

Methodological note: the blind verifier passed the two HIGH findings on first read by tracing only the macro-injected happy path (`__oopsie_traces`), and the Phase 4B reconciliation — instructed to break ties by re-reading source rather than defending the prior — correctly reversed them to MISSED. This is the blind/reconcile split working as intended; a single-pass review would have over-trusted the "looks fine" verdict.
