# oopsie

Ergonomic, structured error handling for Rust — typed context selectors,
automatic backtrace/span-trace capture, and rich colorized reports.

> Pre-release: not yet published to crates.io. Public API changes freely.

## Quick start

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
| [`oopsie`](crates/oopsie) | The facade: `#[oopsie]` macro, `Report`, panic hook, prelude. Start here. |
| [`oopsie-core`](crates/oopsie-core) | Core types: `Backtrace`, `SpanTrace`, `Diagnostic`, `Welp`. |
| [`oopsie-macros`](crates/oopsie-macros) | Proc macros: `#[oopsie]` attribute and `Oopsie` derive. |

## Features

| Feature | Default | Toolchain | Effect |
|---------|---------|-----------|--------|
| `fancy` | yes | stable | `Report`, color output, panic hook |
| `chrono` | no | stable | `chrono::DateTime<Local>` timestamps for `traced(timestamp(chrono = true))`; no direct `chrono` dependency required |
| `unstable` | no | nightly | umbrella for the two features below |
| `unstable-error-generic-member-access` | no | nightly | trace/diagnostic surfacing through the Provider API |
| `unstable-try-trait-v2` | no | nightly | `?` converts directly into `Report` |

MSRV: 1.89. Development uses the nightly pinned in `rust-toolchain.toml`.

## Development

```sh
just test        # full matrix: stable/default + nightly/--features unstable, then doctests
just test-bless  # re-bless insta snapshots and trybuild stderr
just clippy      # lints exactly as CI runs them (-D warnings)
```

Snapshot tests are only valid for the two blessed combos — **stable + default
features** and **nightly + `--features unstable`**. Other feature combinations
compare against the wrong snapshots.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
