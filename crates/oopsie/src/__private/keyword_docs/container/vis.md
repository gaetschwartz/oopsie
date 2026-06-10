Overrides the visibility of the generated selectors (and their module, if any).
Defaults to the error type's own visibility. A variant-level `vis(...)` overrides
this for that one variant's selector.

Forms: `vis(pub)`, `vis(pub(crate))`, ...

### Example
```
#[oopsie::oopsie]
#[oopsie(module(errs), vis(pub(crate)))]
pub enum AppError {
    #[oopsie("not found: {key}")]
    Missing { key: String },
}

fn main() {
    let err = errs::Missing { key: "k".to_owned() }.build();
    assert_eq!(err.to_string(), "not found: k");
}
```
