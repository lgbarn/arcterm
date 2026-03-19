#!/usr/bin/env bash
# Verify ArcTerm rebrand completeness.
# Searches user-visible files for remaining WezTerm references.
# Exits non-zero if any are found.
# Excludes: internal crate names, Rust import paths, code comments referencing upstream history.

set -euo pipefail

cd "$(dirname "$0")/.."

SEARCH_PATHS=(
  wezterm-gui/src/update.rs
  config/src/config.rs
  config/src/ssh.rs
  wezterm-font/src/lib.rs
  mux/src/localpane.rs
  window/src/os/macos/app.rs
  window/src/os/macos/window.rs
  window/src/os/macos/menu.rs
  wezterm-toast-notification/src/macos.rs
  wezterm-gui/build.rs
  strip-ansi-escapes/src/main.rs
  ci/generate-workflows.py
  ci/deploy.sh
  ci/create-release.sh
  ci/check-rust-version.sh
  ci/windows-installer.iss
  .github/FUNDING.yml
)

# Asset files (may be renamed — check both old and new names)
ASSET_PATHS=(
  assets/macos/WezTerm.app
  assets/macos/ArcTerm.app
  assets/wezterm.desktop
  assets/arcterm.desktop
  assets/wezterm.appdata.xml
  assets/arcterm.appdata.xml
  assets/flatpak
  assets/shell-completion
)

FOUND=0

echo "=== ArcTerm Rebrand Verification ==="
echo ""

# Check source and CI files
for f in "${SEARCH_PATHS[@]}"; do
  if [ -f "$f" ]; then
    # Search for wezterm references, excluding:
    # - Rust use/mod/crate imports (e.g., use wezterm_font::)
    # - Crate name references in Cargo paths
    # - Comments that reference upstream history (// ... wezterm ...)
    MATCHES=$(grep -inE '(wezterm|wezfurlong|wez\.wezterm)' "$f" \
      | grep -v '^\s*//' \
      | grep -v 'use wezterm' \
      | grep -v 'wezterm::' \
      | grep -v 'wezterm_' \
      | grep -v 'mod wezterm' \
      | grep -v 'crate::' \
      | grep -v 'package.*=.*"wezterm' \
      | grep -v 'wezterm-gui\|wezterm-font\|wezterm-mux\|wezterm-ssh\|wezterm-client\|wezterm-blob\|wezterm-cell\|wezterm-char\|wezterm-dynamic\|wezterm-escape\|wezterm-input\|wezterm-open\|wezterm-surface\|wezterm-toast\|wezterm-uds\|wezterm-version\|wezterm-gui-subcommands' \
      || true)
    if [ -n "$MATCHES" ]; then
      echo "FAIL: $f"
      echo "$MATCHES"
      echo ""
      FOUND=1
    fi
  fi
done

# Check asset directories/files
for f in "${ASSET_PATHS[@]}"; do
  if [ -e "$f" ]; then
    MATCHES=$(grep -rinE '(wezterm|wezfurlong|wez\.wezterm)' "$f" \
      | grep -v 'wezterm-gui\|wezterm-font\|wezterm-mux\|wezterm-ssh' \
      || true)
    if [ -n "$MATCHES" ]; then
      echo "FAIL: $f"
      echo "$MATCHES"
      echo ""
      FOUND=1
    fi
  fi
done

# Check if old WezTerm.app directory still exists
if [ -d "assets/macos/WezTerm.app" ]; then
  echo "FAIL: assets/macos/WezTerm.app directory still exists (should be ArcTerm.app)"
  FOUND=1
fi

if [ "$FOUND" -eq 0 ]; then
  echo "PASS: No remaining WezTerm references found in user-visible files."
  exit 0
else
  echo ""
  echo "FAIL: WezTerm references found. See above for details."
  exit 1
fi
