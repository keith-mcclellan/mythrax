# Mythrax Workspace Rules

## Parallel Test Execution
- **Mandate**: Always run test suites in parallel using `cargo nextest run` or the `cargo t` alias.
- **Why**: The default `cargo test` runs test suites sequentially which triggers database lock contentions and significantly slows down the E2E verification loop.
- **Fast Mocking**: Always specify the `MYTHRAX_TEST_MOCK=1` environment variable when running unit and integration tests (e.g. `MYTHRAX_TEST_MOCK=1 cargo t --features mlx`) to bypass multi-gigabyte Hugging Face model downloads and GPU VRAM allocations.
