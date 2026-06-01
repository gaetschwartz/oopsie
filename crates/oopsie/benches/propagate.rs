#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
//! Cost of `?` + `.context(..)` on the hot path versus a bare `Result`.

use std::hint::black_box;
use std::io;

use criterion::{Criterion, criterion_group, criterion_main};
use oopsie::prelude::*;
use oopsie::{RustBacktrace, oopsie, set_rust_backtrace_override};

#[oopsie]
#[oopsie("wrapped: {context}")]
struct WrapError {
    context: String,
    source: io::Error,
}

#[inline(never)]
fn io_result(fail: bool) -> io::Result<u64> {
    if black_box(fail) {
        Err(io::Error::other("boom"))
    } else {
        Ok(black_box(42))
    }
}

fn with_context(fail: bool) -> Result<u64, WrapError> {
    let value = io_result(fail).context(WrapOopsie {
        context: "operation",
    })?;
    Ok(value)
}

fn bench_propagate(c: &mut Criterion) {
    set_rust_backtrace_override(RustBacktrace::Disabled);
    let mut group = c.benchmark_group("propagate");

    group.bench_function("baseline_ok", |b| b.iter(|| black_box(io_result(false))));
    group.bench_function("context_ok", |b| b.iter(|| black_box(with_context(false))));
    group.bench_function("context_err", |b| b.iter(|| black_box(with_context(true))));

    group.finish();
}

criterion_group!(benches, bench_propagate);
criterion_main!(benches);
