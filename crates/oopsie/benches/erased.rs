#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

//! Cost of erasing an error into the serializable representation and emitting JSON.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use oopsie::backtrace::set_override;
use oopsie::erased::ErasedError;
use oopsie::{RustBacktrace, oopsie};

#[oopsie(traced)]
#[oopsie("erased target {id}")]
struct ErasedTarget {
    id: u32,
}

fn make() -> ErasedTarget {
    ErasedTargetOopsie { id: 1u32 }.build()
}

fn bench_erased(c: &mut Criterion) {
    set_override(RustBacktrace::Enabled);
    let mut group = c.benchmark_group("erased");

    group.bench_function("from_error", |b| {
        b.iter_batched(
            make,
            |e| black_box(ErasedError::from_error(e)),
            BatchSize::SmallInput,
        );
    });

    let erased = ErasedError::from_error(make());
    group.bench_function("to_json", |b| {
        b.iter(|| black_box(serde_json::to_string(&erased).unwrap()));
    });

    group.finish();
}

criterion_group!(benches, bench_erased);
criterion_main!(benches);
