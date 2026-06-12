### Example
```
use oopsie::SpanTrace;

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("boom")]
    Boom {
        #[oopsie(spantrace)]
        st: SpanTrace,
    },
}

let err = Boom.build();
assert_eq!(err.to_string(), "boom");
```
