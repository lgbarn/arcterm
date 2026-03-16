---
plan: "1.1"
phase: config-runtime-hardening
status: complete
commit: a24c135
---

# SUMMARY-1.1 — M-4: Scrollback Lines Config Cap

## What Was Done

Added `scrollback_lines` validation to `ArctermConfig` in `arcterm-app/src/config.rs`:

- **`MAX_SCROLLBACK_LINES: usize = 1_000_000`** — module-level constant defining the cap.
- **`fn validate(mut self) -> Self`** — private method that clamps `scrollback_lines` and emits `log::warn!` with original and capped values if exceeded.
- Wired `validate()` into:
  - `load()` — `cfg.validate()` on the happy-path `Ok(cfg)` arm.
  - `load_with_overlays()` — `.validate()` chained after `.unwrap_or_default()`.

## Tests Added

| Test | Result |
|---|---|
| `scrollback_lines_capped_at_maximum` | PASS — `999_999_999_999` clamped to `1_000_000` |
| `scrollback_lines_below_cap_unchanged` | PASS — `500_000` unchanged |

All 22 existing config tests continue to pass. Clippy is clean (`-D warnings`).

## Deviations

None. Implementation matches the plan exactly.

## Note on Pre-existing Broken State

During initial test runs, `arcterm-render/src/gpu.rs` and `arcterm-render/src/renderer.rs` showed unstaged modifications that broke compilation (return type of `GpuState::new` changed to `Result<Self, String>` without updating callers). These were pre-existing workspace changes unrelated to this plan. After `git stash pop` the files resolved to their committed clean state, allowing compilation and test execution to proceed normally.
