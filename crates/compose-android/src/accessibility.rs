//! The semantics tree for Android's accessibility (docs/07-platforms.md, "The rest of the
//! platform glue"): `Accessibility.kt`, an `AccessibilityNodeProvider`, reads it as virtual
//! views when a service (TalkBack, Switch Access) asks.
//!
//! The merged tree is written in pre-order, one record per node: its virtual id (its semantics
//! id), its parent's (-1 for the root's children), its bounds in the view's pixels, flags,
//! role, and its texts. Little-endian, as the display list.

use compose_ui::{
    Rect, Role, SemanticsActions as A, SemanticsProperties as P, SharedString, ToggleableState,
    platform::{FrameStats, NodeId, SemanticsSnapshot, UiHost},
};

use std::collections::HashMap;

use crate::serialize::Writer;
use crate::text::{byte_offset, utf16_index};
use crate::text_segments::{Granularity, TextSegments, movement};

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
    /// A heading, as `heading()`.
    pub const HEADING: u32 = 1 << 16;
    /// A service focuses on it, as `isScreenReaderFocusable`.
    pub const SCREEN_READER_FOCUSABLE: u32 = 1 << 17;
    /// A live region, as `liveRegion`: a service says a change to it on its own.
    pub const LIVE_POLITE: u32 = 1 << 18;
    pub const LIVE_ASSERTIVE: u32 = 1 << 19;
    pub const LONG_CLICKABLE: u32 = 1 << 20;
    /// It means something to a service, as `isImportantForAccessibility`.
    pub const IMPORTANT: u32 = 1 << 21;
    /// Only an accessibility tool is told of it, as `isSensitiveData`.
    pub const SENSITIVE: u32 = 1 << 22;
    /// Its test tag is its resource id, as `testTagsAsResourceId`.
    pub const TAG_IS_RESOURCE_ID: u32 = 1 << 23;
    /// Its characters' places can be asked for (`character_locations`).
    pub const TEXT_LAID_OUT: u32 = 1 << 24;
    /// Its text can be changed, as `IsEditable`: `EDITABLE` says it's a field.
    pub const CAN_EDIT: u32 = 1 << 25;
}

/// What a service reads that isn't the application's words, which `Accessibility.kt` finds in the
/// library's resources, so that they can be translated (D1294): a state or a role description.
pub mod spoken {
    pub const NONE: u8 = 0;
    pub const ON: u8 = 1;
    pub const OFF: u8 = 2;
    pub const PARTIALLY_CHECKED: u8 = 3;
    pub const SELECTED: u8 = 4;
    pub const NOT_SELECTED: u8 = 5;
    /// A percent follows.
    pub const PERCENT: u8 = 6;
    pub const IN_PROGRESS: u8 = 7;
    pub const EMPTY: u8 = 8;
    pub const TAB: u8 = 9;
    pub const SWITCH: u8 = 10;
}

/// The actions of a node's record, what `ComposeRsNative.performAction` takes: `Accessibility.kt`
/// says which of Android's each is (D1289).
pub mod action {
    pub const CLICK: u8 = 0;
    pub const FOCUS: u8 = 1;
    pub const SET_PROGRESS: u8 = 2;
    /// 3 to 8, `scroll_action`'s bits in order.
    pub const SCROLL_FIRST: u8 = 3;
    pub const LONG_CLICK: u8 = 9;
    pub const EXPAND: u8 = 10;
    pub const COLLAPSE: u8 = 11;
    pub const DISMISS: u8 = 12;
    pub const PAGE_UP: u8 = 13;
    pub const PAGE_DOWN: u8 = 14;
    pub const PAGE_LEFT: u8 = 15;
    pub const PAGE_RIGHT: u8 = 16;
    /// A step of a value, forward and backward.
    pub const STEP_FORWARD: u8 = 17;
    pub const STEP_BACKWARD: u8 = 18;
    pub const CLEAR_FOCUS: u8 = 19;
    pub const SET_TEXT: u8 = 20;
    pub const IME_ENTER: u8 = 21;
    pub const COPY: u8 = 22;
    pub const CUT: u8 = 23;
    pub const PASTE: u8 = 24;
    pub const SET_SELECTION: u8 = 25;
    pub const NEXT_AT_GRANULARITY: u8 = 26;
    pub const PREVIOUS_AT_GRANULARITY: u8 = 27;
    /// `CUSTOM_FIRST + i`: its custom action kept as `i` (`KeptCustomActions`).
    pub const CUSTOM_FIRST: u8 = 100;
}

/// Whether a frame that did `stats` may have changed the semantics (`frame`'s `SEMANTICS`): it
/// recomposed, laid something out, or a modifier node said its semantics changed
/// (`FrameStats::semantics_changed`, D1158). `Accessibility.kt` then reads the tree again when next
/// asked, whether or not a service is on (a field write, as Compose's
/// `currentSemanticsNodesInvalidated`), and tells the services on, if any (D630).
pub fn semantics_may_have_changed(stats: &FrameStats) -> bool {
    stats.recomposed || stats.measured || stats.semantics_changed
}

/// Whether a service should hear it at once, not within 100 ms: a frame where a property changed,
/// as Compose's `onSemanticsChange`; only where things are waits, as its bounds' batches.
pub fn tell_at_once(stats: &FrameStats) -> bool {
    stats.semantics_changed
}

/// The merged tree the bridge keeps, for the record, the actions and the input method's options:
/// built when first asked after a frame that may have changed it, or a change since the last
/// frame (a focus moved by an event), as they'd each build it otherwise (D1259).
#[derive(Default)]
pub struct KeptSemanticsTree(Option<SemanticsSnapshot>);

impl KeptSemanticsTree {
    /// A frame that did `stats` ran: returns whether the semantics may have changed
    /// (`semantics_may_have_changed`), forgetting the tree if so.
    pub fn after_frame(&mut self, stats: &FrameStats) -> bool {
        let changed = semantics_may_have_changed(stats);
        if changed {
            self.0 = None;
        }
        changed
    }

    /// The tree, as it is now.
    pub fn get(&mut self, ui: &UiHost) -> &SemanticsSnapshot {
        if ui.semantics_changed_since_frame() {
            self.0 = None;
        }
        self.0.get_or_insert_with(|| ui.accessibility_tree())
    }
}

/// A node's virtual view id: its semantics id, which no other node or content had (D1262).
pub fn virtual_id(node: &SemanticsSnapshot) -> i32 {
    node.semantics_id
}

/// The ids a node's custom actions have, by label: each keeps its own across the builds of a tree,
/// so that a service asking for what it was shown gets that, or nothing if it's gone, as Compose's
/// `labelToActionId` (D1295).
#[derive(Default)]
pub struct KeptCustomActions(HashMap<i32, Vec<(SharedString, u8)>>);

impl KeptCustomActions {
    /// Gives each custom action in `root`'s tree an id: a label's kept, the free ones for the rest.
    pub fn assign(&mut self, root: &SemanticsSnapshot) {
        let mut kept = HashMap::new();
        self.assign_under(root, &mut kept);
        self.0 = kept;
    }

    fn assign_under(
        &self,
        node: &SemanticsSnapshot,
        kept: &mut HashMap<i32, Vec<(SharedString, u8)>>,
    ) {
        if let Some(actions) = node.unmerged_config().get(&A::CUSTOM_ACTIONS) {
            let before = self.0.get(&virtual_id(node));
            let mut ids: Vec<(SharedString, u8)> = Vec::new();
            for custom in actions.iter() {
                if ids.iter().any(|(label, _)| *label == custom.label) {
                    continue;
                }
                let had = before.and_then(|before| {
                    before.iter().find(|(label, _)| *label == custom.label).map(|(_, id)| *id)
                });
                if let Some(id) = had {
                    ids.push((custom.label.clone(), id));
                }
            }
            for custom in actions.iter() {
                if ids.iter().any(|(label, _)| *label == custom.label) {
                    continue;
                }
                let free = (0..MOST_CUSTOM_ACTIONS).find(|id| ids.iter().all(|(_, it)| it != id));
                if let Some(id) = free {
                    ids.push((custom.label.clone(), id));
                }
            }
            kept.insert(virtual_id(node), ids);
        }
        for child in &node.children {
            self.assign_under(child, kept);
        }
    }

    /// `node`'s custom actions, with their ids, in the order it has them.
    fn of(&self, node: &SemanticsSnapshot) -> Vec<(u8, SharedString)> {
        let Some(actions) = node.unmerged_config().get(&A::CUSTOM_ACTIONS) else {
            return Vec::new();
        };
        let ids = self.0.get(&virtual_id(node));
        actions
            .iter()
            .filter_map(|custom| {
                let ids = ids?;
                let (_, id) = ids.iter().find(|(label, _)| *label == custom.label)?;
                Some((*id, custom.label.clone()))
            })
            .collect()
    }

    /// The label of `id` among `node`'s, if it still has it.
    pub fn label_of(&self, node: i32, id: u8) -> Option<SharedString> {
        let ids = self.0.get(&node)?;
        ids.iter().find(|(_, it)| *it == id).map(|(label, _)| label.clone())
    }
}

/// As Compose's 32 `accessibility_custom_action_` ids.
const MOST_CUSTOM_ACTIONS: u8 = 32;

/// Writes the merged tree `root` (its children: the root itself is the view).
pub fn write_tree(root: &SemanticsSnapshot, custom: &mut KeptCustomActions, out: &mut Writer) {
    write_tree_with_cursor(root, custom, &TextCursor::default(), out);
}

/// `write_tree`, a text's selection where a service left its cursor (`text_selection_of`).
pub fn write_tree_with_cursor(
    root: &SemanticsSnapshot,
    custom: &mut KeptCustomActions,
    cursor: &TextCursor,
    out: &mut Writer,
) {
    out.bytes.clear();
    custom.assign(root);
    let before = compose_ui::traversal_before(root);
    let tags_are_ids = tags_are_resource_ids(root, false);
    for (order, child) in in_written_order(root) {
        let inherited = Inherited { clip: root.bounds, tags_are_ids };
        write_node(child, -1, inherited, &before, custom, cursor, order, out);
    }
}

/// What a node gets from those around it.
#[derive(Clone, Copy)]
struct Inherited {
    /// What the scrolling containers around it clip it to.
    clip: Rect,
    /// Whether its test tag is a resource id, unless it says otherwise.
    tags_are_ids: bool,
}

fn tags_are_resource_ids(node: &SemanticsSnapshot, around: bool) -> bool {
    node.unmerged_config().get(&P::TEST_TAGS_AS_RESOURCE_ID).copied().unwrap_or(around)
}

/// As Compose's `ParcelSafeTextLength`: what goes in a node or an event, at most so many UTF-16 units.
const PARCEL_SAFE_LENGTH: usize = 100_000;

/// `text` cut to what's safe to send, a surrogate pair whole, as Compose's `trimToSize`.
pub fn parcel_safe(text: &str) -> &str {
    // Never fewer bytes than units.
    if text.len() <= PARCEL_SAFE_LENGTH {
        return text;
    }
    let mut units = 0;
    for (at, c) in text.char_indices() {
        units += c.len_utf16();
        if units > PARCEL_SAFE_LENGTH {
            return &text[..at];
        }
    }
    text
}

/// The text and description `node` is read by. One record stands for a merging node and all it
/// merged, and a service reading a description doesn't read a text beside it: with both, the
/// description says all of it in order; a field's value being its own, all of it is its hint then
/// (D1653).
pub fn read_as(node: &SemanticsSnapshot) -> (String, String) {
    let config = &node.config;
    match &node.reading_order {
        Some(reading) if config.contains(&P::EDITABLE_TEXT) => {
            (reading.as_str().to_string(), String::new())
        }
        Some(reading) => (join(config.get(&P::TEXT)), reading.as_str().to_string()),
        None => (join(config.get(&P::TEXT)), join(config.get(&P::CONTENT_DESCRIPTION))),
    }
}

/// Children with their drawing order, as a node has them: by `traversal_index` for a merging
/// traversal group, as Compose's (`isTraversalGroup && isMerging && childrenSize > 1`), a scrim's
/// index otherwise leaving it in its place under the sheet.
fn in_written_order(node: &SemanticsSnapshot) -> Vec<(usize, &SemanticsSnapshot)> {
    let own = node.unmerged_config();
    let mut children: Vec<_> = node.children.iter().enumerate().collect();
    let sorts = own.get(&P::IS_TRAVERSAL_GROUP) == Some(&true)
        && own.is_merging_semantics_of_descendants
        && children.len() > 1;
    if sorts {
        let index = |child: &SemanticsSnapshot| {
            child.unmerged_config().get(&P::TRAVERSAL_INDEX).copied().unwrap_or(0.0)
        };
        children.sort_by(|a, b| index(a.1).total_cmp(&index(b.1)));
    }
    children
}

/// The node a touch at `(x, y)` in the root finds for a service, as Compose's
/// `hitTestSemanticsAndInteropAt`: front to back, in drawing order, those not important looked
/// through to what's around them; a sibling under one hit isn't looked at.
pub fn node_at(root: &SemanticsSnapshot, x: f32, y: f32) -> Option<i32> {
    enum Hit {
        Nothing,
        /// Something's here, that a service doesn't read, so what's around it is asked.
        LookedThrough,
        Found(i32),
    }
    fn visit(node: &SemanticsSnapshot, x: f32, y: f32, clip: Rect) -> Hit {
        let shown = clipped(node.bounds, clip);
        let scrolls = scroll_range(node).is_some();
        let inner = if scrolls { shown.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)) } else { clip };
        let mut hit = Hit::Nothing;
        for child in node.children.iter().rev() {
            match visit(child, x, y, inner) {
                Hit::Nothing => continue,
                found @ Hit::Found(_) => return found,
                Hit::LookedThrough => {
                    hit = Hit::LookedThrough;
                    break;
                }
            }
        }
        let on_it = shown.is_some_and(|shown| {
            x >= shown.left && x < shown.right && y >= shown.top && y < shown.bottom
        });
        if !on_it {
            return hit;
        }
        if compose_ui::is_important_for_accessibility(node) {
            return Hit::Found(virtual_id(node));
        }
        Hit::LookedThrough
    }
    for child in root.children.iter().rev() {
        match visit(child, x, y, root.bounds) {
            Hit::Nothing => continue,
            Hit::Found(id) => return Some(id),
            Hit::LookedThrough => return None,
        }
    }
    None
}

use compose_ui::clipped_by as clipped;

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
fn flags(node: &SemanticsSnapshot) -> u32 {
    let config = &node.config;
    let mut flags = 0;
    let set = |flags: &mut u32, on: bool, bit: u32| {
        if on {
            *flags |= bit;
        }
    };
    set(&mut flags, is_clickable(node), flag::CLICKABLE);
    set(&mut flags, node.unmerged_config().contains(&A::ON_LONG_CLICK), flag::LONG_CLICKABLE);
    set(&mut flags, compose_ui::is_important_for_accessibility(node), flag::IMPORTANT);
    // Focusable as Compose's `isFocusable`, from the node's own configuration.
    set(&mut flags, node.unmerged_config().contains(&P::FOCUSED), flag::FOCUSABLE);
    set(&mut flags, config.get(&P::FOCUSED) == Some(&true), flag::FOCUSED);
    set(&mut flags, config.contains(&P::EDITABLE_TEXT), flag::EDITABLE);
    set(&mut flags, config.get(&P::IS_EDITABLE) == Some(&true), flag::CAN_EDIT);
    set(&mut flags, is_password(node), flag::PASSWORD);
    set(&mut flags, config.contains(&P::DISABLED), flag::DISABLED);
    set(&mut flags, config.contains(&P::IS_DIALOG), flag::DIALOG);
    set(&mut flags, determinate_range(node).is_some(), flag::RANGE);
    set(&mut flags, config.contains(&P::HEADING), flag::HEADING);
    set(&mut flags, compose_ui::is_screen_reader_focusable(node), flag::SCREEN_READER_FOCUSABLE);
    set(&mut flags, config.get(&P::IS_SENSITIVE_DATA) == Some(&true), flag::SENSITIVE);
    set(
        &mut flags,
        node.unmerged_config().contains(&A::GET_TEXT_LAYOUT_RESULT),
        flag::TEXT_LAID_OUT,
    );
    match node.unmerged_config().get(&P::LIVE_REGION) {
        Some(compose_ui::LiveRegionMode::Polite) => flags |= flag::LIVE_POLITE,
        Some(compose_ui::LiveRegionMode::Assertive) => flags |= flag::LIVE_ASSERTIVE,
        None => {}
    }
    if let Some(&state) = config.get(&P::TOGGLEABLE_STATE) {
        flags |= flag::CHECKABLE;
        set(&mut flags, state == ToggleableState::On, flag::CHECKED);
    }
    if let Some(&selected) = config.get(&P::SELECTED) {
        // A tab's is the platform's selected; anything else's is checked, its state saying
        // "selected", as Compose's (`getInfoIsCheckable`).
        if config.get(&P::ROLE) == Some(&Role::Tab) {
            set(&mut flags, selected, flag::SELECTED);
        } else {
            flags |= flag::CHECKABLE;
            set(&mut flags, selected, flag::CHECKED);
        }
    }
    flags
}

/// Whether a click is offered: a selected tab or radio button can't be selected again.
fn is_clickable(node: &SemanticsSnapshot) -> bool {
    let own = node.unmerged_config();
    let one_of = matches!(own.get(&P::ROLE), Some(Role::Tab | Role::RadioButton));
    own.contains(&A::ON_CLICK) && !(one_of && own.get(&P::SELECTED) == Some(&true))
}

/// A password shown masked, as Compose's `isPassword`.
fn is_password(node: &SemanticsSnapshot) -> bool {
    node.config.contains(&P::PASSWORD)
        && node.config.get(&P::IS_PASSWORD_OBFUSCATED) != Some(&false)
}

/// The node's range, an indeterminate one having none.
fn determinate_range(node: &SemanticsSnapshot) -> Option<compose_ui::ProgressBarRangeInfo> {
    let range = node.config.get(&P::PROGRESS_BAR_RANGE_INFO)?;
    (*range != compose_ui::ProgressBarRangeInfo::INDETERMINATE).then_some(*range)
}

/// What a service says of the node's state, as Compose's `getInfoStateDescriptionOrNull`: the
/// application's words first, then what its state says; a field that says nothing is "empty",
/// for a service not to step over it. Returns a `spoken` code and a percent.
pub fn state_description_of(node: &SemanticsSnapshot) -> (u8, u8) {
    let config = &node.config;
    let role = config.get(&P::ROLE).copied();
    if config.contains(&P::EDITABLE_TEXT) {
        let silent = |text: Option<&compose_ui::StringList>| text.is_none_or(|all| all.is_empty());
        let empty = silent(config.get(&P::CONTENT_DESCRIPTION))
            && silent(config.get(&P::TEXT))
            && config.get(&P::EDITABLE_TEXT).is_none_or(|text| text.as_str().is_empty())
            && config.get(&P::HINT_TEXT).is_none_or(|text| text.as_str().is_empty());
        return (if empty { spoken::EMPTY } else { spoken::NONE }, 0);
    }
    if config.contains(&P::STATE_DESCRIPTION) {
        return (spoken::NONE, 0);
    }
    match config.get(&P::TOGGLEABLE_STATE) {
        // A switch: a service's own "checked" is wrong for it, as Compose says of TalkBack.
        Some(ToggleableState::On) if role == Some(Role::Switch) => return (spoken::ON, 0),
        Some(ToggleableState::Off) if role == Some(Role::Switch) => return (spoken::OFF, 0),
        Some(ToggleableState::Indeterminate) => return (spoken::PARTIALLY_CHECKED, 0),
        _ => {}
    }
    if let Some(&selected) = config.get(&P::SELECTED)
        && role != Some(Role::Tab)
    {
        return (if selected { spoken::SELECTED } else { spoken::NOT_SELECTED }, 0);
    }
    if config.contains(&P::PROGRESS_BAR_RANGE_INFO) {
        let Some(range) = determinate_range(node) else { return (spoken::IN_PROGRESS, 0) };
        let length = range.range.1 - range.range.0;
        let progress = if length == 0.0 { 0.0 } else { (range.current - range.range.0) / length };
        let progress = progress.clamp(0.0, 1.0);
        // 0 % and 100 % only when it's exactly that.
        let percent = if progress == 0.0 {
            0
        } else if progress == 1.0 {
            100
        } else {
            ((progress * 100.0).round() as u8).clamp(1, 99)
        };
        return (spoken::PERCENT, percent);
    }
    (spoken::NONE, 0)
}

/// Record: id (i32), parent (i32), bounds (4 × f32, clipped by the scrolling containers around:
/// `clip`), flags (u32), role (u8: 0 none, 1 button, 2 checkbox, 3 switch, 4 radio button,
/// 5 tab, 6 image, 7 dropdown list), text, content description, editable text, state
/// description, error, pane title (each a length and UTF-8 bytes), the range (current, min, max: 3 × f32)
/// if the range flag is set, and the scroll (offset, largest, the viewport's length along its
/// axis: 3 × f32, and the scroll actions offered: u8, `scroll_action`) if the scroll flag is; then
/// who a service reads it before (i32, -1 for none), its drawing order (u32), its class name (a
/// length and UTF-8 bytes), its role description, its state and that state's percent (three
/// `spoken`-code u8s), its selection (2 × i32, -1 for none) and how it's moved through (u8, 0:
/// it isn't, `movement_granularities`), its test tag (a length and UTF-8 bytes), and its actions
/// (a count: u8; each `action`'s u8 and a label).
#[allow(clippy::too_many_arguments)]
fn write_node(
    node: &SemanticsSnapshot,
    parent: i32,
    around: Inherited,
    before: &HashMap<i32, i32>,
    custom: &KeptCustomActions,
    cursor: &TextCursor,
    drawing_order: usize,
    out: &mut Writer,
) {
    let config = &node.config;
    let id = virtual_id(node);
    let clip = around.clip;
    let visible = clipped(node.bounds, clip);
    let tags_are_ids = tags_are_resource_ids(node, around.tags_are_ids);
    let bounds = visible.unwrap_or(node.bounds);
    let scroll = scroll_range(node);
    let mut flags = flags(node);
    if visible.is_none() || node.hidden {
        flags |= flag::HIDDEN;
    }
    if node.rtl {
        flags |= flag::RTL;
    }
    if tags_are_ids {
        flags |= flag::TAG_IS_RESOURCE_ID;
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
    let (text, description) = read_as(node);
    write_string(out, parcel_safe(&text));
    write_string(out, parcel_safe(&description));
    write_string(out, parcel_safe(text_of(config.get(&P::EDITABLE_TEXT))));
    write_string(out, text_of(config.get(&P::STATE_DESCRIPTION)));
    write_string(out, text_of(config.get(&P::ERROR)));
    // From its own configuration, as Compose's delegate.
    write_string(out, text_of(node.unmerged_config().get(&P::PANE_TITLE)));
    if let Some(info) = determinate_range(node) {
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
    out.u32(before.get(&id).copied().unwrap_or(-1) as u32);
    out.u32(drawing_order as u32);
    let (class_name, role_description) = class_and_role(node);
    write_string(out, class_name);
    out.u8(role_description);
    let (state, percent) = state_description_of(node);
    out.u8(state);
    out.u8(percent);
    let (selection_start, selection_end) = text_selection_of(node, cursor);
    out.u32(selection_start as u32);
    out.u32(selection_end as u32);
    out.u8(movement_granularities(node) as u8);
    write_string(out, text_of(node.unmerged_config().get(&P::TEST_TAG)));
    let actions = actions_of(node, custom);
    out.u8(actions.len() as u8);
    for (action, label) in &actions {
        out.u8(*action);
        write_string(out, label.as_ref().map_or("", |label| label.as_str()));
    }
    // A scrolling container clips what's in it.
    let inner =
        if scroll.is_some() { visible.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)) } else { clip };
    let inherited = Inherited { clip: inner, tags_are_ids };
    for (order, child) in in_written_order(node) {
        write_node(child, id, inherited, before, custom, cursor, order, out);
    }
}

/// The class name and role description a service reads, from the node's own configuration, as
/// Compose's delegate: a field's or a text's, a role's where nothing's below it (an image's only
/// if it's what's focused on), a scrolling container's so a service scrolls it; last, what the
/// application says it is (`accessibility_class_name`). A merging node's stays with children left
/// beside it, where Compose's would be in a fake node among them (D1653).
pub fn class_and_role(node: &SemanticsSnapshot) -> (&str, u8) {
    let own = node.unmerged_config();
    let mut class = "android.view.View";
    let mut role_description = spoken::NONE;
    if own.contains(&P::EDITABLE_TEXT) {
        class = "android.widget.EditText";
    }
    if own.contains(&P::TEXT) {
        class = "android.widget.TextView";
    }
    let stands_for_itself = node.children.is_empty() || own.is_merging_semantics_of_descendants;
    if let Some(role) = own.get(&P::ROLE).filter(|_| stands_for_itself) {
        match role {
            Role::Tab => role_description = spoken::TAB,
            Role::Switch => role_description = spoken::SWITCH,
            Role::Button => class = "android.widget.Button",
            Role::Checkbox => class = "android.widget.CheckBox",
            Role::RadioButton => class = "android.widget.RadioButton",
            Role::Image => class = "android.widget.ImageView",
            Role::DropdownList => class = "android.widget.Spinner",
            Role::ValuePicker => class = "android.widget.NumberPicker",
            // A role added later: none the platform knows.
            _ => {}
        }
    }
    if own.contains(&P::PROGRESS_BAR_RANGE_INFO) {
        class = if own.contains(&A::SET_PROGRESS) {
            "android.widget.SeekBar"
        } else {
            "android.widget.ProgressBar"
        };
    }
    let has_collection = own.contains(&P::COLLECTION_INFO);
    if own.contains(&A::SCROLL_BY) && !has_collection {
        if own.contains(&P::HORIZONTAL_SCROLL_AXIS_RANGE) {
            class = "android.widget.HorizontalScrollView";
        }
        if own.contains(&P::VERTICAL_SCROLL_AXIS_RANGE) {
            class = "android.widget.ScrollView";
        }
    }
    if let Some(named) = own.get(&P::ACCESSIBILITY_CLASS_NAME) {
        class = named.as_str();
    }
    (class, role_description)
}

/// What a service may ask of `node`, from its own configuration, as Compose's delegate: nothing
/// but the accessibility focus (which `Accessibility.kt` keeps) when it's disabled.
pub fn actions_of(
    node: &SemanticsSnapshot,
    custom: &KeptCustomActions,
) -> Vec<(u8, Option<SharedString>)> {
    let mut actions = standard_actions(node);
    if !node.config.contains(&P::DISABLED) {
        for (id, label) in custom.of(node) {
            actions.push((action::CUSTOM_FIRST + id, Some(label)));
        }
    }
    actions
}

/// `actions_of` but the custom ones.
fn standard_actions(node: &SemanticsSnapshot) -> Vec<(u8, Option<SharedString>)> {
    let own = node.unmerged_config();
    let mut actions = Vec::new();
    // These a disabled text offers too: it's read as any other.
    if let Some(copy) = own.get(&A::COPY_TEXT) {
        actions.push((action::COPY, copy.label.clone()));
    }
    if has_iterable_text(node) {
        let label = own.get(&A::SET_SELECTION).and_then(|it| it.label.clone());
        actions.push((action::SET_SELECTION, label));
        actions.push((action::NEXT_AT_GRANULARITY, None));
        actions.push((action::PREVIOUS_AT_GRANULARITY, None));
    }
    if node.config.contains(&P::DISABLED) {
        return actions;
    }
    if let Some(set) = own.get(&A::SET_TEXT) {
        actions.push((action::SET_TEXT, set.label.clone()));
    }
    let mut labelled = |key, action: u8| {
        let found: Option<&compose_ui::AccessibilityAction<compose_ui::ActionFn>> = own.get(key);
        if let Some(found) = found {
            actions.push((action, found.label.clone()));
        }
    };
    labelled(&A::ON_IME_ACTION, action::IME_ENTER);
    labelled(&A::CUT_TEXT, action::CUT);
    // Only where it would land; `Accessibility.kt` asks the clipboard.
    if own.get(&P::FOCUSED) == Some(&true) {
        labelled(&A::PASTE_TEXT, action::PASTE);
    }
    if is_clickable(node) {
        labelled(&A::ON_CLICK, action::CLICK);
    }
    labelled(&A::ON_LONG_CLICK, action::LONG_CLICK);
    labelled(&A::PAGE_UP, action::PAGE_UP);
    labelled(&A::PAGE_DOWN, action::PAGE_DOWN);
    labelled(&A::PAGE_LEFT, action::PAGE_LEFT);
    labelled(&A::PAGE_RIGHT, action::PAGE_RIGHT);
    labelled(&A::EXPAND, action::EXPAND);
    labelled(&A::COLLAPSE, action::COLLAPSE);
    labelled(&A::DISMISS, action::DISMISS);
    match own.get(&P::FOCUSED) {
        Some(true) => actions.push((action::CLEAR_FOCUS, None)),
        Some(false) => actions.push((action::FOCUS, None)),
        None => {}
    }
    if let Some(range) = node.config.get(&P::PROGRESS_BAR_RANGE_INFO) {
        if own.contains(&A::SET_PROGRESS) {
            let label = own.get(&A::SET_PROGRESS).and_then(|it| it.label.clone());
            actions.push((action::SET_PROGRESS, label));
            if range.current < range.range.1 {
                actions.push((action::STEP_FORWARD, None));
            }
            if range.current > range.range.0 {
                actions.push((action::STEP_BACKWARD, None));
            }
        }
    } else {
        let scroll = scroll_actions(node);
        for bit in 0..6 {
            if scroll & (1 << bit) != 0 {
                actions.push((action::SCROLL_FIRST + bit, None));
            }
        }
    }
    actions
}

/// What a service moves through in `node`, as Compose's `getIterableTextForAccessibility`: from its
/// own configuration, its description before its text.
pub fn iterable_text(node: &SemanticsSnapshot) -> Option<String> {
    let own = node.unmerged_config();
    if let Some(all) = own.get(&P::CONTENT_DESCRIPTION) {
        return Some(all.iter().map(|text| text.as_str()).collect::<Vec<_>>().join(","));
    }
    if let Some(text) = own.get(&P::EDITABLE_TEXT) {
        return Some(text.as_str().to_string());
    }
    own.get(&P::TEXT)?.iter().next().map(|text| text.as_str().to_string())
}

/// Whether `iterable_text` has anything, without making it: asked of every node.
fn has_iterable_text(node: &SemanticsSnapshot) -> bool {
    let own = node.unmerged_config();
    if let Some(all) = own.get(&P::CONTENT_DESCRIPTION) {
        return all.iter().any(|text| !text.as_str().is_empty()) || all.iter().count() > 1;
    }
    if let Some(text) = own.get(&P::EDITABLE_TEXT) {
        return !text.as_str().is_empty();
    }
    own.get(&P::TEXT)
        .and_then(|all| all.iter().next())
        .is_some_and(|text| !text.as_str().is_empty())
}

/// How a service moves through `node`, `movement`'s bits: lines and pages only where there's a
/// layout to find them in, not a description, and not a field without the focus (TalkBack doesn't
/// handle them there, as Compose says).
pub fn movement_granularities(node: &SemanticsSnapshot) -> i32 {
    if !has_iterable_text(node) {
        return 0;
    }
    let mut granularities = movement::CHARACTER | movement::WORD | movement::PARAGRAPH;
    let own = node.unmerged_config();
    let described = own.get(&P::CONTENT_DESCRIPTION).is_some_and(|all| !all.is_empty());
    let unfocused_field = own.contains(&P::EDITABLE_TEXT) && own.get(&P::FOCUSED) != Some(&true);
    if !described && own.contains(&A::GET_TEXT_LAYOUT_RESULT) && !unfocused_field {
        granularities |= movement::LINE | movement::PAGE;
    }
    granularities
}

/// Where a service left its cursor in a text with no selection of its own, as Compose's
/// `accessibilityCursorPosition`, and the movement it's yet to hear of (D1651).
#[derive(Default)]
pub struct TextCursor {
    /// The node it's in, and where (-1: nowhere yet).
    at: Option<(i32, i32)>,
    moved: Option<TextMoved>,
    /// What's to tell with the next changes: a plain text's moves change nothing else.
    told: Vec<Change>,
}

/// A movement a selection change is yet to precede, as Compose's `PendingTextTraversedEvent`.
struct TextMoved {
    id: i32,
    forward: bool,
    granularity: i32,
    from: i32,
    to: i32,
    at: std::time::Instant,
}

impl TextMoved {
    /// Told only soon after it was asked for, as Compose's `TextTraversedEventTimeoutMillis`.
    const LASTS: std::time::Duration = std::time::Duration::from_millis(1000);

    fn change(self, text: &str, password: bool) -> Option<Change> {
        if self.at.elapsed() > TextMoved::LASTS {
            return None;
        }
        let kind =
            if self.forward { ChangeKind::TextMovedForward } else { ChangeKind::TextMovedBackward };
        Some(Change {
            from: self.from,
            removed: self.to,
            added: self.granularity,
            password,
            ..Change::of(self.id, kind).saying(text)
        })
    }
}

impl TextCursor {
    /// For a test: as if the move a change is yet to precede was asked for `ago`.
    #[doc(hidden)]
    pub fn move_asked_ago(&mut self, ago: std::time::Duration) {
        if let Some(moved) = &mut self.moved {
            moved.at = std::time::Instant::now().checked_sub(ago).expect("after the start");
        }
    }

    fn position_in(&self, id: i32) -> i32 {
        self.at.filter(|(node, _)| *node == id).map_or(-1, |(_, position)| position)
    }
}

/// `node`'s selection as a service reads it, as Compose's `getAccessibilitySelectionStart` and
/// `End`, in UTF-16 units of `iterable_text`: its own if it has one and isn't read by its
/// description, else where the service's cursor is. A disabled field's can't be set, so a service
/// keeps a place of its own, or it would go nowhere (D1651).
pub fn text_selection_of(node: &SemanticsSnapshot, cursor: &TextCursor) -> (i32, i32) {
    if !node.config.contains(&P::DISABLED)
        && let Some(selection) = own_selection_of(node.unmerged_config())
    {
        return selection;
    }
    let position = cursor.position_in(virtual_id(node));
    (position, position)
}

/// The selection a configuration says it has, in UTF-16 units of its text.
fn own_selection_of(own: &compose_ui::SemanticsConfiguration) -> Option<(i32, i32)> {
    if own.contains(&P::CONTENT_DESCRIPTION) {
        return None;
    }
    let range = own.get(&P::TEXT_SELECTION_RANGE)?;
    let text = match own.get(&P::EDITABLE_TEXT) {
        Some(text) => text.as_str(),
        None => own.get(&P::TEXT).and_then(|all| all.iter().next()).map_or("", |it| it.as_str()),
    };
    Some((utf16_index(text, range.min()) as i32, utf16_index(text, range.max()) as i32))
}

/// What a step of a value moves it by: a step, or a twentieth of the range when continuous, as
/// Compose's delegate.
pub fn progress_step(info: compose_ui::ProgressBarRangeInfo) -> f32 {
    let length = info.range.1 - info.range.0;
    if info.steps > 0 { length / (info.steps + 1) as f32 } else { length / 20.0 }
}

// ---- What changed -------------------------------------------------------------------------------

/// A change a service hears, as `Accessibility.kt` sends it (D1291, D1296).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ChangeKind {
    /// `TYPE_WINDOW_CONTENT_CHANGED`, `CONTENT_CHANGE_TYPE_UNDEFINED`.
    Undefined = 0,
    /// `CONTENT_CHANGE_TYPE_SUBTREE`: what's below the node changed, or from the view (its id -1),
    /// what's at the top or only where things are.
    Subtree = 1,
    /// A field's text that a service can't set.
    Text = 2,
    ContentDescription = 3,
    StateDescription = 4,
    Error = 5,
    Checked = 6,
    /// `TYPE_WINDOW_STATE_CHANGED`, `CONTENT_CHANGE_TYPE_PANE_APPEARED`, with its title.
    PaneAppeared = 7,
    PaneDisappeared = 8,
    PaneTitle = 9,
    /// `TYPE_VIEW_SELECTED` of a tab selected, with its merged text and description.
    TabSelected = 10,
    /// `TYPE_VIEW_TEXT_CHANGED`: `from`, `removed` and `added` are in UTF-16 units of `before`
    /// and `text`.
    TextChanged = 11,
    /// `TYPE_VIEW_TEXT_SELECTION_CHANGED`: `from` to `removed`, of `added` units, in `text`.
    TextSelection = 12,
    /// `TYPE_VIEW_FOCUSED`.
    Focused = 13,
    /// `TYPE_WINDOW_STATE_CHANGED` of the root of the window now taking the keys.
    Window = 14,
    /// `TYPE_VIEW_TEXT_TRAVERSED_AT_MOVEMENT_GRANULARITY`, by the next or the previous: over
    /// `from` to `removed`, at the granularity `added`.
    TextMovedForward = 15,
    TextMovedBackward = 16,
}

/// One change: the node's id (-1: the view), its kind, and what it says.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Change {
    pub id: i32,
    pub kind: ChangeKind,
    pub text: String,
    /// The text before a `TextChanged`; a `TabSelected`'s description.
    pub before: String,
    pub from: i32,
    pub removed: i32,
    pub added: i32,
    /// A password's: a service doesn't echo it, as `event.isPassword`.
    pub password: bool,
}

impl Change {
    fn of(id: i32, kind: ChangeKind) -> Change {
        Change {
            id,
            kind,
            text: String::new(),
            before: String::new(),
            from: 0,
            removed: 0,
            added: 0,
            password: false,
        }
    }

    fn saying(mut self, text: &str) -> Change {
        self.text = text.to_string();
        self
    }
}

/// What a change is looked for in, as Compose's `SemanticsNodeCopy`.
#[derive(Clone, PartialEq)]
struct KeptNode {
    bounds: Rect,
    children: Vec<i32>,
    pane_title: Option<SharedString>,
    text: String,
    description: String,
    state: Option<SharedString>,
    toggle: Option<ToggleableState>,
    selected: Option<bool>,
    error: Option<SharedString>,
    editable: Option<SharedString>,
    settable: bool,
    selection: Option<compose_ui::TextRange>,
    focused: Option<bool>,
    password: bool,
    range: Option<(u32, u32, u32)>,
    role: Option<Role>,
    hidden: bool,
    flags: u32,
    actions: Vec<(u8, Option<SharedString>)>,
    custom_labels: Vec<SharedString>,
}

/// The tree when a service was last told, to tell it what changed since (D1291).
pub struct KeptForChanges {
    /// Where a service moved through a text, what it's yet to hear of that.
    pub cursor: TextCursor,
    nodes: HashMap<i32, KeptNode>,
    window: Option<NodeId>,
    /// A service just turned on: the next tree is what it starts from, nothing told of it.
    forgotten: bool,
}

impl Default for KeptForChanges {
    /// From an empty root, as Compose's first `previousSemanticsRoot`: what's there at the first
    /// tell is told, with no service turning on since.
    fn default() -> KeptForChanges {
        let mut nodes = HashMap::new();
        nodes.insert(ROOT_KEPT, kept_root(Rect::new(0.0, 0.0, 0.0, 0.0), Vec::new()));
        KeptForChanges { cursor: TextCursor::default(), nodes, window: None, forgotten: false }
    }
}

fn kept_root(bounds: Rect, children: Vec<i32>) -> KeptNode {
    KeptNode {
        bounds,
        children,
        pane_title: None,
        text: String::new(),
        description: String::new(),
        state: None,
        toggle: None,
        selected: None,
        error: None,
        editable: None,
        settable: false,
        selection: None,
        focused: None,
        password: false,
        range: None,
        role: None,
        hidden: false,
        flags: 0,
        actions: Vec::new(),
        custom_labels: Vec::new(),
    }
}

fn kept_of(node: &SemanticsSnapshot, out: &mut HashMap<i32, KeptNode>) {
    let (config, own) = (&node.config, node.unmerged_config());
    let mut custom_labels: Vec<SharedString> = own
        .get(&A::CUSTOM_ACTIONS)
        .map_or(Vec::new(), |all| all.iter().map(|it| it.label.clone()).collect());
    custom_labels.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    custom_labels.dedup();
    let (text, description) = read_as(node);
    out.insert(
        virtual_id(node),
        KeptNode {
            bounds: node.bounds,
            children: node.children.iter().map(virtual_id).collect(),
            pane_title: own.get(&P::PANE_TITLE).cloned(),
            text,
            description,
            state: config.get(&P::STATE_DESCRIPTION).cloned(),
            toggle: config.get(&P::TOGGLEABLE_STATE).copied(),
            selected: config.get(&P::SELECTED).copied(),
            error: config.get(&P::ERROR).cloned(),
            editable: config.get(&P::EDITABLE_TEXT).cloned(),
            settable: own.contains(&A::SET_TEXT),
            selection: config.get(&P::TEXT_SELECTION_RANGE).copied(),
            focused: own.get(&P::FOCUSED).copied(),
            password: is_password(node),
            range: config
                .get(&P::PROGRESS_BAR_RANGE_INFO)
                .map(|it| (it.current.to_bits(), it.range.0.to_bits(), it.range.1.to_bits())),
            role: config.get(&P::ROLE).copied(),
            hidden: node.hidden,
            flags: flags(node),
            actions: standard_actions(node),
            custom_labels,
        },
    );
    for child in &node.children {
        kept_of(child, out);
    }
}

/// `text`'s length in the UTF-16 units Android counts in.
fn utf16_length(text: &str) -> i32 {
    text.encode_utf16().count() as i32
}

/// A selection event's change: `range` is in `text`'s bytes.
fn selection_change(id: i32, text: &str, range: compose_ui::TextRange, password: bool) -> Change {
    let at = |offset: usize| {
        let offset = offset.min(text.len());
        if text.is_char_boundary(offset) { utf16_length(&text[..offset]) } else { 0 }
    };
    let (one, other) = (at(range.start), at(range.end));
    Change {
        from: one.min(other),
        removed: one.max(other),
        added: utf16_length(text),
        password,
        ..Change::of(id, ChangeKind::TextSelection).saying(text)
    }
}

/// The edit between `old` and `new`, by their common start and end, as Compose's: in UTF-16 units.
fn text_change(id: i32, old: &str, new: &str, password: bool) -> Change {
    let (before, after): (Vec<u16>, Vec<u16>) =
        (old.encode_utf16().collect(), new.encode_utf16().collect());
    let shortest = before.len().min(after.len());
    let start = (0..shortest).take_while(|&at| before[at] == after[at]).count();
    let end = (0..shortest - start)
        .take_while(|&at| before[before.len() - 1 - at] == after[after.len() - 1 - at])
        .count();
    Change {
        before: old.to_string(),
        from: start as i32,
        removed: (before.len() - end - start) as i32,
        added: (after.len() - end - start) as i32,
        password,
        ..Change::of(id, ChangeKind::TextChanged).saying(new)
    }
}

/// `Subtree`s of the nodes whose children changed, from the top, as Compose's
/// `sendAccessibilitySemanticsStructureChangeEvents`: nothing below one that did. Returns
/// whether the root's was.
fn structure_changes(
    before: &HashMap<i32, KeptNode>,
    now: &HashMap<i32, KeptNode>,
    id: i32,
    changes: &mut Vec<Change>,
) {
    let (Some(old), Some(new)) = (before.get(&id), now.get(&id)) else { return };
    if old.children != new.children {
        let source = if id == ROOT_KEPT { -1 } else { id };
        changes.push(Change::of(source, ChangeKind::Subtree));
        return;
    }
    for &child in &new.children {
        structure_changes(before, now, child, changes);
    }
}

impl KeptForChanges {
    /// A service turned on: it reads the tree anew, hearing no change from before.
    pub fn forget(&mut self) {
        self.forgotten = true;
    }

    /// What changed between the tree kept and `root`, as Compose's `checkForSemanticsChanges`:
    /// subtrees, then each property with a type of its own, one with no type for the rest, then
    /// panes coming and going, then the window.
    pub fn changes(&mut self, ui: &UiHost, root: &SemanticsSnapshot) -> Vec<Change> {
        let mut now = HashMap::new();
        for child in &root.children {
            kept_of(child, &mut now);
        }
        now.insert(
            ROOT_KEPT,
            kept_root(root.bounds, root.children.iter().map(virtual_id).collect()),
        );
        let window = ui.focused_window();
        let before = std::mem::replace(&mut self.nodes, now);
        let window_before = std::mem::replace(&mut self.window, window);
        let told = std::mem::take(&mut self.cursor.told);
        if std::mem::take(&mut self.forgotten) {
            return Vec::new();
        }
        let now = &self.nodes;
        let mut changes = told;

        structure_changes(&before, now, ROOT_KEPT, &mut changes);
        let told_from_the_view = changes.iter().any(|change| change.id == -1);
        let moved = before.iter().any(|(id, old)| {
            *id != ROOT_KEPT && now.get(id).is_some_and(|new| new.bounds != old.bounds)
        });
        if moved && !told_from_the_view {
            changes.push(Change::of(-1, ChangeKind::Subtree));
        }

        // By id, so that what's told doesn't depend on a map's order.
        let mut ids: Vec<i32> = before.keys().chain(now.keys()).copied().collect();
        ids.sort_unstable();
        ids.dedup();
        for &id in &ids {
            if let (Some(old), Some(new)) = (before.get(&id), now.get(&id))
                && old != new
            {
                property_changes(id, old, new, &mut changes);
            }
        }

        fn title_of(node: &KeptNode) -> &str {
            node.pane_title.as_ref().map_or("", |it| it.as_str())
        }
        for &id in &ids {
            let (old, new) = (before.get(&id), now.get(&id));
            let (had, has) = (
                old.is_some_and(|it| it.pane_title.is_some()),
                new.is_some_and(|it| it.pane_title.is_some()),
            );
            if had && !has {
                let title = old.map_or("", title_of);
                changes.push(Change::of(id, ChangeKind::PaneDisappeared).saying(title));
            } else if !had && has {
                let title = new.map_or("", title_of);
                changes.push(Change::of(id, ChangeKind::PaneAppeared).saying(title));
            }
        }

        if window != window_before {
            let first = root.children.first().filter(|_| window.is_some()).map_or(-1, virtual_id);
            changes.push(Change::of(first, ChangeKind::Window));
        }
        // A field's move, once its selection changed: told after that, as Compose's
        // `sendPendingTextTraversedAtGranularityEvent`.
        if let Some(moved) = &self.cursor.moved
            && let Some(at) = changes
                .iter()
                .position(|it| it.kind == ChangeKind::TextSelection && it.id == moved.id)
        {
            let (text, password) = (changes[at].text.clone(), changes[at].password);
            let told = self.cursor.moved.take().and_then(|moved| moved.change(&text, password));
            if let Some(told) = told {
                changes.insert(at + 1, told);
            }
        }
        changes
    }
}

/// What changed of a node still there, as Compose's `sendSemanticsPropertyChangeEvents`.
fn property_changes(id: i32, old: &KeptNode, new: &KeptNode, changes: &mut Vec<Change>) {
    // Each of these says so itself, then asks `ViewRootImpl` to read it again (Compose's
    // b/192295060).
    let mut undefined = false;
    if let (Some(a), Some(b)) = (&old.pane_title, &new.pane_title)
        && a != b
    {
        changes.push(Change::of(id, ChangeKind::PaneTitle).saying(b.as_str()));
    }
    let mut change = |kind| changes.push(Change::of(id, kind));
    if old.state != new.state || old.range != new.range {
        change(ChangeKind::StateDescription);
        change(ChangeKind::Undefined);
    }
    if old.toggle != new.toggle {
        change(ChangeKind::Checked);
        change(ChangeKind::Undefined);
    }
    if old.error != new.error {
        change(ChangeKind::Error);
    }
    if old.selected != new.selected {
        if new.role == Some(Role::Tab) && new.selected == Some(true) {
            changes.push(Change {
                before: new.description.clone(),
                ..Change::of(id, ChangeKind::TabSelected).saying(&new.text)
            });
        } else if new.role == Some(Role::Tab) {
            changes.push(Change::of(id, ChangeKind::Undefined));
        } else {
            changes.push(Change::of(id, ChangeKind::StateDescription));
            changes.push(Change::of(id, ChangeKind::Undefined));
        }
    }
    if old.description != new.description {
        changes.push(Change::of(id, ChangeKind::ContentDescription).saying(&new.description));
    }
    if old.focused != new.focused {
        if new.focused == Some(true) {
            changes.push(Change::of(id, ChangeKind::Focused));
        }
        changes.push(Change::of(id, ChangeKind::Undefined));
    }
    // A plain text: no type of its own.
    undefined |= old.text != new.text;
    if let Some(shown) = &new.editable {
        let was = old.editable.as_ref().map_or("", |it| it.as_str());
        let shown = shown.as_str();
        let became = old.editable.is_some() && old.password != new.password;
        if became {
            // A password shown or masked: a change of cursor, as Compose's two events, or a
            // service would read the text as edited (b/247891690).
            let reset = compose_ui::TextRange::new(0, 0);
            changes.push(selection_change(id, shown, reset, new.password));
            let range = new.selection.unwrap_or(reset);
            changes.push(selection_change(id, shown, range, new.password));
        } else if was != shown {
            if new.settable {
                changes.push(text_change(id, was, shown, new.password));
            } else {
                changes.push(Change::of(id, ChangeKind::Text));
            }
        }
        if !became
            && old.selection != new.selection
            && let Some(range) = new.selection
        {
            changes.push(selection_change(id, shown, range, new.password));
        }
    }
    undefined |= old.custom_labels != new.custom_labels || old.actions != new.actions;
    undefined |= old.flags != new.flags
        && old.state == new.state
        && old.toggle == new.toggle
        && old.selected == new.selected
        && old.range == new.range
        && old.focused == new.focused;
    undefined |= old.hidden != new.hidden;
    if undefined {
        changes.push(Change::of(id, ChangeKind::Undefined));
    }
}

/// The root's place among the kept, an id no layout node has.
const ROOT_KEPT: i32 = i32::MIN;

/// Writes `changes`: each its id (i32), its kind (u8), whether it's a password's (u8), its text
/// and what was before (each a length and UTF-8 bytes), `from`, `removed` and `added` (3 × i32).
pub fn write_changes(changes: &[Change], out: &mut Writer) {
    out.bytes.clear();
    for change in changes {
        out.u32(change.id as u32);
        out.u8(change.kind as u8);
        out.u8(change.password as u8);
        write_string(out, parcel_safe(&change.text));
        write_string(out, parcel_safe(&change.before));
        out.u32(change.from as u32);
        out.u32(change.removed as u32);
        out.u32(change.added as u32);
    }
}

/// An accessibility service's action on virtual view `id` (`ComposeRsNative.performAction`), one of
/// `action`'s: a scroll a page (the viewport as shown), as Compose's delegate works them out
/// (`scroll_amount`, D491, D496); a custom one by the label it was given (D1295).
pub fn perform_action(
    ui: &mut UiHost,
    tree: &SemanticsSnapshot,
    custom: &KeptCustomActions,
    id: i32,
    code: i32,
    value: f32,
) -> bool {
    let Some((found, shown)) = find_shown(tree, id) else { return false };
    let node = found.id;
    let Ok(code) = u8::try_from(code) else { return false };
    match code {
        action::CLICK => ui.perform_semantics_action(node, &A::ON_CLICK),
        // The view has left touch mode by then (`performSemanticsAction`).
        action::FOCUS => ui.perform_semantics_action(node, &A::REQUEST_FOCUS),
        action::CLEAR_FOCUS => {
            let focused = found.unmerged_config().get(&P::FOCUSED) == Some(&true);
            if focused {
                ui.clear_focus();
            }
            focused
        }
        action::SET_PROGRESS => {
            ui.perform_semantics_action_with(node, &A::SET_PROGRESS, |set| set(value))
        }
        action::SCROLL_FIRST..=8 => {
            let direction = 1u8 << (code - action::SCROLL_FIRST);
            let Some((x, y)) = scroll_amount(found, direction, shown) else {
                return false;
            };
            ui.perform_semantics_action_with(node, &A::SCROLL_BY, |scroll| scroll(x, y))
        }
        action::LONG_CLICK => ui.perform_semantics_action(node, &A::ON_LONG_CLICK),
        action::EXPAND => ui.perform_semantics_action(node, &A::EXPAND),
        action::COLLAPSE => ui.perform_semantics_action(node, &A::COLLAPSE),
        action::DISMISS => ui.perform_semantics_action(node, &A::DISMISS),
        action::PAGE_UP => ui.perform_semantics_action(node, &A::PAGE_UP),
        action::PAGE_DOWN => ui.perform_semantics_action(node, &A::PAGE_DOWN),
        action::PAGE_LEFT => ui.perform_semantics_action(node, &A::PAGE_LEFT),
        action::PAGE_RIGHT => ui.perform_semantics_action(node, &A::PAGE_RIGHT),
        action::STEP_FORWARD | action::STEP_BACKWARD => {
            let Some(&range) = found.config.get(&P::PROGRESS_BAR_RANGE_INFO) else {
                return false;
            };
            let step = progress_step(range);
            let moved = if code == action::STEP_FORWARD { step } else { -step };
            let to = (range.current + moved).clamp(range.range.0, range.range.1);
            ui.perform_semantics_action_with(node, &A::SET_PROGRESS, |set| set(to))
        }
        action::CUSTOM_FIRST.. => {
            let Some(label) = custom.label_of(id, code - action::CUSTOM_FIRST) else {
                return false;
            };
            ui.perform_custom_action(node, label.as_str())
        }
        _ => false,
    }
}

/// What an accessibility service asks of a text (`ComposeRsNative.performTextAction`), one of
/// `action`'s: `first` and `second` are a selection's ends, or a movement's granularity, and
/// `text` a new text; D1651.
#[allow(clippy::too_many_arguments)]
pub fn perform_text_action(
    ui: &mut UiHost,
    tree: &SemanticsSnapshot,
    cursor: &mut TextCursor,
    id: i32,
    code: i32,
    first: i32,
    second: i32,
    extend_selection: bool,
    text: &str,
) -> bool {
    let Some(node) = find(tree, id) else { return false };
    let Ok(code) = u8::try_from(code) else { return false };
    match code {
        action::SET_TEXT => ui.perform_semantics_action_with(node.id, &A::SET_TEXT, |set| {
            set(text.to_string().into())
        }),
        action::IME_ENTER => ui.perform_semantics_action(node.id, &A::ON_IME_ACTION),
        action::COPY => ui.perform_semantics_action(node.id, &A::COPY_TEXT),
        action::CUT => ui.perform_semantics_action(node.id, &A::CUT_TEXT),
        action::PASTE => ui.perform_semantics_action(node.id, &A::PASTE_TEXT),
        action::SET_SELECTION => set_selection(ui, node, cursor, first, second, false),
        action::NEXT_AT_GRANULARITY | action::PREVIOUS_AT_GRANULARITY => {
            let forward = code == action::NEXT_AT_GRANULARITY;
            let page = find_shown(tree, id).map_or(node.bounds, |(_, shown)| shown).height();
            move_through(ui, node, cursor, first, forward, extend_selection, page)
        }
        _ => false,
    }
}

/// Where `node`'s characters are, as Compose's `getBoundingBoxes`: `length` of them from `start`, in
/// UTF-16 units, the second of a surrogate pair where the pair is; at most `MOST_LOCATIONS`; in the
/// view's pixels, each cut to what's shown of the node and `None` if none of it is or it's past the
/// text. `None` without a layout.
pub fn character_locations(
    ui: &mut UiHost,
    node: &SemanticsSnapshot,
    start: i32,
    length: i32,
) -> Option<Vec<Option<Rect>>> {
    let layout = text_layout_of(ui, node)?;
    let origin = ui.text_layout_origin(node.id)?;
    let text = layout.layout_input.text.text().clone();
    let units = utf16_length(text.as_str());
    Some(
        (0..length.clamp(0, MOST_LOCATIONS))
            .map(|at| {
                let unit = start.saturating_add(at);
                if unit < 0 || unit >= units {
                    return None;
                }
                let bounds = layout.get_bounding_box(character_at(text.as_str(), unit as usize));
                let in_root = Rect::new(
                    bounds.left + origin.x,
                    bounds.top + origin.y,
                    bounds.right + origin.x,
                    bounds.bottom + origin.y,
                );
                clipped(in_root, node.bounds)
            })
            .collect(),
    )
}

/// What a service may ask the places of at once: more would be too much to send anyway.
const MOST_LOCATIONS: i32 = 20_000;

/// The byte offset of the character UTF-16 `unit` is part of.
fn character_at(text: &str, unit: usize) -> usize {
    let mut units = 0;
    for (at, c) in text.char_indices() {
        units += c.len_utf16();
        if units > unit {
            return at;
        }
    }
    text.len()
}

/// `node`'s text's layout, if it has one.
fn text_layout_of(
    ui: &mut UiHost,
    node: &SemanticsSnapshot,
) -> Option<compose_ui::TextLayoutResult> {
    let mut layouts = Vec::new();
    ui.perform_semantics_action_with(node.id, &A::GET_TEXT_LAYOUT_RESULT, |get| get(&mut layouts));
    layouts.into_iter().next()
}

/// Moves a service's place in `node`'s text, as Compose's `traverseAtGranularity`: the piece of it
/// after or before, selected from where it was if `extend_selection` and it's a field. A page is
/// `page` high. A move a field's mapping brings back where it was (the `-` of `12-34`) goes on to
/// the next piece, or a service would never get past it (D1651).
fn move_through(
    ui: &mut UiHost,
    node: &SemanticsSnapshot,
    cursor: &mut TextCursor,
    granularity: i32,
    forward: bool,
    extend_selection: bool,
    page: f32,
) -> bool {
    let id = virtual_id(node);
    if cursor.at.is_none_or(|(there, _)| there != id) {
        cursor.at = Some((id, -1));
    }
    let Some(text) = iterable_text(node).filter(|text| !text.is_empty()) else { return false };
    let layout = match granularity {
        movement::LINE | movement::PAGE => match text_layout_of(ui, node) {
            Some(layout) => Some(layout),
            None => return false,
        },
        _ => None,
    };
    let granularity_of = match (granularity, &layout) {
        (movement::CHARACTER, _) => Granularity::Character,
        (movement::WORD, _) => Granularity::Word,
        (movement::PARAGRAPH, _) => Granularity::Paragraph,
        (movement::LINE, Some(layout)) => Granularity::Line(layout),
        (movement::PAGE, Some(layout)) => Granularity::Page(layout, page),
        _ => return false,
    };
    let segments = TextSegments::new(&text);
    let (start, end) = text_selection_of(node, cursor);
    let was_at = if end == -1 { if forward { 0 } else { segments.len() } } else { end };
    let own = node.unmerged_config();
    let extends = extend_selection
        && !own.contains(&P::CONTENT_DESCRIPTION)
        && own.contains(&P::EDITABLE_TEXT);
    let mut current = was_at;
    let mut moved_at_all = false;
    loop {
        let range = if forward {
            segments.following(granularity_of, current)
        } else {
            segments.preceding(granularity_of, current)
        };
        let Some((from, to)) = range else {
            cursor.moved = None;
            return moved_at_all;
        };
        moved_at_all = true;
        let edge = if forward { to } else { from };
        let (selection_start, selection_end) = if extends {
            let start = if start != -1 {
                start
            } else if forward {
                from
            } else {
                to
            };
            (start, edge)
        } else {
            (edge, edge)
        };
        cursor.moved =
            Some(TextMoved { id, forward, granularity, from, to, at: std::time::Instant::now() });
        set_selection(ui, node, cursor, selection_start, selection_end, true);
        // Where it is now, which the field's own mapping says.
        let now = ui
            .with_tree(|tree| tree.unmerged_semantics_config(node.id))
            .and_then(|own| own_selection_of(&own))
            .filter(|_| !node.config.contains(&P::DISABLED))
            .map_or(edge, |(_, end)| end);
        if if forward { now > was_at } else { now < was_at } {
            return true;
        }
        current = edge;
    }
}

/// As Compose's `setAccessibilitySelection`: `node`'s own selection, by its action, if it has one
/// and is enabled; else the service's cursor, there and said so at once, what it can select being
/// a position only.
fn set_selection(
    ui: &mut UiHost,
    node: &SemanticsSnapshot,
    cursor: &mut TextCursor,
    start: i32,
    end: i32,
    traversal: bool,
) -> bool {
    let own = node.unmerged_config();
    if own.contains(&A::SET_SELECTION) && !node.config.contains(&P::DISABLED) {
        let shown = own.get(&P::EDITABLE_TEXT).map_or("", |text| text.as_str());
        let bytes = |unit: i32| {
            if unit < 0 { unit } else { byte_offset(shown, unit as usize) as i32 }
        };
        let length = utf16_length(shown);
        if start > length || end > length {
            return false;
        }
        let (start, end) = (bytes(start), bytes(end));
        let mut done = false;
        ui.perform_semantics_action_with(node.id, &A::SET_SELECTION, |set| {
            done = set(start, end, traversal);
            done
        });
        return done;
    }
    let id = virtual_id(node);
    if start == end && end == cursor.position_in(id) {
        return false;
    }
    let Some(text) = iterable_text(node) else { return false };
    let length = utf16_length(&text);
    let position = if start >= 0 && start == end && end <= length { start } else { -1 };
    cursor.at = Some((id, position));
    let shown = !text.is_empty();
    let (position, length) = if shown { (position, length) } else { (0, 0) };
    let selected = Change {
        from: position,
        removed: position,
        added: length,
        ..Change::of(id, ChangeKind::TextSelection).saying(&text)
    };
    cursor.told.push(selected);
    if let Some(moved) = cursor.moved.take_if(|moved| moved.id == id)
        && let Some(told) = moved.change(&text, false)
    {
        cursor.told.push(told);
    }
    true
}

/// The scroll range of `node` along its axis (horizontal: `true`), from its own
/// configuration, as Compose's accessibility reads it (D491): vertical first.
fn scroll_range(node: &SemanticsSnapshot) -> Option<(compose_ui::ScrollAxisRange, bool)> {
    let own = node.unmerged_config();
    own.get(&P::VERTICAL_SCROLL_AXIS_RANGE)
        .map(|&range| (range, false))
        .or(own.get(&P::HORIZONTAL_SCROLL_AXIS_RANGE).map(|&range| (range, true)))
}

/// The node of virtual id `id` in `root`'s tree.
pub fn find(root: &SemanticsSnapshot, id: i32) -> Option<&SemanticsSnapshot> {
    if virtual_id(root) == id {
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
pub fn scroll_actions(node: &SemanticsSnapshot) -> u8 {
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
pub fn find_shown(root: &SemanticsSnapshot, id: i32) -> Option<(&SemanticsSnapshot, Rect)> {
    fn visit(node: &SemanticsSnapshot, id: i32, clip: Rect) -> Option<(&SemanticsSnapshot, Rect)> {
        let shown = clipped(node.bounds, clip);
        if virtual_id(node) == id {
            return Some((node, shown.unwrap_or(node.bounds)));
        }
        let scrolls = scroll_range(node).is_some();
        let inner = if scrolls { shown.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)) } else { clip };
        node.children.iter().find_map(|child| visit(child, id, inner))
    }
    if virtual_id(root) == id {
        return Some((root, root.bounds));
    }
    root.children.iter().find_map(|child| visit(child, id, root.bounds))
}

/// What `action` (one `scroll_action` bit) scrolls `node` by, `page` its viewport as shown (its
/// width a page horizontally, its height vertically):
/// the `ScrollBy` amounts, as Compose's delegate works them out (a page, back for backward,
/// left or up, the other way if `reverse_scrolling`, and for left or right when right to
/// left); `None` if it can't scroll that way (D491).
pub fn scroll_amount(node: &SemanticsSnapshot, action: u8, page: Rect) -> Option<(f32, f32)> {
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
