Sets the variant's (or struct's) `Display` message using `format!` semantics,
with named (`{field}`) or positional (`{}`) interpolation over the item's fields.
Without any display, the variant or struct name is used verbatim.

Forms: `display = "..."`, `display("...")`, `display("... {}", expr)`. Every
argument after the string goes to `format!`.

The short form `#[oopsie("... {}", expr, key = value)]` gives `format!` only
the arguments its string consumes: as many leading non-`name = value`
arguments as it has positional slots, and each `name = value` whose `name` it
references. The remaining arguments are `#[oopsie(...)]` keywords, as if
written in their own attribute. A referenced name wins over a keyword of the
same name.

### Example
```
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
```
