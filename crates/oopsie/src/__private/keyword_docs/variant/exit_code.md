Sets the process exit code for this variant, surfaced via
`Diagnostic::oopsie_exit_code` and used by `Report`'s `Termination` impl when
the error is returned from `main`. Accepts an integer literal in `1..=255`
(`0` is rejected: it denotes success, contradictory on an error path). A
container-level `exit_code` is the default for every variant; a variant's own
value overrides it.

Forms: `exit_code = N`.

### Example
```
use oopsie::Diagnostic as _;

#[oopsie::oopsie]
#[oopsie(module(false), exit_code = 1)]
pub enum AppError {
    #[oopsie("bad usage")]
    Usage,
    #[oopsie("config error: {path}")]
    #[oopsie(exit_code = 78)]
    Config { path: String },
}

assert_eq!(Usage.build().oopsie_exit_code().map(|n| n.get()), Some(1));
assert_eq!(
    Config { path: String::from("x") }.build().oopsie_exit_code().map(|n| n.get()),
    Some(78)
);
```
