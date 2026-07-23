# Verification report: F4

## Claim (finder: dx-api-docs)

**Title:** README no_std feature table lists `test-utils`, which is not a feature of the `oopsie` facade
**File:** README.md line 92
**Severity:** medium (dx)

The README's "Feature | no_std?" table has a row `test-utils | implies std`, but
`test-utils` exists only on `oopsie-core`; the facade has no such feature, so
`features = ["test-utils"]` on the `oopsie` dependency is a cargo error. The
table also omits the genuine facade feature `settings`.

## What I checked

1. **README.md lines 77-99** (`/Users/gaetan/dev/oopsie/README.md`): the
   "no_std support" section contains the table; line 92 is indeed
   `| test-utils | implies std |`. Every other row (`serde`, `extras`,
   `tracing`, `chrono`, `jiff`, `fancy`) is a facade feature. The table sits
   directly under facade-centric prose ("Build with `default-features =
   false`"), and line 75 points readers at the facade's docs.rs feature-flags
   docs — nothing scopes the table to the whole workspace or names
   `oopsie-core`.

2. **Facade features** (`/Users/gaetan/dev/oopsie/crates/oopsie/Cargo.toml`
   lines 17-36): `default`, `std`, `unstable-error-generic-member-access`,
   `unstable-try-trait-v2`, `unstable`, `fancy`, `serde`, `tracing`, `chrono`,
   `jiff`, `extras`, `settings`. No `test-utils`. Line 59 uses
   `oopsie-core = { workspace = true, features = ["test-utils"] }` as a
   dev-dependency — consistent with the finder's evidence.

3. **oopsie-core features** (`/Users/gaetan/dev/oopsie/crates/oopsie-core/Cargo.toml`
   line 24): `test-utils = ["std", "dep:insta", "dep:target-triple", "dep:konst"]`
   — the feature exists only here.

4. **Facade's own feature-flags doc table**
   (`/Users/gaetan/dev/oopsie/crates/oopsie/src/lib.rs` lines 319-332): lists
   `settings` and does NOT list `test-utils`. So the README table both adds a
   bogus row and drops a real one relative to the authoritative crate docs.

5. **Empirical cargo check**: built a throwaway crate in /tmp with
   `oopsie = { path = ".../crates/oopsie", features = ["test-utils"] }`.
   `cargo check` fails at resolution with a hard error:
   `package 'f4test' depends on 'oopsie' with feature 'test-utils' but 'oopsie'
   does not have that feature.` (cargo helpfully lists the available features).
   Cleaned up afterwards.

## Refutation attempts

- *Maybe the table is workspace-wide, not facade-scoped?* Refuted: every other
  row is a facade feature, the surrounding prose and the docs.rs link are
  facade-scoped, and no crate column or annotation says otherwise. A reader
  following the README would reasonably write `features = ["test-utils"]` on
  the `oopsie` dependency and hit a build error.
- *Maybe cargo only warns on unknown features?* Refuted empirically — it is a
  hard resolution error (see command output above).
- *Maybe `settings` is not really no_std-compatible?* `settings =
  ["oopsie-macros/settings"]` (facade Cargo.toml line 36) pulls only a
  proc-macro feature — proc macros run on the host, so it imposes no `std`
  requirement on the target. The omission is a real doc gap, though minor.

## Verdict

**Confirmed** (severity adjusted: medium → **low**).

Both factual claims hold: `test-utils` is not a facade feature and produces a
hard cargo error if requested on `oopsie`, and `settings` is a genuine
no_std-compatible facade feature missing from the table. The README table also
contradicts the crate's own feature-flags doc table, which gets both right.

Severity downgrade rationale: the failure is a build-time resolution error
whose message explicitly lists the valid features, so a user who hits it
recovers in seconds; it does not affect anyone who ignores the bogus row. This
is a docs-accuracy polish issue, not a "meaningful DX drag" at medium weight.
The finder's suggested fix (drop/annotate the `test-utils` row, add a
`settings` row) is correct and safe — it only touches README prose.
