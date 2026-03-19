# Implementation Plan: Complete ArcTerm Rebrand

**Branch**: `001-rebrand-completion` | **Date**: 2026-03-18 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-rebrand-completion/spec.md`

## Summary

Complete the ArcTerm rebrand by fixing the three critical issues (update checker
pointing at upstream, CI pipelines targeting upstream infrastructure, config file
still named `wezterm.lua`) and eliminating all remaining WezTerm identity strings
from user-visible surfaces, assets, and deployment scripts. ~25 files across
Rust source, CI scripts, platform assets, and metadata. No new crates or
architectural changes — this is surgical string replacement with one behavioral
addition (config file fallback with deprecation notice).

## Technical Context

**Language/Version**: Rust (edition 2021, min 1.71.0) + Python 3 (CI scripts) + Shell (deploy scripts)
**Primary Dependencies**: No new dependencies. Modifying existing code only.
**Storage**: N/A — configuration files on disk (read-only from our perspective)
**Testing**: `cargo test --all` via cargo-nextest. Grep-based verification for brand strings.
**Target Platform**: macOS, Linux, Windows (cross-platform desktop app)
**Project Type**: Desktop application (GPU-accelerated terminal emulator)
**Performance Goals**: N/A — no performance-sensitive changes
**Constraints**: All existing tests must continue to pass. Upstream merge compatibility preserved.
**Scale/Scope**: ~25 files modified, ~200 string replacements, 1 behavioral change (config fallback)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Upstream Compatibility | PASS | Modifications are to user-visible strings only. Internal crate names unchanged. Config change adds new paths alongside old ones. All changes are in files already touched by the initial rebrand. |
| II. Security by Default | PASS | No security-sensitive changes. Update checker URL change actually improves security (no longer reporting to wrong endpoint). CI changes prevent publishing to wrong infrastructure. |
| III. Local-First AI | N/A | No AI features in this change. |
| IV. Extension Isolation | PASS | SshMultiplexing::WezTerm retained as deprecated alias — no breaking change to Lua plugin API. |
| V. Test Preservation | PASS | All existing tests must pass. SC-006 explicitly requires `cargo test --all` green. |
| VI. Minimal Surface Area | PASS | No new features or abstractions. Config fallback is the minimum needed for migration. |
| Fork Management: CI/CD isolation | PASS | This is the primary goal — ensuring CI doesn't target upstream. |
| Fork Management: Config file | PASS | Adding `arcterm.lua` with `wezterm.lua` fallback per constitution. |

**Gate result: PASS** — no violations.

## Project Structure

### Documentation (this feature)

```text
specs/001-rebrand-completion/
├── plan.md              # This file
├── research.md          # Phase 0: file audit with line numbers
├── data-model.md        # Phase 1: config resolution order, enum changes, identity map
├── quickstart.md        # Phase 1: verification steps
├── checklists/
│   └── requirements.md  # Spec quality checklist
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

This feature modifies existing files only — no new source directories.

```text
# Rust source (behavioral changes)
config/src/config.rs           — Config file search order + deprecation notice
config/src/ssh.rs              — SshMultiplexing enum + deprecated alias
wezterm-gui/src/update.rs      — Update checker URLs + User-Agent

# Rust source (string replacements)
wezterm-font/src/lib.rs        — Font error message URLs
mux/src/localpane.rs           — Exit behavior hyperlink
window/src/os/macos/app.rs     — Quit dialog + ObjC class names
wezterm-toast-notification/src/macos.rs — ObjC notification class
wezterm-gui/build.rs           — Windows VERSIONINFO + macOS bundle path
strip-ansi-escapes/src/main.rs — Doc comment

# CI / deploy scripts
ci/generate-workflows.py       — Workflow generation (source of truth)
ci/deploy.sh                   — Package build + upload
ci/create-release.sh           — Release notes
ci/windows-installer.iss       — Windows installer
ci/check-rust-version.sh       — Error message URL

# Platform assets
assets/macos/WezTerm.app/ → assets/macos/ArcTerm.app/  — Bundle rename + Info.plist
assets/wezterm.desktop     — Linux desktop entry
assets/wezterm.appdata.xml — AppStream metadata
assets/flatpak/org.wezfurlong.wezterm.json              — Flatpak manifest
assets/flatpak/org.wezfurlong.wezterm.template.json      — Flatpak template
assets/flatpak/org.wezfurlong.wezterm.appdata.template.xml — Flatpak appdata
assets/shell-completion/{bash,zsh,fish}                  — Regenerated from CLI source

# GitHub metadata
.github/FUNDING.yml        — Funding links
```

**Structure Decision**: No new directories. All changes are modifications to
existing files in place, with the exception of the macOS bundle directory rename
(`WezTerm.app` → `ArcTerm.app`) and Flatpak file renames.

## Complexity Tracking

No constitution violations — this section is empty.
