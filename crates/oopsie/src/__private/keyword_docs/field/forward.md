On a source field, opt in to forwarding diagnostic traces from that source through
this error type's `Diagnostic` impl. `backtrace` and `spantrace` are forwarded by
default; `location` is opt-in. A forwarded trace is read from the source rather than
captured here, so the wrapping type stores no trace of its own for it — keeping
nested errors small.

Forms: `forward`, `forward(true)`, `forward(false)`,
`forward(backtrace = …, spantrace = …, location = …)`.

### Example
```
use oopsie::{Contextual as _, Diagnostic as _};

#[oopsie::oopsie(traced)]
#[oopsie(module(false))]
pub enum LeafError {
    #[oopsie("leaf error")]
    Leaf,
}

#[oopsie::oopsie(traced)]
#[oopsie(module(false))]
pub enum WrapError {
    #[oopsie("wrap error")]
    Wrap {
        #[oopsie(forward)]
        source: LeafError,
    },
}

fn main() {
    let leaf = Leaf.build();
    let _: WrapError = Wrap.build_error(leaf);
}
```
