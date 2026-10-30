//! The order accessibility reads a tree in, as Compose's `SemanticsSort`: rows of what overlaps
//! vertically, top to bottom, each row in reading order, then by `traversal_index`, a traversal
//! group's nodes together. In Rust, so the bridges share it and it's tested headless (D1289).

use std::collections::HashMap;

use crate::semantics::{SemanticsProperties, SemanticsSnapshot};
use crate::unit::Rect;

/// What reads something of its own, as a service focuses on it, as Compose's
/// `isScreenReaderFocusable`, for a node of the merged tree: not hidden, and merging, or a leaf that
/// says something.
pub fn is_screen_reader_focusable(node: &SemanticsSnapshot) -> bool {
    if node.hidden {
        return false;
    }
    let own = node.unmerged_config();
    if own.is_merging_semantics_of_descendants {
        return true;
    }
    let description =
        own.get(&SemanticsProperties::CONTENT_DESCRIPTION).is_some_and(|all| !all.is_empty());
    let says = description
        || own.contains(&SemanticsProperties::EDITABLE_TEXT)
        || own.get(&SemanticsProperties::TEXT).is_some_and(|all| !all.is_empty())
        || own.contains(&SemanticsProperties::STATE_DESCRIPTION)
        || own.contains(&SemanticsProperties::TOGGLEABLE_STATE)
        || own.contains(&SemanticsProperties::SELECTED)
        || own.contains(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO);
    says && node
        .children
        .iter()
        .all(|child| child.unmerged_config().contains(&SemanticsProperties::LINK_TEST_MARKER))
}

/// `bounds` within `clip`, if they overlap.
pub fn clipped_by(bounds: Rect, clip: Rect) -> Option<Rect> {
    let rect = Rect::new(
        bounds.left.max(clip.left),
        bounds.top.max(clip.top),
        bounds.right.min(clip.right),
        bounds.bottom.min(clip.bottom),
    );
    (rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

/// What a scrolling container clips: it shows what's in it within itself. Which is all that's
/// clipped here, the bounds as `write_node`'s record has them.
fn clip_inside(node: &SemanticsSnapshot, shown: Option<Rect>, clip: Rect) -> Rect {
    let own = node.unmerged_config();
    let scrolls = own.contains(&SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
        || own.contains(&SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE);
    if scrolls { shown.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)) } else { clip }
}

#[derive(Clone, Copy)]
struct Placed<'a> {
    node: &'a SemanticsSnapshot,
    /// As shown: empty when clipped away.
    bounds: Rect,
    clip: Rect,
}

impl<'a> Placed<'a> {
    fn of(node: &'a SemanticsSnapshot, clip: Rect) -> Placed<'a> {
        let shown = clipped_by(node.bounds, clip);
        Placed {
            node,
            bounds: shown.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            clip: clip_inside(node, shown, clip),
        }
    }

    fn children(self) -> impl Iterator<Item = Placed<'a>> {
        self.node.children.iter().map(move |child| Placed::of(child, self.clip))
    }

    fn traversal_index(&self) -> f32 {
        let own = self.node.unmerged_config();
        own.get(&SemanticsProperties::TRAVERSAL_INDEX).copied().unwrap_or(0.0)
    }
}

/// The nodes under `root` a service focuses on, `root` left out unless it's one, in the order it
/// reads them, as `subtreeSortedByGeometryGrouping`.
pub fn sorted_for_accessibility(root: &SemanticsSnapshot) -> Vec<&SemanticsSnapshot> {
    let place = Placed { node: root, bounds: root.bounds, clip: root.bounds };
    sorted_subtree(place, std::slice::from_ref(&place)).iter().map(|place| place.node).collect()
}

/// Who a service reads each node before: a node's id to the next one's, as `idToBeforeMap`.
pub fn traversal_before(root: &SemanticsSnapshot) -> HashMap<i32, i32> {
    let order = sorted_for_accessibility(root);
    order.windows(2).map(|pair| (pair[0].semantics_id, pair[1].semantics_id)).collect()
}

fn sorted_subtree<'a>(parent: Placed<'a>, list: &[Placed<'a>]) -> Vec<Placed<'a>> {
    let mut containers = HashMap::new();
    let mut geometry = Vec::new();
    for place in list {
        depth_first(*place, &mut geometry, &mut containers);
    }
    sort_by_geometry(parent, geometry, &containers)
}

fn depth_first<'a>(
    place: Placed<'a>,
    geometry: &mut Vec<Placed<'a>>,
    containers: &mut HashMap<i32, Vec<Placed<'a>>>,
) {
    let own = place.node.unmerged_config();
    let is_group = own.get(&SemanticsProperties::IS_TRAVERSAL_GROUP) == Some(&true);
    if (is_group || is_screen_reader_focusable(place.node)) && !place.node.hidden {
        geometry.push(place);
    }
    if is_group {
        let children: Vec<_> = place.children().collect();
        containers.insert(place.node.semantics_id, sorted_subtree(place, &children));
    } else {
        for child in place.children() {
            depth_first(child, geometry, containers);
        }
    }
}

/// Into the first row it overlaps vertically, the row's cover narrowed to the two; `false` with
/// none, as `placedEntryRowOverlaps`.
fn into_overlapping_row<'a>(rows: &mut [(Rect, Vec<Placed<'a>>)], place: Placed<'a>) -> bool {
    let (top, bottom) = (place.bounds.top, place.bounds.bottom);
    if top >= bottom {
        return false;
    }
    for (cover, row) in rows {
        if cover.top < cover.bottom && top.max(cover.top) < bottom.min(cover.bottom) {
            cover.top = cover.top.max(top);
            cover.bottom = cover.bottom.min(bottom);
            row.push(place);
            return true;
        }
    }
    false
}

fn sort_by_geometry<'a>(
    parent: Placed<'a>,
    list: Vec<Placed<'a>>,
    containers: &HashMap<i32, Vec<Placed<'a>>>,
) -> Vec<Placed<'a>> {
    let rtl = parent.node.rtl;
    let mut rows: Vec<(Rect, Vec<Placed<'a>>)> = Vec::new();
    for place in list {
        if !into_overlapping_row(&mut rows, place) {
            rows.push((place.bounds, vec![place]));
        }
    }
    rows.sort_by(|a, b| a.0.top.total_cmp(&b.0.top).then(a.0.bottom.total_cmp(&b.0.bottom)));
    let in_row = |a: &Placed<'a>, b: &Placed<'a>| {
        let (one, other) = (a.bounds, b.bounds);
        let along =
            if rtl { other.right.total_cmp(&one.right) } else { one.left.total_cmp(&other.left) };
        let across = one.top.total_cmp(&other.top).then(one.bottom.total_cmp(&other.bottom));
        let far_side =
            if rtl { other.left.total_cmp(&one.left) } else { one.right.total_cmp(&other.right) };
        along
            .then(across)
            .then(far_side)
            .then(a.node.z_index.total_cmp(&b.node.z_index))
            .then(a.node.place_order.cmp(&b.node.place_order))
            .then(a.node.semantics_id.cmp(&b.node.semantics_id))
    };
    let mut sorted = Vec::new();
    for (_, mut row) in rows {
        row.sort_by(in_row);
        sorted.append(&mut row);
    }
    // Stable: the highest indices last, everything else in place.
    sorted.sort_by(|a, b| a.traversal_index().total_cmp(&b.traversal_index()));

    // A group's nodes after it, it only if it's focused on itself.
    let mut out = Vec::with_capacity(sorted.len());
    for place in sorted {
        match containers.get(&place.node.semantics_id) {
            Some(children) => {
                if is_screen_reader_focusable(place.node) {
                    out.push(place);
                }
                out.extend_from_slice(children);
            }
            None => out.push(place),
        }
    }
    out
}
