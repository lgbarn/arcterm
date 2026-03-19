---
description: "Task list for Complete ArcTerm Rebrand"
---

# Tasks: Complete ArcTerm Rebrand

**Input**: Design documents from `/specs/001-rebrand-completion/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, quickstart.md

**Tests**: Not explicitly requested in the spec. Verification is via `cargo test --all` (existing tests) and grep-based brand string checks.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: No new project structure needed. This feature modifies existing files only. Setup phase creates the verification tooling.

- [x] T001 Create a rebrand verification script at `ci/verify-rebrand.sh` that greps for "wezterm" (case-insensitive) in user-visible files and exits non-zero if matches are found (excluding internal crate names, import paths, and upstream history comments)

**Checkpoint**: Verification script exists and can be run to detect remaining WezTerm strings.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: No foundational tasks. All user stories are independent string/config changes with no shared infrastructure dependencies.

**Checkpoint**: Proceed directly to user story phases.

---

## Phase 3: User Story 1 — Safe Update Checking (Priority: P1)

**Goal**: Update checker contacts ArcTerm's own releases, not WezTerm's.

**Independent Test**: Run ArcTerm with `ARCTERM_ALWAYS_SHOW_UPDATE_UI=1` and verify logs show `lgbarn/arcterm` URL, not `wezterm/wezterm`.

- [x] T002 [US1] Replace GitHub releases URL from `wezterm/wezterm` to `lgbarn/arcterm` for both latest and nightly endpoints in `wezterm-gui/src/update.rs` (lines 59, 64)
- [x] T003 [US1] Replace User-Agent header from `wezterm/wezterm-{version}` to `arcterm/{version}` in `wezterm-gui/src/update.rs` (line 44)
- [x] T004 [US1] Rename environment variable `WEZTERM_ALWAYS_SHOW_UPDATE_UI` to `ARCTERM_ALWAYS_SHOW_UPDATE_UI` in `wezterm-gui/src/update.rs` (lines 84, 157)
- [x] T005 [US1] Replace changelog URL from `wezterm.org/changelog.html` to a TODO placeholder or ArcTerm equivalent in `wezterm-gui/src/update.rs` (lines 95, 194)
- [x] T006 [US1] Verify `cargo test --all` passes after update checker changes

**Checkpoint**: Update checker targets ArcTerm releases. `cargo test --all` green.

---

## Phase 4: User Story 2 — Safe CI/CD Pipelines (Priority: P1)

**Goal**: CI tag workflows publish only to ArcTerm-owned targets. No pushes to upstream WezTerm infrastructure.

**Independent Test**: Run `python3 ci/generate-workflows.py` and grep generated `.yml` files for `wez/`, `push.fury.io`, `homebrew-wezterm`, `winget-pkgs` — all should return empty.

- [x] T007 [P] [US2] Update `ci/generate-workflows.py` to disable or redirect all Homebrew tap pushes (replace `wez/homebrew-wezterm` and `wez/homebrew-wezterm-linuxbrew` references with commented-out TODO blocks)
- [x] T008 [P] [US2] Update `ci/generate-workflows.py` to disable Fury.io package uploads (comment out `push.fury.io/wez/` references with TODO)
- [x] T009 [P] [US2] Update `ci/generate-workflows.py` to disable winget PR creation (comment out `wez/winget-pkgs` references with TODO)
- [x] T010 [P] [US2] Update `ci/generate-workflows.py` to disable Flatpak hub pushes (comment out `flathub/org.wezfurlong.wezterm` references with TODO)
- [x] T011 [US2] Update `ci/generate-workflows.py` to replace remaining upstream refs: artifact names (`WezTerm-*.zip` → `ArcTerm-*.zip`), safe directory path, and repository guard (`wezterm/wezterm` → `lgbarn/arcterm`)
- [x] T012 [P] [US2] Update `ci/deploy.sh` to replace packager identity (`Wez Furlong <wez@wezfurlong.org>` → ArcTerm maintainer), homepage URLs (`wezterm.org` → TODO), and artifact names throughout
- [x] T013 [P] [US2] Update `ci/create-release.sh` to replace all `wezterm.org` links in release notes template with ArcTerm equivalents or TODOs
- [x] T014 [P] [US2] Update `ci/check-rust-version.sh` to replace `wezterm.org/install/source.html` error message URL (line 22)
- [x] T015 [US2] Regenerate CI workflows by running `python3 ci/generate-workflows.py` and verify no upstream references in generated `.github/workflows/*.yml` files
- [x] T016 [US2] Verify `cargo test --all` passes after CI pipeline changes

**Checkpoint**: CI workflows contain no upstream targets. Generated `.yml` files verified clean.

---

## Phase 5: User Story 3 — ArcTerm Config File (Priority: P2)

**Goal**: ArcTerm searches for `arcterm.lua` as primary config, with `wezterm.lua` as deprecated fallback.

**Independent Test**: Start ArcTerm with `arcterm.lua` in config dir — loads it. Start with only `wezterm.lua` — loads it with deprecation log. Start with both — `arcterm.lua` wins.

- [x] T017 [US3] Add `arcterm.lua` as primary config file name in the config search path in `config/src/config.rs` — add entries for `~/.arcterm.lua`, `$XDG_CONFIG_HOME/arcterm/arcterm.lua`, and `<exe_dir>/arcterm.lua` before the existing `wezterm.lua` entries (around lines 1009-1025)
- [x] T018 [US3] Add `ARCTERM_CONFIG_FILE` and `ARCTERM_CONFIG_DIR` environment variable support as primary alternatives to `WEZTERM_CONFIG_FILE`/`WEZTERM_CONFIG_DIR` in `config/src/config.rs` (lines 1029-1062, 1133-1135)
- [x] T019 [US3] Add deprecation log notice when `wezterm.lua` or `WEZTERM_CONFIG_FILE` is used as fallback — log a warning like "Note: Using wezterm.lua for compatibility. Consider renaming to arcterm.lua" in `config/src/config.rs`
- [x] T020 [US3] Update runtime directory paths from `.join("wezterm")` to `.join("arcterm")` with fallback to `wezterm` directory in `config/src/config.rs` (lines 1742-1761)
- [x] T021 [US3] Verify `cargo test --all` passes after config file changes

**Checkpoint**: Config loading supports `arcterm.lua` primary with `wezterm.lua` fallback and deprecation notice. Tests green.

---

## Phase 6: User Story 4 — Consistent Brand Identity (Priority: P2)

**Goal**: All user-visible strings across all platforms say "ArcTerm" — no WezTerm references remain in assets, error messages, or platform metadata.

**Independent Test**: Run `ci/verify-rebrand.sh` — zero matches for WezTerm in user-visible files.

### macOS

- [x] T022 [P] [US4] Rename `assets/macos/WezTerm.app/` directory to `assets/macos/ArcTerm.app/` and update all 14+ `Info.plist` entries from WezTerm to ArcTerm (bundle ID → `com.lgbarn.arcterm`, display name, permission descriptions)
- [x] T023 [P] [US4] Replace macOS quit dialog strings in `window/src/os/macos/app.rs` — change "Terminate WezTerm?" to "Quit ArcTerm?" and update dialog body text (lines 30-31)
- [x] T024 [P] [US4] Rename ObjC class `WezTermAppDelegate` to `ArcTermAppDelegate` in `window/src/os/macos/app.rs` (line 16) and update associated selectors (lines 96, 138, 175-176)
- [x] T025 [P] [US4] Rename ObjC class `WezTermNotifDelegate` to `ArcTermNotifDelegate` in `wezterm-toast-notification/src/macos.rs` (line 36)

### Windows

- [x] T026 [P] [US4] Update Windows installer `ci/windows-installer.iss` — change `MyAppName` to "ArcTerm", `MyAppURL` to ArcTerm URL, `OutputBaseFilename` to `ArcTerm-Setup`, and all `AppUserModelID` and "Open WezTerm here" strings
- [x] T027 [P] [US4] Update Windows VERSIONINFO in `wezterm-gui/build.rs` — change FileDescription and ProductName to "ArcTerm" (lines 122, 127) and update macOS bundle path reference to `ArcTerm.app` (lines 168, 175)

### Linux

- [x] T028 [P] [US4] Update `assets/wezterm.desktop` — change Name to "ArcTerm", Comment, Icon to `com.lgbarn.arcterm`, StartupWMClass to `com.lgbarn.arcterm`, and Exec/TryExec paths. Rename file to `assets/arcterm.desktop`
- [x] T029 [P] [US4] Update `assets/wezterm.appdata.xml` — replace app-id, name, description, URLs, developer name. Rename file to `assets/arcterm.appdata.xml`
- [x] T030 [P] [US4] Update Flatpak files in `assets/flatpak/` — change app-id from `org.wezfurlong.wezterm` to `com.lgbarn.arcterm` in all three files (`*.json`, `*.template.json`, `*.appdata.template.xml`), update config path, icon/desktop/appdata install paths, and rename files

### Error Messages and Links

- [x] T031 [P] [US4] Replace font error URLs in `wezterm-font/src/lib.rs` — change `wezterm.org/config/fonts.html` to TODO placeholder or remove (lines 418, 852)
- [x] T032 [P] [US4] Replace exit behavior hyperlink in `mux/src/localpane.rs` — change `wezterm.org` OSC 8 link to TODO or remove (line 269)
- [x] T033 [P] [US4] Update doc comment in `strip-ansi-escapes/src/main.rs` — change "part of WezTerm" and GitHub URL (lines 11, 13)

### SSH and Lua API

- [x] T034 [US4] Add `SshMultiplexing::ArcTerm` variant in `config/src/ssh.rs` as the new default, keep `WezTerm` as deprecated serde alias (line 22). Update `Default` impl (line 29) and default config (line 129). Update doc comments to reference ArcTerm.

### Shell Completions

- [x] T035 [US4] Update the `--class` default value from `org.wezfurlong.wezterm` to `com.lgbarn.arcterm` in the CLI argument parser source (likely `wezterm-gui-subcommands/` or `wezterm-gui/src/main.rs`), then regenerate shell completions

### macOS ObjC Internals

- [x] T036 [P] [US4] Rename ObjC class names `WezTermWindow` and `WezTermWindowView` in `window/src/os/macos/window.rs` (lines 1821-1822) and menu class in `window/src/os/macos/menu.rs` (line 367)

- [x] T037 [US4] Run `ci/verify-rebrand.sh` to confirm zero WezTerm matches in user-visible files
- [x] T038 [US4] Verify `cargo test --all` passes after all brand identity changes

**Checkpoint**: All user-visible surfaces say "ArcTerm". Verification script confirms zero matches. Tests green.

---

## Phase 7: User Story 5 — Funding and Community Identity (Priority: P3)

**Goal**: GitHub repository metadata points to ArcTerm maintainer, not upstream.

**Independent Test**: Read `.github/FUNDING.yml` and confirm no references to `wez`, `WezFurlong`, or `wezfurlong`.

- [x] T039 [US5] Update or remove `.github/FUNDING.yml` — replace `github: wez`, `patreon: WezFurlong`, `ko_fi: wezfurlong`, `liberapay: wez` with `lgbarn` equivalents or remove the file entirely

**Checkpoint**: Funding links point to ArcTerm maintainer.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Final verification across all user stories.

- [x] T040 Run full `cargo test --all` to verify all existing tests pass
- [x] T041 Run `cargo fmt --all` to ensure formatting is clean
- [x] T042 Run `ci/verify-rebrand.sh` for final brand string verification
- [x] T043 Run `python3 ci/generate-workflows.py` and verify generated workflows are clean
- [x] T044 Update `specs/001-rebrand-completion/spec.md` status from "Draft" to "Complete"

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **User Story 1 (Phase 3)**: Depends on Setup — update checker is independent
- **User Story 2 (Phase 4)**: Depends on Setup — CI pipeline is independent
- **User Story 3 (Phase 5)**: Depends on Setup — config changes are independent
- **User Story 4 (Phase 6)**: Depends on Setup — brand identity is independent, but T035 (shell completions) depends on CLI source changes which should happen after T034 (SSH enum)
- **User Story 5 (Phase 7)**: Depends on Setup — funding is independent
- **Polish (Phase 8)**: Depends on ALL user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Independent. No dependencies on other stories.
- **User Story 2 (P1)**: Independent. No dependencies on other stories.
- **User Story 3 (P2)**: Independent. No dependencies on other stories.
- **User Story 4 (P2)**: T034 (SSH enum) should complete before T035 (shell completions regeneration). T022 (macOS bundle rename) should complete before T027 (build.rs bundle path).
- **User Story 5 (P3)**: Independent. No dependencies on other stories.

### Parallel Opportunities

User stories 1-5 can all proceed in parallel since they touch different files:
- US1: `wezterm-gui/src/update.rs`
- US2: `ci/generate-workflows.py`, `ci/deploy.sh`, `ci/create-release.sh`, `ci/check-rust-version.sh`
- US3: `config/src/config.rs`
- US4: `assets/`, `window/src/os/macos/`, `wezterm-font/`, `mux/`, `config/src/ssh.rs`, completions
- US5: `.github/FUNDING.yml`

Within US4 (largest story), all macOS/Windows/Linux/error-message tasks are parallelizable since they touch different files.

---

## Parallel Example: User Story 4

```bash
# Launch all platform tasks in parallel (different files):
Task: "T022 [P] [US4] Rename macOS app bundle + update Info.plist"
Task: "T023 [P] [US4] Replace macOS quit dialog strings"
Task: "T024 [P] [US4] Rename ObjC class WezTermAppDelegate"
Task: "T025 [P] [US4] Rename ObjC class WezTermNotifDelegate"
Task: "T026 [P] [US4] Update Windows installer"
Task: "T027 [P] [US4] Update Windows VERSIONINFO in build.rs"
Task: "T028 [P] [US4] Update Linux desktop entry"
Task: "T029 [P] [US4] Update AppStream metadata"
Task: "T030 [P] [US4] Update Flatpak files"
Task: "T031 [P] [US4] Replace font error URLs"
Task: "T032 [P] [US4] Replace exit behavior hyperlink"
Task: "T033 [P] [US4] Update strip-ansi-escapes doc comment"
Task: "T036 [P] [US4] Rename macOS ObjC window class names"

# Then sequential (has dependencies):
Task: "T034 [US4] Add SshMultiplexing::ArcTerm variant"
Task: "T035 [US4] Update CLI --class default and regenerate completions"
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 Only)

1. Complete Phase 1: Setup (verification script)
2. Complete Phase 3: User Story 1 (update checker) — **eliminates critical risk**
3. Complete Phase 4: User Story 2 (CI pipelines) — **eliminates critical risk**
4. **STOP and VALIDATE**: Run `cargo test --all` + `ci/verify-rebrand.sh`
5. Both critical safety issues resolved — safe to tag releases

### Incremental Delivery

1. US1 + US2 → Critical risks eliminated (safe releases)
2. Add US3 → Config file identity established
3. Add US4 → Full brand consistency across all platforms
4. Add US5 → Community identity complete
5. Each story adds value without breaking previous stories

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- CI workflows are generated from `ci/generate-workflows.py` — never edit `.yml` files directly
- Shell completions are auto-generated — fix at CLI argument parser source
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
