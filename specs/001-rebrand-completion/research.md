# Research: Complete ArcTerm Rebrand

**Date**: 2026-03-18
**Feature**: 001-rebrand-completion

## Research Summary

Full audit of all files containing user-visible WezTerm references. Findings
organized by change category with file paths and line numbers.

## Decision 1: Update Checker Strategy

**Decision**: Point update checker at `lgbarn/arcterm` GitHub releases. If no
releases exist, gracefully report "No updates available."

**Rationale**: The update checker is live code that makes HTTP requests. It must
target the correct repository immediately. No intermediate "disable" step is
needed — the GitHub API returns a 404 for missing releases, which the existing
error handling already covers.

**Alternatives considered**:
- Disable update checker entirely → Loses a useful feature, requires re-enabling later
- Point at a dummy URL → Adds unnecessary complexity

## Decision 2: CI Pipeline Strategy

**Decision**: Disable (comment out) all deployment steps that push to external
registries (Homebrew, Fury.io, winget, Flatpak). Gate re-enablement on ArcTerm
having its own accounts. Changes must be made in `ci/generate-workflows.py`
(the source of truth), not the generated `.yml` files.

**Rationale**: ArcTerm has no Homebrew tap, Fury account, or winget package yet.
Removing the code entirely would make it harder to re-enable. Commenting it out
with `# TODO(arcterm): re-enable when ArcTerm packaging accounts exist` is safer.

**Alternatives considered**:
- Remove deployment code entirely → Harder to restore
- Create ArcTerm accounts first → Blocks this work on external setup

## Decision 3: Config File Naming

**Decision**: Add `arcterm.lua` as the primary config file name. Search order:
1. `ARCTERM_CONFIG_FILE` env var (new, primary)
2. `arcterm.lua` in config directories
3. `WEZTERM_CONFIG_FILE` env var (fallback, deprecated)
4. `wezterm.lua` in config directories (fallback, deprecated)

Log a deprecation notice when fallback paths are used.

**Rationale**: Clean migration path. New users get `arcterm.lua`. WezTerm
migrants don't need to rename immediately. Constitution requires `arcterm.lua`
primary with `wezterm.lua` fallback.

**Alternatives considered**:
- Only support `arcterm.lua` → Breaks migration from WezTerm
- Symlink approach → Platform-dependent, fragile

## Decision 4: SshMultiplexing Enum

**Decision**: Add `SshMultiplexing::ArcTerm` variant. Keep `WezTerm` as a
deprecated alias via serde rename. Default to `ArcTerm`.

**Rationale**: This is a public Lua API surface. Removing `WezTerm` breaks
existing configs. Adding `ArcTerm` as the new name with `WezTerm` as alias
preserves backward compatibility per constitution Principle IV.

## Decision 5: Documentation Links

**Decision**: Replace `wezterm.org` links with a placeholder TODO comment
for now. ArcTerm does not have its own documentation site yet.

**Rationale**: Linking to upstream docs is confusing for ArcTerm users.
Removing links with a TODO is honest. When ArcTerm docs exist, the TODOs
provide a searchable list of places to update.

**Alternatives considered**:
- Keep wezterm.org links → Confusing brand identity
- Create ArcTerm docs first → Blocks this work

## Decision 6: macOS App Bundle Rename

**Decision**: Rename `assets/macos/WezTerm.app/` to `assets/macos/ArcTerm.app/`.
Update all references in `ci/deploy.sh`, `wezterm-gui/build.rs`, and
`ci/generate-workflows.py`.

**Rationale**: The bundle name is the macOS identity. All CI scripts reference
this path so they must be updated in lockstep.

## Decision 7: Flatpak/AppStream Identity

**Decision**: Change app-id from `org.wezfurlong.wezterm` to `com.lgbarn.arcterm`.
Rename all Flatpak JSON/XML files accordingly.

**Rationale**: The Flatpak app-id is a global identifier. Using the upstream
author's reverse-DNS would conflict if both apps are installed.

## Decision 8: Shell Completions

**Decision**: Fix the `--class` default in the CLI argument parser source code.
Regenerate completion files. The completion files themselves are auto-generated
and should not be hand-edited.

**Rationale**: Shell completions embed the `org.wezfurlong.wezterm` default
class from CLI arg definitions. Fixing at the source propagates correctly.

## File Impact Audit

### Critical (blocks safe releases)

| File | Changes | FR |
|------|---------|----|
| `wezterm-gui/src/update.rs` | URLs, User-Agent, env var | FR-001, FR-002 |
| `ci/generate-workflows.py` | All upstream refs (~30 occurrences) | FR-003, FR-004 |
| `ci/deploy.sh` | Packager, URLs, artifact names (~60 occurrences) | FR-016 |
| `ci/create-release.sh` | Release note links | FR-016 |
| `.github/FUNDING.yml` | All four funding links | FR-013 |

### High (user-visible branding)

| File | Changes | FR |
|------|---------|----|
| `config/src/config.rs` | Config file names, env vars, paths | FR-005, FR-006 |
| `assets/macos/WezTerm.app/Contents/Info.plist` | 14 string replacements | FR-007 |
| `window/src/os/macos/app.rs` | Quit dialog, ObjC class names | FR-012 |
| `ci/windows-installer.iss` | Publisher, URLs, shell extension text | FR-008 |
| `wezterm-gui/build.rs` | VERSIONINFO, bundle path | FR-009 |
| `assets/wezterm.desktop` | Name, icon, WM class | FR-010 |
| `wezterm-font/src/lib.rs` | Error message URLs | FR-011 |
| `config/src/ssh.rs` | SshMultiplexing enum | FR-014 |
| `assets/shell-completion/*` | Auto-generated, fix at CLI source | FR-015 |

### Medium (metadata and packaging)

| File | Changes | FR |
|------|---------|----|
| `assets/wezterm.appdata.xml` | App store metadata | FR-010 |
| `assets/flatpak/org.wezfurlong.wezterm.json` | Flatpak manifest | FR-010 |
| `assets/flatpak/org.wezfurlong.wezterm.template.json` | Flatpak template | FR-010 |
| `assets/flatpak/org.wezfurlong.wezterm.appdata.template.xml` | Flatpak appdata | FR-010 |
| `wezterm-toast-notification/src/macos.rs` | ObjC class name | FR-012 |
| `mux/src/localpane.rs` | Exit behavior hyperlink | FR-011 |
| `ci/check-rust-version.sh` | Error message URL | FR-016 |

### Low (internal / comments)

| File | Changes | FR |
|------|---------|----|
| `strip-ansi-escapes/src/main.rs` | Doc comment | FR-016 |
