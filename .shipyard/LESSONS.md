# Shipyard Lessons Learned

## [2026-03-16] Milestone: Arcterm v0.1.0

### What Went Well
- Phased architecture with wave-based parallelism worked extremely well — 8 phases completed in a single session
- TDD for data structure modules (grid, layout, tab, keymap, selection) caught design issues early
- Code reviews between waves caught real bugs: missing esc_dispatch guard, PTY receiver API mismatch, begin_frame error handling
- The simplification pass after Phase 3 consolidated genuine duplicates (PaneId, PaneNode, TermModes)
- Research agents prevented multiple wrong turns (glyphon version mismatch, vte APC drop, wasmtime API)

### Surprises / Discoveries
- glyphon 0.9 required wgpu 25, not wgpu 28 — version mismatch caught by dependency validation task
- vte 0.15 silently drops ALL APC sequences — required a custom pre-processor for Kitty graphics
- HiDPI text/cursor misalignment: glyphon TextArea.top is physical pixels but we were passing logical — only visible on HiDPI displays
- wasmtime Component Model type validation is extremely strict — hand-crafting WAT test components requires exact type encoding
- Rust edition 2024 let-chains (`if let ... && let ...`) eliminated many collapsible-if clippy warnings

### Pitfalls to Avoid
- Don't skip clippy -D warnings after every wave — lint debt compounds fast
- Don't assume library version compatibility without testing (glyphon/wgpu version matrix)
- Don't use `pub` fields on core structs without thinking about invariants (Grid.cells, Grid.cursor)
- Don't duplicate types across modules "temporarily" — consolidate immediately or it becomes permanent
- Always test on HiDPI — coordinate space bugs are invisible on 1x displays

### Process Improvements
- Autonomous mode with per-phase commits is the right workflow for well-defined projects
- Security audit before ship caught real vulnerabilities (plugin path traversal, stdio inheritance)
- The 3-task-per-plan limit kept scope manageable and reviews focused
- Research agents before each phase prevented blocked builders and wrong architectural choices

---

## [2026-03-16] Milestone: Arcterm v0.1.1 — Stabilization

### What Went Well
- Research agents finding issues already fixed saved significant builder effort (7 of 13 issues pre-resolved)
- Team mode with parallel builders across independent crates worked smoothly
- Review gates caught a real underflow regression (scroll_up/delete_lines) before it shipped
- Organizing fixes by code path dependency enabled full parallelism in Phase 9 (4 crates simultaneously)

### Surprises / Discoveries
- wgpu `request_adapter()` returns `Result` not `Option` in this version — API documentation/plan mismatch required builder adaptation
- `try_send` vs `blocking_send` matters in `spawn_blocking` contexts — blocking_send would stall the blocking thread pool
- Pre-existing example files (`window.rs`) break when internal APIs change — examples should be included in CI or at minimum in `cargo check --workspace`

### Pitfalls to Avoid
- In-place copy scroll refactors can introduce usize underflow when `n == region_height` and `top == 0` — always use `checked_sub` for usize arithmetic in range bounds
- Mode 1047 tests were omitted despite being in the plan's `must_haves` — reviewers should cross-reference the frontmatter checklist, not just the code
- Making a field private (scroll_offset) intentionally breaks downstream crates — document the expected breakage clearly and verify only the target crate until the migration phase

### Process Improvements
- The "research first, discover what's already fixed" pattern dramatically reduced builder scope (from 21 planned fixes to ~10 actual code changes)
- Stabilization phases benefit from strict issue-by-issue tracking with the ISSUES.md ledger
- Review-driven issue logging (ISSUE-014 through ISSUE-019) creates a clean backlog for the next release

---

## [2026-03-16] Performance Optimization Pass

### What Went Well
- Codebase exploration agent identified 10 bottlenecks with exact file/line references in one pass
- Snapshot caching eliminated duplicate FairMutex locks per pane per frame
- PTY reader batching reduces lock contention during heavy output bursts
- Release build profile confirmed optimizations work: 84% idle (debug) → 99.1% idle (release)
- Security audit caught a real cleanup gap (cached_snapshots not cleared on pane removal)

### Surprises / Discoveries
- macOS SIP blocks dtruss/dtrace on modern systems — syscall tracing requires workarounds (fs_usage, Docker, or Instruments)
- fs_usage requires exact process name, not binary paths — easy to get wrong
- Debug builds inflate CPU profiles ~20x vs release due to missing inlining
- The #1 perf bottleneck was cosmic_text font fallback (binary search in fontdb), not our render loop or snapshot code
- AtomicI32 with sentinel value is a clean replacement for Mutex<Option<i32>> on write-once values

### Pitfalls to Avoid
- Don't profile debug builds and draw conclusions — always profile release for meaningful data
- Don't assume your code is the bottleneck — third-party library internals (cosmic_text font fallback) dominated the profile
- Don't clone structured blocks per frame when you can borrow — restructuring pane_frames to include PaneId eliminated both the clone and a linear search

### Process Improvements
- Profile before and after optimizations to validate impact with real data
- Use `sample` (macOS) for quick CPU profiles — it works under SIP and gives actionable call stacks
- Batch related optimizations into phases by impact level — highest impact first ensures early wins

---
