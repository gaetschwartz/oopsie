# oopsie 💥

Ergonomic, structured error handling for Rust — typed context selectors,
automatic backtrace / span-trace capture, and rich colorized reports.

[![crates.io](https://img.shields.io/crates/v/oopsie.svg)](https://crates.io/crates/oopsie)
[![docs.rs](https://img.shields.io/docsrs/oopsie)](https://docs.rs/oopsie)
[![license](https://img.shields.io/crates/l/oopsie.svg)](#license)

![A colorized oopsie error report — error code, source chain, and a filtered backtrace](https://raw.githubusercontent.com/gaetschwartz/oopsie/develop/assets/report.png)

`oopsie` is `0.x`: the public API may still change between minor releases.

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

`#[oopsie]` generates context selectors, `Display`, `Debug`, and `Error` impls.
`traced` adds backtrace + span-trace capture and an automatic error code.
`Report` renders errors (and panics, via its panic hook) as colorized reports.

## Crates

| Crate | What it is |
|-------|------------|
| [`oopsie`](https://docs.rs/oopsie) | The facade: `#[oopsie]` macro, `Report`, panic hook, prelude. Start here. |
| [`oopsie-core`](https://docs.rs/oopsie-core) | Core types: `Backtrace`, `SpanTrace`, `Diagnostic`, `Welp`. |
| [`oopsie-macros`](https://docs.rs/oopsie-macros) | Proc macros: `#[oopsie]` attribute and `Oopsie` derive. |

The feature flags are documented in the [crate docs](https://docs.rs/oopsie/latest/oopsie/#feature-flags).

## Development

The minimum supported Rust version is **1.89**. The repository pins a nightly
toolchain in `rust-toolchain.toml`, which the snapshot tests and the
unstable-feature lanes need; stable contributions still build on 1.89.

```sh
just test    # the snapshot-bearing combos (stable + nightly) plus doctests
just clippy  # lint both channels with -D warnings
cargo fmt    # format
```

Snapshot tests only match on the two blessed combinations encoded in the
`just nextest` recipe — stable and nightly, each with
`fancy,serde,tracing,chrono` (nightly also `unstable`). Other feature
combinations compare against the wrong snapshots and skip themselves. Run
`just test-bless` after a change that legitimately shifts a snapshot.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
