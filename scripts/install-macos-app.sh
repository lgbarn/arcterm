#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP="/Applications/ArcTerm.app"
BINARY="$PROJECT_DIR/target/release/wezterm-gui"
ASSETS="$PROJECT_DIR/assets/macos/ArcTerm.app"

# Verify release binary exists
if [ ! -f "$BINARY" ]; then
    echo "Release binary not found. Building..."
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"
fi

echo "Installing ArcTerm.app to /Applications..."

# Remove old bundle if present
sudo rm -rf "$APP"

# Create bundle structure
sudo mkdir -p "$APP/Contents/MacOS"
sudo mkdir -p "$APP/Contents/Resources"

# Copy Info.plist
sudo cp "$ASSETS/Contents/Info.plist" "$APP/Contents/"

# Copy icon
sudo cp "$ASSETS/Contents/Resources/arcterm.icns" "$APP/Contents/Resources/"

# Copy binary
sudo cp "$BINARY" "$APP/Contents/MacOS/wezterm-gui"

# Also install CLI binary to /usr/local/bin
sudo cp "$PROJECT_DIR/target/release/wezterm-gui" /usr/local/bin/arcterm
sudo cp "$PROJECT_DIR/target/release/wezterm" /usr/local/bin/wezterm

# Reset Launch Services so the icon and bundle register properly
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" 2>/dev/null || true

echo "Done. ArcTerm.app installed to /Applications/"
echo "CLI binaries installed to /usr/local/bin/{arcterm,wezterm}"
