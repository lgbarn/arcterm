# Phase 2 Plan Critique
**Phase:** Terminal Fidelity and Configuration
**Date:** 2026-03-15
**Type:** plan-review (pre-execution feasibility stress test)

---

## Overview

This critique evaluates 7 plans across 3 waves (PLAN-1.1 through PLAN-3.2) filed in the Phase 2 directory. All plans are marked `phase: terminal-fidelity`, which aligns with Phase 2 per ROADMAP.md. **CRITICAL ISSUE:** Plans PLAN-3.1 and PLAN-3.2 (wave 3) are included in Phase 2 but declare dependencies on PLAN-2.x and appear logically to belong in Phase 3 (Multiplexer). The critique below treats them as Phase 2 plans per their location, but recommends relocation.

---

## Phase 2 Success Criteria vs. Plan Coverage

| Criterion | Plan(s) | Status |
|-----------|---------|--------|
| 1. Passes 90%+ of vttest basic and cursor movement tests | PLAN-1.1, PLAN-1.2 | Addressed in wave 1 |
| 2. 256-color and truecolor (24-bit) rendering works correctly | PLAN-2.1, PLAN-3.1 | Addressed (PLAN-3.1 in wave 3) |
| 3. Neovim, tmux, and SSH sessions render without visual artifacts | PLAN-1.1, PLAN-1.2, PLAN-3.2 | Addressed (PLAN-3.2 in wave 3) |
| 4. Scrollback buffer supports 10,000+ lines with smooth GPU-accelerated scroll | PLAN-1.1, PLAN-2.3 | Addressed in waves 1–2 |
| 5. `~/.config/arcterm/config.toml` controls font family, font size, color scheme, keybindings, and shell path | PLAN-2.2, PLAN-3.1 | Addressed (color scheme in PLAN-3.1 wave 3) |
| 6. Selection and clipboard (copy/paste) work via mouse drag and keyboard shortcuts | PLAN-2.3 | Addressed in wave 2 |
| 7. Frame rate exceeds 120 FPS during fast cat-of-large-file output | PLAN-2.1, PLAN-3.2 | Addressed (optimization in PLAN-3.2 wave 3) |

**Verdict:** All 7 success criteria are covered across the 7 plans. However, 2 criteria (#2 color schemes and #3 artifact-free vim/tmux) require PLAN-3.x (wave 3), which extends Phase 2 beyond typical sprint scope.

---

## Per-Plan Findings

### PLAN-1.1: Scrollback Buffer, Scroll Regions, and Grid Mode State
**Wave:** 1 | **Dependencies:** None
**Files:** arcterm-core/src/{grid.rs, cell.rs, lib.rs}

**Assessment:**
- ✓ Files exist
- ✓ API surface defined: Grid receives `scrollback: VecDeque`, `scroll_region`, `modes: TermModes`, `scroll_offset`, `rows_for_viewport()`
- ✓ 3 tasks with clear TDD-first approach (tests written before implementation)
- ✓ No file conflicts
- ✓ Core foundational plan; no hidden interdependencies

**Risk:** HIGH
- **Why:** `rows_for_viewport()` is foundational for 4 downstream plans (PLAN-2.1, PLAN-2.3, PLAN-3.2). Implementation complexity is non-trivial: integrating VecDeque scrollback with viewport offset and alt-screen buffer requires careful pointer/slice semantics.
- **Verification command feasibility:** PASS. `cargo test --package arcterm-core -- --test-threads=1` is executable now.

**Recommendation:** READY. TDD approach mitigates risk. Suggest blocking until all Task 1–3 tests pass before advancing to wave 2.

---

### PLAN-1.2: DEC Private Modes and Extended VT Sequences
**Wave:** 1 | **Dependencies:** None
**Files:** arcterm-vt/src/{processor.rs, handler.rs, lib.rs}

**Assessment:**
- ✓ Files exist
- ✓ Handler trait well-specified: 16 new method signatures with clear VT sequence→method mapping
- ✓ No TDD requirement (appropriate for adapter/dispatcher layer)
- ✓ No file conflicts with PLAN-1.1 (different crate)
- ✓ Implicit dependency on PLAN-1.1 at runtime (processor dispatches to handler which calls Grid methods), but no file-level coupling

**Risk:** MEDIUM
- **Why:** DEC private mode dispatch requires careful parsing of intermediates parameter. The specification says "check if `intermediates` contains `0x3F`", but doesn't clarify behavior if multiple intermediates are present. Test coverage is critical.
- **Verification command feasibility:** PASS. `cargo test --package arcterm-vt` is executable.

**Recommendation:** READY. Suggest adding tests for edge cases: empty params, multi-param modes, unimplemented modes.

---

### PLAN-2.1: Background Color Rendering and Dirty-Row Optimization
**Wave:** 2 | **Dependencies:** ["1.1"]
**Files:** arcterm-render/src/{text.rs, renderer.rs, gpu.rs, lib.rs, quad.rs (NEW)}, Cargo.toml

**Assessment:**
- ✓ 4 existing files, 1 new file
- ✓ API surface clear: `QuadRenderer` with `prepare()` and `render()` methods
- ✓ Cargo.toml already exists; bytemuck dependency to be added
- ✓ 3 tasks, no TDD requirement (GPU code is integration-tested via build + visual inspection)
- ✗ **CRITICAL:** Hard dependency on `Grid::rows_for_viewport()` from PLAN-1.1. Task 2 says "use `grid.rows_for_viewport()` if the method exists, otherwise fall back to `grid.rows()`". This fallback is problematic: it means the dirty-row optimization won't apply to scrollback views, reducing the 120 FPS target.

**Risk:** HIGH
- **Why:** Dependency on PLAN-1.1's viewport logic is tight. If PLAN-1.1 fails to implement `rows_for_viewport()` correctly, PLAN-2.1's renderer will silently degrade. The fallback doesn't fail loudly.
- **Verification command feasibility:** PASS. `cargo build --package arcterm-render` will catch missing methods at compile time (not fallback gracefully at runtime).

**File conflict with PLAN-2.2:** Both plans modify `arcterm-render/src/renderer.rs`. PLAN-2.1 modifies `render_frame()` to add quad rendering. PLAN-2.2 modifies `Renderer::new()` to parameterize font_size. The file overlap note in PLAN-2.2 says these are "distinct code regions". **Recommend PLAN-2.1 executes first, then PLAN-2.2 adapts.**

**Recommendation:** READY WITH CAUTION. Execution order: PLAN-2.1 must complete before PLAN-2.2. Remove the fallback in Task 2; require `rows_for_viewport()` to exist (fail fast). Add a compile-time assertion or test that renderer calls the viewport method on a grid with scroll_offset > 0.

---

### PLAN-2.2: TOML Configuration System with Hot-Reload
**Wave:** 2 | **Dependencies:** ["1.1"]
**Files:** arcterm-app/{Cargo.toml, src/config.rs (NEW), src/main.rs}, arcterm-render/src/renderer.rs

**Assessment:**
- ✓ New config.rs module is self-contained
- ✓ Cargo.toml changes are additive: `toml`, `serde`, `dirs`, `notify`
- ✓ 3 tasks with TDD for config parsing (Task 1)
- ✓ Dependency on PLAN-1.1 is loose: only uses `max_scrollback` field. If field doesn't exist, defaults apply.
- ✓ Hot-reload via notify is well-scoped

**Risk:** LOW
- **Why:** Config module is isolated. File overlap with PLAN-2.1 is minor (both touch renderer.rs but distinct regions per plan note). notify + file watcher complexity is manageable.
- **Verification command feasibility:** PASS. `cargo test --package arcterm-app -- config` is executable.

**API Surface Match:** Task 2 requires modifying `Renderer::new()` signature to accept `font_size: f32`. Current signature is `pub fn new(window: Arc<Window>) -> Self`. Change is backward-compatible if font_size has a default, but PLAN-2.2 does not mention a default; it says "remove the FONT_SIZE constant or keep it as a fallback". **Unclear if fallback is intended.** Recommend clarification.

**Hidden Dependency:** PtySession::new does not accept a shell parameter in the current codebase. PLAN-2.2 Task 2 step 3 says "modify Terminal::new and PtySession::new to accept an optional shell path override". **PtySession::new currently hardcodes shell detection via `std::env::var("SHELL")`**. This is a file modification in arcterm-pty/src/session.rs not listed in files_touched. **FLAG: Missing file.** Recommend adding `arcterm-pty/src/session.rs` to files_touched.

**Recommendation:** CAUTION. Clarify:
1. Does `Renderer::new()` font_size parameter have a default, or is FONT_SIZE constant required as fallback?
2. Acknowledge that `arcterm-pty/src/session.rs` is modified (currently unlisted).
3. Update files_touched to include `arcterm-pty/src/session.rs`.

---

### PLAN-2.3: Mouse Events, Text Selection, Clipboard, and Scroll Viewport
**Wave:** 2 | **Dependencies:** ["1.1", "1.2"]
**Files:** arcterm-app/{Cargo.toml, src/main.rs, src/input.rs, src/selection.rs (NEW), src/terminal.rs}

**Assessment:**
- ✓ New selection.rs module is well-specified with CellPos, SelectionMode, Selection, Clipboard
- ✓ Cargo.toml adds `arboard = "3"` for clipboard (new dependency)
- ✓ 3 tasks with TDD for selection model (Task 1)
- ✓ Dependencies on PLAN-1.1 (scroll_offset) and PLAN-1.2 (mouse reporting mode flags) are reasonable
- ✓ No file conflicts with PLAN-2.1 or PLAN-2.2

**Risk:** MEDIUM
- **Why:** Clipboard lifetime management via arboard. Research note in Task 1 step 8 says "arboard must live for app lifetime per research finding #5" — this is a design constraint. If arboard is dropped prematurely, clipboard operations fail. Ensure Clipboard instance is stored in AppState and never dropped until app exit.
- **Verification command feasibility:** PASS. `cargo test --package arcterm-app -- selection` is executable.

**API Surface:** Task 1 defines `pixel_to_cell()` and `word_boundaries()` as module-level functions. Task 2 references `grid.rows_for_viewport()` (from PLAN-1.1). Task 3 generates selection quads and passes to QuadRenderer — **explicit dependency on PLAN-2.1's quad pipeline**. If quads are not rendered, selection overlay won't display.

**Note on Grid mutation:** Task 3 adds `grid_mut(&mut self) -> &mut Grid` method to Terminal. This method is not currently in terminal.rs; it will be added. Also adds `set_scroll_offset()` method. Both are new APIs.

**Recommendation:** READY. Execution order must respect PLAN-2.1 (quad pipeline exists before selection quads are generated). Clarify in Task 1 that arboard::Clipboard must be stored in AppState with static lifetime (dropped only at app exit).

---

### PLAN-3.1: Color Schemes (Built-in and Custom)
**Wave:** 3 | **Dependencies:** ["2.1", "2.2"]
**Files:** arcterm-app/{src/colors.rs (NEW), src/config.rs, src/main.rs}, arcterm-render/{src/text.rs, src/renderer.rs}

**Assessment:**
- ✓ New colors.rs module is self-contained (8 color schemes, palette management)
- ✓ Dependencies on PLAN-2.1 (quad renderer must exist to color quads) and PLAN-2.2 (config module must exist to read color_scheme field) are clear
- ✓ TDD for color module (Step 1 says "Write tests FIRST")
- ✗ **CRITICAL ISSUE:** This plan is located in phase/2/plans but declares wave 3 and extends Phase 2 execution by a full wave. Per ROADMAP.md, Phase 2 is "Terminal Fidelity and Configuration" with success criterion #5 stating "`~/.config/arcterm/config.toml` controls... color scheme". This plan fulfills that criterion but is placed in wave 3, deferring color scheme functionality to the final wave of Phase 2.

**Risk:** MEDIUM
- **Why:** Wave 3 placement delays a success criterion (color scheme) to the end of Phase 2. If phase closure is time-boxed, wave 3 may not complete, leaving the phase incomplete. Recommend moving to wave 2 if scope allows, or clarifying phase boundary.

**Recommendation:** CAUTION. Recommend:
1. Confirm phase 2 scope closure includes wave 3 completion.
2. Consider moving to wave 2 if critical for phase shippability.
3. Verify that generic `Color::Default` rendering is adequate until color schemes are added (e.g., vim should render with a neutral palette in wave 2, then beautiful palettes in wave 3).

---

### PLAN-3.2: Performance Optimization and Integration Verification
**Wave:** 3 | **Dependencies:** ["2.1", "2.3"]
**Files:** arcterm-render/src/gpu.rs, arcterm-app/{src/main.rs, src/input.rs, src/terminal.rs}

**Assessment:**
- ✓ Dependencies on PLAN-2.1 (quad pipeline + dirty-row optimization required for 120 FPS) and PLAN-2.3 (mouse/selection handling) are clear
- ✓ 5 tasks targeting performance, app-cursor-keys mode, and bracketed-paste integration
- ✗ **CRITICAL ISSUE:** Same as PLAN-3.1 — wave 3 placement defers performance optimization to end of Phase 2. Success criterion #7 ("Frame rate exceeds 120 FPS") is not achieved until wave 3. If phase closure happens after wave 2, criterion #7 is unmet.

**Risk:** HIGH
- **Why:** Frame rate target (120 FPS) is core to Phase 2 shippability. Deferring optimization to wave 3 creates risk of phase completion failure. If wave 3 overruns, Phase 2 ships with <120 FPS performance.

**Recommendation:** REVISE. Recommend:
1. Move performance work into wave 2 (coordinate with PLAN-2.1 to finalize dirty-row optimization before PLAN-3.2 verification).
2. Explicitly clarify whether wave 3 is "optional" or "required for phase completion".

---

## Cross-Plan Dependencies and Ordering

### Dependency Graph

```
Wave 1:
  PLAN-1.1 (no deps)
  PLAN-1.2 (no deps)

Wave 2:
  PLAN-2.1 (depends on PLAN-1.1)
  PLAN-2.2 (depends on PLAN-1.1)
  PLAN-2.3 (depends on PLAN-1.1, PLAN-1.2)

Wave 3:
  PLAN-3.1 (depends on PLAN-2.1, PLAN-2.2)
  PLAN-3.2 (depends on PLAN-2.1, PLAN-2.3)
```

### Implicit Ordering Within Waves

**Wave 2 execution order (recommended):**
1. PLAN-2.1 first (quad pipeline must exist before PLAN-2.2 and PLAN-2.3 generate quads)
2. PLAN-2.2 and PLAN-2.3 can run in parallel after PLAN-2.1 completes

**Wave 3 execution order (recommended):**
1. PLAN-3.1 and PLAN-3.2 can run in parallel (no direct coupling)

### Hidden Dependencies (Identified During Critique)

| Issue | From | To | Severity | Mitigation |
|-------|------|----|-----------|-------------|
| `rows_for_viewport()` is essential for scrollback rendering | PLAN-2.1, PLAN-2.3 | PLAN-1.1 | HIGH | Block wave 2 until PLAN-1.1 Task 3 completes; test viewport rendering |
| Quad renderer must exist before selection quads generated | PLAN-2.3 | PLAN-2.1 | HIGH | Execute PLAN-2.1 before PLAN-2.3; verify `QuadRenderer::prepare()` signature |
| PtySession::new shell parameter missing from files_touched | PLAN-2.2 | arcterm-pty | MEDIUM | Add `arcterm-pty/src/session.rs` to files_touched in PLAN-2.2 |
| Renderer::new font_size parameterization unclear | PLAN-2.2 | arcterm-render | MEDIUM | Clarify: is font_size optional with FONT_SIZE fallback, or required? |
| Color scheme reading requires config module | PLAN-3.1 | PLAN-2.2 | MEDIUM | Ensure PLAN-2.2 config parsing is complete before PLAN-3.1 starts |
| Frame rate optimization deferred to wave 3 | PLAN-3.2 | Phase 2 | HIGH | Clarify: is 120 FPS a wave 2 or wave 3 requirement? |

---

## File Coverage and Conflicts

### Files Modified in Phase 2

| File | Plans | Conflict? |
|------|-------|-----------|
| arcterm-core/src/grid.rs | PLAN-1.1 | No |
| arcterm-core/src/cell.rs | PLAN-1.1 | No |
| arcterm-core/src/lib.rs | PLAN-1.1, PLAN-1.2 (task 3) | No (separate regions) |
| arcterm-vt/src/processor.rs | PLAN-1.2 | No |
| arcterm-vt/src/handler.rs | PLAN-1.2 | No |
| arcterm-vt/src/lib.rs | PLAN-1.2 | No |
| arcterm-render/src/quad.rs | PLAN-2.1 (NEW) | No |
| arcterm-render/src/text.rs | PLAN-2.1, PLAN-3.1 | Potential (PLAN-2.1 Task 2 modifies prepare_grid; PLAN-3.1 modifies for color palette). Recommend sequential execution. |
| arcterm-render/src/renderer.rs | PLAN-2.1, PLAN-2.2, PLAN-3.1 | **YES** (3-way conflict). PLAN-2.1 modifies render_frame; PLAN-2.2 modifies new(); PLAN-3.1 modifies for palette. Recommend execution order: PLAN-2.1 → PLAN-2.2 → PLAN-3.1. |
| arcterm-render/src/gpu.rs | PLAN-3.2 | No |
| arcterm-render/Cargo.toml | PLAN-2.1 (adds bytemuck) | No (additive) |
| arcterm-app/src/config.rs | PLAN-2.2, PLAN-3.1 | Potential (PLAN-2.2 creates module; PLAN-3.1 modifies to add color parsing). Recommend sequential. |
| arcterm-app/src/main.rs | PLAN-2.2, PLAN-2.3, PLAN-3.1, PLAN-3.2 | **YES** (4-way conflict). Multiple plans add event handlers, state fields, config loading. Recommend careful merge planning. |
| arcterm-app/src/input.rs | PLAN-2.3, PLAN-3.2 | Potential (PLAN-2.3 adds mouse handlers; PLAN-3.2 wires app-cursor-keys). Recommend sequential. |
| arcterm-app/src/selection.rs | PLAN-2.3 (NEW) | No |
| arcterm-app/src/colors.rs | PLAN-3.1 (NEW) | No |
| arcterm-app/src/terminal.rs | PLAN-2.3, PLAN-3.2 | Potential (PLAN-2.3 adds grid_mut and set_scroll_offset; PLAN-3.2 may modify). Recommend sequential. |
| arcterm-app/Cargo.toml | PLAN-2.2 (adds toml, serde, dirs, notify), PLAN-2.3 (adds arboard), PLAN-3.1 (if any) | No (all additive) |
| arcterm-pty/src/session.rs | PLAN-2.2 (implied, NOT listed) | **YES** (unlisted file modification) |

**Summary:** 3 multi-way conflicts detected (renderer.rs, main.rs) + 1 unlisted file (session.rs). Recommend explicit execution ordering and thorough merge testing.

---

## Verification Command Feasibility

All verification commands (`cargo build`, `cargo test`) are feasible in current codebase state. Current status: **builds successfully**. Verification commands in each plan are runnable and will either pass (plan is feasible) or fail with concrete error messages (plan has blocking issues).

---

## Task Scope and Complexity

| Plan | Tasks | Avg Task Size | Complexity | Red Flag |
|------|-------|---------------|------------|----------|
| PLAN-1.1 | 3 | Large | High | VecDeque integration, viewport math |
| PLAN-1.2 | 2 | Large | High | DEC mode dispatch, mode flags |
| PLAN-2.1 | 3 | Medium | High | wgpu shader, quad geometry, dirty-row hashing |
| PLAN-2.2 | 3 | Medium | Medium | Config parsing, hot-reload, file watcher |
| PLAN-2.3 | 3 | Large | High | Selection model, clipboard lifetime, scroll coordination |
| PLAN-3.1 | 3+ (spec incomplete) | Medium | Medium | Color palette management, scheme overrides |
| PLAN-3.2 | 5 | Small | High | Performance profiling, mode dispatch wiring |

**No plan exceeds 5 tasks. All are within scope.** PLAN-1.1 and PLAN-2.3 are highest complexity; recommend assigning experienced Rust developers to these.

---

## Success Criteria Mapping

| Success Criterion | Addressed By | Wave | Risk |
|-------------------|--------------|------|------|
| 1. Passes 90%+ vttest | PLAN-1.1, PLAN-1.2 | 1 | HIGH (depends on correct VT parsing) |
| 2. 256-color and truecolor | PLAN-2.1, PLAN-3.1 | 2–3 | MEDIUM (color pipeline deferred to wave 3) |
| 3. Neovim, tmux, SSH render correctly | PLAN-1.1, PLAN-1.2, PLAN-3.2 | 1–3 | HIGH (artifacts may appear in wave 2 before optimization) |
| 4. 10,000+ line scrollback | PLAN-1.1, PLAN-2.3 | 1–2 | LOW (straightforward VecDeque + viewport) |
| 5. Config system | PLAN-2.2, PLAN-3.1 | 2–3 | LOW (config module is clean; color scheme deferred) |
| 6. Selection & clipboard | PLAN-2.3 | 2 | MEDIUM (clipboard lifetime + arboard integration) |
| 7. 120+ FPS | PLAN-2.1, PLAN-3.2 | 2–3 | HIGH (optimization deferred to wave 3) |

---

## Recommendations

### READY (No blockers)
- **PLAN-1.1** — Foundational; TDD reduces risk. Proceed.
- **PLAN-1.2** — Handler extension is straightforward. Proceed with emphasis on test coverage for edge cases.
- **PLAN-2.1** — Well-specified; GPU code is integration-tested. Remove the "fallback to rows()" clause. Proceed with PLAN-2.1 first in wave 2.
- **PLAN-2.3** — Selection model is well-designed. Proceed after PLAN-2.1 (quad pipeline dependency).

### CAUTION (Minor clarifications needed)
- **PLAN-2.2** — Clarify Renderer::new() signature (font_size optional or required?). Add `arcterm-pty/src/session.rs` to files_touched. Proceed with clarifications.

### REVISE (Structural changes recommended)
- **PLAN-3.1** — Consider moving to wave 2 if color scheme is essential for Phase 2 completion. Alternatively, clarify that Color::Default is sufficient for phase closure and color schemes are a "nice-to-have" enhancement in wave 3.
- **PLAN-3.2** — **Move frame rate optimization to wave 2 or explicitly clarify wave 3 is required for Phase 2 to ship.** Success criterion #7 (120+ FPS) cannot be deferred without phase failure.

### Action Items Before Execution
1. **Execution order for wave 2:** PLAN-2.1 → (PLAN-2.2 || PLAN-2.3 in parallel). Do not start PLAN-2.3 until PLAN-2.1 QuadRenderer is merged.
2. **Clarify PLAN-2.2 Renderer::new() signature:** Is font_size parameter optional with fallback to FONT_SIZE constant?
3. **Add unlisted file:** PLAN-2.2 must explicitly list `arcterm-pty/src/session.rs` in files_touched.
4. **Phase 2 scope decision:** Decide if PLAN-3.1 and PLAN-3.2 are wave 3 (required for phase) or wave 3+ (post-phase polish). If required, commit to extended timeline.
5. **Merge planning for conflicted files:**
   - `arcterm-render/src/renderer.rs`: 3-way conflict (PLAN-2.1 → PLAN-2.2 → PLAN-3.1)
   - `arcterm-app/src/main.rs`: 4-way conflict (PLAN-2.2, PLAN-2.3, PLAN-3.1, PLAN-3.2)
   Recommend explicit merge strategy (e.g., sequential file ownership, staged PRs).

---

## Verdict

**VERDICT: CAUTION → READY (with action items)**

### Tally
- **READY:** 5 plans (PLAN-1.1, PLAN-1.2, PLAN-2.1, PLAN-2.3, PLAN-3.1 with caveats)
- **CAUTION:** 1 plan (PLAN-2.2 — needs clarifications)
- **REVISE:** 1 plan (PLAN-3.2 — scope decision required)

### Summary Statement

The plans are **feasible and well-specified** overall. File paths, API surfaces, and dependency graphs are sound. No blockers prevent execution of waves 1–2. However, **three issues must be resolved before proceeding:**

1. **PLAN-2.2 clarity:** Clarify Renderer::new() font_size API and add unlisted `arcterm-pty/src/session.rs` file.
2. **Wave 3 scope decision:** Confirm PLAN-3.1 and PLAN-3.2 are required for Phase 2 completion, or defer to Phase 3. Success criteria #2 (color) and #7 (120 FPS) cannot be half-implemented.
3. **Execution ordering:** Lock in sequential execution (PLAN-2.1 before PLAN-2.2 and PLAN-2.3) and prepare merge strategy for multi-way conflicts in renderer.rs and main.rs.

**Once these are resolved, proceed with execution.**

---

## Appendix: Files Referenced by Plans

### New Files (to be created)
- arcterm-render/src/quad.rs (PLAN-2.1)
- arcterm-app/src/config.rs (PLAN-2.2)
- arcterm-app/src/selection.rs (PLAN-2.3)
- arcterm-app/src/colors.rs (PLAN-3.1)

### Existing Files Modified
- arcterm-core/src/{grid.rs, cell.rs, lib.rs} (PLAN-1.1)
- arcterm-vt/src/{processor.rs, handler.rs, lib.rs} (PLAN-1.2)
- arcterm-render/src/{quad.rs (new), text.rs, renderer.rs, gpu.rs, lib.rs} (PLAN-2.1, PLAN-3.1, PLAN-3.2)
- arcterm-app/src/{config.rs (new), colors.rs (new), selection.rs (new), main.rs, input.rs, terminal.rs} (PLAN-2.2, PLAN-2.3, PLAN-3.1, PLAN-3.2)
- arcterm-pty/src/session.rs (PLAN-2.2, **unlisted**)

### Cargo.toml Additions
- arcterm-render: bytemuck (PLAN-2.1)
- arcterm-app: toml, serde, dirs, notify (PLAN-2.2); arboard (PLAN-2.3)

---

**Report prepared:** 2026-03-15
**Reviewer:** Senior Verification Engineer
**Next step:** Address action items and proceed to build verification.
