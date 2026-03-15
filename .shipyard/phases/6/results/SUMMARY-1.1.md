# SUMMARY-1.1.md — Phase 6, Plan 1.1: WIT Interface Definition + wasmtime Host Runtime

## Status: Complete

All three tasks executed, verified, and committed.

## Tasks Completed

### Task 1: Create `arcterm-plugin` crate with WIT world

**Commits:** `5fa0b75 shipyard(phase-6): add arcterm-plugin crate with WIT world definition`

**Files created:**
- `arcterm-plugin/Cargo.toml` — workspace-integrated crate with wasmtime 42, wasmtime-wasi 42, tokio, log, serde, serde_json, toml, anyhow
- `arcterm-plugin/wit/arcterm.wit` — package `arcterm:plugin@0.1.0` with `interface types` and world `arcterm-plugin`
- `arcterm-plugin/src/lib.rs` — module declarations for `host`, `runtime`, `types`
- `arcterm-plugin/src/types.rs` — `PluginId` newtype and re-exports of bindgen-generated types
- `Cargo.toml` (workspace) — added `arcterm-plugin` to members; added workspace deps for wasmtime/wasmtime-wasi/anyhow

**WIT world structure:**
```
interface types {
    record color { r: u8, g: u8, b: u8 }
    record styled-line { text: string, fg: option<color>, bg: option<color>, bold: bool, italic: bool }
    enum event-kind { pane-opened, pane-closed, command-executed, workspace-switched }
    variant plugin-event { pane-opened(string), pane-closed(string), command-executed(string), workspace-switched(string) }
    record tool-schema { name: string, description: string, input-schema: string }
}
world arcterm-plugin {
    use types.{color, styled-line, event-kind, plugin-event, tool-schema};
    import log: func(msg: string);
    import render-text: func(line: styled-line);
    import subscribe-event: func(kind: event-kind);
    import get-config: func(key: string) -> option<string>;
    import register-mcp-tool: func(schema: tool-schema);
    export load: func();
    export update: func(event: plugin-event) -> bool;
    export render: func() -> list<styled-line>;
}
```

**Verification:** `cargo check -p arcterm-plugin` passed.

### Task 2: Implement `PluginHostData`, `PluginRuntime`, `PluginInstance`

**Commits:** `b0b34e1 shipyard(phase-6): implement PluginHostData, PluginRuntime, and PluginInstance`

**Files created:**
- `arcterm-plugin/src/host.rs` — `bindgen!` macro expansion, `PluginHostData` struct with `WasiView` impl, `ArctermPluginImports` impl, 10 MB `StoreLimits`
- `arcterm-plugin/src/runtime.rs` — `PluginRuntime` (engine + linker), `PluginInstance` (store + typed instance)

**Key API:**
- `PluginRuntime::new()` — creates Engine with component model + epoch interruption, adds WASI p2 sync + custom imports to Linker
- `PluginRuntime::load_plugin(bytes, config)` — compiles component, creates store with 10 MB limit, instantiates, calls `load`
- `PluginRuntime::engine()` — exposes engine for test-side compilation
- `PluginInstance::call_update(&event)` — returns `bool`
- `PluginInstance::call_render()` — clears draw buffer, calls render, returns `Vec<StyledLine>` (populated via `render_text` host import)

**Verification:** `cargo check -p arcterm-plugin` passed.

### Task 3: Integration tests for PluginRuntime (TDD)

**Commits:** `aa403f8 shipyard(phase-6): add integration tests for PluginRuntime compilation and timing`

**Files created:**
- `arcterm-plugin/tests/runtime_test.rs` — three integration tests using a WAT component

**Tests:**
- `test_runtime_creation` — `PluginRuntime::new()` succeeds, engine is accessible
- `test_component_compiles` — a valid WAT component compiles via `Component::new`
- `test_load_timing` — compilation completes in < 50 ms

**Verification:** `cargo test -p arcterm-plugin` — 3 passed, 0 failed.

## Deviations and Technical Notes

### wasmtime 42 API changes from prior versions

1. **WasiView trait**: No longer has separate `table()` and `ctx()` methods. New API: `fn ctx(&mut self) -> WasiCtxView<'_>` where `WasiCtxView` contains both `ctx: &mut WasiCtx` and `table: &mut ResourceTable`.
2. **ArctermPluginImports functions are infallible**: bindgen! generates `fn render_text(&mut self, line: StyledLine)` (not `Result`). All host import implementations return their value directly.
3. **bindgen! generated struct name**: World name `arcterm-plugin` produces `ArctermPlugin` (not `ArcTermPlugin`).
4. **Empty trait requirement**: bindgen! generates `arcterm::plugin::types::Host` for the types interface, requiring `impl arcterm::plugin::types::Host for PluginHostData {}`.

### WAT component binary encoding

The integration test WAT component uses explicit component-level type annotations to satisfy wasmtime's canonical ABI validator:

```wat
(type $load_ty   (func))
(type $update_ty (func (param "event" u32) (result bool)))
(type $render_ty (func))
```

**Why not wasm-encoder?** The wasm-encoder binary encoding approach was blocked by wasmparser's named-type constraint: types used in import function signatures must be in the validator's `imported_types` set. Locally-defined types (Records, Enums, Variants) can only enter that set by being imported/exported as type entities — but importing a type with `TypeBounds::Eq(N)` creates a *new* type alias ID, not the original ID. Subsequent type definitions that reference the ORIGINAL type ID still fail the named-type check. This is a fundamental wasmparser validation rule (line 826-830 in `wasmparser/src/validator/component.rs`).

**Why WAT with explicit types?** The WAT text format supports `(type $name (func ...))` declarations that are used by canonical lift, satisfying the type-checking without the named-type complexity. The simplified types (`u32`, `bool`) are sufficient for testing Engine compilation and timing; WIT type-level correctness is validated at compile time by the `bindgen!` macro.

**Implication for production use**: Real guest plugins must be compiled with `cargo-component` or `wit-bindgen` to produce a component binary with the correct WIT types. The test component validates the host runtime infrastructure, not end-to-end WIT type compatibility.

### WIT syntax discoveries

- Types cannot appear at package level; they must be in `interface` or `world` blocks
- Same-package types use `use types.{...}` in the world; cross-package types use `use pkg:name/iface.{...}`
- WAT `canon lift` requires explicit `(type ...)` annotations when the component type cannot be inferred from the core function signature alone

## Final State

```
arcterm-plugin/
  Cargo.toml
  wit/arcterm.wit
  src/
    lib.rs
    host.rs       (bindgen!, PluginHostData, WasiView, ArctermPluginImports)
    runtime.rs    (PluginRuntime, PluginInstance)
    types.rs      (PluginId, re-exports)
  tests/
    runtime_test.rs  (3 integration tests, all passing)
```

`cargo test -p arcterm-plugin`: 3 passed, 0 failed
`cargo check --workspace`: clean
