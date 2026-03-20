# Repository Guidelines

## Project Structure & Module Organization

This repository is intended for a Rust project. Keep root files limited to project-wide docs and Rust configuration such as `Cargo.toml`, `Cargo.lock`, `README.md`, and this guide.

LLM-facing product docs live in `docs/LLM_GUIDE.md` and `docs/CLI_REFERENCE.md`. If the CLI surface or config semantics change, update those files in the same change.

Use the standard Cargo layout unless there is a strong reason not to:

```text
src/         application code
src/main.rs  binary entry point
src/lib.rs   shared library code
tests/       integration tests
benches/     benchmarks when needed
assets/      sample inputs or static assets
```

Prefer small modules grouped by feature or domain. Keep business logic in `lib.rs` modules and keep `main.rs` thin.

## Build, Test, and Development Commands

Use Cargo from the repository root.

- `cargo check`: fast compile validation during development
- `cargo run`: build and run the default binary locally
- `cargo test`: run unit and integration tests
- `cargo fmt --check`: verify formatting
- `cargo clippy --all-targets --all-features -D warnings`: run lint checks
- `cargo build --release`: produce an optimized build

## Coding Style & Naming Conventions

Use `rustfmt` and `clippy`; commit formatted code only. Follow standard Rust style with 4-space indentation.

- Modules, files, functions, and variables: `snake_case`
- Types, traits, and enums: `PascalCase`
- Constants and statics: `SCREAMING_SNAKE_CASE`
- Crate names: short, descriptive, `snake_case`

Name files after the primary module they contain. Avoid `mod.rs` nesting unless it improves clarity.

## Testing Guidelines

Place unit tests next to the code under `#[cfg(test)]` and cross-module or CLI tests in `tests/`. Cover normal behavior, edge cases, and at least one failure path for new logic.

Before opening a PR, run `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -D warnings`.

## Commit & Pull Request Guidelines

This repository does not have established git history yet. Use short, imperative commit subjects with an optional type prefix, for example `feat: add image resize pipeline`.

PRs should include:

- a short summary of the change
- the reason for the change
- test or lint evidence
- screenshots or sample output when behavior is visible

## Security & Configuration

Do not commit secrets, API keys, or machine-specific config. If environment variables become necessary, keep local values in `.env` files ignored by git and document required keys in `.env.example` or `README.md`.
