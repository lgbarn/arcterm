# Security Audit Report — Phase 10

## Executive Summary

**Verdict:** PASS
**Risk Level:** Low

Phase 10 contains three mechanical, app-layer changes: a scroll_offset API migration to private-field accessors, ctrl key mapping extraction into a helper function, and a render-only Unicode substitution for the cursor glyph. None of these changes introduce authentication, network I/O, external input parsing, or dependency additions. No secrets, unsafe blocks, or exploitable conditions were found across any changed file. The phase is clear to ship.

### What to Do

No required actions. One advisory item is noted below for awareness.

### Themes

- All changes are pure refactors or render-layer additions with no trust-boundary crossings.
- Input from the OS keyboard layer is mapped to fixed byte sequences; the mapping table is complete and closed.

---

## Detailed Findings

### Critical

None.

### Important

None.

### Advisory

- `arcterm-app/src/input.rs:14` — `ctrl_char_byte` handles `a–z`, `[`, `\`, `]` but silently returns `None` for all other characters (digits, punctuation). This is correct terminal behavior, but there is no test asserting that an unmapped character (e.g., `'1'`) returns `None`. A future regression could accidentally widen the mapping. Adding one negative-path test would close the gap — effort: Trivial.

---

## Cross-Component Analysis

The scroll_offset migration is consistent across all call sites. The private field in `arcterm-core::Grid` is accessed exclusively through the `scroll_offset()` getter and `set_scroll_offset()` setter, both of which enforce clamping to `scrollback.len()`. The remaining `scroll_offset` field references in `main.rs` (lines 2548, 2572, 2595, 2605) are on `OverlayReviewState`, a separate struct unrelated to the grid API — not a migration gap.

The cursor substitution in `text.rs` is render-only. `substitute_cursor_char` operates on a borrowed row slice and returns a new `Vec<char>`; the underlying `Cell` data is never mutated. There is no path by which a crafted terminal sequence could influence the substitution logic — it is purely driven by the cursor position integer and a two-character blank check (`' '` or `'\0'`).

The ctrl mapping helper produces fixed-length `Vec<u8>` outputs with values in `0x01–0x1d`. There is no allocation beyond a single-byte vector, no string formatting, and no external input reaches the mapping logic except the `char` value already extracted from a `winit` `KeyEvent` — a trusted OS event type.

---

## Analysis Coverage

| Area | Checked | Notes |
|------|---------|-------|
| Code Security (OWASP) | Yes | No injection, auth, session, or deserialization surface in changed code |
| Secrets & Credentials | Yes | No secrets found in any changed file or new summary doc |
| Dependencies | Yes | No Cargo.toml or Cargo.lock changes in this phase |
| Infrastructure as Code | N/A | No IaC files changed |
| Docker/Container | N/A | No Dockerfile changes |
| Configuration | Yes | No config files changed; no debug flags or CORS settings touched |

---

## Dependency Status

No dependency changes in Phase 10.

| Package | Version | Known CVEs | Status |
|---------|---------|-----------|--------|
| — | — | — | N/A |

---

## IaC Findings

No infrastructure changes in Phase 10.
