On a source field, opt in to forwarding diagnostic traces from that source through
this error type's `Diagnostic` impl. `backtrace` and `spantrace` are forwarded by
default; `location` is opt-in. Any forwarded trace suppresses the corresponding
injected field on the wrapping type.

Forms: `forward`, `forward(true)`, `forward(false)`,
`forward(backtrace = …, spantrace = …, location = …)`.

### Example
```
use oopsie::{oopsie, Diagnostic as _};

#[oopsie::oopsie(traced)]
pub enum LeafError {
    #[oopsie("leaf error")]
    Leaf,
}

#[oopsie::oopsie(traced)]
pub enum WrapError {
    #[oopsie("wrap error")]
    Wrap {
        #[oopsie(forward)]
        source: LeafError,
    },
}
```
