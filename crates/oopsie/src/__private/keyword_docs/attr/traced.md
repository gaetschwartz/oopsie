Injects trace-capture fields into every variant (or the struct): a backtrace and
a span trace, both enabled by default. The nested options tune each part —
mentioning one never disables the others — and `traced(false)` opts the type out
entirely. With `traced` active, an auto-generated error code is also attached to
each variant unless [`code`](traced::code) is disabled.

Forms: `traced`, `traced(false)`,
`traced(backtrace(...), spantrace(...), timestamp(...), packed = ..., boxed = ...)`.

### Example
```
#[oopsie::oopsie(traced)]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
