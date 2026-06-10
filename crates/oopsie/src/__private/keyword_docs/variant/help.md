Attaches static help text to the variant or struct, surfaced via
`Diagnostic::oopsie_help_text` and shown by `Report`. The text follows `format!`
semantics, so it can interpolate the item's fields. For help derived from a
field's runtime `Display`, use the field-level `#[oopsie(help)]` instead.

Forms: `help = "..."`, `help("...")`, `help("... {}", expr)`.

### Example
```
use oopsie::Diagnostic as _;

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie(display("config missing"), help = "run `app init` to create one")]
    NoConfig,
}

let err = NoConfig.build();
assert_eq!(err.oopsie_help_text().as_deref(), Some("run `app init` to create one"));
```
