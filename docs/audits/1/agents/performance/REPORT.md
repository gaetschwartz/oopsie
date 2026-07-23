# Performance audit — oopsie workspace

## Scope and method

Dimension: runtime and compile-time performance. Focus files: `trace_printer.rs`,
`report.rs`, `spantrace.rs`, `backtrace.rs`, `erased/mod.rs`, `chain.rs`, `welp.rs`,
plus everything they transitively reach (marker.rs, traits.rs, diagnostic.rs,
lib.rs of both runtime crates, the macro codegen in `oopsie-macros`, and the
criterion benches).

Method:

- Close reading of all focus files and their callees, looking for unnecessary
  allocation/cloning in hot paths, O(n²) behavior in printers, redundant work in
  generated code, and capture happening when disabled.
- Ran the existing criterion benches (`propagate`, `capture`, `render`,
  `construct`, `erased`) for baseline numbers.
- Wrote a throwaway harness under `/tmp/oopsie-perf-audit` (never in the repo)
  with a counting global allocator and `Instant`-based micro-timers to measure:
  allocations per `Report::new` / per render / per `ErasedError::from_error`,
  traced-construction cost with and without subscribers/spans, spantrace
  capture/clone cost, and type sizes.
- Ran `cargo clippy -p oopsie-core -p oopsie --lib` (clean).

Measured baselines (Apple Silicon, release):

| Path | Cost |
|---|---|
| `.context()` propagation, Ok branch | 1.4 ns (pure passthrough) |
| `.context()` propagation, Err branch (bt disabled) | ~100 ns (dominated by `io::Error` construction) |
| `Backtrace::capture()` disabled | 5.3 ns |
| `Backtrace::capture()` enabled (unresolved) | 14 µs (inherent stack walk) |
| `resolve()` (symbolication) | 6.9 µs |
| traced construct, bt disabled, no subscriber / ErrorLayer / 2 spans | 46 / 46 / 66 ns |
| `SpanTrace::capture()` no subscriber / 2 spans | 4 / 40 ns |
| `Report` render (`Report::new(e).to_string()`) | ~34 µs (capture + resolve + render) |
| `ErasedError::from_error` | 22.7 µs / 67 allocs |

## Findings

### F1 — `Report::new` eagerly symbolicates the backtrace even when the report is never rendered

- **File:** `crates/oopsie/src/report.rs:56-60` (called from `new` at :71, `run`
  at :129, `from_residual` at :359)
- **Severity:** low
- **Category:** performance
- **Description:** `resolve_backtrace` unconditionally clones the error's
  backtrace and calls `backtrace.resolve()` at `Report` construction. The doc
  comment justifies this as "repeated rendering never re-symbolicates", but
  that property is already guaranteed by the `LazyLock` inside
  `oopsie_core::Backtrace` (`crates/oopsie-core/src/backtrace.rs:327-359`):
  clones share one `Arc<Lazy>`, so the first render would resolve exactly once
  regardless. The eager call therefore buys nothing for the rendered case and
  is pure waste when a `Report` is constructed but never displayed (inspected
  via `error()`, consumed via `into_error()`, stored for later, or dropped).
- **Evidence:** counting-allocator harness: `Report::new(err).no_colors()` with
  no render costs **293 allocations** (all symbolication); `Report::ok()` costs
  0. Construction-only cost is ~7 µs + ~300 allocs per unrendered report.

  ```rust
  fn resolve_backtrace(res: &Result<(), E>) -> Option<oopsie_core::Backtrace> {
      let backtrace = res.as_ref().err()?.oopsie_backtrace()?.clone();
      backtrace.resolve();
      (!backtrace.frames().is_empty()).then_some(backtrace)
  }
  ```
- **Suggested fix:** defer resolution to the first `Display`/`Debug` render
  (the `LazyLock` dedupes it). The empty-frames pre-filter can move into
  `write_backtrace`, which already early-returns for `None`. If eager
  resolution at a controlled point is a deliberate UX choice for
  `Termination`, keep it but fix the doc comment — the "repeated rendering"
  rationale is satisfied by the `Lazy` cache either way.

### F2 — Every render re-materializes all backtrace frames (per-`Display` allocation)

- **File:** `crates/oopsie/src/trace_printer.rs:116-129` and :726-730
- **Severity:** low
- **Category:** performance
- **Description:** `BacktraceProvider::frames()` for `Backtrace` walks every
  frame and clones every symbol name into a `Box<str>` and every filename into
  a `Box<Path>`; `write_backtrace` then allocates a second
  `Vec<Option<&BacktraceFrame>>` of the same length. This happens on **every**
  render: displaying the same `Report` twice (e.g. a `log::error!("{report}")`
  followed by `Termination::report`'s `eprintln!("{self}")`, or a Debug print
  after a Display print) re-demangles and re-allocates everything, even though
  symbol resolution itself is cached. The materialization is also paid in full
  when the frame filter ends up masking every frame (the all-masked early
  return at :734 happens after it).
- **Evidence:** counting-allocator harness: render #1 and render #2 of the
  same `Report` each cost **50 allocations** (a tiny binary; real backtraces
  scale this to hundreds) — no amortization across renders.

  ```rust
  let all_frames = bt.frames();                          // Vec<BacktraceFrame>, owned strings
  let mut filtered: Vec<_> = all_frames.iter().map(Some).collect();
  ```
- **Suggested fix:** cold path, so this is polish, not a defect that bites
  normal use. If repeated rendering matters, cache the materialized
  `Vec<BacktraceFrame>` in `Report` at construction (next to the resolved
  `Backtrace`), or change `BacktraceProvider` to lend frames
  (`fn frames(&self, f: &mut dyn FnMut(&BacktraceFrame))`) so a render borrows
  instead of cloning.

### F3 — `SpanTrace::eq` allocates a `String` per span per comparison in debug builds

- **File:** `crates/oopsie-core/src/spantrace.rs:93-101` (`capture_fields`)
  and :116-140 (`PartialEq::eq`)
- **Severity:** low
- **Category:** performance
- **Description:** In debug builds, every `==` on a `SpanTrace` walks the whole
  span stack of `a` and `to_owned()`s each span's formatted fields into a
  `VecDeque`, then compares pairwise. `PartialEq` on errors containing a
  `SpanTrace` field (user-derived) inherits this: comparing two such errors
  allocates O(spans) strings per comparison. Nothing inside the crate calls
  this outside tests, and the release profile drops the strings entirely
  (callsite-only comparison, `capture_fields → ()`), so impact is limited to
  debug-build test/dev workloads.
- **Evidence:**

  ```rust
  let mut a_frames = VecDeque::with_capacity(2);
  a.with_spans(|a_md, a_fields| {
      a_frames.push_back((a_md.callsite(), capture_fields(a_fields))); // String per span, debug only
      true
  });
  ```
- **Suggested fix:** compare incrementally instead of materializing: collect
  only callsites (pointers, no allocation) and compare field strings lazily
  only when callsites are equal; or skip the field-string capture entirely
  when the two traces have different span counts (a cheap counting pre-pass),
  which avoids all allocations on the common unequal path.

## Checked and cleared

- **Hot propagation path.** `ResultExt::context` / `OptionExt::context`
  (`crates/oopsie-core/src/traits.rs:287-334, 371-396`) build the error only on
  the `Err`/`None` branch with a direct `match` (no `map_err` closure), which
  is also required for `#[track_caller]`. Measured: Ok path 1.4 ns vs 1.1 ns
  baseline. Clean.
- **Capture when disabled.** `Backtrace::capture()` with capture disabled
  stores `Inner::Disabled(backtrace::Backtrace::from(vec![]))`
  (`crates/oopsie-core/src/backtrace.rs:201-206`) — an empty `Vec` does not
  allocate, no marker TLS read, no stack walk. Measured: 5.3 ns. `SpanTrace`
  without the `tracing` feature is a ZST stub; with `tracing`, capture without
  a subscriber/ErrorLayer is 4 ns. Clean.
- **One `Box` per capture for `LazyResolve`** (`backtrace.rs:361-382`): the
  boxed `FnOnce` is acknowledged in a comment with a concrete deallocation
  plan once `type_alias_impl_trait` stabilizes (matching what std's own
  `Backtrace` does). One allocation per *enabled* capture; cold path. Accepted.
- **`Welp::wrap` always captures fresh traces at the wrap site**
  (`crates/oopsie-core/src/welp.rs:123-138`). In a chain of N wraps, N-1
  captures are never surfaced. This is inherent: the body is generic, so the
  `CaptureProbe` autoref trick cannot select the extracting impl (the comment
  at :127-131 says exactly this), and spantrace capture measured at 4-66 ns.
  Backtrace capture (14 µs) only happens when the user explicitly enabled it
  via env/override. Accepted as design.
- **`error_backtrace_frame_filter` scans `GENERATED_SITES` per frame**
  (`trace_printer.rs:346-369, 489-502`): O(frames × sites) worst case, but
  `matches_site` rejects on crate-name and line-number equality before any
  path work, and the whole path runs once per report render. Not worth a
  hash index.
- **All-masked early return materializes frames first** (`trace_printer.rs:726-735`):
  noted under F2; on its own a non-issue since a fully-masked trace is rare.
- **`marker.rs` TLS dance** (`current()` take/clone/set): one `Arc` refcount
  bump per enabled capture. Cheap.
- **`ErasedError` source-chain re-materialization** (`erased/mod.rs:81-113`):
  lazy via `OnceLock`, built once, `source_chain` immutable so it can't go
  stale. `from_error_ref`'s `ToString` per cause is inherent to erasure; the
  cycle cap (`MAX_SOURCE_CHAIN_DEPTH + 1`) is correct.
- **`Chain` iterator** (`chain.rs`): honest cursor, no allocation, fused.
  Documented infinite-loop-on-cycle semantics match `anyhow::Chain`.
- **Theme access**: `get_theme()` takes an `RwLock` read and clones a 32-byte
  `Copy` palette 1-3 times per render. Trivial. (The `const _` size assertion
  at `theme.rs:34-37` pins this.)
- **Color detection**: env detection cached in an `AtomicU8`; atomic loads on
  the render path. Fine.
- **`Backtrace` layout**: measured 32 bytes (`Inner::Disabled` holds a
  16-byte `backtrace::Backtrace` inline so the enum can't shrink to the
  8-byte `Arc` variant). An `Option<Arc<Lazy>>` repr plus a static empty
  backtrace would save ~8 bytes per inline-stored `Backtrace`, but the default
  `traced` config boxes the traces field anyway, so this only affects
  `boxed = false` users. Marginal; not a finding.
- **Generated-code bounds**: selectors carry only projected parameters
  (`gen_selectors.rs` `selector_shape`); `Contextual` impls add no bounds
  beyond the user's own; no forced extra monomorphization found. The `Into`
  per-field params are the intended ergonomics. Tuple `Capturable` impls
  (A..I) monomorphize only on use.
- **Proc-macro compile time**: manifest reads (`settings` feature) are cached
  per-process via `OnceLock` (`utils/settings.rs:247-328`); the pretty-printer
  is hand-rolled (no `prettyplease` dependency); the `#[oopsie]` attribute
  parses container attrs once and threads them into the derive layer. No
  double-expansion found.
- **`panic_hook.rs`**: capture-once-at-construction then format; hook
  refcounting is mutex-guarded but only on `Report::run` entry/exit, not per
  panic. Fine.
- **Benches exist** for the paths that matter (propagate, capture, construct,
  render, erased, vs_ecosystem) and their numbers are sane (see table above).

## Residual risks

- **`GENERATED_SITES` at scale**: I did not simulate a binary with thousands
  of `#[oopsie]` invocations; the per-render O(frames × sites) scan could
  become noticeable there, though each comparison early-rejects on crate name.
- **Macro expansion time at scale**: not measured (no timing harness for
  proc-macro expansion over hundreds of derives in one crate). Code reading
  found nothing superlinear, but a crate with very large error enums
  (hundreds of variants) was not stress-tested.
- **Nightly paths** (`unstable-error-generic-member-access`, provider API):
  reviewed by reading only; not compiled or measured on nightly.
- **`SpanTrace::eq`** release-profile behavior (callsite-only comparison) is
  a documented deliberate semantic difference, but I did not verify how it
  interacts with user code that relies on `PartialEq` for dedup in release
  builds.
- **owo-colors write amplification**: `write_backtrace_frame` issues ~10 small
  `write!` calls per frame, each wrapping a `Styled` value; for a `String`
  sink this is fine, but I did not profile rendering into an unbuffered
  `io::Write` (the public API renders into `fmt::Formatter`, so this is
  theoretical).
