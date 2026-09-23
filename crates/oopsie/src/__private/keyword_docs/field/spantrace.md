Marks this field as the captured span trace, surfaced via
`Diagnostic::oopsie_spantrace`. The field may be of any type implementing
`Capturable` and `Borrow<SpanTrace>`, such as an alias or a wrapper of your own;
it is auto-captured (and excluded from the selector). Marking a field is only
needed when its type isn't recognized automatically: unmarked fields are detected
by a type whose last path segment is `SpanTrace` (optionally boxed).

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
