//! Wrapping an `io::Error` with a context message across error libraries.
//!
//! Backtraces are off for oopsie (override); anyhow and eyre follow
//! `RUST_BACKTRACE`, which is unset in a normal `cargo bench` run.

use std::hint::black_box;
use std::io;

use criterion::{Criterion, criterion_group, criterion_main};
use oopsie::{RustBacktrace, oopsie, set_rust_backtrace_override};

#[oopsie]
#[oopsie("wrap failed: {ctx}")]
struct OopsieErr {
    ctx: &'static str,
    source: io::Error,
}

#[derive(Debug, snafu::Snafu)]
#[snafu(display("wrap failed: {}", ctx))]
struct SnafuErr {
    ctx: &'static str,
    source: io::Error,
}

#[inline(always)]
fn io_err() -> Result<(), io::Error> {
    Err(io::Error::other("boom"))
}

fn bench_wrap(c: &mut Criterion) {
    set_rust_backtrace_override(RustBacktrace::Disabled);
    let mut group = c.benchmark_group("wrap_io_error");

    group.bench_function("oopsie", |b| {
        b.iter(|| {
            black_box(oopsie::ResultExt::context(
                io_err(),
                OopsieErrOopsie { ctx: "op" },
            ))
        });
    });

    group.bench_function("snafu", |b| {
        b.iter(|| {
            black_box(snafu::ResultExt::context(
                io_err(),
                SnafuErrSnafu { ctx: "op" },
            ))
        });
    });

    group.bench_function("anyhow", |b| {
        b.iter(|| black_box(anyhow::Context::context(io_err(), "op")));
    });

    group.bench_function("eyre", |b| {
        b.iter(|| black_box(eyre::WrapErr::wrap_err(io_err(), "op")));
    });

    group.finish();
}

criterion_group!(benches, bench_wrap);
criterion_main!(benches);
