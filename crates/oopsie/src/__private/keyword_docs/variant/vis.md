Overrides the visibility of this one variant's selector, taking precedence over
any container-level `vis(...)`. Useful for exposing a single selector from an
otherwise crate-private error.

Forms: `vis(pub)`, `vis(pub(crate))`, `vis = "pub(crate)"`.

### Example
```
#[oopsie::oopsie]
#[oopsie(module(errs))]
enum AppError {
    #[oopsie("not found: {key}")]
    #[oopsie(vis(pub(crate)))]
    Missing { key: String },
}

fn main() {
    let err = errs::Missing { key: "k".to_owned() }.build();
    assert_eq!(err.to_string(), "not found: k");
}
```
