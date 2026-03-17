# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust 2024 workspace. Shared search, indexing, storage, and output logic lives in `crates/vg-core/src/`. CLI entrypoints live in `crates/vg-cli/src/main.rs` (`vg`) and `crates/vg-indexer/src/main.rs` (`vg-index`). Benchmarks live in `crates/vg-core/benches/`. Use `tests/fixtures/` for reusable sample files and keep longer-form design or test assets in `docs/`. `tests/integration/` is reserved for end-to-end cases that exercise the binaries.

## Build, Test, and Development Commands
Use Cargo from the workspace root:

- `cargo run -p vg-cli -- "OAuth2 token" ./tests/fixtures`: run hybrid search against sample data.
- `cargo run -p vg-cli -- --vg-semantic "用户认证" ./tests/fixtures`: run semantic-only search.
- `cargo run -p vg-indexer -- ./tests/fixtures`: build or refresh the local index only.
- `cargo test`: run all unit tests and doc tests across the workspace.
- `cargo fmt --check`: verify formatting before commit.
- `cargo bench --bench search_pipeline --no-run`: confirm the Criterion benchmark target still compiles.

## Coding Style & Naming Conventions
Follow default Rust style with `rustfmt` formatting and 4-space indentation. Use `snake_case` for modules, files, functions, and tests; use `UpperCamelCase` for structs and enums; keep constants in `SCREAMING_SNAKE_CASE`. Add new logic in the crate that owns the behavior: `vg-core` for reusable domain code, thin binaries for argument wiring and process exit handling. Keep project-owned CLI flags under the `--vg-*` prefix.

## Testing Guidelines
Unit tests currently live beside the implementation in `#[cfg(test)] mod tests` blocks, with descriptive names such as `split_args_keeps_passthrough`. Reuse `tests/fixtures/` for parser, indexing, and output regressions. No coverage gate is enforced yet, but every behavior change should include focused tests. For performance-sensitive code, keep `criterion` benchmarks building cleanly.

## Commit & Pull Request Guidelines
Git history is still minimal (`Initial commit`), so follow the repository convention instead of copying history: `<type>: <中文描述>`, for example `feat: 增加语义搜索阈值参数`. Valid types are `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, and `chore`. PRs should include a short summary, key changes, linked issue or task, and the commands you ran. Include CLI output samples when changing user-visible search results or JSON format.

## Security & Configuration Tips
Do not commit secrets, local cache directories, downloaded model files, or generated SQLite indexes. Prefer temporary or custom cache paths when testing destructive indexing changes.
