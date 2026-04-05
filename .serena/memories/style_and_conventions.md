# Code Style & Conventions

## Formatting
- **rustfmt** with `style_edition = "2024"` (see `rustfmt.toml`)
- Rust edition 2024

## Naming
- Standard Rust conventions: `snake_case` for functions/variables, `PascalCase` for types/traits
- Module files named after their domain concept (e.g., `extractor.rs`, `noise.rs`, `crawler.rs`)

## Code Style
- Public API functions have `///` doc comments with parameter descriptions
- Internal/private modules use `pub(crate)` visibility (e.g., `noise`, `data_island`)
- Error types use `thiserror` derive macros
- All output types derive `Serialize`/`Deserialize` for JSON output
- `#[serde(skip_serializing_if = "...")]` used to omit empty/None fields from JSON
- Feature flags used sparingly (e.g., `quickjs` feature in webclaw-core)
- `#[allow(dead_code)]` used selectively on internal modules
- Tests are in `#[cfg(test)] mod tests` blocks within the same file
- No excessive doc comments on internal code — only on public API boundaries

## Architecture Patterns
- Core crate is a pure function: `&str` HTML in → `ExtractionResult` out
- Fallback/retry strategies are layered (scored extraction → relaxed options → body selector → data islands → JS eval)
- Error handling via `Result<T, CustomError>` with `thiserror`
- Workspace dependencies declared in root `Cargo.toml` and referenced with `{ workspace = true }`

## Type Hints
- Rust's strong type system; no additional annotation conventions beyond standard Rust
- `Option<T>` for optional fields, `Vec<T>` for collections
