# Design: Replace snafu with native oopsie infrastructure

**Date:** 2026-03-08
**Status:** Approved

## Motivation

oopsie currently depends on `snafu` for error derive macros, context selectors, and extension traits. The `snafu` crate's `unstable-provider-api` feature requires nightly Rust, but removing it breaks compilation because snafu unconditionally enables nightly features in its workspace dependency. Rather than fighting snafu's feature gating, we replace it entirely with our own infrastructure — giving us full control over the stable/unstable split and a cleaner, unified API.

## Goals

- Remove the `snafu` dependency entirely
- Replicate only the snafu functionality that oopsie actually uses
- Unify the attribute syntax under `#[oopsie(...)]` — no more `#[snafu(...)]`
- `#[oopsie]` attribute macro implicitly adds `#[derive(Oopsie)]`
- `#[derive(Oopsie)]` can also be used standalone
- Maintain the `unstable` feature gate for Provider API support
- Both stable and nightly Rust should compile (with degraded functionality on stable)

## Non-goals

- `Whatever` type, `FromString` trait
- `ensure!`, `whatever!`, `ensure_whatever!` macros
- `#[snafu(transparent)]` (we have `#[oopsie(transparent)]` with different semantics)
- `#[snafu::report]` attribute macro
- `ErrorCompat` trait
- `no_std` / `core::error` support
- `AsErrorSource` trait
- `ChainCompat` error chain iterator

## Crate Architecture

No changes to the crate structure:

```
oopsie (facade)
├── oopsie-core       # Traits + core types
├── oopsie-macros     # #[oopsie] attribute + #[derive(Oopsie)]
└── oopsie-daisy      # FancyReport, ErasedError
```

## Traits & Types (oopsie-core)

### New traits (replacing snafu)

#### `IntoError<E>`

Converts a context selector + source into an error. Used by `ResultExt::context()`.

```rust
pub trait IntoError<E: std::error::Error> {
    type Source;

    #[track_caller]
    fn into_error(self, source: Self::Source) -> E;
}
```

#### `GenerateImplicitData`

Auto-fills fields marked with `#[oopsie(auto)]` (e.g., backtrace, spantrace). Already exists in oopsie-core against snafu's trait — we define our own identical version.

```rust
pub trait GenerateImplicitData {
    #[track_caller]
    fn generate() -> Self;

    #[track_caller]
    fn generate_with_source(source: &dyn std::error::Error) -> Self
    where
        Self: Sized,
    {
        let _ = source;
        Self::generate()
    }
}
```

#### `ResultExt<T, E>`

Extension trait on `Result` for ergonomic error context.

```rust
pub trait ResultExt<T, E> {
    fn context<C, E2>(self, context: C) -> Result<T, E2>
    where
        C: IntoError<E2, Source = E>,
        E2: std::error::Error;

    fn with_context<F, C, E2>(self, context: F) -> Result<T, E2>
    where
        F: FnOnce(&mut E) -> C,
        C: IntoError<E2, Source = E>,
        E2: std::error::Error;
}
```

#### `OptionExt<T>`

Extension trait on `Option` for converting `None` to errors.

```rust
pub trait OptionExt<T> {
    fn context<C, E>(self, context: C) -> Result<T, E>
    where
        C: IntoError<E, Source = NoneError>,
        E: std::error::Error;

    fn with_context<F, C, E>(self, context: F) -> Result<T, E>
    where
        F: FnOnce() -> C,
        C: IntoError<E, Source = NoneError>,
        E: std::error::Error;
}
```

#### `NoneError`

Unit type used as `IntoError::Source` for option context selectors.

```rust
#[derive(Debug, Copy, Clone)]
pub struct NoneError;
```

### Existing types (unchanged)

- `Backtrace` — backtrace wrapper with `GenerateImplicitData` impl (updated to use our trait)
- `Spantrace` — span trace wrapper with `GenerateImplicitData` impl (updated)
- `ErrorCode` — error code newtype
- `HelpText` — help text newtype
- `ColorConfig` — color output configuration
- `TracingLevel` — tracing level enum
- `MayBoxResult` — behind `unstable` feature

### Removed re-exports

- `pub use snafu::{OptionExt, ResultExt}` — replaced by our own traits
- `pub mod private { pub use snafu; }` — removed entirely

## Attribute Syntax

### `#[oopsie]` on the type (attribute macro)

When used as an attribute macro on a type, `#[oopsie]` does:
1. Implicitly adds `#[derive(Oopsie)]`
2. Injects backtrace and spantrace fields (as today)
3. Adds `#[oopsie(auto)]` to injected fields
4. Adds `#[oopsie(provide(...))]` for backtrace, spantrace, error code, help text

This is the "batteries-included" entry point.

### `#[derive(Oopsie)]` standalone

Can be used without `#[oopsie]` for cases where you want context selectors and Error/Display generation without automatic field injection. Everything below applies to both `#[oopsie]` and `#[derive(Oopsie)]`.

### Enum-level attributes

```rust
#[oopsie]                              // default: module on, no suffix, vis = pub(crate)
#[oopsie(module)]                      // explicit module (default name: snake_case of enum)
#[oopsie(module(custom_name))]         // custom module name
#[oopsie(module(false))]               // disable module wrapping
#[oopsie(vis = pub)]                   // default visibility for all selectors
#[oopsie(suffix)]                      // enable "Oopsie" suffix on selectors
#[oopsie(suffix = "Error")]            // custom suffix
```

Defaults when using `#[oopsie]`:
- `module`: **on** (module name = snake_case of enum name)
- `suffix`: **off** (selector name = variant name, with trailing "Error" stripped)
- `vis`: `pub(crate)`

### Variant-level attributes

**Short form** — when display is the only attribute:
```rust
#[oopsie("Connection to {host} failed")]
```

**Short form with format args:**
```rust
#[oopsie("Got {} errors from {host}", count)]
```

**Long form** — when combining display with other attributes:
```rust
#[oopsie(display("Parse error: {}", source), transparent)]
#[oopsie(display("Failed"), help = "Try again", code = "app::retry")]
```

Available variant/struct-level attributes:
- `display("format", args...)` — display message (long form)
- `"format string"` / `"format", args...` — display message (short form, first positional)
- `transparent` — generate `From` impl instead of context selector
- `help = "..."` — help text (injected via Provider API)
- `code = "..."` — error code (injected via Provider API)
- `vis = <visibility>` — override selector visibility for this variant

### Field-level attributes

```rust
source: io::Error,                           // auto-detected by name "source"
#[oopsie(from)]
cause: io::Error,                            // explicit source marker (non-"source" name)
#[oopsie(from(std::io::Error, Box::new))]
inner: Box<std::io::Error>,                  // source with type transformation
#[oopsie(auto)]
backtrace: Box<Backtrace>,                   // auto-fill via GenerateImplicitData
#[oopsie(provide(ref, Type => expr))]
field: T,                                    // Provider API (behind unstable feature)
```

Rules:
- A field named `source` is **automatically** detected as the error source
- `#[oopsie(from)]` is only needed for non-`source`-named fields
- `#[oopsie(from(Type, transform))]` enables source transformation (on any field name)
- At most one source field per variant/struct
- `#[oopsie(auto)]` fields are excluded from context selectors

## Code Generation: `#[derive(Oopsie)]`

### Context selectors

For each non-transparent variant, generate a context selector struct:

```rust
// Input:
#[oopsie("Connection to {host} failed")]
Connection {
    host: String,
    source: io::Error,
    #[oopsie(auto)]
    backtrace: Box<Backtrace>,
}

// Generated (inside module if module is enabled):
#[derive(Debug, Copy, Clone)]
pub(crate) struct Connection<__T0> {
    pub host: __T0,
}
```

- Selector includes only **user fields** (not source, not auto fields)
- Generic type parameters with `Into<FieldType>` bounds
- Source-only variants (no user fields) get a unit struct
- Leaf variants (no source) get `build()` and `fail()` methods
- Source variants get `IntoError` impl

### `IntoError` implementation

```rust
impl<__T0: Into<String>> IntoError<AppError> for Connection<__T0> {
    type Source = io::Error;

    #[track_caller]
    fn into_error(self, source: Self::Source) -> AppError {
        let backtrace = GenerateImplicitData::generate_with_source(&source);
        AppError::Connection {
            host: self.host.into(),
            source,
            backtrace,
        }
    }
}
```

For source transformation (`#[oopsie(from(Type, transform))]`):
```rust
type Source = Type;

fn into_error(self, source: Self::Source) -> AppError {
    let source = (transform)(source);  // apply transformation
    // ... rest same
}
```

### Leaf variant methods

For variants without a source field:

```rust
impl<__T0: Into<String>> NotFound<__T0> {
    #[must_use]
    #[track_caller]
    pub fn build(self) -> AppError {
        let backtrace = GenerateImplicitData::generate();
        AppError::NotFound {
            path: self.path.into(),
            backtrace,
        }
    }

    #[track_caller]
    pub fn fail<__T>(self) -> Result<__T, AppError> {
        Err(self.build())
    }
}
```

### `From` impl for transparent variants

```rust
// #[oopsie(transparent)]
impl From<serde_json::Error> for AppError {
    #[track_caller]
    fn from(source: serde_json::Error) -> Self {
        AppError::Parse { source }
    }
}
```

### `Display` implementation

```rust
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection { host, .. } => write!(f, "Connection to {host} failed"),
            Self::Parse { source, .. } => write!(f, "Parse error: {}", source),
            Self::NotFound { path, .. } => write!(f, "File not found: {path}"),
        }
    }
}
```

### `Error` implementation

```rust
impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Connection { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::NotFound { .. } => None,
        }
    }

    #[cfg(feature = "unstable")]
    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            Self::Connection { source, backtrace, .. } => {
                request.provide_ref::<Backtrace>(backtrace.as_ref());
                source.provide(request);
            }
            Self::Parse { source, .. } => {
                source.provide(request);
            }
            Self::NotFound { backtrace, .. } => {
                request.provide_ref::<Backtrace>(backtrace.as_ref());
            }
        }
    }
}
```

Provider API behavior:
- Source fields: forward `source.provide(request)` to chain providers
- Auto/provide fields: call `request.provide_ref()` or `request.provide_value()`
- Explicit `#[oopsie(provide(...))]` attributes generate additional provide calls
- All behind `#[cfg(feature = "unstable")]`

## `#[oopsie]` attribute macro behavior

When `#[oopsie]` is used as an attribute macro (not just derive), it additionally:

1. **Adds `#[derive(Oopsie)]`** to the type if not present
2. **Injects fields** into each variant/struct:
   - `__oopsie_backtrace: Box<Backtrace>` with `#[oopsie(auto)]`
   - `__oopsie_spantrace: Box<Spantrace>` with `#[oopsie(auto)]`
3. **Adds provide attributes** for injected fields:
   - `#[oopsie(provide(ref, Backtrace => __oopsie_backtrace.as_ref()))]`
   - `#[oopsie(provide(ref, Spantrace => __oopsie_spantrace.as_ref()))]`
4. **Adds provide for code/help** when `code = "..."` or `help = "..."` is present:
   - `#[oopsie(provide(ErrorCode => ErrorCode::from(...)))]`
   - `#[oopsie(provide(HelpText => HelpText("...")))]`
5. **Sets enum defaults**: module on, suffix off, vis = pub(crate)

### Attribute macro parameters

Same as today:
```rust
#[oopsie]                                      // all defaults
#[oopsie(path = "my_crate::oopsie")]           // custom path to oopsie crate
#[oopsie(backtrace(enabled = false))]          // disable backtrace injection
#[oopsie(spantrace(type = "MySpantrace"))]     // custom spantrace type
```

## `LowerExp` implementation (daisy feature)

When the `daisy` feature is enabled, `#[oopsie]` generates a `LowerExp` impl that formats the error using `FancyReport`:

```rust
#[cfg(feature = "daisy")]
impl fmt::LowerExp for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        FancyReport::from(self).fmt(f)
    }
}
```

This is unchanged from today.

## Feature gates

### `unstable` feature

Controls:
- `#![feature(error_generic_member_access)]` in oopsie-core
- `provide()` method generation in `#[derive(Oopsie)]`
- `Spantrace::extract()` using Provider API
- `GenerateImplicitData::generate_with_source()` using Provider API to check existing data
- `MayBoxResult` / try trait support

### `daisy` feature

Controls:
- `LowerExp` impl generation in `#[oopsie]`
- oopsie-daisy dependency in facade

## Migration from snafu

### Attribute mapping

| snafu | oopsie |
|-------|--------|
| `#[derive(Snafu)]` | `#[derive(Oopsie)]` or `#[oopsie]` |
| `#[snafu(display("..."))]` | `#[oopsie("...")]` or `#[oopsie(display("..."))]` |
| `#[snafu(source)]` | auto-detected for `source` field, or `#[oopsie(from)]` |
| `#[snafu(source(from(T, f)))]` | `#[oopsie(from(T, f))]` |
| `#[snafu(implicit)]` | `#[oopsie(auto)]` |
| `#[snafu(context(false))]` | `#[oopsie(transparent)]` |
| `#[snafu(visibility(pub))]` | `#[oopsie(vis = pub)]` |
| `#[snafu(module(name))]` | `#[oopsie(module(name))]` |
| `#[snafu(context(suffix(X)))]` | `#[oopsie(suffix = "X")]` |
| `#[snafu(provide(...))]` | `#[oopsie(provide(...))]` |
| `snafu::ResultExt` | `oopsie::ResultExt` |
| `snafu::OptionExt` | `oopsie::OptionExt` |
| `snafu::IntoError` | `oopsie::IntoError` |
| `snafu::GenerateImplicitData` | `oopsie::GenerateImplicitData` |
| `XxxSnafu` | `Xxx` (no suffix by default) |

### Dependency changes

Remove from all Cargo.toml files:
- `snafu = { ... }` (workspace and per-crate)

No new external dependencies needed — the derive macro infrastructure already exists in oopsie-macros (syn, quote, darling, proc-macro2).

## Testing strategy

- **oopsie-macros**: Update existing macro expansion tests to use new syntax, verify generated code
- **oopsie-core**: Update trait implementations, test ResultExt/OptionExt behavior
- **oopsie-daisy**: Update test error types from `#[derive(Snafu)] #[snafu(...)]` to `#[oopsie("...")]`
- Verify: `cargo +stable check --no-default-features` compiles
- Verify: `cargo check` (nightly with defaults) compiles
- Verify: `cargo nextest run` passes
- Verify: `cargo clippy` clean

## Full example

```rust
use oopsie::prelude::*;

#[oopsie]
#[derive(Debug)]
pub enum AppError {
    #[oopsie(display("Connection to {host}:{port} failed"), help = "Check network settings")]
    Connection {
        host: String,
        port: u16,
        source: io::Error,
    },

    #[oopsie("Failed to parse config")]
    ConfigParse {
        source: serde_json::Error,
    },

    #[oopsie(display("Unexpected I/O: {}", source), transparent)]
    Io {
        source: io::Error,
    },

    #[oopsie(display("File not found: {path}"), code = "app::not_found")]
    NotFound {
        path: String,
    },

    #[oopsie("Conversion failed")]
    Convert {
        #[oopsie(from(RawError, |e| e.into_boxed()))]
        source: Box<dyn std::error::Error>,
    },
}

fn connect(host: &str, port: u16) -> Result<(), AppError> {
    std::net::TcpStream::connect((host, port))
        .context(app_error::Connection { host, port })?;
    Ok(())
}

fn load_config(path: &str) -> Result<Config, AppError> {
    let data = std::fs::read_to_string(path)
        .map_err(|_| app_error::NotFound { path }.build())?;
    let config: Config = serde_json::from_str(&data)
        .context(app_error::ConfigParse)?;
    Ok(config)
}
```
