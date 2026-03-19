# WASM Test Fixtures

These test fixtures require `wasm-tools` or `cargo-component` to compile.

To generate them:

```bash
# Install wasm-tools
cargo install wasm-tools

# The fixtures are minimal WASM components used by integration tests.
# Until compiled, tests that depend on them are skipped with a
# file-existence check.
```

## hello.wasm
A minimal plugin that calls `log::info("Hello from WASM plugin!")` in `init()`.

## crasher.wasm
A plugin that panics during `init()` to test crash containment.
