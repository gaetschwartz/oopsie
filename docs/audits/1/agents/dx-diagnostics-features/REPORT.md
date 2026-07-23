# Audit: DX — compile-error quality and feature-matrix correctness

Auditor: dx-diagnostics-features
Repo: /Users/gaetan/dev/oopsie (branch develop, HEAD 5c55047)

## Scope and method

Two halves:

1. **Macro diagnostics.** Built throwaway crates under `/tmp` (`/tmp/oopsie-scratch`,
   `/tmp/oopsie-settings`, `/tmp/oopsie-nostd`, `/tmp/oopsie-uni`) depending on the
   workspace crates by path, then deliberately misused `#[oopsie::oopsie]` /
   `#[derive(Oopsie)]` / `traced(...)` / `#[oopsie(...)]` container, variant and
   field attributes, and `[package|workspace.metadata.oopsie]` settings, inspecting
   the emitted rustc diagnostics for span accuracy and actionability. Also ran the
   repo's own compile-fail suites on both toolchains:
   - `cargo +stable test -p oopsie --test compile_fail` — PASS
   - `cargo test -p oopsie --test compile_fail` (nightly-2026-06-10) — PASS
   - `OOPSIE_SETTINGS_E2E=1 cargo +stable test -p oopsie --test settings --no-default-features --features fancy,serde,tracing,chrono` — PASS

2. **Feature matrix.** `cargo check` powersets:
   - oopsie: all 256 subsets of {std, fancy, serde, tracing, chrono, jiff, extras, settings} on stable — all PASS
   - oopsie-core: all 128 subsets of {std, serde, tracing, chrono, jiff, extras, test-utils} on stable — all PASS
   - nightly-only combos (`unstable`, `unstable-error-generic-member-access`, `unstable-try-trait-v2`, plus all-features) — PASS
   - bare-metal (`thumbv7em-none-eabihf`): `crates/nostd-smoke`, `oopsie-core` (no-default, +serde), `oopsie` (no-default, +serde, +settings), and a /tmp no_std crate using `traced` — PASS except as noted in F5.

Toolchains: stable-aarch64-apple-darwin (default) and nightly-2026-06-10 (repo-pinned).

## Findings

### F1 — `settings` feature leaks across crates via feature unification; workspace builds are non-hermetic — HIGH

**File:** crates/oopsie/Cargo.toml:36 (`settings = ["oopsie-macros/settings"]`), crates/oopsie-macros/src/utils/settings.rs:303-330

**Severity:** high. **Category:** correctness (feature matrix).

The `settings` feature is a flag on the *proc-macro crate*. Cargo unifies features
per build: if any crate in the build graph enables `oopsie/settings`, the single
`oopsie-macros` build has `settings` on for *every* crate compiled in that
invocation. Because the macros read each expanded crate's own manifest (and its
workspace root), a crate that never opted into `settings` still gets
`[workspace.metadata.oopsie]` defaults — including `max-size` — applied to its
derives. Concretely: whether a crate compiles depends on which *other* crates are
built alongside it.

Evidence (/tmp/oopsie-uni: virtual workspace with `[workspace.metadata.oopsie] max-size = 8`,
crate-a with `features = ["settings"]`, crate-b without, crate-b has a 24-byte error type):

    cargo build -p crate-b          → Finished (OK)
    cargo build --workspace         → error[E0080]: evaluation panicked: `BErr` is 24 bytes,
                                      must be ≤ 8 ... set by `[workspace.metadata.oopsie] max-size`

The same workspace manifest both passes and fails depending on build selection. This
also contradicts the docs (crates/oopsie/src/lib.rs:354-364, keyword_docs/container/size.md:
"applying to every member crate that opts into the `settings` feature") — crate-b
opted into nothing and was capped anyway. `traced = true` at the workspace level
leaks the same way (fields injected into a non-opting crate's errors).

**Suggested fix:** the proc macro cannot detect which crate enabled the feature, so
gating per-crate needs a different mechanism: e.g. drop the `settings` cargo feature
and always read manifests (paying the toml_edit/glob dep cost for everyone), or make
opt-in explicit in the manifest itself (only apply *workspace* settings to crates
whose own manifest carries an `[package.metadata.oopsie]` table, empty included), or
at minimum document the unification behavior prominently.

### F2 — Invalid `suffix = "..."` string panics the proc macro — HIGH

**File:** crates/oopsie-macros/src/derive/gen_selectors.rs:723-742 (`ident_maybe_raw`)

**Severity:** high (proc-macro panic). **Category:** dx.

`selector_name` concatenates the stripped variant/struct name with a user-supplied
`suffix` string and calls `ident_maybe_raw`. On `syn::parse_str::<Ident>` failure the
code only special-cases `self | Self | super | crate | _` into a real error and calls
`Ident::new_raw(name, span)` for everything else — which **panics** on any name with
punctuation/whitespace. The comment claims "names that cannot be raw either are a
real error, not a panic"; that is only true for those five names.

Evidence (/tmp/oopsie-scratch):

    #[oopsie::oopsie]
    #[oopsie(suffix = "with space")]   // or "-x", "(", etc.
    pub enum E { #[oopsie("x")] A }

    error: custom attribute panicked
     --> src/main.rs:1:1
      = help: message: `"Awith space"` is not a valid identifier

No span, no mention of `suffix`, no hint. Note the manifest settings path *does*
validate (`is_ident_fragment`, settings.rs:78-80) — the per-type attribute does not.
Adjacent wart: `suffix = " "` silently behaves like `suffix(false)` because
`"A "` parses as the ident `A` (trailing whitespace is not a token).

**Suggested fix:** in `ident_maybe_raw`, validate the composed name as an identifier
fragment (the same `is_ident_fragment` check used for manifests, applied to the
suffix before concatenation, or a char-class check on the composed name) and return a
spanned `syn::Error` naming the `suffix` attribute and its invalid value.

### F3 — Crate docs claim `fancy` is a default feature; it is not — MEDIUM

**File:** crates/oopsie/src/lib.rs:323 vs crates/oopsie/Cargo.toml:18

**Severity:** medium. **Category:** dx (docs).

The feature table says:

    //! | `fancy` | yes | stable | `Report`, colorized output, and the panic hook |

but `default = ["std"]` (and `git show 6351689` shows `fancy` was never in
`default`). A user following the docs gets:

    error[E0433]: cannot find `Report` in `oopsie`
      note: found an item that was configured out: the item is gated behind the `fancy` feature

The README ("`std` is on by default") is correct; only the rustdoc table is wrong.
The feature table's "Default" column is the one place users check what they get for
`oopsie = "..."`.

**Suggested fix:** change the `fancy` row's Default cell to `no` (the rest of the
column was verified against Cargo.toml and is correct).

### F4 — Manifest `max-size` is silently skipped on generic error types, while docs claim it caps "every error" — MEDIUM

**File:** crates/oopsie-macros/src/derive/mod.rs:84 and :118 (`cap_info.filter(|_| input.generics.params.is_empty())`); doc: crates/oopsie/src/__private/keyword_docs/container/size.md ("caps every error derived in that crate, as if each carried `#[oopsie(size(..=N))]`")

**Severity:** medium. **Category:** dx / silent behavior.

An explicit per-type `size(...)` on a generic error type is a hard error
("`size(...)` cannot be combined with generic parameters"), but the manifest
`max-size` cap is *silently skipped* for generic types:

    [package.metadata.oopsie]
    max-size = 8

    #[oopsie::oopsie]
    pub enum E<T: std::fmt::Debug> { #[oopsie("x {v:?}")] A { v: T } }  // compiles clean

The documented equivalence ("as if each carried `size(..=N)`") is false for generic
types: the per-type form errors, the manifest form no-ops. A project relying on the
cap gets a false sense of safety for exactly the generic error types. Silent
feature-dependent behavior with no warning.

**Suggested fix:** either emit a warning-style diagnostic when the cap is skipped
due to generics, or document the exemption in the size keyword doc and the settings
table (lib.rs:356).

### F5 — `traced(timestamp)` under no_std, and `timestamp(chrono = true)` without the `chrono` feature, fail with cryptic `__private` resolution errors — MEDIUM

**File:** crates/oopsie-macros/src/traced/config.rs:70-79; crates/oopsie-core/src/lib.rs:342-349

**Severity:** medium. **Category:** dx (feature matrix).

Generated code references `#oopsie_path::__private::SystemTime` (or
`__private::chrono`), which only exist under the corresponding cargo features. The
macro cannot know the consumer's feature set, so misuse surfaces as:

no_std (`oopsie` with `default-features = false`, thumbv7em):

    error[E0425]: cannot find type `SystemTime` in module `::oopsie::__private`

`traced(timestamp(chrono = true))` without `features = ["chrono"]`:

    error[E0433]: cannot find `chrono` in `__private`

Neither message mentions `std`/`chrono` or what to enable. Everything else about
`traced` works fine under no_std (verified: backtrace/spantrace/location/code inject
and compile on thumbv7em), so timestamp is the one cliff, and the crate otherwise
advertises clean no_std support. Note `crates/nostd-smoke` exercises only the
untraced path, so CI has no no_std coverage for `traced` at all.

**Suggested fix:** under `not(feature = "std")` / `not(feature = "chrono")`, export
deliberately-named stubs from `__private` (e.g. a `SystemTime` alias to an
uninhabited marker whose name/doc points at the feature) so the diagnostic names the
missing feature; add a `traced` case to `crates/nostd-smoke` to lock in the
supported no_std surface.

### F6 — Unknown `#[oopsie(...)]` key on a struct loses the "Available values" list; struct-level `traced` gets no guidance — LOW

**File:** crates/oopsie-macros/src/derive/parse.rs:649-668 (`StructAttrs`)

**Severity:** low. **Category:** dx.

Same mistake, two error qualities:

    #[derive(Debug, Oopsie)]
    #[oopsie(bogus_key)]
    pub enum E { A }        // error: Unknown field: `bogus_key`. Available values: `exit_code`, `module`, `path`, `size`, `suffix`, `vis`

    #[derive(Debug, Oopsie)]
    #[oopsie(bogus_key)]
    pub struct S { m: String }  // error: Unknown field: `bogus_key`   ← no list

And `#[oopsie(traced)]` on a derive-struct gives the bare "Unknown field: `traced`"
with no pointer to the `#[oopsie::oopsie(traced)]` attribute macro — while enums got
a dedicated, excellent message ("`traced` on an enum variant only applies when the
enum is traced with `#[oopsie::oopsie(traced)]`; add `traced` to the enum or remove
this"). The missing list appears to be a darling behavior difference once the struct
has several non-flattened fields, but the asymmetry is user-visible.

**Suggested fix:** intercept the darling error in `StructAttrs::from_attrs` (or add a
`traced` pre-pass mirroring `reject_inert_variant_traced`) so struct users get the
same available-values list and a targeted hint for `traced`.

### F7 — User field colliding with an injected trace field name produces 6 cascading rustc errors — LOW

**File:** crates/oopsie-macros/src/traced/inject.rs:21-53 (`check_existing_fields`)

**Severity:** low. **Category:** dx.

`check_existing_fields` suppresses injection by *type* (backtrace/spantrace/traces/
location/timestamp types) and additionally by the mangled *name* only for
`__oopsie_timestamp`. A user field named `__oopsie_traces` (or `__oopsie_backtrace`,
`__oopsie_location`, `__oopsie_spantrace`) of an unrelated type does not suppress
injection, so the macro emits a duplicate field and the user gets six call-site-spanned
errors (E0416, E0124, E0025 x2, E0308, E0062), all pointing at
`#[oopsie::oopsie(traced)]` rather than at their field:

    #[oopsie::oopsie(traced)]
    pub enum E { #[oopsie("x {v}")] A { __oopsie_traces: u8, v: u8 } }

**Suggested fix:** check all injected idents by name (not just `__oopsie_timestamp`)
and, on collision with a non-trace-typed field, emit one targeted error at the user
field: "`__oopsie_traces` conflicts with a field injected by `traced`; rename it".

### F8 — Container keywords swallowed as display args on variants yield "cannot find value `module`" — LOW

**File:** crates/oopsie-macros/src/derive/parse.rs:398-429, 460-484 (`reject_keyword_args` scope split)

**Severity:** low. **Category:** dx.

`reject_keyword_args` exists to catch `#[oopsie("fmt", transparent)]`-style mistakes,
but on an enum variant it only flags *variant* keywords. A container keyword in the
same position is parsed as a format argument and fails later, in generated code:

    #[oopsie::oopsie]
    pub enum E {
        #[oopsie("x", module)]      // error[E0425]: cannot find value `module` in this scope (+ "argument never used")
        A,
    }
    #[oopsie("x {}", module(false))] // error[E0425]: cannot find function `module` in this scope

A variant can't have a field-collision concern for *container-only* keywords (those
are never legal on a variant), so flagging them there is unambiguous when the
variant has no field of that name.

**Suggested fix:** in the `DisplayScope::Variant` arm, also reject `CONTAINER_KEYWORDS`
(with a message pointing at the enum-level placement), keeping the field-name escape
hatch.

### F9 — `vis = pub(crate)` bare form errors with "expected an expression" — LOW

**File:** crates/oopsie-macros/src/derive/parse.rs:288-294 (documented in the `EnumContainerAttrs` doc comment)

**Severity:** low. **Category:** dx.

    #[oopsie(vis = pub(crate))]
    // error: expected an expression
    //   --> #[oopsie(vis = pub(crate))]
    //                      ^^^

The span lands on `pub` but nothing tells the user the two accepted spellings
(`vis(pub(crate))` or `vis = "pub(crate)"`). The code comment says there is "no
FromMeta-side hook to intercept it" — true at the darling layer, but a token-level
pre-pass (like `extract_short_display`) over `vis` name-value pairs could catch the
`pub` token and emit the targeted message.

**Suggested fix:** pre-scan `#[oopsie(...)]` attribute args for `vis = pub` and error
with "`vis` takes `vis(pub(crate))` or `vis = \"pub(crate)\"`, not a bare expression".

### F10 — Packed-boxing conflict error is spanned at the type name, not the offending toggle — LOW

**File:** crates/oopsie-macros/src/oopsie_attr/mod.rs:119,175 (`let span = item.span();`), crates/oopsie-macros/src/traced/args.rs:154-168 (`validate`)

**Severity:** low. **Category:** dx.

    #[oopsie::oopsie(traced(spantrace(boxed = false)))]
    pub enum E { ... }
    // error: `packed` requires backtrace and spantrace to share one boxing mode; ...
    //  --> src/main.rs:2:1
    // 2 | pub enum E {
    //   |        ^^^

The parameter is even named `args_span`, but the call sites pass the item's span.
The message itself is good; the span should land on the `boxed` toggle inside
`traced(...)` (or at least on the attribute). Same for structs.

**Suggested fix:** thread the `traced(...)` meta span (available during
`OopsieAttrArgs::from_list`) into `expand_enum`/`expand_struct` and pass it to
`validate`.

### F11 — `SynParse` errors leak the literal placeholder "key" to users — LOW

**File:** crates/oopsie-macros/src/utils/mod.rs:172-196

**Severity:** low. **Category:** dx.

    #[oopsie(vis)]        // error: expected a value, e.g. `key(...)` or `key = "..."`
    #[oopsie(vis = 42)]   // error: expected `key(...)` or `key = "..."`

The word `key` is a template placeholder that was never substituted — users see a
generic `key` instead of `vis` (or `path`). Spans are correct; only the text is off.

**Suggested fix:** darling's `FromMeta` doesn't receive the field name, so either
hand-roll the two `SynParse` uses (`vis`, `path`) with the real key in the message,
or post-process darling errors at the `from_attrs` level to rewrite `key` to the
actual field.

### F12 — Error type defined in a fn body without `module(false)` gives a misleading resolution error — LOW

**File:** crates/oopsie/src/lib.rs:201-203 (documented requirement); diagnostics from generated module

**Severity:** low (documented, but the compiler output actively misleads). **Category:** dx.

    fn f() {
        #[oopsie::oopsie]
        pub enum E { #[oopsie("x")] A }
    }
    // error[E0425]: cannot find type `E` in this scope
    //   = help: a struct with a similar name exists — `A`  (nonsense: suggests renaming E to A)
    // error[E0433]: cannot find type `E` in this scope

The docs do call out the `module(false)` requirement, and with it the code works
(verified). But the emitted error sends users toward renaming their type rather than
toward `module(false)`. On stable the macro cannot detect fn-local expansion, so a
fix is non-trivial; flagging as a known-sharp edge.

**Suggested fix:** none cheap on stable; keep the docs callout, possibly strengthen
the `module` keyword doc to name the exact E0425/E0433 symptom.

### F13 — `CARGO_WORKSPACE_DIR` override is honored but undocumented — LOW

**File:** crates/oopsie-macros/src/utils/settings.rs:69, 199-203

**Severity:** low. **Category:** dx (docs).

The workspace-root discovery honors a `CARGO_WORKSPACE_DIR` env override (useful for
hermetic/vendored builds), but it appears nowhere in user-facing docs — only in the
source. Users who need it can't discover it.

**Suggested fix:** one line in the "Project-wide settings" section of
crates/oopsie/src/lib.rs.

## Checked and cleared

- **trybuild suites assert real output.** `compile_fail` passes on both stable and
  the pinned nightly, including the channel-split `stable/`/`nightly/` variants; the
  `.stderr` files match what I reproduced by hand in scratch crates
  (e.g. `keyword_as_display_arg`). Settings e2e fixtures (incl. `workspace_bad`,
  `workspace_excluded`, `workspace_precedence`) pass on stable.
- **Feature powerset compiles.** All 256 stable feature subsets of `oopsie` and all
  128 of `oopsie-core` check clean; nightly `unstable*` combos check clean; no
  combination fails to build.
- **no_std matrix.** `nostd-smoke` builds for `thumbv7em-none-eabihf`; `oopsie` and
  `oopsie-core` build no-default-features and with `serde`/`settings` on the same
  target; `traced` (without timestamp) works under no_std.
- **Feature implications are consistent.** Every std-requiring feature (`fancy`,
  `tracing`, `chrono`, `jiff`, `extras`, `test-utils`) implies `std` in both crates,
  so no combination yields a half-std build. README's no_std table matches Cargo.toml.
- **Good diagnostics verified by hand** (right span + actionable message): darling
  did-you-mean for typos (`displya` → "Did you mean `display`?"), container-only key
  on a variant (lists available values), short display on the enum container,
  duplicate display, unmatched `}` in static help, `transparent` without/with extra
  fields, two source fields, two help fields, inert `traced` on an enum variant with
  plain derive, discriminant + traced conflict (spans the discriminant), tuple
  variants, derive on union, attribute on a fn, selector name collision (both sites
  spanned), `size(..)` / `size(..0)` / empty ranges, `exit_code` range validation,
  `forward` on a non-source field, settings manifest errors (unknown key lists valid
  keys; `max-size = 0`; `default-suffix = true`; bad vis) all spanned at the macro
  with the section named, and size-cap violations blame the largest variant with the
  manifest section credited.
- **`size(...)` const-eval assertion quality.** Violations produce a single
  `evaluation panicked: `E` is 24 bytes, must be ≤ 4; largest variant `A` is 24 bytes`
  spanned at the offending variant; cfg-stripped fields are handled in the payload
  ranking.
- **Workspace discovery semantics.** `find_root_from` (exclude globs, explicit
  member beats exclude, `package.workspace` pointer, `target/package` and
  `$CARGO_HOME` walk stops) matches Cargo behavior; unit-tested over temp trees;
  `member_excluded` correctly treats "under root but not excluded" as a member
  (Cargo hard-errors in the only ambiguous case anyway).
- **`Tristate`/`fold` precedence** (per-attribute > manifest > hardcoded) is
  thoroughly unit-tested, including the explicit-false-over-manifest-on cases.
- **`exit_code`, `help`+`transparent`, empty enums, unit structs** all behave
  sensibly (compile or targeted error).

## Residual risks

- **`timestamp(provide = true)` on stable** compiles; I did not fully trace whether
  the generated `provide` is inert or observable on stable (the docs frame it as a
  provider-API/nightly feature). If it silently no-ops on stable, that's a mild
  doc/behavior gap; if it works through `Diagnostic`, fine.
- **`serde` swaps the spantrace field formatter** (per the justfile comment driving
  the snapshot matrix). Intentional, but rendering output changes across feature
  combos; only snapshot tests pin this, not docs.
- **darling stock messages** ("Unexpected type `int`", "expected `,`") leak through
  for malformed values (`module = 42`, `from(String)`); understandable but not
  guided. Low value in intercepting every one, but they contrast with the otherwise
  polished errors.
- **`suffix`/`module` validation asymmetry** beyond F2: manifest settings validate
  identifier fragments (`is_ident_fragment`); per-type `suffix` does not. Any future
  per-type string-key addition needs the same audit.
- **OnceLock-cached settings** assume one crate per proc-macro process; true today
  (rustc spawns per crate), but a future cargo change to share proc-macro processes
  across crates would serve stale settings. Not actionable now.
- **`matches_any` lossy path globbing** (`to_string_lossy`) could mis-match non-UTF-8
  member paths; niche.
- **Beta/custom-channel rustc** skips the channel-specific trybuild variants
  (deliberate); diagnostics on those channels are unpinned.
