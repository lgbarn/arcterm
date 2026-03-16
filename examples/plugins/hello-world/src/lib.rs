// Hello-world example plugin for Arcterm.
//
// Demonstrates the minimal guest API surface:
//   - load()   — receives config; initialises plugin state.
//   - render() — returns styled lines via host::render_text().
//   - update() — handles events; returns true when re-render is needed.
//
// To build (requires wasm32-wasip2 target and cargo-component):
//   cargo component build --release
//
// Or with plain cargo + wasm-tools post-processing:
//   cargo build --target wasm32-wasip2 --release
//   wasm-tools component new target/wasm32-wasip2/release/hello_world_plugin.wasm \
//       -o hello_world_plugin.wasm

// Generate host/guest bindings from the WIT interface.
// The path is relative to the Cargo.toml for this crate.
wit_bindgen::generate!({
    path: "../../../arcterm-plugin/wit/arcterm.wit",
    world: "arcterm-plugin",
});

use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Plugin state — stored in thread-locals since WASM is single-threaded.
// ---------------------------------------------------------------------------

thread_local! {
    /// The last key character typed by the user (updated via KeyInput events).
    static LAST_KEY: RefCell<String> = RefCell::new(String::new());

    /// All characters typed since plugin load, accumulated for display.
    static TYPED: RefCell<String> = RefCell::new(String::new());
}

// ---------------------------------------------------------------------------
// Guest export implementations
// ---------------------------------------------------------------------------

struct HelloWorldPlugin;

impl Guest for HelloWorldPlugin {
    /// Called once when the plugin is first loaded.
    ///
    /// Receives a config map from the host (empty for default loads).
    fn load() {
        // Log our startup to the host's log output.
        host::log("hello-world plugin: loaded");
    }

    /// Called by the host when this plugin's pane needs to be drawn.
    ///
    /// Returns styled lines by calling `host::render_text` for each line.
    /// The host's draw buffer is cleared before `render()` is called, so
    /// every call must emit the full frame.
    fn render() -> Vec<StyledLine> {
        let last_key = LAST_KEY.with(|k| k.borrow().clone());
        let typed = TYPED.with(|t| t.borrow().clone());

        // Line 1: greeting in bold green.
        host::render_text(StyledLine {
            text: "Hello from WASM plugin!".to_string(),
            fg: Some(Color { r: 80, g: 220, b: 80 }),
            bg: None,
            bold: true,
            italic: false,
        });

        // Line 2: last key pressed (cyan).
        let key_line = if last_key.is_empty() {
            "Press any key...".to_string()
        } else {
            format!("Last key: {}", last_key)
        };
        host::render_text(StyledLine {
            text: key_line,
            fg: Some(Color { r: 80, g: 200, b: 220 }),
            bg: None,
            bold: false,
            italic: false,
        });

        // Line 3: accumulated typed text (white).
        if !typed.is_empty() {
            host::render_text(StyledLine {
                text: format!("Typed: {}", typed),
                fg: Some(Color { r: 200, g: 200, b: 200 }),
                bg: None,
                bold: false,
                italic: false,
            });
        }

        // Return an empty Vec — the lines are delivered via render_text() above.
        // (The host collects them from the draw buffer populated by render_text calls.)
        Vec::new()
    }

    /// Called by the host when a terminal event occurs.
    ///
    /// Returns `true` if the event was consumed and a re-render is needed.
    fn update(event: PluginEvent) -> bool {
        match event {
            PluginEvent::KeyInput(payload) => {
                // Record the key name for display.
                let key_str = payload.key_char
                    .unwrap_or_else(|| payload.key_name.clone());

                LAST_KEY.with(|k| *k.borrow_mut() = key_str.clone());

                // Append printable characters to the typed buffer.
                // Skip control key combinations (Ctrl+anything).
                if !payload.modifiers.ctrl && !payload.modifiers.alt {
                    if key_str.len() == 1 {
                        TYPED.with(|t| t.borrow_mut().push_str(&key_str));
                    }
                }

                true // request re-render
            }
            // All other events are not consumed.
            _ => false,
        }
    }
}

// Register this struct as the implementation of the guest interface.
export!(HelloWorldPlugin);
