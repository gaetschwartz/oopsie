//! Cost of rendering a `Report` — the path that pays symbol resolution.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use oopsie::{Report, RustBacktrace, oopsie, set_rust_backtrace_override};

#[oopsie(traced)]
#[oopsie("render target {id}")]
struct RenderError {
    id: u32,
}

fn make() -> RenderError {
    RenderOopsie { id: 7u32 }.build()
}

fn bench_render(c: &mut Criterion) {
    set_rust_backtrace_override(RustBacktrace::Enabled);
    let mut group = c.benchmark_group("render");

    group.bench_function("display_no_colors", |b| {
        b.iter_batched(
            make,
            |e| black_box(Report::from_std(e).no_colors().to_string()),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("display_colored", |b| {
        b.iter_batched(
            make,
            |e| black_box(Report::from_std(e).force_colors().to_string()),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("debug", |b| {
        b.iter_batched(
            make,
            |e| black_box(format!("{:?}", Report::from_std(e))),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_render);
criterion_main!(benches);
