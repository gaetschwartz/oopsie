Inside `traced(...)`: stores the backtrace and span trace as one packed
`(Backtrace, SpanTrace)` field (default: on). `packed = false` keeps them as two
separate fields, which also allows boxing each independently. Packing requires
backtrace and spantrace to share one boxing mode.

Forms: `packed`, `packed = true`, `packed = false`, `packed(true)`,
`packed(false)`.

### Example
```
#[oopsie::oopsie(traced(packed = false))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
