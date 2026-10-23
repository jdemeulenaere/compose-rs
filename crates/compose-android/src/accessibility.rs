//! The semantics tree for Android's accessibility (docs/07-platforms.md, "The rest of the
//! platform glue"): `Accessibility.kt`, an `AccessibilityNodeProvider`, reads it as virtual
//! views when a service (TalkBack, Switch Access) asks.
//!
//! The merged tree is written in pre-order, one record per node: its virtual id (its node's
//! index), its parent's (-1 for the root's children), its bounds in the view's pixels, flags,
//! role, and its texts. Little-endian, as the display list.

use compose_ui::{
    Rect, Role, SemanticsConfiguration, ToggleableState,
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

fn join(texts: &[compose_ui::SharedString]) -> String {
    texts.iter().map(|text| text.as_str()).collect::<Vec<_>>().join(" ")
}

fn flags(config: &SemanticsConfiguration) -> u32 {
    let mut flags = 0;
    let set = |flags: &mut u32, on: bool, bit: u32| {
        if on {
            *flags |= bit;
        }
    };
    set(&mut flags, config.clickable, flag::CLICKABLE);
    set(&mut flags, config.focusable, flag::FOCUSABLE);
    set(&mut flags, config.focused, flag::FOCUSED);
    set(&mut flags, config.editable, flag::EDITABLE);
    set(&mut flags, config.password, flag::PASSWORD);
    set(&mut flags, config.disabled, flag::DISABLED);
    set(&mut flags, config.is_dialog, flag::DIALOG);
    set(&mut flags, config.progress_bar_range_info.is_some(), flag::RANGE);
    if let Some(state) = config.toggleable_state {
        flags |= flag::CHECKABLE;
        set(&mut flags, state == ToggleableState::On, flag::CHECKED);
    }
    if let Some(selected) = config.selected {
        // A radio button's selection is its checked state, as Android's.
        if config.role == Some(Role::RadioButton) {
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
/// axis: 3 × f32) if the scroll flag is.
fn write_node(node: &SemanticsNode, parent: i32, clip: Rect, out: &mut Writer) {
    let config = &node.config;
    let id = virtual_id(node.id);
    let visible = clipped(node.bounds, clip);
    let bounds = visible.unwrap_or(node.bounds);
    let scroll = config
        .vertical_scroll_axis_range
        .map(|range| (range, false))
        .or(config.horizontal_scroll_axis_range.map(|range| (range, true)));
    let mut flags = flags(config);
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
    out.u8(match config.role {
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
    write_string(out, &join(&config.text));
    write_string(out, &join(&config.content_description));
    write_string(out, config.editable_text.as_deref().unwrap_or(""));
    write_string(out, config.state_description.as_deref().unwrap_or(""));
    write_string(out, config.error.as_deref().unwrap_or(""));
    if let Some(info) = config.progress_bar_range_info {
        out.f32(info.current);
        out.f32(info.range.0);
        out.f32(info.range.1);
    }
    if let Some((range, horizontal)) = scroll {
        out.f32(range.value);
        out.f32(range.max_value);
        // A page: the viewport's length along the axis, unclipped.
        out.f32(if horizontal { node.bounds.width() } else { node.bounds.height() });
    }
    // A scrolling container clips what's in it.
    let inner =
        if scroll.is_some() { visible.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)) } else { clip };
    for child in &node.children {
        write_node(child, id, inner, out);
    }
}

/// The node of virtual id `id` in `root`'s tree.
pub fn find(root: &SemanticsNode, id: i32) -> Option<&SemanticsNode> {
    if virtual_id(root.id) == id {
        return Some(root);
    }
    root.children.iter().find_map(|child| find(child, id))
}
