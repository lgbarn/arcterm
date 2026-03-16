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
