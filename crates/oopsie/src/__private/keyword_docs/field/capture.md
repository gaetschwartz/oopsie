Auto-fills this field via the `Capturable` trait when the error is constructed,
so it is excluded from the context selector instead of being caller-supplied.
Trace-typed fields (`Backtrace`, `SpanTrace`, packed traces) get this
automatically; `capture(false)` opts such a field back onto the selector.

Forms: `capture`, `capture(false)`.

### Example
```
#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("request {id} failed")]
    Failed {
        id: u32,
        #[oopsie(capture)]
        at: std::time::SystemTime,
    },
}

// `at` is captured automatically, so the selector only takes `id`.
let err = Failed { id: 7u32 }.build();
assert_eq!(err.to_string(), "request 7 failed");
```
