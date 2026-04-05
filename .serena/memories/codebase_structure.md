# Codebase Structure

```
webclaw/
├── Cargo.toml                 # Workspace root with [patch.crates-io] for TLS forks
├── Cargo.lock
├── CLAUDE.md                  # AI assistant instructions
├── rustfmt.toml               # style_edition = "2024"
├── .cargo/config.toml         # RUSTFLAGS: reqwest_unstable
├── crates/
│   ├── webclaw-core/src/
│   │   ├── lib.rs             # Public API: extract(), extract_with_options()
│   │   ├── extractor.rs       # Readability-style content scoring
│   │   ├── noise.rs           # Shared noise filter (tags, ARIA, class/ID patterns)
│   │   ├── data_island.rs     # JSON data island extraction (React/Next.js/CMS)
│   │   ├── markdown.rs        # HTML→Markdown with URL resolution
│   │   ├── llm/               # 9-step LLM optimization pipeline
│   │   ├── types.rs           # Core data structures
│   │   ├── metadata.rs        # OG, Twitter Card, meta tag extraction
│   │   ├── domain.rs          # Domain detection (Article, Social, etc.)
│   │   ├── diff.rs            # Content change tracking
│   │   ├── brand.rs           # Brand identity extraction
│   │   ├── structured_data.rs # JSON-LD Schema.org extraction
│   │   ├── youtube.rs         # YouTube-specific extraction
│   │   ├── js_eval.rs         # QuickJS runtime for JS data blobs
│   │   └── error.rs           # ExtractError type
│   ├── webclaw-fetch/src/
│   │   ├── lib.rs             # Module exports
│   │   ├── client.rs          # FetchClient with TLS impersonation
│   │   ├── browser.rs         # Browser profiles (Chrome, Firefox)
│   │   ├── crawler.rs         # BFS same-origin crawler
│   │   ├── sitemap.rs         # Sitemap discovery and parsing
│   │   ├── proxy.rs           # Proxy pool rotation
│   │   ├── document.rs        # DOCX, XLSX, CSV parsing
│   │   ├── reddit.rs          # Reddit-specific handling
│   │   └── linkedin.rs        # LinkedIn-specific handling
│   ├── webclaw-llm/           # LLM provider chain
│   ├── webclaw-pdf/           # PDF extraction
│   ├── webclaw-mcp/src/
│   │   ├── main.rs            # MCP server entry point
│   │   ├── server.rs          # Server setup
│   │   ├── tools.rs           # MCP tool definitions
│   │   └── cloud.rs           # Cloud integration
│   └── webclaw-cli/src/
│       ├── main.rs            # CLI entry point
│       └── cloud.rs           # Cloud integration
├── skill/                     # Claude Code skills
├── examples/                  # Usage examples
├── benchmarks/                # Performance benchmarks
├── deploy/                    # Deployment configs
├── .github/                   # CI/CD workflows
└── assets/                    # Static assets
```
