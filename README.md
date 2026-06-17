# oopsie 💥

Structured, context-rich error handling for Rust — define your error type once,
attach it to any `Result` with `.context(...)`, and render colorized diagnostic
reports with automatic backtraces and span traces.

`oopsie` is `0.x`: the public API may still change between minor releases.

[![crates.io](https://img.shields.io/crates/v/oopsie.svg)](https://crates.io/crates/oopsie)
[![docs.rs](https://img.shields.io/docsrs/oopsie)](https://docs.rs/oopsie)
[![MSRV](https://img.shields.io/badge/MSRV-1.89-blue.svg)](https://crates.io/crates/oopsie)
[![license](https://img.shields.io/crates/l/oopsie.svg)](#license)

![A colorized oopsie report — typed error code, source chain, span trace, and backtrace](https://raw.githubusercontent.com/gaetschwartz/oopsie/develop/assets/report.png)

## 📦 Install

```sh
cargo add oopsie
```

## 🚀 Quick start

```rust
use oopsie::prelude::*;

#[oopsie::oopsie(traced)]
pub enum AppError {
    #[oopsie("Connection to {host} failed")]
    Connect { host: String, source: std::io::Error },
}

fn connect(host: &str) -> Result<(), AppError> {
    std::net::TcpStream::connect(host)
        .context(app_oopsies::Connect { host })?;
    Ok(())
}

fn main() -> oopsie::Report<AppError> {
    oopsie::Report::run(|| connect("127.0.0.1:5432"))
}
```

`#[oopsie]` generates a *context selector* per variant — here `app_oopsies::Connect`,
in a module named after the type (`AppError` → `app_oopsies`) — alongside `Display`,
`Debug`, and `Error` impls. `.context(selector)` attaches your typed error to any
`Result` or `Option`. `traced` adds a captured backtrace (and a span trace with the
`tracing` feature) plus an automatic error code, and `Report` renders the result —
including panics, via its hook — as the colorized output shown above.

Unlike `thiserror`, the attachment points and the renderer come built in; unlike
`anyhow` / `eyre`, your errors stay strongly typed.

## Crates

| Crate | What it is |
|-------|------------|
| [`oopsie`](https://docs.rs/oopsie) | The facade — `#[oopsie]`, `Report`, panic hook, prelude. **Start here.** |
| [`oopsie-core`](https://docs.rs/oopsie-core) | Lower-level types (`Backtrace`, `SpanTrace`, `Diagnostic`, `Welp`), used through the facade. |
| [`oopsie-macros`](https://docs.rs/oopsie-macros) | The `#[oopsie]` attribute and `Oopsie` derive. |

Feature flags are documented in the [crate docs](https://docs.rs/oopsie/latest/oopsie/#feature-flags).

## Development

The MSRV is **1.89**. A nightly toolchain is pinned in `rust-toolchain.toml` for the
snapshot tests and the unstable-feature lanes — you don't need it to use the crate.

```sh
just test    # snapshot-bearing combos (stable + nightly) plus doctests
just clippy  # lint both channels with -D warnings
cargo fmt    # format
```

Snapshot tests only match on the blessed feature combos; run `just test-bless`
after a change that legitimately shifts one.

## Acknowledgements

`oopsie` builds on ideas from the Rust error-handling ecosystem:

- [**snafu**](https://github.com/shepmaster/snafu) — the context-selector pattern
  behind `#[oopsie]`: a selector per variant, attached to a `Result` with `.context(...)`.
- [**color-eyre**](https://github.com/eyre-rs/color-eyre) — the rich, colorized
  rendering of the error chain, span trace, and backtrace, and the matching panic hook.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
