# Verification of F11 — `vis` (and struct `suffix`) silently ignored on `transparent` items

## Claim

`VariantAttrs` accepts `vis` and `StructAttrs` accepts `vis`/`suffix`, but the transparent
code paths (`gen_selectors.rs:428` for enum variants, the `variant_attrs.transparent` branch
for structs) emit only a `From` impl and never read those attributes. The attributes are
silently dropped, inconsistent with the codebase's other inert-attribute errors (e.g.
`reject_inert_variant_traced`, the `forward(...)`-on-transparent rejection in
`validate_transparent`). Suggested fix: reject `vis`/`suffix` on transparent items in
`validate_transparent`.

## What I checked

Code read:

- `crates/oopsie-macros/src/derive/gen_selectors.rs` — the transparent variant branch
  (line 428, reached when `v.selector_ident` is `None`) emits only a `From` impl and
  `continue`s at line 479, before `variant_attrs.visibility()` is consulted at line 485.
  The struct transparent branch (line 609) returns the `From` impl before `vis` (computed
  line 582) and `effective_suffix` (read line 655) are ever used.
- `crates/oopsie-macros/src/derive/model.rs` — `validate_transparent` (line 223) checks only
  source presence, redundant `forward(...)`, and extra user fields. No `vis`/`suffix`
  rejection. `ResolvedStruct::resolve` even deliberately skips the selector-name check for
  transparent structs (lines 187-191), confirming no selector exists for the attributes to
  apply to.
- `crates/oopsie-macros/src/derive/parse.rs` — `VariantAttrs.vis` (line 604) and
  `StructAttrs.vis` (line 655) plus `StructAttrs.container.suffix` (via flattened
  `EnumContainerAttrsInner`, line 213) are all accepted keys with no transparent-conditional
  validation.
- `crates/oopsie-macros/src/derive/mod.rs` — for a transparent struct, `wrap_in_module` is
  skipped entirely (lines 131-140), so `attrs.visibility()` used for `module_vis`
  (lines 126-130) is also dead on that path.
- Grep for every `.visibility()` caller: only `gen_selectors.rs:485` (variant, unreachable
  for transparent), `gen_selectors.rs:582` (struct, unused on the transparent branch), and
  `mod.rs:74/127` (container vis for module wrapping; struct case skipped for transparent).
- Tests: `crates/oopsie/tests/derive_transparent.rs` contains neither `vis` nor `suffix`;
  `derive_suffix_vis.rs` contains no `transparent` case. No trybuild fixture pins the
  accepted-but-ignored behavior as intentional.

Reproduction (throwaway crate `/tmp/verify-f11` depending on the workspace `oopsie`):

    #[derive(Debug, Oopsie)]
    enum EnumErr {
        #[oopsie(display("x"), transparent, vis(pub))]
        Io { source: std::io::Error },
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(display("x"), transparent, vis(pub), suffix("Blah"))]
    struct StructErr { source: std::io::Error }

- `cargo build` succeeds with zero warnings — both `vis(pub)` and `suffix("Blah")` are
  silently accepted.
- Referencing `StructErrBlah` afterwards fails with `E0425: cannot find value` — proving no
  selector was emitted, so neither attribute had any effect.

## Refutation attempts that failed

1. *Maybe `vis` has another consumer on the transparent path.* Refuted: the exhaustive grep
   of `.visibility()` callers shows no reader reachable from a transparent item.
2. *Maybe a test or stderr fixture pins this as accepted-by-design.* Refuted: no test
   combines `transparent` with `vis`/`suffix`; the compile-fail fixtures for transparent
   cover only missing source and extra fields.
3. *Maybe the behavior is harmless because transparent means "no selector", so `vis`/
   `suffix` are vacuous rather than wrong.* This is exactly the finding: the keys are
   accepted yet meaningless, while the same file's `validate_transparent` rejects the
   equally-vacuous `forward(...)` as redundant, and `reject_inert_variant_traced` rejects an
   inert variant-level `traced`. The inconsistency is real.
4. *Maybe the suggested fix would break something.* Not a refutation — the fix would be a
   new compile error for code whose attributes currently do nothing, matching existing
   practice. (Implementation note: `validate_transparent` currently takes only a span and
   `&CategorizedFields`; it would need the attrs passed in.)

## Verdict

**Confirmed.** Severity **low** is correct: nothing misbehaves at runtime and no wrong code
is generated — the defect is purely a silent no-op attribute, a DX/consistency wart. One
extra observation that slightly broadens (not refutes) the finding: `module(...)` on a
transparent struct is likewise silently ignored (`wrap_in_module` is skipped), the same
class of issue.
