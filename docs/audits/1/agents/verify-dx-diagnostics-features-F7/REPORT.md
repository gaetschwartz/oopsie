# Verification: F7 — User field colliding with an injected trace field name produces 6 cascading rustc errors

## Claim

`check_existing_fields` (crates/oopsie-macros/src/traced/inject.rs:21) suppresses trace-field
injection by *type* (backtrace/spantrace/traces/location/timestamp predicates) and by mangled
*name* only for `__oopsie_timestamp`. A user field named `__oopsie_traces` (or
`__oopsie_backtrace` / `__oopsie_spantrace` / `__oopsie_location`) of an unrelated type does
not suppress injection, so the macro emits a duplicate field and the user gets six cascading
rustc errors (E0416, E0124, E0025 x2, E0308, E0062), all spanned at the `#[oopsie::oopsie(traced)]`
attribute rather than at their field. Severity: low (dx).

## What I checked

1. **Code read** — `crates/oopsie-macros/src/traced/inject.rs` (lines 12–53, 89–138): confirmed
   the asymmetry. `check_existing_fields` name-matches only `__oopsie_timestamp` (line 26–29);
   the other four injected fields are detected solely via the type predicates in
   `field_detect.rs` (`is_traces_type`, `is_backtrace_type`, `is_spantrace_type`,
   `is_location_type`). A field `__oopsie_traces: u8` matches none of them, so
   `existence.has_traces` stays false and `inject_into_named` pushes a second
   `__oopsie_traces: Box<(Backtrace, SpanTrace)>` field. The `__oopsie_timestamp` name-check
   exists for re-expansion of already-injected fields (per the comment at lines 22–25), not as
   collision protection, so the finder's reading of the asymmetry is accurate.
2. **Guard search** — `grep` for `reserved` / `conflict` / `__oopsie_*` across
   `crates/oopsie-macros/src`: the only "reserved/colliding-name" check (derive/model.rs:189)
   concerns selector names, not injected trace fields. No other layer rejects user idents with
   the `__oopsie_` prefix.
3. **Reproduction** — built a throwaway crate at /tmp/f7-repro depending on the local
   `oopsie` (features = ["tracing"]) with exactly the claim's example:

   ```rust
   #[oopsie::oopsie(traced)]
   pub enum E {
       #[oopsie("x {v}")]
       A { __oopsie_traces: u8, v: u8 },
   }
   ```

   `cargo check` output: **exactly 6 errors** — E0416, E0124, E0025 (x2), E0308, E0062 —
   matching the claim's list verbatim. Every primary span is `src/lib.rs:1:1` (the attribute);
   the E0308 message is the misleading "expected `Box<(Backtrace, SpanTrace)>`, found `u8`".
   Only the secondary note on E0124/E0025 points at the user's actual field.
4. **Test coverage** — `crates/oopsie/tests/compile-fail/` has no fixture for an injected-name
   collision (`two_backtrace_fields` and `packed_traces_with_standalone` cover type-level
   duplicates, not a mangled-name collision with an unrelated type). The unit tests in
   inject.rs don't cover it either.

## Attempts to refute

- *Unreachable / guarded elsewhere?* No — no reserved-name guard exists on the traced path; the
  duplicate reaches rustc. Reproduced end-to-end.
- *Error count exaggerated?* No — the count (6) and error codes match the claim exactly.
- *The fix would break re-expansion?* The suggested fix (name-check all injected idents and
  emit one targeted error on collision with a non-trace-typed field) is compatible with the
  re-expansion guard; injected fields are trace-typed, so re-expansion would still be
  suppressed by type. No breakage argument found.
- *Impact negligible?* Impact is genuinely small: it requires a user to name a field with the
  crate's internal `__oopsie_` mangled prefix *and* give it an unrelated type, and it fails
  loudly at compile time. That justifies "low", but the DX (6 off-target errors, one actively
  misleading E0308) is real.

## Verdict

**Confirmed**, severity **low** (unchanged).

The mechanism, the error list, the span attribution, and the absence of mitigating guards or
tests all reproduce exactly as claimed. The only debatable point is severity, and "low" is
already the floor for a loud compile-time failure on an adversarial field name.
