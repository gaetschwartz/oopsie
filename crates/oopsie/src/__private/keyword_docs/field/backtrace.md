Marks this field as the captured backtrace, surfaced via
`Diagnostic::oopsie_backtrace`. The field's type must have `Backtrace` as its
last path segment; it is auto-captured (and excluded from the selector). Marking
a field is only needed when its type isn't recognized automatically.

Forms: `backtrace`.

### Example
```
use oopsie::Backtrace;

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("boom")]
    Boom {
        #[oopsie(backtrace)]
        bt: Backtrace,
    },
}

let err = Boom.build();
assert_eq!(err.to_string(), "boom");
```
