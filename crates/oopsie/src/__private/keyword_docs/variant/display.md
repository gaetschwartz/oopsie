Sets the variant's (or struct's) `Display` message using `format!` semantics,
with named (`{field}`) or positional (`{}`) interpolation over the item's fields.
This is the long form of the short `#[oopsie("msg")]`; use it when combining
display with other keywords in the same attribute. Without any display, the
variant or struct name is used verbatim.

Forms: `display("msg {field}")`, `display("msg {}", expr)`.

### Example
```
#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie(display("{count} items missing from {place}"))]
    Missing { count: u32, place: String },
}

let err = Missing { count: 3u32, place: "cache".to_owned() }.build();
assert_eq!(err.to_string(), "3 items missing from cache");
```
