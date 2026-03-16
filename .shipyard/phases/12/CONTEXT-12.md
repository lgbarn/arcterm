# Phase 12 — Context & Decisions

## Phase Summary

Replace `arcterm-core`, `arcterm-vt`, and `arcterm-pty` with `alacritty_terminal`. Build OSC 7770 pre-filter. Rewire renderer to read alacritty's grid. Reconnect AI features. Delete old crates.

Design document: `.shipyard/designs/2026-03-16-engine-migration-design.md`

## Decisions

### D1: alacritty_terminal version strategy
**Decision:** Use latest stable release from crates.io. Accept its API as-is. Do not fork.
**Rationale:** Keeps us on the upgrade path. If API gaps are found, we adapt our code rather than forking.

### D2: PTY strategy
**Decision:** Use alacritty's full PTY module (`alacritty_terminal::tty`) for PTY creation and I/O. Drop `portable-pty` dependency.
**Rationale:** Cleanest integration — one PTY system, not two. Alacritty handles platform differences internally.

### D3: Test strategy
**Decision:** Integration tests only. Delete unit tests in removed crates. Write new integration tests that verify end-to-end behavior (PTY output → pre-filter → Term → renderer data). Trust alacritty's own test suite for VT correctness.
**Rationale:** Unit tests for deleted code are worthless. Integration tests verify our integration points — the pre-filter, the renderer bridge, and AI feature reconnection.

### D4: OSC 7770 interception strategy
**Decision:** Pre-filter on the PTY byte stream, before bytes reach alacritty's Term. Stateful scanner modeled on existing ApcScanner. Also intercepts Kitty graphics APC sequences.
**Rationale:** Alacritty doesn't know about OSC 7770 and shouldn't. Pre-filter keeps alacritty stock and updatable.

### D5: Crate disposition
**Decision:** Remove `arcterm-core`, `arcterm-vt`, `arcterm-pty` entirely. No adapter layers.
**Rationale:** Clean break. Less code to maintain. No confusion about which grid/VT layer is active.
