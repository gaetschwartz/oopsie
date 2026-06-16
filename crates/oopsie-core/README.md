# oopsie-core

Core runtime types for the [`oopsie`](https://crates.io/crates/oopsie)
error-handling crate — `Backtrace`, `SpanTrace`, `Diagnostic`, `Welp`, and the
trace-capture machinery.

You almost certainly want the [`oopsie`](https://crates.io/crates/oopsie) facade
instead: it re-exports this crate's public surface alongside the `#[oopsie]`
macro and the colorized `Report` renderer.

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
