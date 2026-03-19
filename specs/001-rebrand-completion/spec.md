# Feature Specification: Complete ArcTerm Rebrand

**Feature Branch**: `001-rebrand-completion`
**Created**: 2026-03-18
**Status**: Complete
**Input**: User description: "Complete ArcTerm rebrand - fix update checker, CI pipelines, config file naming, and remaining WezTerm identity strings"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Safe Update Checking (Priority: P1)

A user running ArcTerm checks for updates. The terminal contacts ArcTerm's own release infrastructure and shows update availability for ArcTerm releases, not WezTerm releases. If no ArcTerm releases exist yet, the update checker indicates no updates are available rather than directing users to a different product.

**Why this priority**: The current update checker points at `wez/wezterm` GitHub releases. ArcTerm users would be told to "update" to WezTerm, which is confusing and could lead to data loss if they install a different binary over their ArcTerm config.

**Independent Test**: Trigger the update check and verify the HTTP request targets `lgbarn/arcterm` releases (or is gracefully disabled), and that no reference to `wez/wezterm` appears in the UI or network traffic.

**Acceptance Scenarios**:

1. **Given** ArcTerm is running, **When** the automatic update check fires, **Then** it queries the ArcTerm GitHub releases endpoint (not WezTerm's)
2. **Given** no ArcTerm releases exist on GitHub, **When** the update check runs, **Then** it displays "No updates available" rather than an error or WezTerm release info
3. **Given** a newer ArcTerm release exists, **When** the update check runs, **Then** the notification says "ArcTerm Update Available" with the correct version and a link to the ArcTerm release page

---

### User Story 2 - Safe CI/CD Pipelines (Priority: P1)

A maintainer tags a release on the ArcTerm repository. The CI/CD pipeline builds, packages, and publishes artifacts to ArcTerm-owned infrastructure only. No workflow step contacts upstream WezTerm accounts, Homebrew taps, Fury.io, or winget repositories belonging to Wez Furlong.

**Why this priority**: The current CI tag workflows would attempt to push to upstream WezTerm infrastructure if triggered. This could overwrite upstream releases or fail with credential errors, and represents a critical safety issue.

**Independent Test**: Review all CI workflow generation scripts and verify no references to upstream accounts remain. Trigger a dry-run or test tag and confirm no external pushes occur.

**Acceptance Scenarios**:

1. **Given** a release tag is pushed to `lgbarn/arcterm`, **When** CI workflows execute, **Then** all artifacts are published to ArcTerm-owned targets only
2. **Given** a release tag is pushed, **When** the Homebrew step runs, **Then** it updates an ArcTerm Homebrew tap (or is disabled), not `wez/homebrew-wezterm`
3. **Given** a release tag is pushed, **When** the package upload step runs, **Then** it pushes to ArcTerm's package registry (or is disabled), not `push.fury.io/wez/`
4. **Given** CI workflows are generated from `ci/generate-workflows.py`, **When** workflows are regenerated, **Then** the output contains only ArcTerm-specific targets

---

### User Story 3 - ArcTerm Config File (Priority: P2)

A user creates a configuration file for ArcTerm. The terminal searches for `arcterm.lua` as the primary config file name. For users migrating from WezTerm, the terminal also accepts `wezterm.lua` as a fallback but logs a deprecation notice suggesting they rename it.

**Why this priority**: Config file naming is the primary user-facing identity of the terminal. Users who set up ArcTerm fresh should use `arcterm.lua`. Migration from WezTerm should be smooth.

**Independent Test**: Place an `arcterm.lua` in the config directory and verify it loads. Place only a `wezterm.lua` and verify it loads with a deprecation notice. Place both and verify `arcterm.lua` takes precedence.

**Acceptance Scenarios**:

1. **Given** `arcterm.lua` exists in the config directory, **When** ArcTerm starts, **Then** it loads `arcterm.lua`
2. **Given** only `wezterm.lua` exists (no `arcterm.lua`), **When** ArcTerm starts, **Then** it loads `wezterm.lua` and logs a deprecation notice
3. **Given** both `arcterm.lua` and `wezterm.lua` exist, **When** ArcTerm starts, **Then** `arcterm.lua` takes precedence and `wezterm.lua` is ignored
4. **Given** neither config file exists, **When** ArcTerm starts, **Then** it uses default configuration without error

---

### User Story 4 - Consistent Brand Identity (Priority: P2)

A user interacts with ArcTerm across all platform surfaces — macOS app bundle, Windows installer, Linux desktop entry, shell completions, error messages, and documentation links. Every user-visible string identifies the application as "ArcTerm" with no references to "WezTerm" in normal usage.

**Why this priority**: Inconsistent branding confuses users, makes bug reports harder to triage, and undermines the fork's identity. All user-visible surfaces must be consistent.

**Independent Test**: Search all user-visible strings, asset files, and documentation links for "WezTerm" or "wezfurlong" and verify none remain (excluding internal crate names kept for upstream merge compatibility and code comments referencing upstream history).

**Acceptance Scenarios**:

1. **Given** ArcTerm is installed on macOS, **When** the user views the app in Finder, Dock, or Activity Monitor, **Then** it displays "ArcTerm" everywhere
2. **Given** ArcTerm encounters a font error, **When** the error message is displayed, **Then** any help links point to ArcTerm documentation (or are removed), not `wezterm.org`
3. **Given** a user installs ArcTerm on Linux, **When** they view the app in their desktop environment, **Then** the app name, icon identifier, and WM class all say "ArcTerm" / use ArcTerm identifiers
4. **Given** a user views shell completions for ArcTerm, **When** completion text references the application, **Then** it says "ArcTerm", not "WezTerm"

---

### User Story 5 - Funding and Community Identity (Priority: P3)

A user or contributor visits the ArcTerm GitHub repository. Funding links, issue templates, and community references point to ArcTerm's maintainer and community, not upstream WezTerm's.

**Why this priority**: Misdirected funding and contributor confusion undermine the fork's sustainability. Lower priority because it doesn't affect runtime behavior.

**Independent Test**: Review `.github/FUNDING.yml`, issue templates, and repository metadata for references to upstream accounts.

**Acceptance Scenarios**:

1. **Given** a user visits the ArcTerm GitHub repository, **When** they click "Sponsor", **Then** they see ArcTerm's funding links (or the sponsor button is disabled), not Wez Furlong's accounts
2. **Given** a contributor opens an issue, **When** they see issue templates, **Then** templates reference ArcTerm, not WezTerm

---

### Edge Cases

- What happens when a user has both `arcterm.lua` and `wezterm.lua` with conflicting settings? ArcTerm loads only `arcterm.lua` and ignores `wezterm.lua` entirely.
- What happens when CI secrets for ArcTerm's package registries are not configured? Deployment steps fail gracefully with clear error messages rather than silently falling back to upstream targets.
- What happens when upstream merges reintroduce WezTerm strings? The merge must be reviewed for re-introduced branding. A grep-based verification step should catch regressions.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The update checker MUST query `https://api.github.com/repos/lgbarn/arcterm/releases/latest` instead of the WezTerm releases endpoint
- **FR-002**: The HTTP User-Agent header MUST identify as `arcterm/<version>`, not `wezterm/wezterm-<version>`
- **FR-003**: All CI workflow generation scripts MUST produce workflows that publish only to ArcTerm-owned infrastructure
- **FR-004**: CI tag workflows MUST NOT reference `wez/homebrew-wezterm`, `push.fury.io/wez/`, `wez/winget-pkgs`, or any other upstream-owned target
- **FR-005**: The config loader MUST search for `arcterm.lua` as the primary config file name
- **FR-006**: The config loader MUST fall back to `wezterm.lua` if `arcterm.lua` is not found, logging a deprecation notice
- **FR-007**: The macOS app bundle MUST be renamed from `WezTerm.app` to `ArcTerm.app` with all `Info.plist` entries updated
- **FR-008**: The Windows installer MUST identify the publisher as the ArcTerm maintainer and link to ArcTerm URLs
- **FR-009**: The Windows VERSIONINFO MUST display "ArcTerm" as both FileDescription and ProductName
- **FR-010**: The Linux `.desktop` entry MUST use `Name=ArcTerm` and an ArcTerm-specific icon/WM class identifier
- **FR-011**: All font error messages MUST NOT link to `wezterm.org` documentation
- **FR-012**: The macOS quit dialog MUST say "Quit ArcTerm?" not "Terminate WezTerm?"
- **FR-013**: `.github/FUNDING.yml` MUST be updated or removed to reflect ArcTerm's maintainer
- **FR-014**: The `SshMultiplexing` enum MUST add an `ArcTerm` variant as the preferred value, with `WezTerm` retained as a deprecated alias for backward compatibility
- **FR-015**: Shell completion scripts MUST reference ArcTerm identifiers, not WezTerm/wezfurlong
- **FR-016**: Release notes templates and deploy scripts MUST reference ArcTerm URLs and artifact names

### Key Entities

- **Config File**: The user's terminal configuration (`arcterm.lua` or `wezterm.lua`), loaded at startup, determines all terminal behavior
- **Release Artifact**: Built binary packages (`.dmg`, `.deb`, `.rpm`, `.zip`, `.msi`) distributed to users, must carry ArcTerm identity
- **CI Workflow**: Generated from `ci/generate-workflows.py`, controls build/test/deploy pipeline

## Assumptions

- ArcTerm does not yet have its own Homebrew tap, Fury.io account, or winget package. CI deployment steps for these targets will be disabled (commented out or gated behind secrets) rather than pointed at new infrastructure that doesn't exist yet.
- The `lgbarn/arcterm` GitHub repository will host releases via GitHub Releases.
- Internal Objective-C class names (e.g., `WezTermAppDelegate`) will be renamed to ArcTerm equivalents. While these are not normally user-visible, they appear in crash reports and system logs.
- The AppStream/Flatpak app-id will change from `org.wezfurlong.wezterm` to a new ArcTerm identifier (e.g., `com.lgbarn.arcterm`).
- Documentation links currently pointing to `wezterm.org` will either be redirected to ArcTerm docs (if they exist) or removed with a TODO for when ArcTerm documentation is established.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A full-text search for "wezterm" (case-insensitive) across all user-visible strings, CI scripts, asset files, and deployment configs returns zero matches — excluding internal Rust crate names, import paths, and code comments referencing upstream history
- **SC-002**: The update checker successfully queries the ArcTerm releases endpoint and displays correct information (or gracefully reports no updates)
- **SC-003**: A simulated release tag push triggers CI workflows that reference only ArcTerm-owned targets — no network requests to upstream infrastructure
- **SC-004**: ArcTerm starts successfully with an `arcterm.lua` config file and loads it without fallback
- **SC-005**: ArcTerm starts with only a `wezterm.lua` present and logs a visible deprecation notice recommending rename to `arcterm.lua`
- **SC-006**: All existing tests continue to pass (`cargo test --all` green) after all rebrand changes
