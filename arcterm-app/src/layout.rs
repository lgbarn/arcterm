//! Pane tree layout engine — core types, layout computation, navigation,
//! mutation, zoom, and border quad generation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// PaneId
// ---------------------------------------------------------------------------

/// Opaque identifier for a single terminal pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaneId(pub u64);

static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

impl PaneId {
    /// Allocate the next unique `PaneId`.
    pub fn next() -> Self {
        PaneId(NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed))
    }
}

// ---------------------------------------------------------------------------
// PixelRect
// ---------------------------------------------------------------------------

/// An axis-aligned rectangle in physical pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PixelRect {
    /// Returns `true` if the point `(px, py)` lies inside this rectangle.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x
            && px < self.x + self.width
            && py >= self.y
            && py < self.y + self.height
    }

    /// Centre x coordinate.
    pub fn cx(&self) -> f32 {
        self.x + self.width / 2.0
    }

    /// Centre y coordinate.
    pub fn cy(&self) -> f32 {
        self.y + self.height / 2.0
    }
}

// ---------------------------------------------------------------------------
// Direction / Axis
// ---------------------------------------------------------------------------

/// Cardinal direction for pane focus navigation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Split axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal, // split into left / right halves
    Vertical,   // split into top / bottom halves
}

// ---------------------------------------------------------------------------
// BorderQuad
// ---------------------------------------------------------------------------

/// A coloured rectangle to draw as a pane border.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderQuad {
    pub rect: PixelRect,
    /// RGBA colour, each component 0–255.
    pub color: [u8; 4],
}

// ---------------------------------------------------------------------------
// PaneNode — the recursive pane tree
// ---------------------------------------------------------------------------

/// A node in the pane tree.
///
/// * `Leaf`   — a single terminal pane.
/// * `HSplit` — two panes side-by-side (left | right).
/// * `VSplit` — two panes stacked (top / bottom).
#[derive(Clone, Debug)]
pub enum PaneNode {
    Leaf {
        pane_id: PaneId,
    },
    HSplit {
        /// Fraction [0.0, 1.0] of available width given to the left child.
        ratio: f32,
        left: Box<PaneNode>,
        right: Box<PaneNode>,
    },
    VSplit {
        /// Fraction [0.0, 1.0] of available height given to the top child.
        ratio: f32,
        top: Box<PaneNode>,
        bottom: Box<PaneNode>,
    },
}

impl PaneNode {
    // -----------------------------------------------------------------------
    // Layout computation
    // -----------------------------------------------------------------------

    /// Recursively compute the pixel rectangle for every leaf pane.
    ///
    /// `border_px` pixels are consumed on each interior edge of a split.
    pub fn compute_rects(
        &self,
        available: PixelRect,
        border_px: f32,
    ) -> HashMap<PaneId, PixelRect> {
        let mut out = HashMap::new();
        self.compute_rects_into(available, border_px, &mut out);
        out
    }

    fn compute_rects_into(
        &self,
        rect: PixelRect,
        border_px: f32,
        out: &mut HashMap<PaneId, PixelRect>,
    ) {
        match self {
            PaneNode::Leaf { pane_id } => {
                out.insert(*pane_id, rect);
            }
            PaneNode::HSplit { ratio, left, right } => {
                let ratio = ratio.clamp(0.0, 1.0);
                let left_w = ((rect.width - border_px) * ratio).max(0.0);
                let right_x = rect.x + left_w + border_px;
                let right_w = (rect.x + rect.width - right_x).max(0.0);

                let left_rect = PixelRect {
                    x: rect.x,
                    y: rect.y,
                    width: left_w,
                    height: rect.height,
                };
                let right_rect = PixelRect {
                    x: right_x,
                    y: rect.y,
                    width: right_w,
                    height: rect.height,
                };
                left.compute_rects_into(left_rect, border_px, out);
                right.compute_rects_into(right_rect, border_px, out);
            }
            PaneNode::VSplit { ratio, top, bottom } => {
                let ratio = ratio.clamp(0.0, 1.0);
                let top_h = ((rect.height - border_px) * ratio).max(0.0);
                let bot_y = rect.y + top_h + border_px;
                let bot_h = (rect.y + rect.height - bot_y).max(0.0);

                let top_rect = PixelRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: top_h,
                };
                let bot_rect = PixelRect {
                    x: rect.x,
                    y: bot_y,
                    width: rect.width,
                    height: bot_h,
                };
                top.compute_rects_into(top_rect, border_px, out);
                bottom.compute_rects_into(bot_rect, border_px, out);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Query helpers
    // -----------------------------------------------------------------------

    /// Returns `true` if this sub-tree contains a leaf with `id`.
    pub fn find_leaf(&self, id: PaneId) -> bool {
        match self {
            PaneNode::Leaf { pane_id } => *pane_id == id,
            PaneNode::HSplit { left, right, .. } => left.find_leaf(id) || right.find_leaf(id),
            PaneNode::VSplit { top, bottom, .. } => top.find_leaf(id) || bottom.find_leaf(id),
        }
    }

    /// Collect all `PaneId`s present in this sub-tree.
    pub fn all_pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    fn collect_ids(&self, out: &mut Vec<PaneId>) {
        match self {
            PaneNode::Leaf { pane_id } => out.push(*pane_id),
            PaneNode::HSplit { left, right, .. } => {
                left.collect_ids(out);
                right.collect_ids(out);
            }
            PaneNode::VSplit { top, bottom, .. } => {
                top.collect_ids(out);
                bottom.collect_ids(out);
            }
        }
    }

    /// Rebuild this tree with each leaf's `PaneId` replaced by the value in
    /// `id_map`.  Leaves whose id is not present in the map are left unchanged.
    ///
    /// This is used by the workspace restore path to swap placeholder IDs
    /// (allocated by `WorkspacePaneNode::to_pane_tree`) for the actual IDs
    /// of the spawned PTY panes (allocated by repeated `PaneId::next()` calls).
    #[allow(dead_code)] // Available for future workspace restore callers
    pub fn remap_pane_ids(&self, id_map: &HashMap<PaneId, PaneId>) -> PaneNode {
        match self {
            PaneNode::Leaf { pane_id } => PaneNode::Leaf {
                pane_id: id_map.get(pane_id).copied().unwrap_or(*pane_id),
            },
            PaneNode::HSplit { ratio, left, right } => PaneNode::HSplit {
                ratio: *ratio,
                left: Box::new(left.remap_pane_ids(id_map)),
                right: Box::new(right.remap_pane_ids(id_map)),
            },
            PaneNode::VSplit { ratio, top, bottom } => PaneNode::VSplit {
                ratio: *ratio,
                top: Box::new(top.remap_pane_ids(id_map)),
                bottom: Box::new(bottom.remap_pane_ids(id_map)),
            },
        }
    }

    /// Returns `true` if this sub-tree contains a leaf with `id`.
    /// Alias for [`find_leaf`] provided for caller ergonomics.
    #[allow(dead_code)]
    pub fn contains_pane(&self, id: PaneId) -> bool {
        self.find_leaf(id)
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------

    /// Find the nearest pane in `dir` relative to `current` using the
    /// pre-computed `rects` map.
    ///
    /// Returns `None` when `current` is already the edge pane in that
    /// direction or when `rects` contains only one entry.
    pub fn focus_in_direction(
        &self,
        current: PaneId,
        dir: Direction,
        rects: &HashMap<PaneId, PixelRect>,
    ) -> Option<PaneId> {
        let cur_rect = rects.get(&current)?;
        let cx = cur_rect.cx();
        let cy = cur_rect.cy();

        let mut best: Option<(PaneId, f32)> = None;

        for (&id, rect) in rects {
            if id == current {
                continue;
            }
            let rx = rect.cx();
            let ry = rect.cy();

            // For each direction keep only candidates that are strictly
            // on the correct side and pick the closest by primary axis,
            // with secondary axis overlap as a tie-breaker.
            let score = match dir {
                Direction::Left => {
                    if rx >= cx {
                        continue;
                    }
                    // Prefer right-most (largest rx) among left candidates.
                    // We invert so smallest score wins.
                    cx - rx
                }
                Direction::Right => {
                    if rx <= cx {
                        continue;
                    }
                    rx - cx
                }
                Direction::Up => {
                    if ry >= cy {
                        continue;
                    }
                    cy - ry
                }
                Direction::Down => {
                    if ry <= cy {
                        continue;
                    }
                    ry - cy
                }
            };

            // Weight by secondary-axis distance to prefer rects that
            // actually overlap on the cross-axis.
            let cross_penalty = match dir {
                Direction::Left | Direction::Right => (ry - cy).abs(),
                Direction::Up | Direction::Down => (rx - cx).abs(),
            };
            let weighted = score + cross_penalty * 0.5;

            if best.map(|(_, s)| weighted < s).unwrap_or(true) {
                best = Some((id, weighted));
            }
        }

        best.map(|(id, _)| id)
    }

    // -----------------------------------------------------------------------
    // Tree mutation
    // -----------------------------------------------------------------------

    /// Split the leaf identified by `target` along `axis`, inserting `new_id`
    /// as the second child (right or bottom).
    ///
    /// Returns `true` on success, `false` if `target` was not found.
    pub fn split(&mut self, target: PaneId, axis: Axis, new_id: PaneId) -> bool {
        match self {
            PaneNode::Leaf { pane_id } => {
                if *pane_id != target {
                    return false;
                }
                let original = PaneNode::Leaf { pane_id: *pane_id };
                let sibling = PaneNode::Leaf { pane_id: new_id };
                *self = match axis {
                    Axis::Horizontal => PaneNode::HSplit {
                        ratio: 0.5,
                        left: Box::new(original),
                        right: Box::new(sibling),
                    },
                    Axis::Vertical => PaneNode::VSplit {
                        ratio: 0.5,
                        top: Box::new(original),
                        bottom: Box::new(sibling),
                    },
                };
                true
            }
            PaneNode::HSplit { left, right, .. } => {
                left.split(target, axis, new_id) || right.split(target, axis, new_id)
            }
            PaneNode::VSplit { top, bottom, .. } => {
                top.split(target, axis, new_id) || bottom.split(target, axis, new_id)
            }
        }
    }

    /// Remove the leaf `target` from the tree, promoting its sibling.
    ///
    /// Returns `Some(sibling)` when the removal succeeded and the current node
    /// should be replaced by the returned node, or `None` when:
    /// - `target` was not found in this sub-tree, or
    /// - the root itself is a lone leaf (caller should handle "last pane" logic).
    ///
    /// Callers must detect the "last pane" case by checking `all_pane_ids().len() == 1`
    /// before calling `close`, and refuse the close in that situation.
    pub fn close(&mut self, target: PaneId) -> Option<PaneNode> {
        match self {
            PaneNode::Leaf { .. } => {
                // The caller already handles the single-root-leaf case.
                None
            }
            PaneNode::HSplit { left, right, .. } => {
                // Is the left child a leaf matching target?
                if matches!(left.as_ref(), PaneNode::Leaf { pane_id } if *pane_id == target) {
                    return Some(*right.clone());
                }
                // Is the right child a leaf matching target?
                if matches!(right.as_ref(), PaneNode::Leaf { pane_id } if *pane_id == target) {
                    return Some(*left.clone());
                }
                // Recurse into left.
                if left.find_leaf(target) {
                    if let Some(replacement) = left.close(target) {
                        **left = replacement;
                    }
                    return None;
                }
                // Recurse into right.
                if right.find_leaf(target) {
                    if let Some(replacement) = right.close(target) {
                        **right = replacement;
                    }
                    return None;
                }
                None
            }
            PaneNode::VSplit { top, bottom, .. } => {
                if matches!(top.as_ref(), PaneNode::Leaf { pane_id } if *pane_id == target) {
                    return Some(*bottom.clone());
                }
                if matches!(bottom.as_ref(), PaneNode::Leaf { pane_id } if *pane_id == target) {
                    return Some(*top.clone());
                }
                if top.find_leaf(target) {
                    if let Some(replacement) = top.close(target) {
                        **top = replacement;
                    }
                    return None;
                }
                if bottom.find_leaf(target) {
                    if let Some(replacement) = bottom.close(target) {
                        **bottom = replacement;
                    }
                    return None;
                }
                None
            }
        }
    }

    /// Adjust the split ratio of the split node whose *direct child* is `target`.
    ///
    /// `delta` is added to the ratio; the result is clamped to `[0.05, 0.95]`.
    /// Returns `true` if a split containing `target` was found and adjusted.
    pub fn resize_split(&mut self, target: PaneId, delta: f32) -> bool {
        match self {
            PaneNode::Leaf { .. } => false,
            PaneNode::HSplit { ratio, left, right, .. } => {
                if left.find_leaf(target) || right.find_leaf(target) {
                    *ratio = (*ratio + delta).clamp(0.05, 0.95);
                    return true;
                }
                left.resize_split(target, delta) || right.resize_split(target, delta)
            }
            PaneNode::VSplit { ratio, top, bottom, .. } => {
                if top.find_leaf(target) || bottom.find_leaf(target) {
                    *ratio = (*ratio + delta).clamp(0.05, 0.95);
                    return true;
                }
                top.resize_split(target, delta) || bottom.resize_split(target, delta)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Zoom
    // -----------------------------------------------------------------------

    /// Return a rect map where `pane_id` occupies the full `available` area
    /// and all other panes are invisible (zero-sized).  Useful for a "zoom"
    /// mode that temporarily maximises one pane.
    pub fn compute_zoomed_rect(
        &self,
        pane_id: PaneId,
        available: PixelRect,
    ) -> HashMap<PaneId, PixelRect> {
        let zero = PixelRect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
        self.all_pane_ids()
            .into_iter()
            .map(|id| {
                if id == pane_id {
                    (id, available)
                } else {
                    (id, zero)
                }
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Border quads
    // -----------------------------------------------------------------------

    /// Generate border quads for every interior split edge.
    ///
    /// - `focused`: the currently-focused pane — its borders use
    ///   `focus_color`, all others use `normal_color`.
    /// - `border_px`: width/height of each border strip in pixels.
    pub fn compute_border_quads(
        &self,
        available: PixelRect,
        border_px: f32,
        focused: PaneId,
        normal_color: [u8; 4],
        focus_color: [u8; 4],
    ) -> Vec<BorderQuad> {
        let rects = self.compute_rects(available, border_px);
        let mut quads = Vec::new();
        self.collect_border_quads(
            available,
            border_px,
            focused,
            normal_color,
            focus_color,
            &rects,
            &mut quads,
        );
        quads
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::only_used_in_recursion)]
    fn collect_border_quads(
        &self,
        rect: PixelRect,
        border_px: f32,
        focused: PaneId,
        normal_color: [u8; 4],
        focus_color: [u8; 4],
        rects: &HashMap<PaneId, PixelRect>,
        out: &mut Vec<BorderQuad>,
    ) {
        match self {
            PaneNode::Leaf { .. } => {}
            PaneNode::HSplit { ratio, left, right } => {
                let ratio = ratio.clamp(0.0, 1.0);
                let left_w = ((rect.width - border_px) * ratio).max(0.0);
                let border_x = rect.x + left_w;

                // Determine color: use focus color if either child sub-tree
                // contains the focused pane.
                let color = if left.find_leaf(focused) || right.find_leaf(focused) {
                    focus_color
                } else {
                    normal_color
                };

                out.push(BorderQuad {
                    rect: PixelRect {
                        x: border_x,
                        y: rect.y,
                        width: border_px,
                        height: rect.height,
                    },
                    color,
                });

                // Recurse into children with their sub-rects.
                let left_rect = PixelRect {
                    x: rect.x,
                    y: rect.y,
                    width: left_w,
                    height: rect.height,
                };
                let right_x = border_x + border_px;
                let right_w = (rect.x + rect.width - right_x).max(0.0);
                let right_rect = PixelRect {
                    x: right_x,
                    y: rect.y,
                    width: right_w,
                    height: rect.height,
                };
                left.collect_border_quads(
                    left_rect, border_px, focused, normal_color, focus_color, rects, out,
                );
                right.collect_border_quads(
                    right_rect, border_px, focused, normal_color, focus_color, rects, out,
                );
            }
            PaneNode::VSplit { ratio, top, bottom } => {
                let ratio = ratio.clamp(0.0, 1.0);
                let top_h = ((rect.height - border_px) * ratio).max(0.0);
                let border_y = rect.y + top_h;

                let color = if top.find_leaf(focused) || bottom.find_leaf(focused) {
                    focus_color
                } else {
                    normal_color
                };

                out.push(BorderQuad {
                    rect: PixelRect {
                        x: rect.x,
                        y: border_y,
                        width: rect.width,
                        height: border_px,
                    },
                    color,
                });

                let top_rect = PixelRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: top_h,
                };
                let bot_y = border_y + border_px;
                let bot_h = (rect.y + rect.height - bot_y).max(0.0);
                let bot_rect = PixelRect {
                    x: rect.x,
                    y: bot_y,
                    width: rect.width,
                    height: bot_h,
                };
                top.collect_border_quads(
                    top_rect, border_px, focused, normal_color, focus_color, rects, out,
                );
                bottom.collect_border_quads(
                    bot_rect, border_px, focused, normal_color, focus_color, rects, out,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> PixelRect {
        PixelRect { x, y, width: w, height: h }
    }

    // -----------------------------------------------------------------------
    // Task 1 tests — core types and compute_rects
    // -----------------------------------------------------------------------

    #[test]
    fn test_single_leaf_fills_available() {
        let id = PaneId::next();
        let tree = PaneNode::Leaf { pane_id: id };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let rects = tree.compute_rects(available, 2.0);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[&id], available);
    }

    #[test]
    fn test_hsplit_50_percent() {
        let a = PaneId::next();
        let b = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let rects = tree.compute_rects(available, 2.0);

        let lr = rects[&a];
        let rr = rects[&b];

        // left starts at x=0
        assert_eq!(lr.x, 0.0);
        // widths consume the available minus the border
        assert!((lr.width - 499.0).abs() < 1.0);
        // right starts after left + border
        assert!((rr.x - (lr.x + lr.width + 2.0)).abs() < 1.0);
        // heights are full
        assert_eq!(lr.height, 800.0);
        assert_eq!(rr.height, 800.0);
    }

    #[test]
    fn test_vsplit_50_percent() {
        let a = PaneId::next();
        let b = PaneId::next();
        let tree = PaneNode::VSplit {
            ratio: 0.5,
            top: Box::new(PaneNode::Leaf { pane_id: a }),
            bottom: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let rects = tree.compute_rects(available, 2.0);

        let tr = rects[&a];
        let br = rects[&b];

        assert_eq!(tr.y, 0.0);
        assert!((tr.height - 399.0).abs() < 1.0);
        assert!((br.y - (tr.y + tr.height + 2.0)).abs() < 1.0);
        assert_eq!(tr.width, 1000.0);
        assert_eq!(br.width, 1000.0);
    }

    #[test]
    fn test_nested_split() {
        // HSplit with left=leaf, right=VSplit(leaf, leaf)
        let a = PaneId::next();
        let b = PaneId::next();
        let c = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::VSplit {
                ratio: 0.5,
                top: Box::new(PaneNode::Leaf { pane_id: b }),
                bottom: Box::new(PaneNode::Leaf { pane_id: c }),
            }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let rects = tree.compute_rects(available, 2.0);

        assert_eq!(rects.len(), 3);
        // a occupies left half
        assert_eq!(rects[&a].x, 0.0);
        // b and c are in the right half, stacked
        assert!(rects[&b].x > 400.0);
        assert!(rects[&c].x > 400.0);
        // b is above c
        assert!(rects[&b].y < rects[&c].y);
    }

    #[test]
    fn test_find_leaf() {
        let a = PaneId::next();
        let b = PaneId::next();
        let outside = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        assert!(tree.find_leaf(a));
        assert!(tree.find_leaf(b));
        assert!(!tree.find_leaf(outside));
    }

    #[test]
    fn test_all_pane_ids() {
        let a = PaneId::next();
        let b = PaneId::next();
        let c = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::VSplit {
                ratio: 0.5,
                top: Box::new(PaneNode::Leaf { pane_id: b }),
                bottom: Box::new(PaneNode::Leaf { pane_id: c }),
            }),
        };
        let mut ids = tree.all_pane_ids();
        ids.sort_by_key(|p| p.0);
        let mut expected = vec![a, b, c];
        expected.sort_by_key(|p| p.0);
        assert_eq!(ids, expected);
    }

    // -----------------------------------------------------------------------
    // Task 2 tests — navigation, split, close, resize
    // -----------------------------------------------------------------------

    /// Build a 2-pane horizontal split and verify left/right navigation.
    #[test]
    fn test_navigate_left_right() {
        let a = PaneId::next();
        let b = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let rects = tree.compute_rects(available, 2.0);

        // From left pane, go right → b
        assert_eq!(tree.focus_in_direction(a, Direction::Right, &rects), Some(b));
        // From right pane, go left → a
        assert_eq!(tree.focus_in_direction(b, Direction::Left, &rects), Some(a));
    }

    #[test]
    fn test_navigate_up_down() {
        let a = PaneId::next();
        let b = PaneId::next();
        let tree = PaneNode::VSplit {
            ratio: 0.5,
            top: Box::new(PaneNode::Leaf { pane_id: a }),
            bottom: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let rects = tree.compute_rects(available, 2.0);

        assert_eq!(tree.focus_in_direction(a, Direction::Down, &rects), Some(b));
        assert_eq!(tree.focus_in_direction(b, Direction::Up, &rects), Some(a));
    }

    #[test]
    fn test_navigate_edge_returns_none() {
        let a = PaneId::next();
        let b = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let rects = tree.compute_rects(available, 2.0);

        // Already at left edge, no pane further left
        assert_eq!(tree.focus_in_direction(a, Direction::Left, &rects), None);
        // Already at right edge
        assert_eq!(tree.focus_in_direction(b, Direction::Right, &rects), None);
    }

    #[test]
    fn test_navigate_4_pane_grid() {
        // Layout: HSplit(VSplit(a,b), VSplit(c,d))
        let a = PaneId::next();
        let b = PaneId::next();
        let c = PaneId::next();
        let d = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::VSplit {
                ratio: 0.5,
                top: Box::new(PaneNode::Leaf { pane_id: a }),
                bottom: Box::new(PaneNode::Leaf { pane_id: b }),
            }),
            right: Box::new(PaneNode::VSplit {
                ratio: 0.5,
                top: Box::new(PaneNode::Leaf { pane_id: c }),
                bottom: Box::new(PaneNode::Leaf { pane_id: d }),
            }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let rects = tree.compute_rects(available, 2.0);

        // a(top-left) → right → c(top-right)
        assert_eq!(tree.focus_in_direction(a, Direction::Right, &rects), Some(c));
        // b(bottom-left) → right → d(bottom-right)
        assert_eq!(tree.focus_in_direction(b, Direction::Right, &rects), Some(d));
        // a → down → b
        assert_eq!(tree.focus_in_direction(a, Direction::Down, &rects), Some(b));
        // c → down → d
        assert_eq!(tree.focus_in_direction(c, Direction::Down, &rects), Some(d));
    }

    #[test]
    fn test_split_horizontal() {
        let a = PaneId::next();
        let mut tree = PaneNode::Leaf { pane_id: a };
        let new_id = PaneId::next();
        assert!(tree.split(a, Axis::Horizontal, new_id));
        let ids = tree.all_pane_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&a));
        assert!(ids.contains(&new_id));
    }

    #[test]
    fn test_split_vertical() {
        let a = PaneId::next();
        let mut tree = PaneNode::Leaf { pane_id: a };
        let new_id = PaneId::next();
        assert!(tree.split(a, Axis::Vertical, new_id));
        assert!(matches!(tree, PaneNode::VSplit { .. }));
    }

    #[test]
    fn test_close_leaf_promotes_sibling() {
        let a = PaneId::next();
        let b = PaneId::next();
        let mut tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        let replacement = tree.close(a);
        // Should return sibling b.
        assert!(replacement.is_some());
        let r = replacement.unwrap();
        assert!(matches!(r, PaneNode::Leaf { pane_id } if pane_id == b));
    }

    #[test]
    fn test_close_last_pane_returns_none() {
        let a = PaneId::next();
        let mut tree = PaneNode::Leaf { pane_id: a };
        // Caller must check length; close on a lone leaf returns None (no sibling).
        let replacement = tree.close(a);
        assert!(replacement.is_none());
    }

    #[test]
    fn test_resize_clamps() {
        let a = PaneId::next();
        let b = PaneId::next();
        let mut tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        // Push far right — should clamp to 0.95
        assert!(tree.resize_split(a, 100.0));
        if let PaneNode::HSplit { ratio, .. } = &tree {
            assert!((*ratio - 0.95).abs() < f32::EPSILON);
        }
        // Push far left — should clamp to 0.05
        assert!(tree.resize_split(a, -100.0));
        if let PaneNode::HSplit { ratio, .. } = &tree {
            assert!((*ratio - 0.05).abs() < f32::EPSILON);
        }
    }

    // -----------------------------------------------------------------------
    // Task 3 tests — zoom, border quads
    // -----------------------------------------------------------------------

    #[test]
    fn test_border_quads_count() {
        let a = PaneId::next();
        let b = PaneId::next();
        let c = PaneId::next();
        // HSplit with right=VSplit → 2 interior edges → 2 border quads
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::VSplit {
                ratio: 0.5,
                top: Box::new(PaneNode::Leaf { pane_id: b }),
                bottom: Box::new(PaneNode::Leaf { pane_id: c }),
            }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let quads = tree.compute_border_quads(
            available,
            2.0,
            a,
            [80, 80, 80, 255],
            [255, 200, 0, 255],
        );
        assert_eq!(quads.len(), 2);
    }

    #[test]
    fn test_border_quad_position() {
        let a = PaneId::next();
        let b = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let quads = tree.compute_border_quads(
            available,
            4.0,
            a,
            [80, 80, 80, 255],
            [255, 200, 0, 255],
        );
        assert_eq!(quads.len(), 1);
        let q = quads[0];
        // Border width is 4px
        assert_eq!(q.rect.width, 4.0);
        assert_eq!(q.rect.height, 800.0);
    }

    #[test]
    fn test_border_quad_focus_color() {
        let a = PaneId::next();
        let b = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let normal = [80u8, 80, 80, 255];
        let focus = [255u8, 200, 0, 255];
        let quads = tree.compute_border_quads(available, 2.0, a, normal, focus);
        // The focused pane (a) is in this split → focus color
        assert_eq!(quads[0].color, focus);
    }

    #[test]
    fn test_zoom_returns_full_rect() {
        let a = PaneId::next();
        let b = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::Leaf { pane_id: b }),
        };
        let available = rect(0.0, 0.0, 1000.0, 800.0);
        let rects = tree.compute_zoomed_rect(a, available);
        // Zoomed pane gets full available area
        assert_eq!(rects[&a], available);
        // Other pane gets zero-sized rect
        assert_eq!(rects[&b].width, 0.0);
        assert_eq!(rects[&b].height, 0.0);
    }

    #[test]
    fn test_pixel_rect_contains() {
        let r = rect(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(50.0, 40.0));
        assert!(!r.contains(5.0, 40.0));
        assert!(!r.contains(50.0, 75.0));
    }
}
