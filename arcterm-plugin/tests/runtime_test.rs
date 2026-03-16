/// Integration tests for arcterm-plugin PluginRuntime.
///
/// These tests verify the host runtime infrastructure using a minimal WAT component
/// that compiles via wasmtime's Component Model. The WAT component uses simplified
/// types (not the full WIT record/variant types) to avoid the wasm-encoder named-type
/// constraint; the WIT type-level correctness is verified at compile time by the
/// bindgen! macro in src/host.rs.
use std::time::Instant;

use arcterm_plugin::runtime::PluginRuntime;

// ─────────────────────────────────────────────────────────────────────────────
// Test component
// ─────────────────────────────────────────────────────────────────────────────

/// A minimal valid WebAssembly Component that exports `load`, `update`, and `render`.
///
/// Component-level types are declared explicitly so the canonical lift validator
/// can check that core function signatures match the canonical ABI:
///
/// - `load: () -> ()` → core `() -> ()`
/// - `update: (u32) -> bool` → core `(i32) -> i32`  (u32 flattens to i32, bool to i32)
/// - `render: () -> ()` → core `() -> ()`
///
/// Note: These types are simplified compared to the full WIT world (which uses
/// `plugin-event` and `list<styled-line>`). The simplified types are sufficient
/// to test compilation and timing; WIT type-matching is validated at instantiation
/// time via `PluginRuntime::load_plugin`.
const TEST_COMPONENT_WAT: &str = r#"
(component
  ;; Declare component-level function types explicitly.
  ;; Required so canon lift can validate core function signatures.
  (type $load_ty   (func))
  (type $update_ty (func (param "event" u32) (result bool)))
  (type $render_ty (func))

  (core module $m
    (memory (export "memory") 1)
    (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
      local.get 3
    )
    ;; load: no-op
    (func (export "load"))
    ;; update: u32 param flattens to i32; bool result flattens to i32
    (func (export "update") (param i32) (result i32)
      i32.const 1
    )
    ;; render: returns unit
    (func (export "render"))
  )
  (core instance $i (instantiate $m))

  (func $lift_load   (type $load_ty)   (canon lift (core func $i "load")))
  (func $lift_update (type $update_ty)
    (canon lift (core func $i "update")
      (memory $i "memory")
      (realloc (func $i "cabi_realloc"))
    )
  )
  (func $lift_render (type $render_ty) (canon lift (core func $i "render")))

  (export "load"   (func $lift_load))
  (export "update" (func $lift_update))
  (export "render" (func $lift_render))
)
"#;

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_runtime_creation() {
    let runtime = PluginRuntime::new().expect("PluginRuntime::new() should succeed");
    // Engine is accessible — runtime is functional
    let _engine = runtime.engine();
}

#[test]
fn test_component_compiles() {
    let runtime = PluginRuntime::new().expect("PluginRuntime::new() should succeed");
    let engine = runtime.engine();
    let component = wasmtime::component::Component::new(engine, TEST_COMPONENT_WAT.as_bytes())
        .expect("test component should compile");
    drop(component);
}

#[test]
fn epoch_ticker_stops_on_drop() {
    // Verify that dropping a PluginRuntime signals the epoch ticker thread to stop.
    // The thread holds an Arc<Engine> clone; without a shutdown flag, dropping
    // PluginRuntime leaks the thread and the Arc for the process lifetime.
    // This smoke test confirms no panic occurs and the runtime drops cleanly.
    let runtime = PluginRuntime::new().expect("PluginRuntime::new() should succeed");
    drop(runtime);
    // Give the ticker thread time to observe the shutdown flag and exit.
    std::thread::sleep(std::time::Duration::from_millis(50));
    // If we reach here, the Drop impl ran without panic and the thread is winding down.
}

#[test]
fn test_load_timing() {
    // Verify the Engine can compile a component binary in under 50 ms.
    // Actual load_plugin additionally performs WIT type-matching; this test
    // isolates just the compile latency.
    let runtime = PluginRuntime::new().expect("PluginRuntime::new() should succeed");
    let engine = runtime.engine();

    let start = Instant::now();
    let _component = wasmtime::component::Component::new(engine, TEST_COMPONENT_WAT.as_bytes())
        .expect("component compile should succeed");
    let elapsed = start.elapsed();

    println!("Component compile time: {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 50,
        "Component compilation took {}ms, expected < 50ms",
        elapsed.as_millis()
    );
}
