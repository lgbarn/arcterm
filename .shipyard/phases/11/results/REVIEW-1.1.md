---
plan: "1.1"
phase: config-runtime-hardening
reviewer: shipyard:reviewer
verdict: APPROVE
---

## Stage 1: Spec Compliance
**Verdict:** PASS

### Task 1: Add validate() cap at 1M, wire into both load paths, add two tests
- **Status:** PASS
- **Evidence:**
  - `MAX_SCROLLBACK_LINES: usize = 1_000_000` — module-level constant at `config.rs:149`.
  - `fn validate(mut self) -> Self` — private method on `ArctermConfig` at `config.rs:153–166`. Compares `self.scrollback_lines > MAX_SCROLLBACK_LINES`, emits `log::warn!` with original and capped values, clamps to `MAX_SCROLLBACK_LINES`, returns `self`.
  - `load()` wiring — `cfg.validate()` on the happy-path `Ok(cfg)` arm at `config.rs:215`.
  - `load_with_overlays()` wiring — `.validate()` chained after `.unwrap_or_default()` at `config.rs:321–322`.
  - `scrollback_lines_capped_at_maximum` test at `config.rs:694–700`: parses `scrollback_lines = 999999999999`, calls `validate()`, asserts `== 1_000_000`. Matches plan spec.
  - `scrollback_lines_below_cap_unchanged` test at `config.rs:702–708`: parses `scrollback_lines = 500000`, calls `validate()`, asserts `== 500_000`. Matches plan spec.
- **Notes:** Both required call sites are wired. Warning message includes original value and cap value as specified. Done criteria are fully satisfied.

---

## Stage 2: Code Quality
**Verdict:** APPROVE

### Critical
None.

### Important
None.

### Suggestions
- **Test TOML literal style** — `config.rs:695`: TOML integer `999999999999` lacks numeric underscores. The plan spelled it `999_999_999_999`; TOML supports `_` separators and they aid readability for large literals.
  - Remediation: change to `r#"scrollback_lines = 999_999_999_999"#` (and similarly `500_000` on line 703 for consistency).

---

## Summary
**Verdict:** APPROVE

Implementation matches the spec exactly: `validate()` caps at 1M, logs a warning with both values, and is called in both `load()` and `load_with_overlays()`. Both new tests are meaningful and test the correct behavior through the public API path.

Critical: 0 | Important: 0 | Suggestions: 1
