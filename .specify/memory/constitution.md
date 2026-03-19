<!--
  Sync Impact Report
  ==================
  Version change: 0.0.0 → 1.0.0 (initial ratification)
  Modified principles: N/A (first version)
  Added sections:
    - Core Principles (6 principles)
    - Fork Management Constraints
    - Development Workflow
    - Governance
  Removed sections: N/A
  Templates requiring updates:
    - .specify/templates/plan-template.md — ✅ no changes needed
      (Constitution Check section already references constitution generically)
    - .specify/templates/spec-template.md — ✅ no changes needed
      (user stories and requirements structure is compatible)
    - .specify/templates/tasks-template.md — ✅ no changes needed
      (phase structure and parallel markers are compatible)
  Follow-up TODOs: None
-->

# ArcTerm Constitution

## Core Principles

### I. Upstream Compatibility

All ArcTerm-specific code MUST live in dedicated `arcterm-*` crates or
clearly isolated modules. Internal WezTerm crate names (`wezterm-gui`,
`wezterm-font`, `term`, `mux`, etc.) MUST NOT be renamed. Modifications
to upstream files MUST be minimal and surgical — prefer wrapping over
patching. This ensures `git merge upstream/main` remains tractable.

**Rationale:** ArcTerm's value comes from features built *on top of*
WezTerm, not from diverging the base. Every upstream conflict is a tax
on future development.

### II. Security by Default

WASM plugins MUST run in a capability-based sandbox. No plugin receives
filesystem, network, or terminal I/O access unless the user explicitly
grants it. AI features MUST NOT send terminal content to remote APIs
without explicit user opt-in. Local inference (Ollama) MUST be the
default path. Credential material (SSH keys, API tokens) MUST never be
exposed to plugin or AI subsystems.

**Rationale:** A terminal emulator sees everything — passwords, tokens,
production commands. The security posture must be paranoid by default.

### III. Local-First AI

The AI integration layer MUST work fully offline with Ollama as the
default backend. Remote API support (Claude, etc.) is opt-in via
explicit configuration. AI features MUST degrade gracefully when no
LLM endpoint is available — the terminal MUST remain fully functional
without AI. Zero configuration MUST be the default: if Ollama runs on
`localhost:11434`, AI features work immediately.

**Rationale:** A terminal that requires cloud connectivity to function
is a liability. Local-first ensures privacy, reliability, and instant
responsiveness.

### IV. Extension Isolation

WASM plugins and Lua plugins MUST coexist without interference. Each
plugin system MUST have its own loading path, lifecycle, and error
boundary. A crashing or misbehaving plugin MUST NOT take down the
terminal or affect other plugins. Plugin APIs MUST be versioned and
backward-compatible within a major version.

**Rationale:** Two plugin systems add complexity. Strict isolation
prevents that complexity from becoming instability.

### V. Test Preservation

All existing WezTerm tests MUST continue to pass after any modification.
`cargo test --all` MUST be green before any commit to `main`. New
ArcTerm-specific features MUST include tests in their respective
`arcterm-*` crates. Terminal emulation correctness (VT parsing, escape
sequences) MUST NOT regress — the `TestTerm` harness is the authority.

**Rationale:** WezTerm's test suite is the proof that the terminal
works. Breaking it means breaking the terminal.

### VI. Minimal Surface Area

New features MUST solve a specific, stated problem. Abstractions MUST
NOT be introduced for hypothetical future use. Configuration options
MUST have sensible defaults — users MUST NOT be forced to configure
anything to get a working terminal. When choosing between "flexible
but complex" and "opinionated but simple," prefer simple.

**Rationale:** Terminal emulators are infrastructure. Users want them
to work, not to configure them. Every option is a maintenance burden.

## Fork Management Constraints

- **Upstream remote**: `upstream` → `wez/wezterm`
- **Origin remote**: `origin` → `lgbarn/arcterm`
- **Merge cadence**: Periodically merge upstream via
  `git fetch upstream && git merge upstream/main`
- **User-facing rebrand**: All user-visible strings MUST say "ArcTerm"
  (env vars, dialogs, menus, app IDs, documentation)
- **Internal naming**: Crate names, module paths, and internal
  identifiers retain WezTerm naming to minimize merge conflicts
- **CI/CD isolation**: Release workflows MUST NOT publish to upstream
  WezTerm accounts, repositories, or package registries
- **Config file**: ArcTerm MUST search for `arcterm.lua` (with
  `wezterm.lua` as a fallback for migration)

## Development Workflow

- **Build verification**: `cargo check --package wezterm-gui` for fast
  iteration; `cargo build --release` before merging
- **Test gate**: `cargo test --all` MUST pass before any merge to `main`
- **Formatting**: `cargo fmt --all` MUST produce no changes before merge
- **Commit discipline**: Atomic commits per task; each commit MUST
  compile and pass tests independently
- **Branch strategy**: Feature branches off `main`; PRs required for
  non-trivial changes
- **New crate convention**: ArcTerm-specific crates use the `arcterm-`
  prefix (e.g., `arcterm-wasm-plugin`, `arcterm-ai`)

## Governance

This constitution is the authoritative source for ArcTerm development
principles. All code contributions, design decisions, and architectural
choices MUST be consistent with these principles.

- **Amendments** require updating this document, incrementing the
  version, and recording the change in the Sync Impact Report
- **Versioning** follows semantic versioning: MAJOR for principle
  removal/redefinition, MINOR for new principles or material expansion,
  PATCH for clarifications and wording
- **Compliance review**: Each plan MUST include a Constitution Check
  gate verifying alignment with these principles before implementation
  begins
- **Conflict resolution**: When upstream WezTerm practices conflict
  with ArcTerm principles, ArcTerm principles govern for `arcterm-*`
  crates; upstream conventions govern for modified upstream files

**Version**: 1.0.0 | **Ratified**: 2026-03-18 | **Last Amended**: 2026-03-18
