# hello-world — Arcterm Plugin Example

A minimal example plugin demonstrating the Arcterm WASM plugin API:

- `load()` — logs startup to the host.
- `render()` — emits three styled lines via `host::render_text`:
  - "Hello from WASM plugin!" (bold, green)
  - Last key pressed (cyan)
  - Accumulated typed text (white)
- `update(KeyInput)` — records the key character and returns `true` to trigger
  a re-render.

## Building

### Prerequisites

```sh
# 1. Install the wasm32-wasip2 Rust target (requires Rust 1.82+ / nightly):
rustup target add wasm32-wasip2

# 2. Install cargo-component (produces a proper WebAssembly Component):
cargo install cargo-component
```

### Build

```sh
# Option A: cargo-component (recommended — produces a WIT Component directly)
cargo component build --release

# Option B: plain cargo + wasm-tools post-processing
cargo build --target wasm32-wasip2 --release
wasm-tools component new \
    target/wasm32-wasip2/release/hello_world_plugin.wasm \
    -o hello_world_plugin.wasm
```

### Load in arcterm

```sh
arcterm-app plugin dev /path/to/examples/plugins/hello-world
```

## Notes

- The WIT file is at `arcterm-plugin/wit/arcterm.wit` (relative to the repo root).
- `wit-bindgen = "0.36"` is the guest-side crate; the host uses wasmtime's own
  internal bindings and does not share this dependency.
- The `wasm32-wasip2` standard library is not installed on all CI machines.
  This plugin is an author example only — it is not compiled as part of the main
  workspace build.
