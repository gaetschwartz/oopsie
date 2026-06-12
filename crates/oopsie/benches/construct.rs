#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
//! Error construction cost: plain vs `traced`, and how env-gated capture changes it.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use oopsie::backtrace::set_override;
use oopsie::{RustBacktrace, oopsie};

#[oopsie]
#[oopsie("plain error {code}")]
struct PlainError {
    code: u32,
}

#[oopsie(traced)]
#[oopsie("traced error {code}")]
struct TracedError {
    code: u32,
}

fn bench_construct(c: &mut Criterion) {
    let mut group = c.benchmark_group("construct");

    group.bench_function("plain", |b| {
        b.iter(|| black_box(plain_oopsies::Plain { code: 1u32 }.build()));
    });

    set_override(RustBacktrace::Disabled);
    group.bench_function("traced_backtrace_disabled", |b| {
        b.iter(|| black_box(traced_oopsies::Traced { code: 1u32 }.build()));
    });

    set_override(RustBacktrace::Enabled);
    group.bench_function("traced_backtrace_enabled", |b| {
        b.iter(|| black_box(traced_oopsies::Traced { code: 1u32 }.build()));
    });

    group.finish();
}

criterion_group!(benches, bench_construct);
criterion_main!(benches);
