Marks this field as the captured caller location, surfaced via
`Diagnostic::oopsie_location`. The field's type must be
`&'static Location<'static>` (the standard `std::panic::Location`); it is
auto-captured (and excluded from the selector). Marking a field is only needed
when its type isn't recognized automatically.

Forms: `location`, `location = true`, `location = false`.

### Example
```
use std::panic::Location;

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("boom")]
    Boom {
        #[oopsie(location)]
        at: &'static Location<'static>,
    },
}

let err = Boom.build();
assert_eq!(err.to_string(), "boom");
```
