//! Backtrace capture cost across settings, and the deferred symbol-resolution cost.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use oopsie::{Backtrace, Capturable as _, RustBacktrace, set_rust_backtrace_override};

fn bench_capture(c: &mut Criterion) {
    let mut group = c.benchmark_group("backtrace_capture");

    set_rust_backtrace_override(RustBacktrace::Disabled);
    group.bench_function("disabled", |b| b.iter(|| black_box(Backtrace::capture())));

    set_rust_backtrace_override(RustBacktrace::Enabled);
    group.bench_function("enabled_unresolved", |b| {
        b.iter(|| black_box(Backtrace::capture()));
    });

    set_rust_backtrace_override(RustBacktrace::Full);
    group.bench_function("full_unresolved", |b| {
        b.iter(|| black_box(Backtrace::capture()));
    });

    // Symbol resolution is paid only at render time, never at capture.
    set_rust_backtrace_override(RustBacktrace::Enabled);
    group.bench_function("resolve", |b| {
        b.iter_batched(
            Backtrace::capture,
            |bt| {
                bt.resolve();
                black_box(bt)
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_capture);
criterion_main!(benches);
