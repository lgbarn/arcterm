# Quickstart: WASM Plugin System

## Prerequisites

- Rust toolchain (1.71.0+ — may need bump for wasmtime)
- ArcTerm repository on `002-wasm-plugin-system` branch
- `cargo-component` for building WASM plugins (optional, for plugin development)

## Build and Verify

```bash
# Build ArcTerm with WASM plugin support
cargo build --release

# Run tests
cargo test --all

# Verify the new crate exists
cargo check --package arcterm-wasm-plugin
```

## Test Plugin Loading

### 1. Create a minimal plugin

Create a file `hello.wit`:
```wit
package example:hello@1.0.0;
world arcterm-plugin {
    import arcterm:plugin/log;
    export arcterm:plugin/lifecycle;
}
```

Create `hello.rs`:
```rust
use arcterm::plugin::log;

struct HelloPlugin;

impl arcterm::plugin::lifecycle::Guest for HelloPlugin {
    fn init() -> Result<(), String> {
        log::info("Hello from WASM plugin!");
        Ok(())
    }
    fn destroy() {}
}
```

Build: `cargo component build --release`

### 2. Configure ArcTerm to load it

In `~/.config/arcterm/arcterm.lua`:
```lua
local wezterm = require 'wezterm'

wezterm.plugin.register({
  name = "hello",
  path = wezterm.home_dir .. "/.config/arcterm/plugins/hello.wasm",
  capabilities = { "terminal:read" },
})

return {}
```

### 3. Run and verify

```bash
RUST_LOG=info cargo run --bin wezterm-gui 2>&1 | grep "Hello from WASM"
# Should see: "Hello from WASM plugin!"
```

## Test Capability Enforcement

### Filesystem denied (no capability)

```lua
wezterm.plugin.register({
  name = "fs-test",
  path = wezterm.home_dir .. "/.config/arcterm/plugins/fs-test.wasm",
  capabilities = { "terminal:read" },  -- no fs capability
})
```

Run ArcTerm — if the plugin tries to read a file, the log should show:
```
WARN: Plugin "fs-test" denied capability fs:read — not granted
```

### Filesystem allowed (with capability)

```lua
wezterm.plugin.register({
  name = "fs-test",
  path = wezterm.home_dir .. "/.config/arcterm/plugins/fs-test.wasm",
  capabilities = { "terminal:read", "fs:read:." },
})
```

Run ArcTerm — plugin can read files in the current directory.

## Test Plugin Isolation

### Crash containment

Create a plugin that panics in `init()`:
```rust
fn init() -> Result<(), String> {
    panic!("deliberate crash");
}
```

Run ArcTerm — the terminal should start normally with a log error:
```
ERROR: Plugin "crasher" failed during init: panic: deliberate crash
```

## Test Lua Coexistence

```lua
local wezterm = require 'wezterm'

-- Existing Lua config
local config = wezterm.config_builder()
config.font_size = 14.0

-- WASM plugin alongside Lua
wezterm.plugin.register({
  name = "hello",
  path = wezterm.home_dir .. "/.config/arcterm/plugins/hello.wasm",
  capabilities = { "terminal:read" },
})

return config
```

Both the font size change and the WASM plugin should work.
