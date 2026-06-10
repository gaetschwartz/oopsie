Marks this field as the chained source error, supplied to the selector at
construction (via `.build_error(source)` or `.context(...)`) rather than being a
selector field. A field named `source` is detected automatically; use `from` to
mark a differently-named field. `from(false)` opts a `source`-named field out.
`from(Type, transform)` accepts a `Type` and stores `transform(value)`; a
`Box<T>` source is auto-unboxed so the selector takes `T` directly.

Forms: `from`, `from(true)`, `from(false)`, `from(Type, transform)`.

### Example
```
use oopsie::Contextual as _;

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("read failed")]
    Read {
        #[oopsie(from)]
        cause: std::io::Error,
    },
}

let err = Read.build_error(std::io::Error::other("disk"));
assert_eq!(err.to_string(), "read failed");
```
