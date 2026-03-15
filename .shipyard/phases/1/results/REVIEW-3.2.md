# REVIEW-3.2: CI Pipeline

**Plan:** 3.2 — CI Pipeline
**Reviewer:** Claude Code (claude-sonnet-4-6)
**Date:** 2026-03-15
**Summary commit range:** `2daca03`, `fd05b8c`

---

## Stage 1: Spec Compliance
**Verdict:** FAIL

### Task 1: GitHub Actions CI Workflow
- Status: FAIL
- Evidence: `.github/workflows/ci.yml` exists and is structurally sound. The `check` job targets all three platforms (ubuntu-latest, macos-latest, windows-latest), includes all four required steps (fmt, clippy, build, test), uses the correct actions and flags, and has `timeout-minutes: 30`. The `gpu-test` job runs on ubuntu-latest only, installs Mesa packages, sets `WGPU_BACKEND=vulkan`, and runs `cargo test --package arcterm-render --features gpu-tests` with `continue-on-error: true`. The `[features] gpu-tests = []` entry is present in `arcterm-render/Cargo.toml`.

  **The single failing point:** the workflow file is missing the top-level `name:` field and — more critically — is missing a `fail-fast: false` directive on the matrix strategy. The plan does not require `fail-fast: false` explicitly, so that alone is not a spec deviation. However, the plan specifies:

  > **Trigger:** `on: [push, pull_request]` on all branches.

  The trigger is present (`on: [push, pull_request]`), satisfying the spec.

  **Actual spec deviation — missing `default` key in `[features]`:** The plan states:

  ```toml
  [features]
  default = []
  gpu-tests = []
  ```

  The implemented `arcterm-render/Cargo.toml` contains only:

  ```toml
  [features]
  gpu-tests = []
  ```

  The `default = []` line is absent. The plan text is unambiguous: the full `[features]` block from the spec includes `default = []`. Although omitting `default = []` is functionally equivalent to having it (Cargo assumes an empty default when the key is missing), the done criteria says "Add `[features]` section to `arcterm-render/Cargo.toml`" using the exact block shown, and the shown block includes `default = []`. This is a verbatim spec deviation.

- Notes: All other elements of Task 1 match the spec exactly, including action versions, component list, step names, env var, Mesa package list, and `continue-on-error` placement.

### Task 2: Cargo Aliases and Dev Config
- Status: FAIL
- Evidence: `.cargo/config.toml` was read at `/Users/lgbarn/Personal/myterm/.cargo/config.toml`. The file contains only an `[alias]` section with the three correct aliases (`xt`, `xr`, `xc`). The plan specifies an additional `[build]` section and two `[target.*]` sections:

  ```toml
  [build]
  jobs = 0

  [target.x86_64-apple-darwin]
  rustflags = ["-C", "link-arg=-fuse-ld=lld"]

  [target.aarch64-apple-darwin]
  # Default linker is fine on Apple Silicon
  ```

  None of these three sections are present in the implemented file. The `[build] jobs = 0` setting is the plan's mechanism for using all available CPU cores; the `[target.*]` blocks configure the linker per-platform. Both are part of the specified action for Task 2 and are absent.

- Notes: `cargo xt` was run and all 98 tests pass (arcterm-core: 40, arcterm-pty: 6, arcterm-vt: 52), confirming the alias itself is functional. `cargo xr` and `cargo xc` aliases are also correctly defined. The aliases portion of the task passes; the build config and target sections do not.

---

## Stage 2: Code Quality

Stage 2 is skipped because Stage 1 did not pass.

---

## Issues to Fix

The following items must be corrected before this plan can be marked complete.

**Fix 1 — Add `default = []` to `arcterm-render/Cargo.toml` `[features]` block**

File: `/Users/lgbarn/Personal/myterm/arcterm-render/Cargo.toml`

Current:
```toml
[features]
gpu-tests = []
```

Required by spec:
```toml
[features]
default = []
gpu-tests = []
```

**Fix 2 — Add `[build]` and `[target.*]` sections to `.cargo/config.toml`**

File: `/Users/lgbarn/Personal/myterm/.cargo/config.toml`

Current file contains only the `[alias]` section. The following sections are missing:

```toml
[build]
jobs = 0

[target.x86_64-apple-darwin]
rustflags = ["-C", "link-arg=-fuse-ld=lld"]

[target.aarch64-apple-darwin]
# Default linker is fine on Apple Silicon
```

These must be added above or below the existing `[alias]` block.

---

## Summary
**Verdict:** BLOCK

Two spec deviations prevent a pass: `.cargo/config.toml` is missing the `[build]` and `[target.*]` sections specified in Task 2's action block, and `arcterm-render/Cargo.toml` is missing the `default = []` line from the `[features]` block specified in Task 1. Both fixes are trivial to apply and have no functional risk. Re-review after corrections.

Critical: 0 | Important: 2 (spec deviations) | Suggestions: 0
