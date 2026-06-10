Provides a value or reference derived from this field through the
`std::error::Request` provider API (active with the `unstable` feature), letting
callers retrieve typed data with `Error::request_ref`/`request_value`. The
expression names the field; `ref` provides a borrow of it.

Forms: `provide(Type => expr)`, `provide(ref, Type => expr)`.

### Example
```
#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("lookup failed: {key}")]
    Lookup {
        #[oopsie(provide(ref, str => key.as_str()))]
        key: String,
    },
}

let err = Lookup { key: "missing".to_owned() }.build();
assert_eq!(err.to_string(), "lookup failed: missing");
```
