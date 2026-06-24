# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0-rc.3]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.3
[0.1.0-rc.2]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.2
[0.1.0-rc.1]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.1
