# Mythrax Workspace Rules

## Parallel Test Execution
- **Mandate**: Always run test suites in parallel using `cargo nextest run` or the `cargo t` alias.
- **Why**: The default `cargo test` runs test suites sequentially which triggers database lock contentions and significantly slows down the E2E verification loop.
- **Fast Mocking**: Always specify the `MYTHRAX_TEST_MOCK=1` environment variable when running unit and integration tests (e.g., `MYTHRAX_TEST_MOCK=1 cargo nextest run`). Do NOT specify `--features mlx` for mock tests, to avoid heavy Metal compiler/JIT loading and compilation overhead. If JIT compile errors or startup hangs occur on macOS, ensure `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` is exported.
