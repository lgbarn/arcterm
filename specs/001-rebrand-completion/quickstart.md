# Quickstart: Verifying the ArcTerm Rebrand

## Prerequisites

- Rust toolchain installed (1.71.0+)
- Repository cloned: `git clone https://github.com/lgbarn/arcterm`
- On the `001-rebrand-completion` branch

## Build and Verify

```bash
# Build
cargo build --release

# Run tests (all existing tests must pass)
cargo test --all

# Verify no user-visible WezTerm references remain
# This should return zero matches (excluding crate names and code comments)
grep -ri "wezterm" \
  wezterm-gui/src/update.rs \
  config/src/config.rs \
  assets/macos/ \
  assets/wezterm.desktop \
  ci/deploy.sh \
  ci/generate-workflows.py \
  .github/FUNDING.yml \
  ci/windows-installer.iss \
  | grep -v "//.*wezterm" \
  | grep -v "crate::" \
  | grep -v "use wezterm"
```

## Test Config File Loading

```bash
# Create an arcterm.lua config
mkdir -p ~/.config/arcterm
echo 'local wezterm = require "wezterm"; return {}' > ~/.config/arcterm/arcterm.lua

# Run ArcTerm — should load arcterm.lua
cargo run --bin wezterm-gui

# Test fallback — rename arcterm.lua and use wezterm.lua
mv ~/.config/arcterm/arcterm.lua ~/.config/arcterm/arcterm.lua.bak
mkdir -p ~/.config/wezterm
echo 'local wezterm = require "wezterm"; return {}' > ~/.config/wezterm/wezterm.lua

# Run ArcTerm — should load wezterm.lua with deprecation notice in logs
cargo run --bin wezterm-gui

# Clean up
mv ~/.config/arcterm/arcterm.lua.bak ~/.config/arcterm/arcterm.lua
rm ~/.config/wezterm/wezterm.lua
```

## Verify Update Checker

```bash
# Run with update check forced
ARCTERM_ALWAYS_SHOW_UPDATE_UI=1 cargo run --bin wezterm-gui

# Check logs for the correct GitHub API URL (lgbarn/arcterm, not wezterm/wezterm)
```

## Verify CI Workflows

```bash
# Regenerate CI workflows
python3 ci/generate-workflows.py

# Check for upstream references in generated files
grep -r "wez/" .github/workflows/ | grep -v "#"
grep -r "push.fury.io" .github/workflows/
grep -r "homebrew-wezterm" .github/workflows/
grep -r "winget-pkgs" .github/workflows/
# All should return empty
```
