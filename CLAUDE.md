# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands
- Build: `cargo build`
- Check: `cargo check`
- Run: `cargo run -- [SUBCOMMAND]`
- Test all: `cargo test`
- Test single: `cargo test test_name`
- Lint: `cargo clippy`
- Format: `cargo fmt`

## Code Style Guidelines
- Follow Rust standard naming conventions (snake_case for variables/functions, CamelCase for types)
- Use `anyhow::Result` for error handling with context via `.context()`
- Prefer strongly typed enums with `#[derive]` traits over stringly-typed code
- Struct fields with serde serialization use snake_case in TOML but camelCase naming in code
- Error messages should be descriptive and include context about what failed
- Test functions use underscores (`test_function_name`) and include assertions
- New code should have accompanying tests
- Use `unwrap_or_default()` instead of match statements for simple Option handling