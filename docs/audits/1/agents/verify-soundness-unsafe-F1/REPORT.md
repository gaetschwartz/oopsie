# Verify F1: Recursive Drop/Clone on ChainNode — stack-overflow abort from untrusted erased payload

## Claim

`ChainNode { message: Box<str>, source: Option<Box<Self>> }`
(`crates/oopsie-core/src/erased/mod.rs:98`) has derived `Clone` and
compiler-generated recursive drop glue. The chain is built lazily by
`ErasedError::source()` from `source_chain`, which has **no length cap on the
deserialization path** (`MAX_SOURCE_CHAIN_DEPTH = 128` is enforced only in
`from_error_ref`). An attacker-controlled transported JSON payload with an
oversized flat `source_chain` array causes a fatal stack-overflow abort when a
consumer walks `.source()` and the `ErasedError` is later dropped or cloned.

## What I checked

1. **Read the code** at `crates/oopsie-core/src/erased/mod.rs`:
   - Lines 53–77: `ErasedError` derives `Clone`, `Serialize`, `Deserialize`;
     `source_chain: Vec<Box<str>>` has only `#[serde(default)]` — no custom
     `Deserialize`, no length cap. The `source: OnceLock<Option<Box<ChainNode>>>`
     field is `#[serde(skip)]` and lazily populated.
   - Lines 79–92: `Error::source()` materializes `ChainNode::build(&self.source_chain)`
     via `get_or_init` on first call.
   - Lines 96–113: `ChainNode` shape and iterative builder exactly as claimed.
   - Lines 240–255: `MAX_SOURCE_CHAIN_DEPTH` capping exists only in
     `from_error_ref` (capture path), confirming the wire path is uncapped.
   - Doc comment on `ErasedError` (lines 29–32) explicitly blesses
     "deserializing a transported payload" as an intended construction path, so
     untrusted input is the designed use case.
   - serde_json's 128-deep recursion limit applies to *nested* JSON structures;
     `source_chain` is a flat array of strings, so it does not save us.

2. **Ran the finder's shape-identical probe** (`/tmp/oopsie-drop-check`):
   500,000 links → `exit: 134` (SIGABRT); 200,000 links → drops fine. The
   threshold on an 8 MiB main-thread stack is between 200k and 500k links
   (lower on 2 MiB spawned threads).

3. **Built a true end-to-end probe against the real crate**
   (`/tmp/oopsie-e2e`, path-dep on `crates/oopsie-core` with `features =
   ["serde"]`): constructs a real JSON payload `{"message":"outer",
   "source_chain":["x","x",...]}` with N entries, deserializes with
   `serde_json::from_str::<ErasedError>`, walks `Error::source()` to the end
   (the standard way consumers walk `dyn Error`), then drops or clones:

   ```
   $ ./oopsie-e2e 400000 drop
   walked 400000 sources
   thread 'main' (…) has overflowed its stack
   fatal runtime error: stack overflow, aborting
   exit: 134
   $ ./oopsie-e2e 400000 clone
   walked 400000 sources
   thread 'main' (…) has overflowed its stack
   fatal runtime error: stack overflow, aborting
   exit: 134
   ```

   A 400k-entry payload is only ~2 MB of JSON (`"x",` per entry) — well within
   what an uncapped request body can carry.

4. **Refutation attempts that failed:**
   - *"serde_json recursion limit caps it"* — no, flat arrays aren't nested.
   - *"cap exists elsewhere"* — only in `from_error_ref`; the derived
     `Deserialize` impl is uncapped. No other constructor or wrapper gates it.
   - *"chain is never materialized"* — it is, on the first `.source()` call,
     which any generic error reporter (anyhow/eyre-style chain walkers) makes.
     The `source_walk_yields_transported_chain_in_order` test in the repo
     itself exercises exactly this path.
   - *"drop glue is flattened/TCO'd"* — empirically false; both drop and the
     derived `Clone` (via the populated `OnceLock`) abort.
   - Bonus surface the finder didn't mention: the derived `Debug` on
     `ChainNode`/`ErasedError` also recurses, so `format!("{erased:?}")` on a
     populated instance is a third trigger.

## Verdict

**confirmed** — severity **medium** (as filed).

The defect is real and reproducible end-to-end against the actual crate: an
uncapped wire field materializes into a recursively-dropped/cloned linked
structure, yielding an uncatchable process abort. I kept severity at medium
rather than raising it to high: triggering it needs a multi-hundred-KB
malicious payload plus a consumer that walks `.source()`, and the impact is a
DoS abort rather than memory unsafety or data corruption. It is nonetheless a
genuine crash reachable from untrusted input in the type's explicitly
documented use case, so it should be fixed. The suggested fix (iterative
`Drop` + iterative `Clone` for `ChainNode`, plus capping `source_chain.len()`
in a custom `Deserialize` impl so the wire format shares the 128-entry bound)
is the standard remedy and addresses all three recursion sites (drop, clone,
and — if `Debug` is kept — formatting should be considered too).
