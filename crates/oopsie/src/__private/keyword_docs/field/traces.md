Marks this field as the packed `(Backtrace, SpanTrace)` pair, surfacing both
through `Diagnostic::oopsie_backtrace` and `oopsie_spantrace` from one field. A
marked field may be any type with that shape, such as an alias; unmarked fields
are detected by a `(Backtrace, SpanTrace)` tuple type (optionally boxed). It is
auto-captured and excluded from the selector. A packed `traces` field cannot
coexist with separate `backtrace`/`spantrace` fields.

Forms: `traces`, `traces = true`, `traces = false`.

### Example
```
use oopsie::{Backtrace, SpanTrace};

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("boom")]
    Boom {
        #[oopsie(traces)]
        traces: (Backtrace, SpanTrace),
    },
}

let err = Boom.build();
assert_eq!(err.to_string(), "boom");
```
