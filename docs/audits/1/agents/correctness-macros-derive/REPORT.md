# Audit: Correctness — `#[derive(Oopsie)]` codegen

## Scope and method

Dimension: codegen bugs in `#[derive(Oopsie)]` — attribute parsing, generics
(bounds, defaults, associated types, lifetimes), transparent/forwarded sources,
selector generation, size assertions, `cfg` handling on variants/fields, span
assignment.

Files read closely:

- `crates/oopsie-macros/src/derive/{parse,model,gen_error,gen_selectors,gen_display,gen_module,generics,size,mod}.rs`
- `crates/oopsie-macros/src/utils/{mod,settings}.rs`
- `crates/oopsie-macros/src/traced/field_detect.rs` (trace-type detection used by the derive)
- `crates/oopsie-macros/src/traced/{config,inject}.rs` (what trace injection hands to the derive)
- `crates/oopsie-core/src/{lib,diagnostic,traits}.rs` (`__private` helpers the generated code calls)
- Tests: `crates/oopsie/tests/derive_{error,generics,transparent,cfg,selectors,hygiene,size}.rs`

Method: adversarial reading, then every suspicion was validated by compiling
minimal reproductions against the real macros in throwaway crates under
`/tmp/audit-derive{,2}` (stable toolchain, plus one nightly run for the
`unstable-error-generic-member-access` paths). Baseline established green:
`cargo nextest run -p oopsie-macros` (233 passed) and
`cargo nextest run -p oopsie --test derive_generics --test derive_transparent
--test derive_cfg --test derive_selectors --test derive_error` (111 passed).

## Findings

### F1 — Lifetime bounds on a projected type parameter referencing a free lifetime generate `where T: 'a` on an impl that never declares `'a` (E0261)

**Severity: high**

`predicate_named_params` (crates/oopsie-macros/src/derive/generics.rs:180-191)
only visits `TypeParamBound::Trait` bounds when computing which parameters a
`where` predicate names; `TypeParamBound::Lifetime` bounds are skipped. So the
predicate `T: 'a` is treated as naming only `{T}`. The predicate router in
`gen_selectors.rs` (`SelectorShape::scoped_predicates`, line 263, and
`free_params`, line 279) then:

1. places `T: 'a` on the selector-scoped impl (which declares only the
   projected parameter `T`) → `error[E0261]: use of undeclared lifetime name 'a'`, and
2. fails to place it on the `build`/`fail` methods (which declare the free
   `'a`), because the intersection test with the free set `{'a}` comes out
   empty.

Repro (fails identically with the inline bound `T: 'a + Debug` and with a
`where T: 'a + Debug` clause):

```rust
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum LtBoundError<'a, T: 'a + std::fmt::Debug> {
    #[oopsie("val {value:?}")]
    A { value: T },              // selector projects T; 'a stays free
    #[oopsie("ref {note}")]
    B { note: &'a str },
}
```

Output:

```
error[E0261]: use of undeclared lifetime name `'a`
 --> src/bin/lt_bound.rs:8:26
  |
6 | #[derive(Debug, Oopsie)]
  |                 ------ lifetime `'a` is missing in item created through this procedural macro
8 | enum LtBoundError<'a, T: 'a + fmt::Debug> {
  |                          ^^ undeclared lifetime
```

The configuration is fully expressible — the predicate just needs to land on
`build`/`fail` (`fn build<'a>(self) -> E<'a, T> where T: 'a`), the same routing
`U: From<T>` already gets.

**Fix:** in `predicate_named_params`, also record declared lifetimes appearing
in `TypeParamBound::Lifetime` bounds of `WherePredicate::Type` (mirroring the
`WherePredicate::Lifetime` arm, which already records both sides).

### F2 — Sourced variant with a free *lifetime* parameter slips past the unconstrained-parameter guard and fails with a raw E0207 in generated code

**Severity: medium**

`SelectorShape::unconstrained_error_param` (gen_selectors.rs:225-236)
deliberately returns `false` for `GenericParam::Lifetime(_)`, so the guard that
produces a clear error for type/const parameters never fires for lifetimes.
`SelectorShape::sourced_impl` (line 188) then emits
`impl<'a, T> Contextual<T> for ASelector` with `'a` unconstrained (the
associated `Destination = E<'a, T>` does not constrain it).

```rust
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum FreeLtSourced<'a, T: std::error::Error + 'static> {
    #[oopsie("wrap")]
    A { source: T },
    #[oopsie("ref {note}")]
    B { note: &'a str },
}
```

```
error[E0207]: the lifetime parameter `'a` is not constrained by the impl trait, self type, or predicates
```

Contrast with the const-parameter twin, which gets the intended targeted
diagnostic ("a variant with a `source` field must reference every generic
parameter of the error type …"). The lifetime case is equally unexpressible
(`Destination` is fixed, so `'a` cannot move to a method generic), so it should
get the same treatment.

**Fix:** include lifetimes in the `unconstrained_error_param` check (the
`referenced_names`/`from_source` sets already contain lifetime names via
`param_name`/`names()`).

### F3 — Auto-generated error code is silently lost through `oopsie_error_code()` when the crate is renamed (`path = "..."`)

**Severity: medium**

Trace injection emits the auto code provide using the resolved crate path
(`config.rs:99`: `#oopsie_path::ErrorCode`). `is_error_code_provide`
(gen_error.rs:1195-1213), which decides which `provide(...)` attrs are surfaced
through the stable `oopsie_error_code()` accessor, only matches a bare
`ErrorCode` or one qualified by literally `oopsie`/`oopsie_core`. With a
renamed dependency (`renamed_oopsie = { package = "oopsie" }`) and
`#[oopsie::oopsie(traced, path = "renamed_oopsie")]`, the injected
`provide(renamed_oopsie::ErrorCode => ...)` no longer matches and the stable
accessor returns `None` (the nightly `provide()` path still works, so this is a
stable-only regression). `path = "crate"` (`crate::ErrorCode`) is affected the
same way.

Verified end-to-end:

```rust
#[renamed_oopsie::oopsie(traced, path = "renamed_oopsie")]
pub enum RenamedError { #[oopsie("boom {msg}")] Boom { msg: String } }

fn main() {
    let err = Boom { msg: "x".to_owned() }.build();
    println!("{:?}", err.oopsie_error_code());   // None  (unrenamed: Some(ErrorCode("...::RenamedError::Boom")))
}
```

**Fix:** thread the resolved `oopsie_path` into `is_error_code_provide` and
match the provide type's path against it (prefix comparison on segments) rather
than against hardcoded `oopsie`/`oopsie_core` strings.

### F4 — Selector-name collision checks miss collisions with the error type's own name

**Severity: medium**

The collision check in `ResolvedEnum::resolve` (model.rs:84-111) only compares
selector names *across variants*. Three gaps, all verified:

a) A variant whose stripped selector name equals the **enum** name:
   `enum Config { Config, Other }` → generated `struct Config` next to
   `enum Config`. With `module(false)`: `error[E0428]: the name 'Config' is
   defined multiple times` plus cascades (E0119, E0599). With the default
   module wrapping: `error[E0223]: ambiguous associated type` from the
   `use super::*` shadowing. Neither points at the real problem.

b) A struct with `suffix(false)` and `module(false)` whose name doesn't end in
   `Error` (`struct Failure` → selector `Failure`): E0428. The
   `super::`-qualification that handles the shared-name case in
   `gen_struct_selector` (gen_selectors.rs:583-601) only exists when the
   selector is module-wrapped; the bare case is unguarded. (This one requires
   an explicit opt-in, so it is the least severe of the three.)

c) The cfg exemption (`model.rs:104`) skips the collision check for *any*
   cfg-gated variant, so `#[cfg(all())] Read` + unconditional `ReadError`
   (both → selector `Read`) passes the macro and fails with E0428/E0119. The
   macro cannot prove cfgs mutually exclusive, so this gap may be
   unfixable without false positives on the legitimate mutually-exclusive case
   (which is tested) — but (a) and (b) are unconditionally checkable.

**Fix:** for (a), include the enum's own name in the collision set; for (b),
reject `module(false)` + a suffix resolving to the bare struct name with the
same guidance `ident_maybe_raw` gives. For (c), consider warning-quality
documentation only.

### F5 — Field-level `provide(...)` ignores the field's `#[cfg]`, breaking generated code when the field is stripped

**Severity: medium**

`field_cfg_attrs` is carefully forwarded onto selector fields, match-arm
bindings, and trace-field provide statements — but not onto statements
generated from a field's own `#[oopsie(provide(...))]`:

- gen_error.rs:245-247 (enum `provide()` stmt) and 749-751 (struct) push
  `gen_provide_call(...)` with no `#[cfg]` — the closure still references the
  stripped field → `error[E0425]` under
  `unstable-error-generic-member-access` (verified on nightly). Even when the
  expr does *not* reference the field, the stmt still runs and provides a value
  for a field that no longer exists.
- The stable `oopsie_error_code()` accessor that replays an `ErrorCode`
  provide (enum: gen_error.rs:445-467, struct: 987-1009) emits the arm/method
  gated only on the *variant's* cfg; the field binding inside carries the cfg
  and drops, leaving the body referencing a stripped binding → E0425 **on
  stable**.

Stable repro:

```rust
#[oopsie::oopsie]
#[oopsie(module(false), suffix)]
pub enum CfgCodeRefError {
    #[oopsie("v: {keep}")]
    V {
        #[cfg(any())]
        #[oopsie(provide(oopsie::ErrorCode => oopsie::ErrorCode::from(extra)))]
        extra: &str,
        keep: u32,
    },
}
```

```
error[E0425]: cannot find value `extra` in this scope
```

(identical failure on the struct path, verified.)

**Fix:** apply `field_cfg_for(categorized, field_ident)` to the field-level
provide statements (like `trace_field_cfg` does for trace fields) and to the
code-accessor arm/method generated from a field-level provide.

### F6 — Synthetic generic names `__T` / `__T{i}` collide with user-declared parameters of the same name (E0403)

**Severity: low**

- `fail`'s synthetic `Ok`-type parameter `__T` (gen_selectors.rs:957) clashes
  with a user parameter literally named `__T` when it is free for the variant:
  `enum E<T: Debug, __T: Debug> { A { value: T }, B { other: __T } }` →
  `pub fn fail<__T, __T>` → `error[E0403]: the name '__T' is already used for a
  generic parameter`.
- The `Into` parameters `__T{i}` (gen_selectors.rs:88) clash with a user
  parameter named `__T0`, both when the user parameter is referenced by the
  same variant (selector struct decl `<__T0, __T0>`) and when it is free
  (method generics).
- Related hazard: `SelectorShape::sourced_impl` (gen_selectors.rs:196) picks
  out synthetic parameters by `param_name(param).starts_with("__T")`, which
  also matches a *user* parameter named `__T0`, duplicating it in the impl
  generics.

**Fix:** derive the synthetic names by probing the declared parameter set
(`__T`, `__T0`, … skipping taken names), the same way
`gen_display::formatter` already dedups `__oopsie_fmt`.

### F7 — A field named `__request` shadows the mangled `provide` parameter (E0599, nightly only)

**Severity: low**

The `Error::provide` parameter is mangled to `__request` (gen_error.rs:191,
723) to survive a user field named `request` — but a user field named
`__request` is bound by the destructure and shadows the parameter, so the
generated `#req.provide_ref::<Backtrace>(...)` resolves against the field:

```
error[E0599]: no method named `provide_ref` found for reference `&u32` in the current scope
```

Only reachable with `unstable-error-generic-member-access`; stable emits no
`provide` method.

**Fix:** same dedup-by-probing approach as `formatter()`.

### F8 — A source field whose type's last segment is `Backtrace`/`SpanTrace` is dual-classified as source and trace field (E0416/E0025)

**Severity: low**

`CategorizedFields::from_fields` (parse.rs:1697-1756) records
`backtrace_field`/`spantrace_field`/`traces_field`/`location_field` purely by
attribute or last-segment type match, independent of the source classification
that happens later in the same loop (line 1776). A user error type literally
named `Backtrace` used as a source (`Wrap { source: Backtrace, n: u32 }`)
produces a `Diagnostic` accessor arm whose pattern binds `source` twice:

```
error[E0416]: identifier `source` is bound more than once in the same pattern
error[E0025]: field `source` bound multiple times in the pattern
```

plus a bogus `Borrow::<oopsie::Backtrace>::borrow(source)` body. The
last-segment heuristic is documented, but the *overlap with the source field*
is unhandled.

**Fix:** skip trace-field detection for the field classified as the source
(or reject the combination with a targeted error).

### F9 — A `#[cfg]`-gated field that references a generic parameter leaves the parameter dangling on the stripped selector (E0392)

**Severity: low**

`selector_shape` (gen_selectors.rs:74-99) runs `referenced.add_type` for every
user field including cfg-gated ones, so the parameter is projected onto the
selector struct; when the cfg strips the field, the struct keeps a now-unused
parameter:

```rust
#[oopsie::oopsie]
#[oopsie(module(false), suffix)]
pub enum CfgGenericError2<T: std::fmt::Debug> {
    #[oopsie("v")]
    V { #[cfg(any())] x: T, keep: u32 },
    #[oopsie("w")]
    W { y: T },
}
```

```
error[E0392]: type parameter `T` is never used   (on the generated VOopsie<T>)
error[E0282]: type annotations needed            (cascade at the build site)
```

Unlike F5, the field/type references themselves are correctly gated — only the
parameter projection dangles. A fix would need a `PhantomData` field
(changing the selector's public shape) or a targeted rejection of
cfg-gated parameter-referencing fields.

### F10 — Generated `From` impls for transparent items can collide (E0119) with no targeted diagnostic

**Severity: low**

Both verified:

a) Two transparent variants with the same source type
   (`A { source: io::Error }`, `B { source: io::Error }`) → two
   `impl From<io::Error> for E` → `error[E0119]: conflicting implementations`.
   thiserror rejects duplicate `#[from]` types with its own diagnostic; here
   the user gets a raw coherence error.

b) A transparent variant with an auto-boxed self source,
   `Wrap { source: Box<SelfBox> }`: auto-boxing generates
   `impl From<SelfBox> for SelfBox`, which conflicts with core's blanket
   `impl<T> From<T> for T` → E0119. The desired conversion is genuinely
   unexpressible, so this needs a macro-side error, not better codegen.

**Fix:** detect same-source-type transparent duplicates during
`ResolvedEnum::resolve`, and reject `AutoBoxed` sources whose inner type is
the error type itself, with a targeted message.

### F11 — `vis` (and struct `suffix`) are silently ignored on `transparent` items

**Severity: low**

`VariantAttrs` accepts `vis`, and the struct attribute set accepts `vis` and
`suffix`, but the transparent code paths (gen_selectors.rs:428-480 for enums,
609-653 for structs) emit only a `From` impl and never read them. The codebase
otherwise errors on inert attributes (e.g. `reject_inert_variant_traced` for a
non-traced enum), so silently dropping these is inconsistent.

**Fix:** reject `vis`/`suffix` on transparent items in `validate_transparent`
(or at least on `transparent` variants where `vis` is variant-scoped).

## Checked and cleared

- **Shared `use AsErrorSource as _;` in `oopsie_backtrace`/`oopsie_spantrace`:**
  suspected unused-import warning when one accessor uses the source and the
  other doesn't. Verified no warning — rustc exempts `use Trait as _` imports
  from `unused_imports` (mixed own-field + source-field enum compiles clean).
- **Static help/code literal handling** (`is_static`, `static_lit`,
  `unescape_format_braces`, `unmatched_close_brace`, `format_str_has_placeholder`):
  hand-traced `{{{`, `{{}}`, lone `}`, lone `{` against rustc's format parser
  rules; behavior matches (placeholders only opened by unescaped `{`, lone `}`
  rejected on the static path exactly as rustc rejects it).
- **`exit_code` parsing:** `1..=255` enforced through `u16` → `u8` checked
  conversion; `0`, `300`, non-integers all rejected at parse time.
- **`size(...)` parsing:** all six shapes plus degenerate ranges (`..`, `..0`,
  `64..64`, `64..=32`) handled with targeted errors; `size = 64` name-value
  rejected; span fallback on stable is documented and harmless.
- **Generic-parameter defaults:** `strip_default` is applied everywhere the
  derive re-emits parameters (selector projection, `sourced_impl`,
  `free_params`); `syn::split_for_impl` covers the impls that use it directly.
  Covered by `defaulted_type_and_const_params` tests.
- **`fail` method generic ordering:** free lifetimes are partitioned ahead of
  the synthetic `__T` (`leaf_free_of_lifetime_and_const` passes).
- **Unconstrained type/const parameters on sourced variants:** the guard fires
  with a clear, spanned error (verified with `const N: usize`).
- **Unit variants / empty enums:** `Self::Unit { .. }` patterns and
  `match *self {}` bodies compile; verified with a mixed enum.
- **cfg handling for variants, user fields, trace fields, help fields, and
  `cfg_attr`:** extensively covered by `derive_cfg.rs` and re-verified; the
  only gaps found are F5 and F9.
- **`is_error_code_provide` with a leading `::`:** `::oopsie::ErrorCode`
  matches correctly (matching is on path segments, not the leading colon);
  covered by `derive_error.rs` unstable provide tests.
- **Manifest settings (`settings.rs`):** `deny_unknown_fields` everywhere,
  ident-fragment validation for `module.suffix`/`default-suffix` (empty or
  invalid names rejected, so `Ident::new` in `wrap_in_module` can't panic),
  cap-0 rejection, workspace-root discovery bounded by `$CARGO_HOME` and
  `target/package`, leaf-by-leaf package-over-workspace merge — all with
  thorough unit tests.
- **`strip_angle_brackets`:** projected declaration parameters never have
  bounds/defaults (cleared in `project`), so the rendered list always ends in
  a lone `>` token; the token surgery is safe.
- **Selector `Into` ergonomics vs parameter-typed fields:** E0392 avoidance
  (parameter-typed fields keep their concrete type) is correct and tested;
  shorthand projections (`T::Item`) route `Into` bounds onto the methods that
  declare the free base parameter (commit 3331b65) — verified reading against
  `params_for_into_bound`/`record_projection_base`.
- **`forward(...)` resolution:** tristate parsing, default on/off per trace
  kind, and the own-field conflict rules are all enforced with targeted errors.
- **`transparent` validation:** requires exactly a source, rejects extra user
  fields, rejects redundant `forward(...)`; the From/Contextual bodies run
  capture probes before moving the (possibly transformed) source — order is
  correct in all three generation sites.
- **Display formatter mangling:** `formatter()` dedups against all field
  names; `f`-named-field regression tested (`derive_hygiene.rs`).

## Residual risks

- **Nightly-only paths** (`unstable-error-generic-member-access`): I
  compile-verified the failure modes (F5, F7) but did not exhaustively exercise
  the runtime `provide()` behavior (first-wins ordering across deep chains,
  interplay of field provides with `code`/`help` attrs — see below).
- **Precedence disagreement between the nightly provide path and the stable
  accessors when a user combines a field-level `ErrorCode` provide with a
  `code = "..."` attr:** `provide()` emits field provides first (Request is
  first-wins, so the field wins), while `oopsie_error_code()` prefers the
  `code` attr. A misconfiguration, but the two paths silently disagree; I did
  not verify at runtime.
- **`pub(in self::foo)` visibility**: `lift_into_child_module`'s fallback arm
  would emit `pub(in super::self::foo)`, which is invalid syntax. Pathological;
  untested.
- **Lifetime bounds in `for<'x>` HRTB form** inside type-param bounds
  (`T: for<'x> Trait<&'x U>`): the trait-bound visitor visits the path but I
  did not construct an end-to-end test with a declared parameter nested only
  inside an HRTB.
- **The `traced`/`#[oopsie]` attribute-macro pipeline** (injection ordering,
  mangled-field interactions) is only covered here where it feeds the derive
  (F3); the injection logic itself belongs to a different audit dimension.
