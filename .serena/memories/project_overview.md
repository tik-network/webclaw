# Webclaw - Project Overview

## Purpose
Webclaw is a CLI tool and MCP server for web content extraction into LLM-optimized formats. It fetches web pages, extracts main content using readability-style scoring, converts to markdown, and optionally processes through LLM pipelines.

## Tech Stack
- **Language:** Rust (Edition 2024)
- **Build system:** Cargo workspace
- **Version:** 0.3.2
- **License:** MIT
- **Repository:** https://github.com/0xMassi/webclaw

## Workspace Crates
- `webclaw-core` — Pure extraction engine. WASM-safe. Zero network deps. Takes `&str` HTML, returns structured output.
- `webclaw-fetch` — HTTP client via primp/webclaw-http with TLS fingerprint impersonation. Crawler, sitemap, batch ops, proxy rotation, document parsing.
- `webclaw-llm` — LLM provider chain (Ollama → OpenAI → Anthropic). Uses plain reqwest (not primp-patched).
- `webclaw-pdf` — PDF text extraction via pdf-extract crate.
- `webclaw-mcp` — MCP server (Model Context Protocol) over stdio transport using `rmcp` crate.
- `webclaw-cli` — CLI binary entry point.

## Two Binaries
- `webclaw` (CLI)
- `webclaw-mcp` (MCP server)

## Key Dependencies
- `scraper` for HTML parsing
- `webclaw-http` (custom fork via webclaw-tls) for TLS fingerprinting
- `tokio` for async runtime
- `clap` for CLI argument parsing
- `rmcp` for MCP protocol
- `[patch.crates-io]` in workspace Cargo.toml patches rustls, h2, hyper, hyper-util, reqwest for TLS fingerprinting

## Hard Rules
- **webclaw-core has ZERO network dependencies** — WASM-compatible
- **primp/webclaw-http requires `[patch.crates-io]`** for patched TLS forks
- **RUSTFLAGS set in `.cargo/config.toml`** (`reqwest_unstable` cfg)
- **webclaw-llm uses plain reqwest** (not primp-patched)
