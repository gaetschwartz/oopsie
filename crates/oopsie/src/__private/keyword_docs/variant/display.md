Sets the variant's (or struct's) `Display` message using `format!` semantics,
with named (`{field}`) or positional (`{}`) interpolation over the item's fields.
Without any display, the variant or struct name is used verbatim.

Forms: `display = "..."`, `display("...")`, `display("... {}", expr)`. Every
argument after the string goes to `format!`.

The short form `#[oopsie("... {}", expr, key = value)]` gives `format!` every
argument it would consume. Of the remaining arguments, `#[oopsie(...)]`
keywords apply to the item, as if written in their own attribute, except that
an enum-level keyword on a variant is an error; anything else is passed to
`format!` too, and the compiler reports it. A name the string references stays
a format argument even when it is also a keyword.

### Example
```
use oopsie::Diagnostic as _;

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie(display("{count} items missing from {place}"))]
    Missing { count: u32, place: String },
    #[oopsie("{} retries left", left, code = "E42")]
    Retry { left: u8 },
}

let err = Missing { count: 3u32, place: "cache".to_owned() }.build();
assert_eq!(err.to_string(), "3 items missing from cache");
let err = Retry { left: 2u8 }.build();
assert_eq!(err.to_string(), "2 retries left");
assert_eq!(err.oopsie_error_code().map(|c| c.as_str().to_owned()).as_deref(), Some("E42"));
```
