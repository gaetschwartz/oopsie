# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0-rc.1]: https://github.com/gaetschwartz/oopsie/releases/tag/v0.1.0-rc.1
