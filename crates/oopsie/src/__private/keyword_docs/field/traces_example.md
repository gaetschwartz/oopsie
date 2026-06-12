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
