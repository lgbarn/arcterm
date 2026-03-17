//! Tab model and tab manager for Arcterm's multiplexer layer.
//!
//! A [`Tab`] is a lightweight layout-plus-focus descriptor. It owns the
//! binary split tree ([`PaneNode`]) for one terminal tab and tracks which
//! pane is focused and whether any pane is zoomed. Terminals and PTY
//! receivers are NOT stored here; they live in flat `HashMap`s on
//! `AppState` so that background-tab PTY channels can still be polled.
//!
//! [`TabManager`] owns the ordered list of tabs and the active-tab index.

use crate::layout::{PaneId, PaneNode};

// ---------------------------------------------------------------------------
// Tab
// ---------------------------------------------------------------------------

/// A single terminal tab: a layout tree, a focused pane, and optional zoom.
#[derive(Debug, Clone)]
pub struct Tab {
    /// User-visible name shown in the tab bar (e.g. "Tab 1").
    #[allow(dead_code)]
    pub label: String,
    /// The binary split tree for this tab's pane layout.
    pub layout: PaneNode,
    /// The [`PaneId`] of the currently focused pane within this tab.
    pub focus: PaneId,
    /// If `Some`, the indicated pane is in fullscreen / zoom mode.
    pub zoomed: Option<PaneId>,
}

impl Tab {
    /// Create a new tab with a single-pane layout labelled `label`.
    fn new(pane_id: PaneId, label: String) -> Self {
        Tab {
            label,
            layout: PaneNode::Leaf { pane_id },
            focus: pane_id,
            zoomed: None,
        }
    }

    /// Collect all [`PaneId`]s in this tab's layout tree.
    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.layout.all_pane_ids()
    }
}

// ---------------------------------------------------------------------------
// TabManager
// ---------------------------------------------------------------------------

/// Manages an ordered list of [`Tab`]s and tracks the active tab index.
#[derive(Debug)]
pub struct TabManager {
    /// All open tabs in display order.
    pub tabs: Vec<Tab>,
    /// Index of the currently active tab.
    pub active: usize,
}

impl TabManager {
    /// Create a `TabManager` with a single tab containing one leaf pane.
    pub fn new(initial_pane_id: PaneId) -> Self {
        let tab = Tab::new(initial_pane_id, "Tab 1".to_string());
        TabManager {
            tabs: vec![tab],
            active: 0,
        }
    }

    /// Return a shared reference to the active tab.
    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    /// Return a mutable reference to the active tab.
    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    /// Append a new tab containing a single leaf `pane_id`.
    ///
    /// Returns the index of the newly created tab.
    pub fn add_tab(&mut self, pane_id: PaneId) -> usize {
        let n = self.tabs.len() + 1;
        let label = format!("Tab {n}");
        let tab = Tab::new(pane_id, label);
        self.tabs.push(tab);
        self.tabs.len() - 1
    }

    /// Remove the tab at `index`, returning all [`PaneId`]s it contained.
    ///
    /// The caller is responsible for cleaning up the corresponding
    /// `Terminal` and PTY channel entries.
    ///
    /// # No-op case
    ///
    /// If `index` is out of range, or removing the tab would leave zero
    /// tabs, the operation is a no-op and an empty `Vec` is returned.
    pub fn close_tab(&mut self, index: usize) -> Vec<PaneId> {
        // Never close the last tab.
        if self.tabs.len() <= 1 {
            return Vec::new();
        }
        if index >= self.tabs.len() {
            return Vec::new();
        }

        let removed = self.tabs.remove(index);
        let ids = removed.pane_ids();

        // Keep `active` in a valid range.
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }

        ids
    }

    /// Set the active tab to `index`, clamped to `[0, tab_count - 1]`.
    pub fn switch_to(&mut self, index: usize) {
        self.active = index.min(self.tabs.len().saturating_sub(1));
    }

    /// Return the number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Collect all [`PaneId`]s across every tab.
    ///
    /// Useful for polling PTY channels belonging to background tabs.
    #[allow(dead_code)]
    pub fn all_pane_ids(&self) -> Vec<PaneId> {
        self.tabs.iter().flat_map(|t| t.pane_ids()).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn p(n: u64) -> PaneId {
        PaneId(n)
    }

    fn manager() -> TabManager {
        TabManager::new(p(1))
    }

    // ── Initial state ─────────────────────────────────────────────────────────

    #[test]
    fn new_has_exactly_one_tab() {
        let mgr = manager();
        assert_eq!(mgr.tab_count(), 1, "fresh TabManager must have 1 tab");
    }

    #[test]
    fn new_active_is_zero() {
        let mgr = manager();
        assert_eq!(mgr.active, 0);
    }

    #[test]
    fn active_tab_returns_correct_tab() {
        let mgr = manager();
        let tab = mgr.active_tab();
        assert_eq!(tab.focus, p(1), "focus should be the initial pane id");
        assert_eq!(tab.label, "Tab 1");
    }

    #[test]
    fn new_tab_has_no_zoom() {
        let mgr = manager();
        assert!(mgr.active_tab().zoomed.is_none());
    }

    // ── add_tab ───────────────────────────────────────────────────────────────

    #[test]
    fn add_tab_increases_count() {
        let mut mgr = manager();
        mgr.add_tab(p(2));
        assert_eq!(mgr.tab_count(), 2);
    }

    #[test]
    fn add_tab_returns_correct_index() {
        let mut mgr = manager();
        let idx = mgr.add_tab(p(2));
        assert_eq!(idx, 1, "second tab should be at index 1");
        let idx2 = mgr.add_tab(p(3));
        assert_eq!(idx2, 2, "third tab should be at index 2");
    }

    #[test]
    fn add_tab_does_not_change_active() {
        let mut mgr = manager();
        mgr.add_tab(p(2));
        assert_eq!(mgr.active, 0, "add_tab must not change active index");
    }

    // ── close_tab ─────────────────────────────────────────────────────────────

    #[test]
    fn close_tab_removes_tab_and_returns_ids() {
        let mut mgr = manager();
        mgr.add_tab(p(2));
        assert_eq!(mgr.tab_count(), 2);

        let ids = mgr.close_tab(1);
        assert_eq!(mgr.tab_count(), 1, "tab count must drop to 1");
        assert_eq!(
            ids,
            vec![p(2)],
            "returned ids should be the closed tab's pane"
        );
    }

    #[test]
    fn close_last_tab_is_noop() {
        let mut mgr = manager();
        let ids = mgr.close_tab(0);
        assert!(ids.is_empty(), "closing last tab must be a no-op");
        assert_eq!(mgr.tab_count(), 1);
    }

    #[test]
    fn close_tab_out_of_range_is_noop() {
        let mut mgr = manager();
        mgr.add_tab(p(2));
        let ids = mgr.close_tab(99);
        assert!(ids.is_empty());
        assert_eq!(mgr.tab_count(), 2);
    }

    #[test]
    fn close_tab_adjusts_active_when_active_closed() {
        let mut mgr = manager();
        mgr.add_tab(p(2));
        mgr.add_tab(p(3));
        // Switch to last tab (index 2) then close it.
        mgr.switch_to(2);
        assert_eq!(mgr.active, 2);
        mgr.close_tab(2);
        assert_eq!(mgr.active, 1, "active must clamp to new last index");
    }

    #[test]
    fn close_tab_adjusts_active_when_lower_tab_removed() {
        let mut mgr = manager();
        mgr.add_tab(p(2));
        mgr.add_tab(p(3));
        // Active is 0; remove tab at index 0.
        mgr.close_tab(0);
        // active was 0, which is now the old tab 1 — active should be 0 (clamped)
        assert!(mgr.active < mgr.tab_count());
    }

    // ── switch_to ─────────────────────────────────────────────────────────────

    #[test]
    fn switch_to_changes_active() {
        let mut mgr = manager();
        mgr.add_tab(p(2));
        mgr.switch_to(1);
        assert_eq!(mgr.active, 1);
    }

    #[test]
    fn switch_to_clamps_to_valid_range() {
        let mut mgr = manager();
        mgr.add_tab(p(2));
        // tab_count = 2, valid indices are 0..=1
        mgr.switch_to(100);
        assert_eq!(mgr.active, 1, "out-of-range index clamps to last tab");
    }

    #[test]
    fn switch_to_zero_on_empty_like_manager() {
        let mut mgr = manager();
        mgr.switch_to(0);
        assert_eq!(mgr.active, 0);
    }

    // ── all_pane_ids ──────────────────────────────────────────────────────────

    #[test]
    fn all_pane_ids_collects_from_all_tabs() {
        let mut mgr = manager();
        mgr.add_tab(p(2));
        mgr.add_tab(p(3));
        let mut ids = mgr.all_pane_ids();
        ids.sort_unstable_by_key(|id| id.0);
        assert_eq!(ids, vec![p(1), p(2), p(3)]);
    }

    #[test]
    fn all_pane_ids_single_tab() {
        let mgr = manager();
        assert_eq!(mgr.all_pane_ids(), vec![p(1)]);
    }

    // ── pane_ids helper on Tab ─────────────────────────────────────────────────

    #[test]
    fn tab_pane_ids_returns_leaf_id() {
        let mgr = manager();
        let ids = mgr.active_tab().pane_ids();
        assert_eq!(ids, vec![p(1)]);
    }
}
