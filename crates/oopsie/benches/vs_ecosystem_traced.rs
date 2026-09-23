#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
//! Comparing oopsie against other error libraries when rendering errors that
//! carry a backtrace (`render_traced*`). Split from `vs_ecosystem` because
//! the ecosystem crates read `RUST_BACKTRACE`/`RUST_LIB_BACKTRACE` once and
//! cache the decision process-wide — a single binary can't fairly compare a
//! backtrace-free group and a backtrace-carrying group side by side.
//!
//! [`enable_env_backtraces`] forces `RUST_BACKTRACE=1` at startup so
//! anyhow/eyre capture frames here. oopsie's backtrace comes from
//! `#[oopsie(traced(spantrace(false)))]` plus the thread-local override;
//! snafu's `backtrace` feature captures one too, but its `Report` only
//! renders it under the nightly provider API (not enabled here), so the
//! snafu arm measures a backtrace-free render regardless.

use std::hint::black_box;
use std::io;
use std::sync::Once;

use criterion::{Criterion, criterion_group, criterion_main};
use oopsie::backtrace::set_override;
use oopsie::{Report, RustBacktrace, oopsie};

#[oopsie(traced(spantrace(false)))]
#[oopsie("wrap failed: {ctx}")]
struct TracedError {
    ctx: &'static str,
    source: io::Error,
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[derive(Debug, snafu::Snafu)]
#[snafu(display("wrap failed: {}", ctx))]
struct SnafuTraced {
    ctx: &'static str,
    source: io::Error,
    backtrace: Option<snafu::Backtrace>,
}

#[inline(always)]
fn io_err() -> Result<(), io::Error> {
    Err(io::Error::other("boom"))
}

/// Force `RUST_BACKTRACE=1` so anyhow/eyre/snafu capture frames here. They
/// read the variable once and std locks the decision on the first capture,
/// so this must run before any error is built.
fn enable_env_backtraces() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: runs once before the benchmark body executes and before any
        // backtrace is captured, so no other thread reads or writes the
        // environment concurrently.
        #[expect(
            unsafe_code,
            reason = "edition 2024 makes env::set_var unsafe; the Once guard upholds the no-concurrent-access requirement"
        )]
        unsafe {
            std::env::set_var("RUST_BACKTRACE", "1");
        };
    });
}

/// Plain full-chain rendering of errors that carry a backtrace, across crates.
fn bench_traced(c: &mut Criterion) {
    enable_env_backtraces();
    set_override(RustBacktrace::Enabled);
    let mut group = c.benchmark_group("render_traced");

    let oopsie_report = Report::new(
        oopsie::ResultExt::context(io_err(), TracedOopsie { ctx: "render" }).unwrap_err(),
    )
    .no_colors();
    group.bench_function("oopsie", |b| {
        b.iter(|| black_box(format!("{oopsie_report}")));
    });

    #[cfg(feature = "unstable-error-generic-member-access")]
    {
        let snafu_report = snafu::Report::from_error(
            snafu::ResultExt::context(io_err(), SnafuTracedSnafu { ctx: "render" }).unwrap_err(),
        );
        group.bench_function("snafu", |b| {
            b.iter(|| black_box(format!("{snafu_report}")));
        });
    }

    let anyhow_err = anyhow::Context::context(io_err(), "render").unwrap_err();
    group.bench_function("anyhow", |b| {
        b.iter(|| black_box(format!("{anyhow_err:?}")));
    });

    // eyre is intentionally absent: its default handler renders no backtrace
    // (only color-eyre does), so it would be timing a message-only render here.
    // It appears in `render_traced_colored` via color-eyre instead.

    group.finish();
}

/// Colored rendering of backtrace-carrying errors: oopsie's printer versus
/// color-eyre, the ecosystem's colored-backtrace renderer.
fn bench_traced_colored(c: &mut Criterion) {
    enable_env_backtraces();
    set_override(RustBacktrace::Enabled);
    let _ = color_eyre::install();
    let mut group = c.benchmark_group("render_traced_colored");

    let oopsie_report = Report::new(
        oopsie::ResultExt::context(io_err(), TracedOopsie { ctx: "render" }).unwrap_err(),
    )
    .force_colors();
    group.bench_function("oopsie", |b| {
        b.iter(|| black_box(format!("{oopsie_report}")));
    });

    let color_eyre_report = eyre::WrapErr::wrap_err(io_err(), "render").unwrap_err();
    group.bench_function("color_eyre", |b| {
        b.iter(|| black_box(format!("{color_eyre_report:?}")));
    });

    group.finish();
}

criterion_group!(benches, bench_traced, bench_traced_colored);
criterion_main!(benches);
