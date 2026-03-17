# Simplification Report
**Phase:** 15 — Menu Bar Implementation
**Date:** 2026-03-16
**Files analyzed:** 5 (`Cargo.toml`, `keymap.rs`, `layout.rs`, `main.rs`, `menu.rs`)
**Findings:** 3 high, 3 medium, 4 low

---

## High Priority

### 1. Pane resource cleanup is duplicated verbatim between ClosePane and CloseTab
- **Type:** Consolidate
- **Locations:** `main.rs:1199-1208` (ClosePane, last-pane-in-tab branch), `main.rs:1223-1230` (ClosePane, multi-pane branch), `main.rs:1294-1298` (CloseTab)
- **Description:** Three separate `for id in removed_ids` loops each remove a pane from `self.panes`, `self.image_channels`, `self.nvim_states`, `self.ai_states`, and `self.pane_contexts`, then null out `self.last_ai_pane`. The three loops are nearly identical. The `CloseTab` branch (line 1294) is missing the `nvim_states`, `ai_states`, and `pane_contexts` cleanup that both `ClosePane` branches perform — this is the active bug already caused by the duplication.
- **Suggestion:** Extract a private method `fn remove_pane_resources(&mut self, id: PaneId)` that performs all five map removals and the `last_ai_pane` null-out. Replace every loop body with a call to this method. The `CloseTab` branch at line 1294 then gets the missing cleanup automatically.
- **Impact:** ~25 lines removed, one active cleanup gap closed, future pane-state fields only need to be added in one place.

### 2. DispatchOutcome match arms are copy-pasted at three call sites
- **Type:** Consolidate
- **Locations:** `main.rs:1895-1904` (menu event handler), `main.rs:3390-3399` (keyboard event handler), `main.rs:3423-3428` (palette execute path)
- **Description:** Every call to `dispatch_action` is followed by an identical three-arm match: `Redraw => request_redraw()`, `Exit => event_loop.exit(); return`, `None => {}`. The palette path (line 3423) drops `return` after `exit()` and silently ignores `Redraw` — a divergence already noted by the audit. There is also a standalone `execute_key_action` function at line 3419 that wraps exactly this match but is not used at the two main call sites.
- **Suggestion:** Rename `execute_key_action` to `handle_outcome` (or similar), give it a `should_return: &mut bool` out-parameter for the `Exit` case, and route all three call sites through it. Alternatively, change `dispatch_action` to accept the `ActiveEventLoop` directly and call `exit()` internally — the `Exit` variant then becomes an internal detail. Either approach eliminates the three-way copy.
- **Impact:** ~20 lines removed, eliminates the Redraw-dropped divergence in the palette path, ensures future `DispatchOutcome` variants are handled uniformly.

### 3. `menu.rs` construct-then-insert pattern repeats 27 times with no helper
- **Type:** Refactor
- **Locations:** `menu.rs:50-88` (Shell), `menu.rs:108-162` (Edit), `menu.rs:185-216` (View), `menu.rs:235-375` (Window), `menu.rs:406-419` (Help) — 27 item construction sites total
- **Description:** Every menu item follows the exact same three-step sequence:
  1. `let item = MenuItem::new(label, true, accel);`
  2. `id_map.insert(item.id().clone(), action);`
  3. item reference added to the `append_items` slice.

  The `.id().clone()` call appears 27 times. The `true` enabled flag appears 27 times. There is no helper — each item requires writing the same boilerplate. The comment at line 39-43 explicitly acknowledges a desire for a macro-like pattern but defers it.
- **Suggestion:** Add a local helper closure inside `AppMenu::new()`:
  ```rust
  let mut item = |label: &str, accel: Option<Accelerator>, action: KeyAction| -> MenuItem {
      let it = MenuItem::new(label, true, accel);
      id_map.insert(it.id().clone(), action);
      it
  };
  ```
  Because `muda::MenuItem` is not `Copy`, each call site becomes a one-liner. The `append_items` slices become readable lists of named variables. This does not require a macro — a closure suffices here since all items share the same `enabled=true` flag.
- **Impact:** ~54 lines removed (27 two-line `let`+`insert` pairs collapse to 27 one-liners), the `.id().clone()` noise disappears entirely, adding a new menu item drops from 3 lines to 1.

---

## Medium Priority

### 4. `let focused = focused_id` aliases appear six times for no reason
- **Type:** Remove
- **Locations:** `main.rs:1155`, `main.rs:1189`, `main.rs:1252`, `main.rs:1412`, `main.rs:1431`, `main.rs:1562`
- **Description:** Six match arms shadow `focused_id` with an immediately equivalent `let focused = focused_id;` binding. `focused_id` is already a `PaneId` (Copy), so these aliases serve no purpose — they don't shorten a long path or change mutability. Lines 1200 and 1295 also do `let lid = id;` inside loops for the same reason.
- **Suggestion:** Remove all six `let focused = focused_id;` bindings and the two `let lid = id;` bindings. Use `focused_id` directly throughout each arm.
- **Impact:** 8 lines removed, no behavior change.

### 5. `SaveWorkspace` date arithmetic is an inline Proleptic Gregorian algorithm with no explanation or test
- **Type:** Refactor
- **Locations:** `main.rs:1322-1343`
- **Description:** The 20-line block inside `KeyAction::SaveWorkspace` hand-implements Civil-to-Gregorian date conversion (`era`, `doe`, `yoe`, `doy`, `mp`) to produce a timestamp string. This is the only date-formatting site in the codebase. The algorithm is correct but completely opaque — a future maintainer has no way to verify or debug it without knowing the Euclidean affine formula for calendar arithmetic. The `chrono` crate is already a common Rust ecosystem choice for this exact task; alternatively the `time` crate is already in many Rust projects.
- **Suggestion:** Extract the timestamp generation to a `fn session_name() -> String` function (or use `chrono::Local::now().format("session-%Y%m%d-%H%M").to_string()`). If adding a crate is undesirable, at minimum extract to a standalone function with a doc comment crediting the algorithm source (Hinnant's civil-from-days algorithm). The current inline placement inside a `match` arm makes it impossible to unit-test.
- **Impact:** Removes 18 lines from `dispatch_action`, makes the date logic testable, documents the algorithm's origin.

### 6. `impl Default for AppMenu` delegates entirely to `AppMenu::new()` — one-liner with no callers
- **Type:** Remove
- **Locations:** `menu.rs:442-446`
- **Description:** `Default::new()` simply calls `Self::new()`. The only call site for `AppMenu` in `main.rs` (line 1829) calls `menu::AppMenu::new()` directly — it does not use `Default`. The `Default` impl is unused and adds interface surface for no benefit. Because `AppMenu::new()` panics on `muda` append failures, implementing `Default` implies it is "trivially constructible," which is misleading.
- **Suggestion:** Remove `impl Default for AppMenu`. If a future caller needs `Default`, add it then.
- **Impact:** 5 lines removed, no misleading `Default` impl on a type whose construction can panic.

---

## Low Priority

- **`main.rs:1475` and `main.rs:1549` recompute `self.tab_manager.active_tab().focus` instead of using `focused_id`** — these two arms (`ClearScrollback` and `ResetTerminal`) call `active_tab().focus` again redundantly; `focused_id` was already bound at line 1029 at the top of `dispatch_action`. Remove the redundant calls.

- **`keymap.rs:74-111` comment block `// ---- Menu-only actions (no leader-key binding) ----`** — useful but the section divider style differs from the rest of the file which uses `// ---------------------------------------------------------------------------` banners. Minor style inconsistency, low churn value.

- **`menu.rs:39-43` block comment describes a design decision but gives incorrect advice** — the comment at line 39 says "we can't return the item itself from a closure and then use it again, so we use a macro-like pattern." In fact a closure capturing `&mut id_map` can register and return a `MenuItem` just fine (as described in Finding 3 above). The comment is technically wrong and may confuse future contributors.

- **`main.rs:1295` `let lid = id;` inside `KeyAction::CloseTab`** — identical to the `ClosePane` pattern in Finding 4. Should be removed as part of the same cleanup pass.

---

## Summary

- **Duplication found:** 3 instances across `main.rs` (pane cleanup ×3, dispatch match ×3, menu item registration ×27)
- **Dead code found:** 1 unused `Default` impl (`menu.rs:442-446`), 1 `execute_key_action` function that exists but is not used at the two main call sites it was intended for
- **Complexity hotspots:** `dispatch_action` is 567 lines (lines 1028-1595) — well over the 40-line threshold, though this is acceptable for a central dispatch function with one arm per action. The `SaveWorkspace` arm contains a 20-line opaque algorithm that should be extracted.
- **AI bloat patterns:** 8 no-op variable aliases (`let focused = focused_id`, `let lid = id`), 1 misleading Default impl

**Estimated cleanup impact:** ~110 lines removable or extractable, 1 active bug closed (CloseTab missing pane-state cleanup), dispatch handling made uniform.

---

## Recommendation

**Simplification is recommended before the next phase, not deferred.** Finding 1 is not purely cosmetic — the `CloseTab` branch today does not remove pane state from `nvim_states`, `ai_states`, or `pane_contexts`, because it was written separately from `ClosePane` and the duplication hid the gap. Finding 2 has an already-diverged call site (the palette path silently drops the `Redraw` signal). Findings 3, 4, and 6 are mechanical and low-risk. Findings 1, 2, and 3 together represent the core cleanup; the others can be batched.
