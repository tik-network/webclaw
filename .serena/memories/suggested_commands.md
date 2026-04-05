# Suggested Commands

## Build
```bash
cargo build                    # Debug build
cargo build --release          # Release build (both binaries)
```

## Test
```bash
cargo test --workspace         # All tests
cargo test -p webclaw-core     # Core crate only
cargo test -p webclaw-fetch    # Fetch crate only
cargo test -p webclaw-llm      # LLM crate only
cargo test -p webclaw-mcp      # MCP server only
cargo test -p webclaw-cli      # CLI only
```

## Format & Lint
```bash
cargo fmt --all                # Format all code (uses rustfmt.toml: style_edition = "2024")
cargo fmt --all -- --check     # Check formatting without modifying
cargo clippy --workspace       # Lint all crates
```

## Run
```bash
cargo run -- https://example.com                    # Run CLI
cargo run -- https://example.com --format llm       # LLM-optimized output
cargo run -- https://example.com --crawl --depth 2  # Crawl
cargo run -- https://example.com --map              # Sitemap discovery
cargo run -p webclaw-mcp                            # Run MCP server
```

## Git
```bash
git status                     # Check working tree
git log --oneline -10          # Recent commits
git diff                       # Unstaged changes
```

## System (macOS / Darwin)
```bash
ls -la                         # List files
find . -name "*.rs"            # Find Rust files
grep -r "pattern" crates/      # Search in crates
```
