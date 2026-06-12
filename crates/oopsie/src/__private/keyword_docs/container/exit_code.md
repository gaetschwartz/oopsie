Sets the default process exit code for the error type, surfaced via
`Diagnostic::oopsie_exit_code` and used by `Report`'s `Termination` impl when
the error is returned from `main`. Accepts an integer literal in `1..=255`
(`0` is rejected: it denotes success, contradictory on an error path).

On an enum it applies to every variant that does not set its own `exit_code`;
on a struct it is the struct's exit code.

Forms: `exit_code = N`.

### Example
```
use oopsie::Diagnostic as _;

#[oopsie::oopsie]
#[oopsie(module(false), exit_code = 70)]
pub struct AppError {
    detail: String,
}

let err = App { detail: String::from("boom") }.build();
assert_eq!(err.oopsie_exit_code().map(|n| n.get()), Some(70));
```
