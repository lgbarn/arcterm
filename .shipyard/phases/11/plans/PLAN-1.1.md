---
phase: config-runtime-hardening
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - scrollback_lines capped at 1,000,000 during config load
  - warning logged when value is clamped
  - cap applied in both load() and load_with_overlays()
  - regression test with extreme scrollback_lines value
files_touched:
  - arcterm-app/src/config.rs
tdd: true
---

# PLAN-1.1 — M-4: Scrollback Lines Config Cap

## Context

`scrollback_lines` (config.rs:32) is parsed from TOML with no upper-bound validation.
A value of `usize::MAX` would prevent the scrollback trimming loop in `grid.rs:208`
from ever firing, allowing unbounded memory growth. The fix is a `validate()` method
called at the end of both `load()` (line 175) and `load_with_overlays()` (line 258),
capping the value at `1_000_000` with a `log::warn!` when clamped.

The default value (10,000) and existing tests (`defaults_are_sensible`,
`toml_overrides_fields`) are unaffected -- they use values well below the cap.

## Tasks

<task id="1" files="arcterm-app/src/config.rs" tdd="true">
  <action>
  Write a test first: `scrollback_lines_capped_at_maximum`. Parse a TOML string with
  `scrollback_lines = 999_999_999_999` via `toml::from_str::<ArctermConfig>`. Call the
  new `validate()` method on the result. Assert `cfg.scrollback_lines == 1_000_000`.

  Then implement: add a private method `fn validate(mut self) -> Self` on `ArctermConfig`
  with a module-level constant `const MAX_SCROLLBACK_LINES: usize = 1_000_000`. If
  `self.scrollback_lines > MAX_SCROLLBACK_LINES`, log a warning with the original and
  capped values, then clamp to `MAX_SCROLLBACK_LINES`. Return `self`.

  Call `validate()` in two places:
  1. `load()` line 194: change `cfg` to `cfg.validate()`
  2. `load_with_overlays()` line 302: change `cfg` to `cfg.validate()`

  Add a second test: `scrollback_lines_below_cap_unchanged`. Parse TOML with
  `scrollback_lines = 500_000`, call `validate()`, assert value is unchanged at 500_000.
  </action>
  <verify>cargo test --package arcterm-app -- config::tests::scrollback_lines_capped_at_maximum config::tests::scrollback_lines_below_cap_unchanged --exact</verify>
  <done>Both new tests pass. Existing config tests (`defaults_are_sensible`, `toml_overrides_fields`) still pass. `cargo xc` is clean.</done>
</task>
