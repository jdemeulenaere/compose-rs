//! The semantics tree for Android's accessibility (docs/07-platforms.md, "The rest of the
//! platform glue"): `Accessibility.kt`, an `AccessibilityNodeProvider`, reads it as virtual
//! views when a service (TalkBack, Switch Access) asks.
//!
//! The merged tree is written in pre-order, one record per node: its virtual id (its semantics
//! id), its parent's (-1 for the root's children), its bounds in the view's pixels, flags,
//! role, and its texts. Little-endian, as the display list.

use compose_ui::{
    Rect, Role, SemanticsActions as A, SemanticsProperties as P, SharedString, ToggleableState,
    platform::{FrameStats, SemanticsSnapshot, UiHost},
};

use std::collections::HashMap;

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
    /// A heading, as `heading()`.
    pub const HEADING: u32 = 1 << 16;
    /// A service focuses on it, as `isScreenReaderFocusable`.
    pub const SCREEN_READER_FOCUSABLE: u32 = 1 << 17;
    /// A live region, as `liveRegion`: a service says a change to it on its own.
    pub const LIVE_POLITE: u32 = 1 << 18;
    pub const LIVE_ASSERTIVE: u32 = 1 << 19;
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
    /// `CUSTOM_FIRST + i`: its `i`th custom action.
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

/// Writes the merged tree `root` (its children: the root itself is the view).
pub fn write_tree(root: &SemanticsSnapshot, out: &mut Writer) {
    out.bytes.clear();
    let before = compose_ui::traversal_before(root);
    for child in sorted_children(root) {
        write_node(child, -1, root.bounds, &before, out);
    }
}

/// Children in `traversal_index` order, as Compose's `getSortedChildren`.
fn sorted_children(node: &SemanticsSnapshot) -> Vec<&SemanticsSnapshot> {
    let index = |child: &SemanticsSnapshot| {
        child.unmerged_config().get(&P::TRAVERSAL_INDEX).copied().unwrap_or(0.0)
    };
    let mut children: Vec<_> = node.children.iter().collect();
    children.sort_by(|a, b| index(a).total_cmp(&index(b)));
    children
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
    set(&mut flags, node.unmerged_config().contains(&A::ON_CLICK), flag::CLICKABLE);
    // Focusable as Compose's `isFocusable`, from the node's own configuration.
    set(&mut flags, node.unmerged_config().contains(&P::FOCUSED), flag::FOCUSABLE);
    set(&mut flags, config.get(&P::FOCUSED) == Some(&true), flag::FOCUSED);
    set(&mut flags, config.contains(&P::EDITABLE_TEXT), flag::EDITABLE);
    set(&mut flags, config.contains(&P::PASSWORD), flag::PASSWORD);
    set(&mut flags, config.contains(&P::DISABLED), flag::DISABLED);
    set(&mut flags, config.contains(&P::IS_DIALOG), flag::DIALOG);
    set(&mut flags, config.contains(&P::PROGRESS_BAR_RANGE_INFO), flag::RANGE);
    set(&mut flags, config.contains(&P::HEADING), flag::HEADING);
    set(&mut flags, compose_ui::is_screen_reader_focusable(node), flag::SCREEN_READER_FOCUSABLE);
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
/// description, error, pane title (each a length and UTF-8 bytes), the range (current, min, max: 3 × f32)
/// if the range flag is set, and the scroll (offset, largest, the viewport's length along its
/// axis: 3 × f32, and the scroll actions offered: u8, `scroll_action`) if the scroll flag is; then
/// who a service reads it before (i32, -1 for none), its class name and role description (each a
/// length and UTF-8 bytes), and its actions (a count: u8; each `action`'s u8 and a label).
fn write_node(
    node: &SemanticsSnapshot,
    parent: i32,
    clip: Rect,
    before: &HashMap<i32, i32>,
    out: &mut Writer,
) {
    let config = &node.config;
    let id = virtual_id(node);
    let visible = clipped(node.bounds, clip);
    let bounds = visible.unwrap_or(node.bounds);
    let scroll = scroll_range(node);
    let mut flags = flags(node);
    if visible.is_none() || node.hidden {
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
    // From its own configuration, as Compose's delegate.
    write_string(out, text_of(node.unmerged_config().get(&P::PANE_TITLE)));
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
    out.u32(before.get(&id).copied().unwrap_or(-1) as u32);
    let (class_name, role_description) = class_and_role(node);
    write_string(out, class_name);
    write_string(out, role_description);
    let actions = actions_of(node);
    out.u8(actions.len() as u8);
    for (action, label) in &actions {
        out.u8(*action);
        write_string(out, label.as_ref().map_or("", |label| label.as_str()));
    }
    // A scrolling container clips what's in it.
    let inner =
        if scroll.is_some() { visible.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)) } else { clip };
    for child in sorted_children(node) {
        write_node(child, id, inner, before, out);
    }
}

/// The class name and role description a service reads, from the node's own configuration, as
/// Compose's delegate: a field's or a text's, a role's where nothing's below it (an image's only
/// if it's what's focused on), a scrolling container's so a service scrolls it.
pub fn class_and_role(node: &SemanticsSnapshot) -> (&'static str, &'static str) {
    let own = node.unmerged_config();
    let mut class = "android.view.View";
    let mut role_description = "";
    if own.contains(&P::EDITABLE_TEXT) {
        class = "android.widget.EditText";
    }
    if own.contains(&P::TEXT) {
        class = "android.widget.TextView";
    }
    if let Some(role) = own.get(&P::ROLE).filter(|_| node.children.is_empty()) {
        match role {
            Role::Tab => role_description = "Tab",
            Role::Switch => role_description = "Switch",
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
    if own.contains(&P::PROGRESS_BAR_RANGE_INFO) && own.contains(&A::SET_PROGRESS) {
        class = "android.widget.SeekBar";
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
    (class, role_description)
}

/// What a service may ask of `node`, from its own configuration, as Compose's delegate: nothing
/// but the accessibility focus (which `Accessibility.kt` keeps) when it's disabled.
pub fn actions_of(node: &SemanticsSnapshot) -> Vec<(u8, Option<SharedString>)> {
    let own = node.unmerged_config();
    let mut actions = Vec::new();
    if node.config.contains(&P::DISABLED) {
        return actions;
    }
    let mut labelled = |key, action: u8| {
        let found: Option<&compose_ui::AccessibilityAction<compose_ui::ActionFn>> = own.get(key);
        if let Some(found) = found {
            actions.push((action, found.label.clone()));
        }
    };
    labelled(&A::ON_CLICK, action::CLICK);
    labelled(&A::ON_LONG_CLICK, action::LONG_CLICK);
    labelled(&A::PAGE_UP, action::PAGE_UP);
    labelled(&A::PAGE_DOWN, action::PAGE_DOWN);
    labelled(&A::PAGE_LEFT, action::PAGE_LEFT);
    labelled(&A::PAGE_RIGHT, action::PAGE_RIGHT);
    labelled(&A::EXPAND, action::EXPAND);
    labelled(&A::COLLAPSE, action::COLLAPSE);
    labelled(&A::DISMISS, action::DISMISS);
    let focusable = own.contains(&P::FOCUSED);
    if focusable && node.config.get(&P::FOCUSED) != Some(&true) {
        actions.push((action::FOCUS, None));
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
    if let Some(custom) = own.get(&A::CUSTOM_ACTIONS) {
        for (index, custom) in custom.iter().enumerate().take(32) {
            actions.push((action::CUSTOM_FIRST + index as u8, Some(custom.label.clone())));
        }
    }
    actions
}

/// What a step of a value moves it by: a step, or a twentieth of the range when continuous, as
/// Compose's delegate.
pub fn progress_step(info: compose_ui::ProgressBarRangeInfo) -> f32 {
    let length = info.range.1 - info.range.0;
    if info.steps > 0 { length / (info.steps + 1) as f32 } else { length / 20.0 }
}

// ---- What changed -------------------------------------------------------------------------------

/// A change a service hears, as `Accessibility.kt` sends it (`CONTENT_CHANGE_TYPE`s, D1291).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ChangeKind {
    /// `TYPE_WINDOW_CONTENT_CHANGED`, `CONTENT_CHANGE_TYPE_UNDEFINED`.
    Undefined = 0,
    /// `CONTENT_CHANGE_TYPE_SUBTREE`, from the view: what's in the tree or where it is changed.
    Subtree = 1,
    Text = 2,
    ContentDescription = 3,
    StateDescription = 4,
    Error = 5,
    Checked = 6,
    /// `TYPE_WINDOW_STATE_CHANGED`, `CONTENT_CHANGE_TYPE_PANE_APPEARED`, with its title.
    PaneAppeared = 7,
    PaneDisappeared = 8,
    PaneTitle = 9,
    /// `TYPE_VIEW_SELECTED` of a tab selected, with its merged text.
    TabSelected = 10,
}

/// One change: the node's id (-1: the view), its kind, a text for some.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Change {
    pub id: i32,
    pub kind: ChangeKind,
    pub text: String,
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
    range: Option<(u32, u32, u32)>,
    role: Option<Role>,
    hidden: bool,
    flags: u32,
    actions: Vec<(u8, Option<SharedString>)>,
}

/// The tree when a service was last told, to tell it what changed since (D1291).
#[derive(Default)]
pub struct KeptForChanges(Option<HashMap<i32, KeptNode>>);

fn kept_of(node: &SemanticsSnapshot, out: &mut HashMap<i32, KeptNode>) {
    let (config, own) = (&node.config, node.unmerged_config());
    out.insert(
        virtual_id(node),
        KeptNode {
            bounds: node.bounds,
            children: node.children.iter().map(virtual_id).collect(),
            pane_title: own.get(&P::PANE_TITLE).cloned(),
            text: join(config.get(&P::TEXT)),
            description: join(config.get(&P::CONTENT_DESCRIPTION)),
            state: config.get(&P::STATE_DESCRIPTION).cloned(),
            toggle: config.get(&P::TOGGLEABLE_STATE).copied(),
            selected: config.get(&P::SELECTED).copied(),
            error: config.get(&P::ERROR).cloned(),
            editable: config.get(&P::EDITABLE_TEXT).cloned(),
            range: config
                .get(&P::PROGRESS_BAR_RANGE_INFO)
                .map(|it| (it.current.to_bits(), it.range.0.to_bits(), it.range.1.to_bits())),
            role: config.get(&P::ROLE).copied(),
            hidden: node.hidden,
            flags: flags(node),
            actions: actions_of(node),
        },
    );
    for child in &node.children {
        kept_of(child, out);
    }
}

impl KeptForChanges {
    /// A service turned on: it reads the tree anew, hearing no change from before.
    pub fn forget(&mut self) {
        self.0 = None;
    }

    /// What changed between the tree kept and `root`, as Compose's `checkForSemanticsChanges`:
    /// one for each property said apart, one with no type for the rest; a subtree for what's in
    /// the tree, or where, changing; panes coming and going. Nothing the first time.
    pub fn changes(&mut self, root: &SemanticsSnapshot) -> Vec<Change> {
        let mut now = HashMap::new();
        for child in &root.children {
            kept_of(child, &mut now);
        }
        let roots: Vec<i32> = root.children.iter().map(virtual_id).collect();
        now.insert(
            ROOT_KEPT,
            KeptNode {
                bounds: root.bounds,
                children: roots,
                pane_title: None,
                text: String::new(),
                description: String::new(),
                state: None,
                toggle: None,
                selected: None,
                error: None,
                editable: None,
                range: None,
                role: None,
                hidden: false,
                flags: 0,
                actions: Vec::new(),
            },
        );
        let Some(before) = self.0.replace(now) else { return Vec::new() };
        let now = self.0.as_ref().expect("just kept");
        let mut changes = Vec::new();
        let mut change = |id, kind, text: &str| {
            changes.push(Change { id, kind, text: text.to_string() });
        };
        fn title_of(node: &KeptNode) -> &str {
            node.pane_title.as_ref().map_or("", |it| it.as_str())
        }
        let mut subtree = false;
        // By id, so that what's told doesn't depend on a map's order.
        let mut ids: Vec<i32> = before.keys().chain(now.keys()).copied().collect();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            match (before.get(&id), now.get(&id)) {
                (Some(old), None) => {
                    subtree = true;
                    if old.pane_title.is_some() {
                        change(id, ChangeKind::PaneDisappeared, title_of(old));
                    }
                }
                (None, Some(new)) => {
                    subtree = true;
                    if new.pane_title.is_some() {
                        change(id, ChangeKind::PaneAppeared, title_of(new));
                    }
                }
                (Some(old), Some(new)) if old != new => {
                    subtree |= old.bounds != new.bounds || old.children != new.children;
                    match (&old.pane_title, &new.pane_title) {
                        (None, Some(_)) => change(id, ChangeKind::PaneAppeared, title_of(new)),
                        (Some(_), None) => change(id, ChangeKind::PaneDisappeared, title_of(old)),
                        (Some(a), Some(b)) if a != b => {
                            change(id, ChangeKind::PaneTitle, title_of(new));
                        }
                        _ => {}
                    }
                    if old.state != new.state || old.range != new.range {
                        change(id, ChangeKind::StateDescription, "");
                        change(id, ChangeKind::Undefined, "");
                    }
                    if old.toggle != new.toggle {
                        change(id, ChangeKind::Checked, "");
                        change(id, ChangeKind::Undefined, "");
                    }
                    if old.error != new.error {
                        change(id, ChangeKind::Error, "");
                    }
                    if old.selected != new.selected {
                        if new.role == Some(Role::Tab) && new.selected == Some(true) {
                            change(id, ChangeKind::TabSelected, &new.text);
                        } else if new.role == Some(Role::Tab) {
                            change(id, ChangeKind::Undefined, "");
                        } else {
                            change(id, ChangeKind::StateDescription, "");
                            change(id, ChangeKind::Undefined, "");
                        }
                    }
                    if old.description != new.description {
                        change(id, ChangeKind::ContentDescription, &new.description);
                    }
                    if old.text != new.text || old.editable != new.editable {
                        change(id, ChangeKind::Text, "");
                    }
                    if (old.flags != new.flags || old.actions != new.actions)
                        && old.state == new.state
                        && old.toggle == new.toggle
                        && old.selected == new.selected
                        && old.range == new.range
                    {
                        change(id, ChangeKind::Undefined, "");
                    }
                }
                _ => {}
            }
        }
        if subtree {
            change(-1, ChangeKind::Subtree, "");
        }
        changes
    }
}

/// The root's place among the kept, an id no layout node has.
const ROOT_KEPT: i32 = i32::MIN;

/// Writes `changes`: each its id (i32), its kind (u8), its text (a length and UTF-8 bytes).
pub fn write_changes(changes: &[Change], out: &mut Writer) {
    out.bytes.clear();
    for change in changes {
        out.u32(change.id as u32);
        out.u8(change.kind as u8);
        write_string(out, &change.text);
    }
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
