# SUMMARY-3.2: CI Pipeline

**Plan:** 3.2 — CI Pipeline
**Phase:** 1 (Foundation)
**Branch:** master
**Date:** 2026-03-15
**Status:** Complete

---

## Tasks Executed

### Task 1: GitHub Actions CI Workflow

**Commit:** `2daca03` — `shipyard(phase-1): add GitHub Actions CI pipeline`

Created `.github/workflows/ci.yml` with two jobs:

- **`check` job** — matrix across ubuntu-latest, macos-latest, windows-latest. Runs `cargo fmt`, `cargo clippy`, `cargo build`, and `cargo test` for the three non-GPU crates (arcterm-core, arcterm-vt, arcterm-pty). Uses `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, timeout 30 minutes.
- **`gpu-test` job** — ubuntu-latest only. Installs Mesa software rendering (libvulkan1, mesa-vulkan-drivers, libegl1-mesa-dev), sets `WGPU_BACKEND=vulkan`, and runs `cargo test --package arcterm-render --features gpu-tests` with `continue-on-error: true`.

**Additional change:** Added `[features] gpu-tests = []` to `arcterm-render/Cargo.toml` as the feature flag was not present.

**Verification:** File exists at `.github/workflows/ci.yml`; Python `yaml.safe_load` confirmed valid YAML.

---

### Task 2: Cargo Aliases and Dev Config

**Commit:** `fd05b8c` — `shipyard(phase-1): add cargo aliases for development`

Created `.cargo/config.toml` with three aliases:

| Alias | Expands To |
|-------|-----------|
| `cargo xt` | `cargo test --package arcterm-core --package arcterm-vt --package arcterm-pty` |
| `cargo xr` | `cargo run --package arcterm-app` |
| `cargo xc` | `cargo clippy --workspace --all-targets -- -D warnings` |

**Verification:** `cargo xt` executed successfully. All 98 tests passed:
- arcterm-core: 40 passed
- arcterm-pty: 6 passed
- arcterm-vt: 52 passed

---

## Deviations

None from the plan's intent. One minor addition: the `[features]` section added to `arcterm-render/Cargo.toml` was explicitly called for in the plan ("if not already present") — it was not present, so it was added.

---

## Final State

| File | Status |
|------|--------|
| `.github/workflows/ci.yml` | Created |
| `arcterm-render/Cargo.toml` | Updated (added `[features] gpu-tests = []`) |
| `.cargo/config.toml` | Created |

**Test suite baseline:** 98 tests passing across arcterm-core, arcterm-vt, arcterm-pty.
