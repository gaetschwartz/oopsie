# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0-rc.22] - 2026-08-17

### Fixed

- A `#[oopsie(help)]` field no longer breaks `no_std` consumers. The generated
  help accessor called `.to_string()` through the prelude, which resolves only
  under `std`; it now goes through the same `alloc` facade as the rest of the
  generated code.
- `#[oopsie]` now handles helper attributes and gates wrapped in `cfg_attr`.
  Previously `#[cfg_attr(pred, oopsie(…))]` survived into the emitted item and
  failed to compile once the predicate held, a `cfg_attr`-injected `cfg` on a
  variant left generated selectors and `From` impls referring to a stripped
  variant, and the same on a trace field mis-dropped its accessor arms.
- A `transparent` source of `Box<other::Name>` is no longer rejected when it
  merely shares the enclosing type's name. Only an unqualified path now counts
  as the enclosing type.
- The generated nightly `provide` method no longer collides with a lifetime
  parameter the error type itself declares.
- A message-free `Welp` (`.welp()` / `Welp::from_error`) now surfaces the
  wrapped error's error code and help text, matching how it already surfaced
  the exit code and how generated `transparent` wrappers behave. A `Welp`
  carrying its own message is unchanged: its header stays uncoded.
- `test-utils` no longer panics on Windows. The redaction profile required
  `HOME` to be set and passed filesystem paths to `insta` as regular
  expressions, so a backslashed path aborted the run.
- `[package.metadata.oopsie]` written as an inline table is now honored instead
  of silently ignored, so a `max-size` cap written that way is enforced.
- Workspace member and exclude patterns now match the way Cargo matches them:
  case-sensitively, with `exclude` pruning glob-matched members, and correctly
  under a root path containing glob metacharacters.
- Non-UTF-8 values of `CARGO_MANIFEST_DIR` no longer fail every derive in the
  crate; a workspace root that cannot be tracked for rebuilds now reports why.
- A `module-suffix` that cannot form an identifier is now a manifest
  diagnostic rather than a panic inside the macro.

## [0.1.0-rc.21] - 2026-08-04

### Changed

- Renamed the `settings` cargo feature to `experimental-settings` and
  documented it as an experimental debug tool, including the caveat that
  Cargo's feature unification can apply workspace defaults to a crate that
  did not itself opt in.
- `oopsie-macros` now builds against `syn` 3 and `darling` 0.24. This is an
  internal dependency upgrade: the macro surface, generated code, and minimum
  supported Rust version are all unchanged.

## [0.1.0-rc.20] - 2026-07-12

### Changed

- The `fancy` feature now enables `std` (`fancy = ["std", …]`) instead of being
  an independent feature that failed to compile when combined with a `no_std`
  build. Enabling `fancy` on a `no_std` build now transparently pulls in `std`,
  matching Rust's usual additive-feature convention (a feature enables what it
  needs) rather than erroring. `no_std` builds without `fancy` are unaffected.

## [0.1.0-rc.19] - 2026-07-11

### Added

- `no_std` support (alloc-based) for `oopsie-core` and `oopsie`, gated by a new
  default-on `std` feature. Build with `default-features = false` to target
  `no_std`; an allocator is required. The error types, `Diagnostic`, `Chain`,
  `Welp`, `#[derive(Oopsie)]`, and `SpanTrace` are all available without `std`.
  `serde` (and the `erased` API) work under `no_std` too. The `fancy` feature
  (colorized `Report` rendering), panic hooks, real backtraces, `tracing`, and
  clock-based timestamps require `std`; enabling `fancy` on a `no_std` build is
  a compile error. Under `no_std`, backtraces are always empty.

## [0.1.0-rc.18] - 2026-06-29

### Changed

- The `fancy` feature is no longer enabled by default. Colorized, richly
  formatted reports (and their `owo-colors` / `linkme` dependencies) are now
  opt-in — add `features = ["fancy"]` to keep the previous behavior. The default
  build is now dependency-light and no longer includes any report renderer.

## [0.1.0-rc.17] - 2026-06-26

### Changed

- Error backtraces now use the captured caller location to anchor frame
  trimming. The frame at the `.fail()` / `.welp()` / `.new()` call site marks
  the top of the user-relevant stack, so any capture or macro-generated frames
  left above it are hidden even when symbol-based filtering doesn't recognize
  them. It is best-effort and purely additive: when no frame matches the
  location — for example a wrapped-error chain whose surfaced backtrace and
  location originate at different sites — the existing trimming is left
  untouched.

## [0.1.0-rc.16] - 2026-06-25

### Added

- Reports now render a placeholder in the `SPANTRACE` section when a span trace
  is present but not captured, instead of omitting the section. Empty traces
  (captured with no active span) show `... no spans captured ...`, and
  unsupported ones (no `ErrorLayer` installed) show
  `... span traces unsupported ...`.
- New `SpanTraceStatus` enum (`Captured`, `Empty`, `Unsupported`) describing why
  a span trace is or isn't populated.

### Changed

- `SpanTrace::status()` now returns oopsie's own `SpanTraceStatus` instead of
  re-exposing `tracing-error`'s type.

## [0.1.0-rc.15] - 2026-06-24

### Added

- Enum variants can now opt out of trace injection individually. On a traced
  enum, marking a variant `#[oopsie(traced = false)]` skips every field that
  would otherwise be injected into it — backtrace, span trace, timestamp, and
  caller location — along with its auto-generated error code. A variant that
  opts out can keep an explicit discriminant (`Variant = 1`), which injected
  fields would otherwise make impossible.
- Using a variant-level `traced` toggle on an enum that is not itself traced is
  now rejected at compile time, with the error pointing at the offending
  attribute, instead of being silently ignored.

## [0.1.0-rc.14] - 2026-06-24

### Added

- Generated field selectors now document each field's type. Every selector
  field's doc comment gains a line naming the field's type, rendered as idiomatic
  Rust (for example `Vec<T>`, `HashMap<K, V>`, or `&'a mut str`).

## [0.1.0-rc.13] - 2026-06-23

### Fixed

- An error type declared inside a function body (such as a throwaway type in a
  `#[test]`) no longer fails to compile when a size cap applies to it — whether
  from a per-type `#[oopsie(size(...))]` or a project-wide
  `[workspace.metadata.oopsie]` / `[package.metadata.oopsie]` `max-size`. The
  generated size check now resolves function-local error types.

## [0.1.0-rc.12] - 2026-06-23

### Added

- `#[oopsie(size(...))]` now accepts exclusive-upper ranges: `size(..N)` (fewer
  than `N` bytes) and `size(N..M)` (at least `N` and fewer than `M`). The
  unbounded `size(..)`, the unsatisfiable `size(..0)`, and empty ranges (such as
  `N..N`, or a low bound not below the high bound) are rejected at compile time.

### Changed

- When a type violates a `size(...)` constraint, the compile error now reports
  the type's measured size — and, for an enum, the offending variant's size — for
  example ``` `Error` is 80 bytes, must be ≤ 64; largest variant `Big` is 72
  bytes ``` instead of only restating the limit.

## [0.1.0-rc.11] - 2026-06-23

### Added

- Workspace-wide settings: the same keys accepted by `[package.metadata.oopsie]`
  can now be declared once under `[workspace.metadata.oopsie]` at the workspace
  root. Each knob resolves independently — a member's `[package.metadata.oopsie]`
  entry overrides only the matching workspace knob; the rest are still inherited.
  Crates the workspace `exclude`s are unaffected. When `max-size` is exceeded, the
  error message names the table that set the cap.

## [0.1.0-rc.10] - 2026-06-22

### Added

- A `settings` cargo feature that reads project-wide defaults from a
  `[package.metadata.oopsie]` table in the consumer's `Cargo.toml`. Each key is
  the manifest form of a per-type `#[oopsie(...)]` attribute, which overrides it:
  `max-size` (a default size cap), `default-suffix` and `default-vis` (selector
  suffix and visibility), `module` (`true`/`false` or a `{ enabled, suffix }`
  table), and `traced` (`true`/`false` or a `{ enabled, location, timestamp,
  packed, boxed, code }` table). Defaults are read from each crate's own
  manifest, never its dependencies'.

## [0.1.0-rc.9] - 2026-06-19

### Changed

- When an enum exceeds its `#[oopsie(size(...))]` upper bound, the compile error
  now points at — and names — the variant responsible (the largest one) instead
  of the `size(...)` attribute. Lower-bound (too-small) violations still report
  against the whole type.

## [0.1.0-rc.8] - 2026-06-19

### Removed

- The `OOPSIE_MAX_ERROR_SIZE` environment variable (added in 0.1.0-rc.6). Cap
  error sizes with the per-type `#[oopsie(size(...))]` attribute instead.

## [0.1.0-rc.7] - 2026-06-19

### Added

- Generated `From` conversions for transparent errors now carry a doc comment
  describing the conversion (e.g. "Converts `InnerError` into
  `Wrapper::Inner`."), so they read clearly in rustdoc alongside the context
  selectors.

## [0.1.0-rc.6] - 2026-06-19

### Added

- `OOPSIE_MAX_ERROR_SIZE` environment variable: set it at build time to assert a
  default maximum error size (in bytes) on every `#[derive(Oopsie)]` error in the
  crates you build directly. A per-type `#[oopsie(size(...))]` overrides it;
  dependencies are unaffected. Unset or empty disables it.

## [0.1.0-rc.5] - 2026-06-18

### Changed

- Struct context selectors are again generated at item scope as `<Name>Oopsie`
  (e.g. `LoadError` → `LoadOopsie`) instead of inside a generated
  `<name>_oopsies` module. Structs default to no module plus the `Oopsie`
  suffix; enums keep the module form. Both `module` and `suffix` still override
  per type.

## [0.1.0-rc.4] - 2026-06-18

### Added

- `#[oopsie(forward(...))]` field attribute: on a source field, forward the
  source's backtrace, span trace, and (opt-in) caller location through this
  error's `Diagnostic` impl instead of capturing this layer's own, so nested
  errors stay small. Per-trace configurable via
  `forward(backtrace = …, spantrace = …, location = …)` (`backtrace`/`spantrace`
  forwarded by default, `location` opt-in).

## [0.1.0-rc.3] - 2026-06-17

### Added

- `Style::from_rgb` and `Style::hex("#rrggbb")` constructors.
- `oopsie::tracing::default_error_layer`, for span-trace capture without the
  `serde` feature.

### Changed

- `oopsie::tracing` now requires only the `tracing` feature instead of `tracing`
  + `serde`.
- `Report` paints the caller location in the theme's `hint` color instead of
  plain dim.
- `Theme` is no longer `Copy` (it remains `Clone`).
- Replaced the `report_main` and `spantrace` examples with a single `complete`
  example covering the full feature set.

## [0.1.0-rc.2] - 2026-06-16

### Changed

- Documentation: README overhaul (badges, a colorized report screenshot, sharper
  framing), the feature-flags table moved into the crate docs, and dedicated
  READMEs for `oopsie-core` and `oopsie-macros`.

## [0.1.0-rc.1] - 2026-06-16

Initial release candidate.

### Added

- `#[oopsie]` attribute macro and the `Oopsie` derive: generated context
  selectors plus `Display`, `Debug`, and `Error` impls. `traced` additionally
  captures a backtrace (and, with the `tracing` feature, a span trace), and can
  inject timestamps and caller locations.
- `Report`: a colorized renderer for the error chain, span trace, and backtrace,
  usable as a `main` return type, with a matching panic hook (`fancy` feature).
- `Welp`: a string-shaped error escape hatch for prototypes and one-off errors.
- Type-erased, serializable errors via the `erased` module (`serde` feature).
- Feature flags: `fancy`, `serde`, `tracing`, `chrono`, `jiff`, `extras`, and
  the nightly `unstable-*` set.

[0.1.0-rc.22]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.22
[0.1.0-rc.21]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.21
[0.1.0-rc.20]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.20
[0.1.0-rc.19]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.19
[0.1.0-rc.18]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.18
[0.1.0-rc.17]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.17
[0.1.0-rc.16]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.16
[0.1.0-rc.15]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.15
[0.1.0-rc.14]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.14
[0.1.0-rc.13]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.13
[0.1.0-rc.12]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.12
[0.1.0-rc.11]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.11
[0.1.0-rc.10]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.10
[0.1.0-rc.9]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.9
[0.1.0-rc.8]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.8
[0.1.0-rc.7]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.7
[0.1.0-rc.6]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.6
[0.1.0-rc.5]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.5
[0.1.0-rc.4]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.4
[0.1.0-rc.3]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.3
[0.1.0-rc.2]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.2
[0.1.0-rc.1]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.1
