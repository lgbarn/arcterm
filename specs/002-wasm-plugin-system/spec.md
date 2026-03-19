# Feature Specification: WASM Plugin System

**Feature Branch**: `002-wasm-plugin-system`
**Created**: 2026-03-19
**Status**: Complete
**Input**: User description: "WASM plugin system with capability-based sandbox for extending ArcTerm"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Load and Run a WASM Plugin (Priority: P1)

A plugin developer writes a WASM plugin for ArcTerm and a user installs it. The user adds the plugin to their `arcterm.lua` configuration. When ArcTerm starts, the plugin loads, initializes, and runs within a sandboxed environment. The plugin can read terminal state (visible text, cursor position, current working directory) but cannot access the filesystem, network, or other system resources unless the user explicitly grants those capabilities.

**Why this priority**: This is the foundational capability — without loading and running WASM plugins, nothing else works. It must be independently useful: a plugin that reads terminal state and displays information is valuable on its own.

**Independent Test**: Write a minimal "hello world" WASM plugin that reads the terminal's visible text and logs a message. Add it to the config, start ArcTerm, and verify the plugin initializes and the log message appears.

**Acceptance Scenarios**:

1. **Given** a valid WASM plugin file and a config entry referencing it, **When** ArcTerm starts, **Then** the plugin loads, initializes, and reports its name and version in the logs
2. **Given** a WASM plugin with no capability grants, **When** the plugin attempts to read terminal visible text, **Then** the read succeeds (terminal read is a default capability)
3. **Given** a WASM plugin with no capability grants, **When** the plugin attempts to access the filesystem, **Then** the access is denied and an error is logged without crashing the terminal
4. **Given** a malformed or invalid WASM file referenced in the config, **When** ArcTerm starts, **Then** the terminal starts normally, logs an error about the invalid plugin, and all other plugins continue to function

---

### User Story 2 - Capability-Based Permission Grants (Priority: P1)

A user installs a plugin that needs filesystem access (e.g., a file preview plugin). The user grants specific capabilities in their configuration — for example, read-only access to the current working directory. The plugin receives exactly the permissions granted and no more. The user can review and revoke capabilities at any time by editing their config.

**Why this priority**: Security is a core constitutional principle. The capability sandbox differentiates ArcTerm's plugin system from unrestricted plugin models. Without it, WASM plugins are either too restricted to be useful or too permissive to be safe.

**Independent Test**: Write a plugin that reads a file from the current directory. Run it once without filesystem capability (should fail gracefully). Add the capability grant, restart, and verify it reads the file successfully.

**Acceptance Scenarios**:

1. **Given** a plugin configured with `capabilities = ["fs:read:/home/user/projects"]`, **When** the plugin reads a file within that path, **Then** the read succeeds
2. **Given** a plugin configured with `capabilities = ["fs:read:/home/user/projects"]`, **When** the plugin attempts to write a file, **Then** the write is denied
3. **Given** a plugin configured with `capabilities = ["fs:read:/home/user/projects"]`, **When** the plugin attempts to read a file outside that path (e.g., `/etc/passwd`), **Then** the read is denied
4. **Given** a plugin with `capabilities = ["net:connect:api.example.com:443"]`, **When** the plugin makes an HTTPS request to `api.example.com`, **Then** the request succeeds
5. **Given** a plugin with no network capabilities, **When** the plugin attempts any network connection, **Then** the connection is denied

---

### User Story 3 - Plugin Coexistence with Lua (Priority: P2)

A user has existing Lua configuration and plugins. They install a WASM plugin alongside their Lua setup. Both plugin systems work independently — Lua plugins continue to function exactly as before, and the WASM plugin runs in its own sandbox. There is no interference between the two systems.

**Why this priority**: ArcTerm inherits WezTerm's mature Lua plugin ecosystem. Breaking Lua compatibility to add WASM would alienate existing users. Coexistence is essential for adoption.

**Independent Test**: Start ArcTerm with an existing Lua config that customizes keybindings and colors, plus a WASM plugin. Verify both the Lua customizations and the WASM plugin function correctly.

**Acceptance Scenarios**:

1. **Given** a user config with both Lua customizations and a WASM plugin, **When** ArcTerm starts, **Then** both the Lua config and the WASM plugin load and run without interference
2. **Given** a Lua plugin that registers a keybinding and a WASM plugin that reads terminal state, **When** the user triggers the keybinding, **Then** the Lua action fires and the WASM plugin sees the resulting terminal state change
3. **Given** a crashing WASM plugin, **When** ArcTerm is running with Lua customizations, **Then** the Lua customizations continue to function and the terminal remains usable

---

### User Story 4 - Plugin Lifecycle Management (Priority: P2)

A user wants to manage their plugins — install new ones, disable existing ones, and handle plugin crashes gracefully. Plugins have a well-defined lifecycle (load, initialize, run, destroy) and the user can control which plugins are active through their configuration. If a plugin crashes or hangs, the terminal continues to function normally.

**Why this priority**: Robust lifecycle management prevents plugins from degrading the terminal experience. Users need confidence that a bad plugin won't ruin their workflow.

**Independent Test**: Start ArcTerm with a plugin that deliberately panics during initialization. Verify the terminal starts normally and the plugin is marked as failed.

**Acceptance Scenarios**:

1. **Given** a plugin is listed in the config, **When** ArcTerm starts, **Then** the plugin goes through load → initialize → running states in order
2. **Given** a running plugin, **When** ArcTerm shuts down, **Then** the plugin's destroy callback is invoked before the terminal exits
3. **Given** a plugin that panics during initialization, **When** ArcTerm starts, **Then** the terminal starts normally, logs the error, and marks the plugin as failed
4. **Given** a plugin that enters an infinite loop, **When** the plugin exceeds its execution time budget, **Then** the plugin is terminated and an error is logged without blocking the terminal

---

### User Story 5 - Terminal State API for Plugins (Priority: P3)

A plugin developer writes a plugin that needs to interact with the terminal — reading visible text, responding to output changes, or injecting text into the terminal input. The plugin accesses these capabilities through a well-defined host API that provides terminal state without exposing internal implementation details.

**Why this priority**: The host API is what makes plugins genuinely useful. Without it, plugins can load but can't do anything meaningful. This is lower priority because a minimal read-only API (visible text, CWD) is included in US1.

**Independent Test**: Write a plugin that watches for compiler error patterns in terminal output and highlights the relevant lines. Verify the plugin receives output events and can annotate the terminal display.

**Acceptance Scenarios**:

1. **Given** a plugin with terminal read capability, **When** the terminal displays new output, **Then** the plugin receives a callback with the new text content
2. **Given** a plugin with terminal write capability, **When** the plugin sends text, **Then** the text appears in the terminal as if the user typed it
3. **Given** a plugin with pane metadata capability, **When** the plugin queries pane info, **Then** it receives the current working directory, pane dimensions, and the last command's exit code
4. **Given** a plugin that registers a custom key binding, **When** the user presses that key combination, **Then** the plugin's handler is invoked

---

### Edge Cases

- What happens when two plugins request conflicting capabilities (e.g., both want exclusive terminal write access)? Each plugin operates independently — there is no exclusive access model. Both can write; output interleaving is the plugin developer's responsibility to manage.
- What happens when a plugin's WASM file is updated on disk while ArcTerm is running? The plugin continues using the loaded version. Changes take effect on next restart (or when the user triggers a config reload).
- What happens when a plugin uses more memory than expected? Plugins have a configurable memory limit. Exceeding it terminates the plugin with an out-of-memory error.
- What happens when the user specifies a capability format that is invalid? ArcTerm logs a warning about the invalid capability and loads the plugin with only its valid capabilities.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: ArcTerm MUST load WASM plugins specified in the user's configuration file
- **FR-002**: ArcTerm MUST execute WASM plugins in an isolated sandbox that prevents access to system resources by default
- **FR-003**: Users MUST be able to grant specific capabilities to individual plugins via configuration (filesystem paths, network hosts, terminal I/O)
- **FR-004**: The capability system MUST enforce least-privilege — plugins receive only the capabilities explicitly granted, nothing more
- **FR-005**: A crashing or misbehaving plugin MUST NOT take down the terminal or affect other plugins
- **FR-006**: WASM plugins MUST coexist with the existing Lua plugin system without interference
- **FR-007**: Plugins MUST have a defined lifecycle: load → initialize → running → destroy
- **FR-008**: ArcTerm MUST provide a host API that exposes terminal state to plugins (visible text, cursor position, working directory, pane dimensions)
- **FR-009**: ArcTerm MUST support a terminal write capability that allows plugins to inject text into the terminal input stream (when granted)
- **FR-010**: Plugins MUST be configurable with memory limits and execution time budgets
- **FR-011**: ArcTerm MUST log plugin lifecycle events (load, initialize, error, terminate) at an appropriate log level
- **FR-012**: Invalid or missing plugin files MUST NOT prevent ArcTerm from starting — errors are logged and the terminal proceeds normally
- **FR-013**: Plugin capability grants MUST follow a deny-by-default model: `fs:read:<path>`, `fs:write:<path>`, `net:connect:<host>:<port>`, `terminal:read`, `terminal:write`
- **FR-014**: ArcTerm MUST expose a plugin registration mechanism for custom key bindings that integrates with the existing keybinding system

### Key Entities

- **Plugin**: A WASM binary loaded at startup, identified by name and file path, with a set of granted capabilities and lifecycle state (loading, running, failed, stopped)
- **Capability**: A permission grant from the user to a plugin, scoped to a specific resource type and target (e.g., filesystem path, network host, terminal operation)
- **Host API**: The interface between the terminal and plugins, providing methods to read terminal state, subscribe to events, and (with capability) write to the terminal
- **Plugin Configuration**: The user-facing config block that specifies which plugins to load, their file paths, capability grants, and resource limits

## Assumptions

- WASM plugins are compiled to the WASM Component Model format (not raw WASM modules), enabling richer host-guest interop
- Plugin files are distributed as single `.wasm` files that users place in a known directory (e.g., `~/.config/arcterm/plugins/`)
- The plugin host API is versioned — plugins declare which API version they target, and ArcTerm validates compatibility at load time
- Plugin configuration is declared in the same `arcterm.lua` config file used for all ArcTerm settings, under a `plugins` table
- Capability syntax follows the pattern `<resource>:<operation>:<target>` (e.g., `fs:read:/home/user/projects`)
- Default memory limit per plugin is 64MB; default execution time budget per callback is 100ms — both configurable per-plugin
- Plugin output events (terminal text changes) are delivered asynchronously and may be batched for performance

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A minimal WASM plugin loads and initializes within 500ms of ArcTerm startup
- **SC-002**: A plugin denied a capability receives a clear error and ArcTerm continues running — 100% of denial scenarios handled gracefully
- **SC-003**: Terminal remains responsive (60fps rendering, sub-100ms input latency) while running 5 concurrent WASM plugins
- **SC-004**: A deliberately crashing plugin is contained — the terminal and all other plugins continue functioning in 100% of crash scenarios
- **SC-005**: Existing Lua configuration and plugins work identically with the WASM system enabled — zero regressions
- **SC-006**: A plugin developer can write, compile, configure, and test a "hello world" plugin in under 30 minutes using documentation alone
- **SC-007**: All existing ArcTerm tests pass (`cargo test --all` green) with the WASM plugin system integrated
