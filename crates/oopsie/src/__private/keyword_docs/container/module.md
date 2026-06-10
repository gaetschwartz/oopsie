Wraps the generated context selectors in a module, keeping them out of the
surrounding namespace. Enabled by default for enums (auto-named by stripping a
trailing `Error`, converting to `snake_case`, and appending `_oopsies`);
disabled by default for structs.

Forms: `module`, `module(name)`, `module(false)`.

### Example
```
#[oopsie::oopsie]
#[oopsie(module(my_errors))]
pub enum AppError {
    #[oopsie("not found: {key}")]
    Missing { key: String },
}

fn main() {
    let err = my_errors::Missing { key: "k".to_owned() }.build();
    assert_eq!(err.to_string(), "not found: k");
}
```
