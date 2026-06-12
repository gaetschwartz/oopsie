Wraps the generated context selectors in a module, keeping them out of the
surrounding namespace. Enabled by default for both enums and structs (auto-named
by stripping a trailing `Error`, converting to `snake_case`, and appending
`_oopsies`).

Forms: `module`, `module(true)`, `module(false)`, `module(name)`, `module = "name"`.

An error type declared inside a function body needs `module(false)`: the
generated module cannot reference items local to a function, so wrapping the
selectors in one leaves them unable to name the error type.

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
