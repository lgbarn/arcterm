---
phase: workspaces
plan: "2.1"
wave: 2
dependencies: ["1.1", "1.2"]
must_haves:
  - clap 4 derive-based CLI with open/save/list subcommands
  - arcterm open <workspace> loads TOML and passes WorkspaceFile into App for restore
  - arcterm list prints workspace names from ~/.config/arcterm/workspaces/
  - arcterm save <name> is a stub that prints a message (session capture requires running app state)
  - Default launch (no subcommand) behaves identically to current main()
files_touched:
  - arcterm-app/Cargo.toml
  - arcterm-app/src/main.rs
  - arcterm-app/src/workspace.rs
tdd: false
---

# PLAN-2.1 -- CLI Subcommands with clap 4

## Goal

Add `arcterm open <workspace>`, `arcterm list`, and `arcterm save <name>` subcommands using clap 4 with derive macros. The `open` subcommand loads a workspace TOML file and passes the parsed `WorkspaceFile` into the `App` struct for use during `resumed()`. The `list` subcommand prints available workspace names and exits without starting the GUI. The default (no-subcommand) path preserves current behavior exactly.

## Why Wave 2

This plan depends on PLAN-1.1 (workspace data model for `WorkspaceFile` parsing) and must be in place before Wave 3 (workspace switcher UI calls the same load logic).

## Design Notes

The integration point is the top of `main()`. Currently:

```rust
fn main() {
    env_logger::init();
    let rt = tokio::runtime::Builder::new_multi_thread()...;
    let event_loop = EventLoop::new().unwrap();
    let mut app = App { state: None, ... };
    event_loop.run_app(&mut app).unwrap();
}
```

After this plan:

```rust
fn main() {
    env_logger::init();
    let cli = Cli::parse();
    match cli.command {
        Some(Command::List) => { /* print names, return */ },
        Some(Command::Open { name }) => { /* load workspace, pass to App */ },
        Some(Command::Save { name }) => { /* stub: print message */ },
        None => { /* current default path */ },
    }
    // ... tokio runtime, event loop ...
}
```

The `App` struct gains an `initial_workspace: Option<WorkspaceFile>` field. When `Some`, the `resumed()` handler uses it to spawn panes according to the workspace layout instead of spawning a single default pane.

**Key binding conflict**: `Leader+w` currently maps to `CloseTab` in `keymap.rs`. CONTEXT-5.md specifies `Leader+w` for the workspace switcher. This plan adds a new `KeyAction::OpenWorkspaceSwitcher` variant and rebinds `Leader+w`. `CloseTab` moves to `Leader+W` (Shift+w) to preserve discoverability. This change is documented in the plan but the workspace switcher itself is wired in PLAN-3.1.

## Tasks

<task id="1" files="arcterm-app/Cargo.toml, arcterm-app/src/main.rs" tdd="false">
  <action>Add clap 4 dependency and implement CLI parsing at the top of main().

1. Add `clap = { version = "4", features = ["derive"] }` to `[dependencies]` in `arcterm-app/Cargo.toml`.

2. Define the CLI structs at the top of `main.rs` (before `fn main()`):
   ```
   #[derive(clap::Parser)]
   #[command(name = "arcterm", about = "GPU-rendered AI terminal emulator")]
   struct Cli {
       #[command(subcommand)]
       command: Option<Command>,
   }

   #[derive(clap::Subcommand)]
   enum Command {
       /// Open a named workspace
       Open {
           /// Workspace name (without .toml extension)
           name: String,
       },
       /// Save current session as a workspace
       Save {
           /// Workspace name
           name: String,
       },
       /// List available workspaces
       List,
   }
   ```

3. At the top of `main()`, after `env_logger::init()`, add:
   ```
   let cli = <Cli as clap::Parser>::parse();
   ```

4. Add a match on `cli.command`:
   - `Some(Command::List)`: call `workspace::list_workspaces()` (prints names to stdout), then `return` (no GUI).
   - `Some(Command::Save { name })`: print `"Save command requires a running arcterm session. Use Leader+s from within arcterm."` to stderr, then `return`.
   - `Some(Command::Open { name })`: resolve the workspace path as `workspace::workspaces_dir().join(format!("{name}.toml"))`, call `WorkspaceFile::load_from_file(&path)`. On success, store in a local variable. On error, log the error and exit with code 1.
   - `None`: proceed with current default behavior (no workspace).

5. Add `initial_workspace: Option<workspace::WorkspaceFile>` field to the `App` struct. Set it from the parsed `Open` command result (or `None` for default launch).

6. Add `list_workspaces()` function to `workspace.rs`: reads `workspaces_dir()` with `std::fs::read_dir`, filters for `.toml` files, prints each filename (without extension) to stdout, one per line. If the directory does not exist, print "No workspaces found." and return.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -5 && cargo test -p arcterm-app -- --nocapture 2>&1 | tail -20</verify>
  <done>Build succeeds with clap 4. All existing tests pass. `arcterm-app list` runs without starting the GUI. `arcterm-app open nonexistent` prints an error and exits with code 1. Default launch (no args) works identically to before.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs, arcterm-app/src/workspace.rs" tdd="false">
  <action>Wire workspace restore into the App::resumed() handler so that `arcterm open <name>` spawns panes according to the workspace layout.

1. In the `resumed()` method of `App` (inside `ApplicationHandler`), after the renderer and initial state are created, check if `self.initial_workspace` is `Some(ws)`.

2. If a workspace is present:
   - Call `ws.layout.to_pane_tree()` to get a `(PaneNode, Vec<PaneMetadata>)`.
   - For each `PaneMetadata` in the returned vec, call `state.spawn_pane_with_cwd(size, meta.directory.as_deref().map(Path::new))` to create a terminal in the correct directory.
   - If `meta.command` is `Some(cmd)`, write `cmd + "\n"` to the pane's terminal via `write_input` (replays the command).
   - Set `ws.environment` entries into each pane's terminal env (write `export KEY=VALUE\n` for each).
   - Replace the default single-pane layout with the workspace's `PaneNode` tree.
   - Set the `TabManager`'s active tab layout to the workspace tree.
   - Set window title to include workspace name.

3. If no workspace is present, proceed with current single-pane default (unchanged).

4. Ensure the pane spawning loop correctly maps PaneIds from `to_pane_tree()` output to the terminals created by `spawn_pane_with_cwd()`. The `to_pane_tree()` method returns freshly allocated PaneIds in tree-traversal order; `spawn_pane_with_cwd()` also allocates PaneIds. These must be synchronized -- either `to_pane_tree()` returns the IDs it allocated and `spawn_pane_with_cwd` reuses them, or a post-creation fixup maps IDs. Choose the approach that produces the cleanest code and document it.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -5 && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -10</verify>
  <done>Build succeeds. `arcterm open <workspace>` restores pane layout, working directories, and commands from the TOML file. Default launch still works. Clippy clean.</done>
</task>

<task id="3" files="arcterm-app/src/keymap.rs" tdd="true">
  <action>Rebind Leader+w from CloseTab to OpenWorkspaceSwitcher and move CloseTab to Leader+W (shift).

1. Add `OpenWorkspaceSwitcher` variant to the `KeyAction` enum in `keymap.rs`.

2. In the `LeaderPending` match arm, change `"w"` from `Some(KeyAction::CloseTab)` to `Some(KeyAction::OpenWorkspaceSwitcher)`.

3. Add a new match for uppercase `"W"` that maps to `Some(KeyAction::CloseTab)`.

4. Update the existing test `leader_then_w_close_tab` to expect `KeyAction::OpenWorkspaceSwitcher` instead of `KeyAction::CloseTab`, and rename it to `leader_then_w_opens_workspace_switcher`.

5. Add a new test `leader_then_shift_w_closes_tab` that sends `Leader + "W"` (uppercase) and asserts `KeyAction::CloseTab`.

6. Update the `PaletteAction::CloseTab` mapping in `palette.rs` if it references the `w` key description anywhere.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- keymap --nocapture</verify>
  <done>All keymap tests pass. Leader+w produces OpenWorkspaceSwitcher. Leader+W (shift) produces CloseTab. No regressions in other keymap tests.</done>
</task>
