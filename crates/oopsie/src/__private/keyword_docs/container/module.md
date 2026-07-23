Wraps the generated context selectors in a module, keeping them out of the
surrounding namespace. Enabled by default for enums (auto-named by stripping a
trailing `Error`, converting to `snake_case`, and appending `_oopsies`);
disabled by default for structs.

Forms: `module`, `module(true)`, `module(false)`, `module(name)`, `module = "name"`.

An error type declared inside a function body needs `module(false)`: the
generated module cannot reference items local to a function, so wrapping the
selectors in one leaves them unable to name the error type. Forgetting this
surfaces as rustc error E0425 or E0433 ("cannot find type ... in this scope"),
typically with a nonsense suggestion to rename the type to one of its own
variants — that suggestion is a red herring; add `module(false)` instead.

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
