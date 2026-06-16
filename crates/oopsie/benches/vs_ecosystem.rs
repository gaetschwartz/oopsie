#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
//! Comparing oopsie against other error libraries: wrapping, plain rendering,
//! colored rendering, and rendering with backtraces (the `render_traced*` groups).
//!
//! `RUST_BACKTRACE` is forced to `1` at startup (see [`enable_env_backtraces`])
//! so the ecosystem crates — which read it once and cache the decision — capture
//! frames in the traced groups. The reach is process-wide: anyhow and eyre
//! consequently also carry a backtrace in the non-traced groups. oopsie and
//! snafu only carry one when their error type opts in, so for them the
//! non-traced groups stay backtrace-free.

use std::hint::black_box;
use std::io;
use std::sync::Once;

use criterion::{Criterion, criterion_group, criterion_main};
use oopsie::backtrace::set_override;
use oopsie::{Report, RustBacktrace, oopsie};

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

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("wrap failed: {ctx}")]
struct MietteErr {
    ctx: &'static str,
    #[source]
    source: io::Error,
}

// Backtrace-carrying error types for the `render_traced*` groups. oopsie uses
// the `traced` machinery without the spantrace; anyhow and eyre capture from
// the forced env. snafu's `backtrace` feature captures a backtrace, but its
// `Report` only renders one under the nightly provider API (not enabled here),
// so the snafu arm measures a backtrace-free render.
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

/// Force `RUST_BACKTRACE=1` so anyhow/eyre/snafu capture frames in the traced
/// groups. They read the variable once and std locks the decision on the first
/// capture, so this must run before any error is built.
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

fn bench_wrap(c: &mut Criterion) {
    enable_env_backtraces();
    set_override(RustBacktrace::Disabled);
    let mut group = c.benchmark_group("wrap_io_error");

    group.bench_function("oopsie", |b| {
        b.iter(|| {
            black_box(oopsie::ResultExt::context(
                io_err(),
                oopsie_err_oopsies::OopsieErr { ctx: "op" },
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

    group.bench_function("miette", |b| {
        b.iter(|| {
            let r: miette::Result<()> =
                miette::WrapErr::wrap_err(miette::IntoDiagnostic::into_diagnostic(io_err()), "op");
            black_box(r)
        });
    });

    group.finish();
}

/// Renders a pre-built error (with one source) to a string, so the benchmark
/// measures only formatting — not construction. Each library uses its own
/// idiomatic full-chain renderer: `Report` for oopsie, `snafu::Report` for
/// snafu, alternate `Debug` for anyhow and eyre.
fn bench_render(c: &mut Criterion) {
    enable_env_backtraces();
    set_override(RustBacktrace::Disabled);
    let mut group = c.benchmark_group("render_error");

    let oopsie_report = Report::new(
        oopsie::ResultExt::context(io_err(), oopsie_err_oopsies::OopsieErr { ctx: "render" })
            .unwrap_err(),
    )
    .no_colors();
    group.bench_function("oopsie", |b| {
        b.iter(|| black_box(format!("{oopsie_report}")));
    });

    let snafu_report = snafu::Report::from_error(
        snafu::ResultExt::context(io_err(), SnafuErrSnafu { ctx: "render" }).unwrap_err(),
    );
    group.bench_function("snafu", |b| {
        b.iter(|| black_box(format!("{snafu_report}")));
    });

    let anyhow_err = anyhow::Context::context(io_err(), "render").unwrap_err();
    group.bench_function("anyhow", |b| {
        b.iter(|| black_box(format!("{anyhow_err:?}")));
    });

    let eyre_report = eyre::WrapErr::wrap_err(io_err(), "render").unwrap_err();
    group.bench_function("eyre", |b| {
        b.iter(|| black_box(format!("{eyre_report:?}")));
    });

    let miette_handler =
        miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor());
    let miette_err = MietteErr {
        ctx: "render",
        source: io_err().unwrap_err(),
    };
    group.bench_function("miette", |b| {
        b.iter(|| {
            let mut out = String::new();
            miette_handler.render_report(&mut out, &miette_err).unwrap();
            black_box(out)
        });
    });

    group.finish();
}

/// Renders a pre-built error with colors forced on, comparing oopsie's printer
/// against the two ecosystem renderers that specialize in colored diagnostics.
fn bench_colored(c: &mut Criterion) {
    enable_env_backtraces();
    set_override(RustBacktrace::Disabled);
    // `color_eyre::install` sets a process-global eyre hook, so this group must
    // run last — otherwise the plain `eyre` benches above would pick up the
    // colored handler. eyre attaches the handler at report-creation time, so
    // the report below (built after install) is the only colored one.
    let _ = color_eyre::install();

    let mut group = c.benchmark_group("render_colored");

    let oopsie_report = Report::new(
        oopsie::ResultExt::context(io_err(), oopsie_err_oopsies::OopsieErr { ctx: "render" })
            .unwrap_err(),
    )
    .force_colors();
    group.bench_function("oopsie", |b| {
        b.iter(|| black_box(format!("{oopsie_report}")));
    });

    let color_eyre_report = eyre::WrapErr::wrap_err(io_err(), "render").unwrap_err();
    group.bench_function("color_eyre", |b| {
        b.iter(|| black_box(format!("{color_eyre_report:?}")));
    });

    let miette_handler =
        miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode());
    let miette_err = MietteErr {
        ctx: "render",
        source: io_err().unwrap_err(),
    };
    group.bench_function("miette", |b| {
        b.iter(|| {
            let mut out = String::new();
            miette_handler.render_report(&mut out, &miette_err).unwrap();
            black_box(out)
        });
    });

    group.finish();
}

/// Plain full-chain rendering of errors that carry a backtrace, across crates.
/// oopsie's backtrace comes from `#[oopsie(traced(spantrace(false)))]` + the override; the
/// others rely on the forced `RUST_BACKTRACE` env (see module docs).
fn bench_traced(c: &mut Criterion) {
    enable_env_backtraces();
    set_override(RustBacktrace::Enabled);
    let mut group = c.benchmark_group("render_traced");

    let oopsie_report = Report::new(
        oopsie::ResultExt::context(io_err(), traced_oopsies::Traced { ctx: "render" }).unwrap_err(),
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
        oopsie::ResultExt::context(io_err(), traced_oopsies::Traced { ctx: "render" }).unwrap_err(),
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

criterion_group!(
    benches,
    bench_wrap,
    bench_render,
    bench_traced,
    bench_colored,
    bench_traced_colored
);
criterion_main!(benches);
