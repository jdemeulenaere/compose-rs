//! The semantics tree for Android's accessibility (docs/07-platforms.md, "The rest of the
//! platform glue"): `Accessibility.kt`, an `AccessibilityNodeProvider`, reads it as virtual
//! views when a service (TalkBack, Switch Access) asks.
//!
//! The merged tree is written in pre-order, one record per node: its virtual id (its node's
//! index), its parent's (-1 for the root's children), its bounds in the view's pixels, flags,
//! role, and its texts. Little-endian, as the display list.

use compose_ui::{
    Rect, Role, SemanticsActions as A, SemanticsProperties as P, SharedString, ToggleableState,
    platform::{NodeId, SemanticsNode},
};

use crate::serialize::Writer;

/// The flags of a node's record.
pub mod flag {
    pub const CLICKABLE: u32 = 1;
    pub const FOCUSABLE: u32 = 1 << 1;
    pub const FOCUSED: u32 = 1 << 2;
    pub const EDITABLE: u32 = 1 << 3;
    pub const PASSWORD: u32 = 1 << 4;
    pub const DISABLED: u32 = 1 << 5;
    pub const CHECKABLE: u32 = 1 << 6;
    pub const CHECKED: u32 = 1 << 7;
    pub const SELECTED: u32 = 1 << 8;
    pub const DIALOG: u32 = 1 << 9;
    pub const RANGE: u32 = 1 << 10;
    /// It scrolls its content (`SCROLL_HORIZONTAL`: horizontally, else vertically; reversed:
    /// from its end): its offset and the largest follow the range, if any (D478).
    pub const SCROLL: u32 = 1 << 11;
    pub const SCROLL_HORIZONTAL: u32 = 1 << 12;
    pub const SCROLL_REVERSED: u32 = 1 << 13;
    /// Clipped away by the scrolling containers around it (a lazy list's item scrolled out
    /// while it holds the focus, a column's child out of view): not visible to the user, as
    /// Compose's `isVisibleToUser` from the clipped bounds.
    pub const HIDDEN: u32 = 1 << 14;
    /// Laid out right to left: a horizontal scroll's left and right go the other way (D484).
    pub const RTL: u32 = 1 << 15;
}

/// Whether a frame that did `stats` may have changed the semantics (`frame`'s `SEMANTICS`): it
/// recomposed, laid something out, or a modifier node said its semantics changed
/// (`FrameStats::semantics_changed`, D1158). `Accessibility.kt` then reads the tree again when next
/// asked, whether or not a service is on (a field write, as Compose's
/// `currentSemanticsNodesInvalidated`), and tells the services on, if any (D630).
pub fn semantics_may_have_changed(stats: &compose_ui::platform::FrameStats) -> bool {
    stats.recomposed || stats.measured || stats.semantics_changed
}

/// A node's virtual view id: its index (live nodes have distinct ones).
pub fn virtual_id(node: NodeId) -> i32 {
    (node.to_bits() & 0xFFFF_FFFF) as i32
}

/// Writes the merged tree `root` (its children: the root itself is the view).
pub fn write_tree(root: &SemanticsNode, out: &mut Writer) {
    out.bytes.clear();
    for child in &root.children {
        write_node(child, -1, root.bounds, out);
    }
}

/// `bounds` within `clip`, if they overlap.
fn clipped(bounds: Rect, clip: Rect) -> Option<Rect> {
    let rect = Rect::new(
        bounds.left.max(clip.left),
        bounds.top.max(clip.top),
        bounds.right.min(clip.right),
        bounds.bottom.min(clip.bottom),
    );
    (rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

fn write_string(out: &mut Writer, text: &str) {
    out.u32(text.len() as u32);
    out.bytes.extend_from_slice(text.as_bytes());
}

fn join(texts: Option<&compose_ui::StringList>) -> String {
    texts.into_iter().flatten().map(|text| text.as_str()).collect::<Vec<_>>().join(" ")
}

fn text_of(text: Option<&SharedString>) -> &str {
    text.map_or("", |text| text.as_str())
}

/// The node's flags: its actions from its own configuration, as Compose's accessibility reads
/// them (`unmergedConfig`), the rest from the merged one.
fn flags(node: &SemanticsNode) -> u32 {
    let config = &node.config;
    let mut flags = 0;
    let set = |flags: &mut u32, on: bool, bit: u32| {
        if on {
            *flags |= bit;
        }
    };
    set(&mut flags, node.unmerged_config().contains(&A::ON_CLICK), flag::CLICKABLE);
    set(&mut flags, config.contains(&P::FOCUSED), flag::FOCUSABLE);
    set(&mut flags, config.get(&P::FOCUSED) == Some(&true), flag::FOCUSED);
    set(&mut flags, config.contains(&P::EDITABLE_TEXT), flag::EDITABLE);
    set(&mut flags, config.contains(&P::PASSWORD), flag::PASSWORD);
    set(&mut flags, config.contains(&P::DISABLED), flag::DISABLED);
    set(&mut flags, config.contains(&P::IS_DIALOG), flag::DIALOG);
    set(&mut flags, config.contains(&P::PROGRESS_BAR_RANGE_INFO), flag::RANGE);
    if let Some(&state) = config.get(&P::TOGGLEABLE_STATE) {
        flags |= flag::CHECKABLE;
        set(&mut flags, state == ToggleableState::On, flag::CHECKED);
    }
    if let Some(&selected) = config.get(&P::SELECTED) {
        // A radio button's selection is its checked state, as Android's.
        if config.get(&P::ROLE) == Some(&Role::RadioButton) {
            flags |= flag::CHECKABLE;
            set(&mut flags, selected, flag::CHECKED);
        } else {
            set(&mut flags, selected, flag::SELECTED);
        }
    }
    flags
}

/// Record: id (i32), parent (i32), bounds (4 × f32, clipped by the scrolling containers around:
/// `clip`), flags (u32), role (u8: 0 none, 1 button, 2 checkbox, 3 switch, 4 radio button,
/// 5 tab, 6 image, 7 dropdown list), text, content description, editable text, state
/// description, error (each a length and UTF-8 bytes), the range (current, min, max: 3 × f32)
/// if the range flag is set, and the scroll (offset, largest, the viewport's length along its
/// axis: 3 × f32, and the scroll actions offered: u8, `scroll_action`) if the scroll flag is.
fn write_node(node: &SemanticsNode, parent: i32, clip: Rect, out: &mut Writer) {
    let config = &node.config;
    let id = virtual_id(node.id);
    let visible = clipped(node.bounds, clip);
    let bounds = visible.unwrap_or(node.bounds);
    let scroll = scroll_range(node);
    let mut flags = flags(node);
    if visible.is_none() {
        flags |= flag::HIDDEN;
    }
    if node.rtl {
        flags |= flag::RTL;
    }
    if let Some((range, horizontal)) = scroll {
        flags |= flag::SCROLL;
        if horizontal {
            flags |= flag::SCROLL_HORIZONTAL;
        }
        if range.reverse_scrolling {
            flags |= flag::SCROLL_REVERSED;
        }
    }
    out.u32(id as u32);
    out.u32(parent as u32);
    out.f32(bounds.left);
    out.f32(bounds.top);
    out.f32(bounds.right);
    out.f32(bounds.bottom);
    out.u32(flags);
    out.u8(match config.get(&P::ROLE) {
        None => 0,
        Some(Role::Button) => 1,
        Some(Role::Checkbox) => 2,
        Some(Role::Switch) => 3,
        Some(Role::RadioButton) => 4,
        Some(Role::Tab) => 5,
        Some(Role::Image) => 6,
        Some(Role::DropdownList) => 7,
        // A role added later: none the platform knows.
        Some(_) => 0,
    });
    write_string(out, &join(config.get(&P::TEXT)));
    write_string(out, &join(config.get(&P::CONTENT_DESCRIPTION)));
    write_string(out, text_of(config.get(&P::EDITABLE_TEXT)));
    write_string(out, text_of(config.get(&P::STATE_DESCRIPTION)));
    write_string(out, text_of(config.get(&P::ERROR)));
    if let Some(info) = config.get(&P::PROGRESS_BAR_RANGE_INFO) {
        out.f32(info.current);
        out.f32(info.range.0);
        out.f32(info.range.1);
    }
    if let Some((range, horizontal)) = scroll {
        out.f32(range.value);
        out.f32(range.max_value);
        // A page: the viewport's length along the axis, as shown (clipped), as Compose's
        // `boundsInParent`.
        out.f32(if horizontal { bounds.width() } else { bounds.height() });
        out.u8(scroll_actions(node));
    }
    // A scrolling container clips what's in it.
    let inner =
        if scroll.is_some() { visible.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)) } else { clip };
    for child in &node.children {
        write_node(child, id, inner, out);
    }
}

/// The scroll range of `node` along its axis (horizontal: `true`), from its own
/// configuration, as Compose's accessibility reads it (D491): vertical first.
fn scroll_range(node: &SemanticsNode) -> Option<(compose_ui::ScrollAxisRange, bool)> {
    let own = node.unmerged_config();
    own.get(&P::VERTICAL_SCROLL_AXIS_RANGE)
        .map(|&range| (range, false))
        .or(own.get(&P::HORIZONTAL_SCROLL_AXIS_RANGE).map(|&range| (range, true)))
}

/// The node of virtual id `id` in `root`'s tree.
pub fn find(root: &SemanticsNode, id: i32) -> Option<&SemanticsNode> {
    if virtual_id(root.id) == id {
        return Some(root);
    }
    root.children.iter().find_map(|child| find(child, id))
}

/// The scroll actions of a node's record, as bits: what `Accessibility.kt` offers and asks
/// (`ComposeRsNative.performAction`'s actions 3 to 8, in this order).
pub mod scroll_action {
    pub const FORWARD: u8 = 1;
    pub const BACKWARD: u8 = 1 << 1;
    pub const UP: u8 = 1 << 2;
    pub const DOWN: u8 = 1 << 3;
    pub const LEFT: u8 = 1 << 4;
    pub const RIGHT: u8 = 1 << 5;
}

/// The scroll actions `node` offers, as Compose's `AndroidComposeViewAccessibilityDelegateCompat`
/// (D491): forward where it can scroll forward (towards its end, or its start if
/// `reverse_scrolling`), paired with right (left when right to left) or down; backward and
/// left (right) or up the other way.
pub fn scroll_actions(node: &SemanticsNode) -> u8 {
    use scroll_action::*;
    let forward = |range: compose_ui::ScrollAxisRange| {
        if range.reverse_scrolling { range.value > 0.0 } else { range.value < range.max_value }
    };
    let backward = |range: compose_ui::ScrollAxisRange| {
        if range.reverse_scrolling { range.value < range.max_value } else { range.value > 0.0 }
    };
    let mut actions = 0;
    let own = node.unmerged_config();
    if let Some(&range) = own.get(&P::HORIZONTAL_SCROLL_AXIS_RANGE) {
        if forward(range) {
            actions |= FORWARD | if node.rtl { LEFT } else { RIGHT };
        }
        if backward(range) {
            actions |= BACKWARD | if node.rtl { RIGHT } else { LEFT };
        }
    }
    if let Some(&range) = own.get(&P::VERTICAL_SCROLL_AXIS_RANGE) {
        if forward(range) {
            actions |= FORWARD | DOWN;
        }
        if backward(range) {
            actions |= BACKWARD | UP;
        }
    }
    actions
}

/// The node of virtual id `id` in `root`'s tree, with its bounds as shown: clipped by the
/// scrolling containers around it, as the record's.
pub fn find_shown(root: &SemanticsNode, id: i32) -> Option<(&SemanticsNode, Rect)> {
    fn visit(node: &SemanticsNode, id: i32, clip: Rect) -> Option<(&SemanticsNode, Rect)> {
        let shown = clipped(node.bounds, clip);
        if virtual_id(node.id) == id {
            return Some((node, shown.unwrap_or(node.bounds)));
        }
        let scrolls = scroll_range(node).is_some();
        let inner = if scrolls { shown.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)) } else { clip };
        node.children.iter().find_map(|child| visit(child, id, inner))
    }
    if virtual_id(root.id) == id {
        return Some((root, root.bounds));
    }
    root.children.iter().find_map(|child| visit(child, id, root.bounds))
}

/// What `action` (one `scroll_action` bit) scrolls `node` by, `page` its viewport as shown (its
/// width a page horizontally, its height vertically):
/// the `ScrollBy` amounts, as Compose's delegate works them out (a page, back for backward,
/// left or up, the other way if `reverse_scrolling`, and for left or right when right to
/// left); `None` if it can't scroll that way (D491).
pub fn scroll_amount(node: &SemanticsNode, action: u8, page: Rect) -> Option<(f32, f32)> {
    use scroll_action::*;
    let can = |range: compose_ui::ScrollAxisRange, amount: f32| {
        (amount < 0.0 && range.value > 0.0) || (amount > 0.0 && range.value < range.max_value)
    };
    let own = node.unmerged_config();
    if let Some(&range) = own.get(&P::HORIZONTAL_SCROLL_AXIS_RANGE)
        && action & (FORWARD | BACKWARD | LEFT | RIGHT) != 0
    {
        let mut amount = page.width();
        if action & (LEFT | BACKWARD) != 0 {
            amount = -amount;
        }
        if range.reverse_scrolling {
            amount = -amount;
        }
        if node.rtl && action & (LEFT | RIGHT) != 0 {
            amount = -amount;
        }
        if can(range, amount) {
            return Some((amount, 0.0));
        }
    }
    if let Some(&range) = own.get(&P::VERTICAL_SCROLL_AXIS_RANGE)
        && action & (FORWARD | BACKWARD | UP | DOWN) != 0
    {
        let mut amount = page.height();
        if action & (UP | BACKWARD) != 0 {
            amount = -amount;
        }
        if range.reverse_scrolling {
            amount = -amount;
        }
        if can(range, amount) {
            return Some((0.0, amount));
        }
    }
    None
}
