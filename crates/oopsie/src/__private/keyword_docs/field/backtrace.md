Marks this field as the captured backtrace, surfaced via
`Diagnostic::oopsie_backtrace`. The field may be of any type implementing
`Capturable` and `Borrow<Backtrace>`, such as an alias or a wrapper of your own;
it is auto-captured (and excluded from the selector). Marking a field is only
needed when its type isn't recognized automatically: unmarked fields are detected
by a type whose last path segment is `Backtrace` (optionally boxed).

Forms: `backtrace`, `backtrace = true`, `backtrace = false`.

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
