Marks this field as the captured span trace, surfaced via
`Diagnostic::oopsie_spantrace`. The field's type must have `SpanTrace` as its
last path segment; it is auto-captured (and excluded from the selector). Marking
a field is only needed when its type isn't recognized automatically.

Forms: `spantrace`, `spantrace = true`, `spantrace = false`.

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
