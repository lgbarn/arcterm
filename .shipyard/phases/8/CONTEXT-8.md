# Phase 8 Context — Design Decisions (autonomous defaults)

## Config Overlay System
- Overlay directory: ~/.config/arcterm/overlays/
- Pending overlays: written by AI tools via OSC 7770 type=config_overlay
- Leader+o: shows diff view (reuse palette overlay pattern)
- Accept/reject/edit: a=accept (moves to accepted/), x=reject (deletes), e=edit (opens in $EDITOR)
- Overlay stacking: base config → accepted overlays → workspace overlay

## Config Flatten
- `arcterm config flatten` CLI subcommand
- Reads base config, applies all accepted overlays in order, outputs resolved TOML to stdout

## Cross-Pane Search
- Leader+/ opens search overlay (regex input field)
- Searches across all pane output (scrollback + visible)
- Match highlighting rendered as colored quads
- Navigate matches with n/N (next/prev)
- Esc closes search

## Performance Targets
- Key-to-screen latency: <5ms (switch to PresentMode::Immediate for measurement)
- Cold start: <100ms (defer plugin loading, lazy syntect init)
- Memory: <50MB baseline, <60MB with 4 panes
- Frame rate: >120 FPS (already addressed in Phase 2)

## Release Packaging
- GitHub Actions release workflow with cross-compilation
- macOS: aarch64 + x86_64, code signing deferred
- Linux: x86_64 static binary
- Windows: x86_64 binary + installer deferred
- cargo-dist or manual cross-compile matrix

## Documentation
- man page via clap_mangen
- --help completeness (already via clap derive)
- Example configs in examples/
- Plugin authoring guide in docs/
