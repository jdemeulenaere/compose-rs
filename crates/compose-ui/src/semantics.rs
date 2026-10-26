//! Semantics: what nodes mean (text, role, state, actions), as Compose's semantics tree. Tests
//! find nodes through it, and accessibility bridges read it (docs/06-ui.md).

use std::fmt::{self, Write as _};

use compose_runtime::ComposeArg;
use smallvec::SmallVec;

use crate::text::SharedString;
use crate::tree::NodeId;
use crate::unit::{Offset, Rect};

/// A node's role, as `Role`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum Role {
    Button,
    Checkbox,
    Switch,
    RadioButton,
    Tab,
    Image,
    DropdownList,
}

impl ComposeArg for Role {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// An action on a semantics node, as `SemanticsActions`: what accessibility services (and tests)
/// ask of a node instead of sending it input.
#[derive(Clone, Copy, PartialEq, Debug)]
#[non_exhaustive]
pub enum SemanticsAction {
    /// Click it, as `SemanticsActions.OnClick`.
    OnClick,
    /// Long-click it, as `SemanticsActions.OnLongClick` (`combined_clickable`).
    OnLongClick,
    /// Focus it, as `SemanticsActions.RequestFocus`.
    RequestFocus,
    /// Set its progress to a value in its range (a slider's), as `SemanticsActions.SetProgress`.
    SetProgress(f32),
    /// Scroll its content by `x` and `y` pixels (positive: towards the content's end, whatever
    /// way it runs: a reversed list's later items, a right-to-left row's), as
    /// `SemanticsActions.ScrollBy`: a scrolling container's. Android's accessibility turns its
    /// directions into it as Compose's `AndroidComposeViewAccessibilityDelegateCompat` does
    /// (D478, D491).
    ScrollBy { x: f32, y: f32 },
}

/// How far a scrolling container is scrolled along an axis, as Compose's `ScrollAxisRange`: its
/// offset, the largest it can be (a lazy list's estimated from its average item size where its
/// end isn't laid out), and whether its content runs from the end (a reversed layout).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScrollAxisRange {
    pub value: f32,
    pub max_value: f32,
    pub reverse_scrolling: bool,
}

/// A value in a range, as Compose's `ProgressBarRangeInfo`: a slider's or a progress bar's.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ProgressBarRangeInfo {
    pub current: f32,
    pub range: (f32, f32),
    /// The stops between the ends (0: continuous).
    pub steps: u32,
}

/// A toggle's state, as `ToggleableState`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ToggleableState {
    On,
    Off,
    Indeterminate,
}

impl From<bool> for ToggleableState {
    fn from(value: bool) -> ToggleableState {
        if value { ToggleableState::On } else { ToggleableState::Off }
    }
}

/// The semantics properties of a node, as `SemanticsConfiguration`: what its modifiers set.
#[derive(Clone, Default, PartialEq)]
pub struct SemanticsConfiguration {
    /// Merge descendants' semantics into this node, as a button merges its label.
    pub merge_descendants: bool,
    pub text: SmallVec<[SharedString; 1]>,
    pub editable_text: Option<SharedString>,
    /// Why its input is invalid, as `error(description)`.
    pub error: Option<SharedString>,
    pub content_description: SmallVec<[SharedString; 1]>,
    pub test_tag: Option<SharedString>,
    pub role: Option<Role>,
    pub disabled: bool,
    pub toggleable_state: Option<ToggleableState>,
    pub selected: Option<bool>,
    pub focusable: bool,
    pub focused: bool,
    /// Whether the node has a click action (`clickable`, `toggleable`).
    pub clickable: bool,
    /// Whether it has a long-click action (`combined_clickable` with `on_long_click`).
    pub long_clickable: bool,
    /// Whether the node takes text input (a text field).
    pub editable: bool,
    /// Its text is a password, shown masked, as `password()`.
    pub password: bool,
    /// The node's state in words (a slider's value), as `stateDescription`.
    pub state_description: Option<SharedString>,
    /// Its value in a range (a slider's), as `progressBarRangeInfo`.
    pub progress_bar_range_info: Option<ProgressBarRangeInfo>,
    /// It's a dialog (its own pane, modal), as `isDialog`.
    pub is_dialog: bool,
    /// It's a popup (its own window over the content), as `isPopup`.
    pub is_popup: bool,
    /// Its descendants have no semantics, as `clearAndSetSemantics`.
    pub clear_descendants: bool,
    /// The properties belong to the nearest ancestor with semantics, in both trees, as if its
    /// own modifiers set them: a text field's inner text, which in Compose is one semantics
    /// node with its decoration box.
    pub part_of_parent: bool,
    /// It groups selectable items (radio buttons), as `selectableGroup()`.
    pub selectable_group: bool,
    /// It scrolls its content, horizontally or vertically, as `horizontalScrollAxisRange` and
    /// `verticalScrollAxisRange` (with the `ScrollBy` action): scrolling containers and lazy
    /// lists and grids (D478).
    pub horizontal_scroll_axis_range: Option<ScrollAxisRange>,
    pub vertical_scroll_axis_range: Option<ScrollAxisRange>,
}

impl SemanticsConfiguration {
    /// Whether anything is set.
    pub fn is_empty(&self) -> bool {
        *self == SemanticsConfiguration::default()
    }

    /// Adds `child`'s properties, as merging does: lists append, the rest keep this node's
    /// value if set.
    pub fn merge_child(&mut self, child: &SemanticsConfiguration) {
        self.text.extend(child.text.iter().cloned());
        self.content_description.extend(child.content_description.iter().cloned());
        if self.editable_text.is_none() {
            self.editable_text = child.editable_text.clone();
        }
        if self.error.is_none() {
            self.error = child.error.clone();
        }
        if self.state_description.is_none() {
            self.state_description = child.state_description.clone();
        }
        // A role never merges up, as Compose's (`parentValue`): a clickable row holding an
        // image is no image.
        if self.toggleable_state.is_none() {
            self.toggleable_state = child.toggleable_state;
        }
        if self.selected.is_none() {
            self.selected = child.selected;
        }
        if self.progress_bar_range_info.is_none() {
            self.progress_bar_range_info = child.progress_bar_range_info;
        }
        // Scroll ranges don't merge: Compose reads them, with `ScrollBy`, from a node's own
        // (unmerged) configuration, so a merging parent offers no scroll it can't perform
        // (D491).
        // Compose's default merge policy, parent ?: child: a flag set below is set here.
        self.editable |= child.editable;
        self.password |= child.password;
        self.focused |= child.focused;
        self.focusable |= child.focusable;
        self.disabled |= child.disabled;
    }

    /// Sets the properties `other` sets, as applying an outer semantics modifier over inner
    /// ones: set values (lists too) replace, flags are or'ed, as Compose's `set`.
    pub fn apply(&mut self, other: &SemanticsConfiguration) {
        self.merge_descendants |= other.merge_descendants;
        self.part_of_parent |= other.part_of_parent;
        if !other.text.is_empty() {
            self.text = other.text.clone();
        }
        if !other.content_description.is_empty() {
            self.content_description = other.content_description.clone();
        }
        if other.error.is_some() {
            self.error = other.error.clone();
        }
        if other.editable_text.is_some() {
            self.editable_text = other.editable_text.clone();
        }
        if other.state_description.is_some() {
            self.state_description = other.state_description.clone();
        }
        if other.test_tag.is_some() {
            self.test_tag = other.test_tag.clone();
        }
        if other.role.is_some() {
            self.role = other.role;
        }
        if other.toggleable_state.is_some() {
            self.toggleable_state = other.toggleable_state;
        }
        if other.selected.is_some() {
            self.selected = other.selected;
        }
        if other.progress_bar_range_info.is_some() {
            self.progress_bar_range_info = other.progress_bar_range_info;
        }
        if other.horizontal_scroll_axis_range.is_some() {
            self.horizontal_scroll_axis_range = other.horizontal_scroll_axis_range;
        }
        if other.vertical_scroll_axis_range.is_some() {
            self.vertical_scroll_axis_range = other.vertical_scroll_axis_range;
        }
        self.is_dialog |= other.is_dialog;
        self.is_popup |= other.is_popup;
        self.clear_descendants |= other.clear_descendants;
        self.disabled |= other.disabled;
        self.focusable |= other.focusable;
        self.focused |= other.focused;
        self.clickable |= other.clickable;
        self.long_clickable |= other.long_clickable;
        self.editable |= other.editable;
        self.password |= other.password;
        self.selectable_group |= other.selectable_group;
    }

    /// All text, editable text first, joined by ", ": what text finders match.
    pub fn all_text(&self) -> impl Iterator<Item = &str> {
        self.editable_text.iter().chain(&self.text).map(|text| text.as_str())
    }
}

impl fmt::Debug for SemanticsConfiguration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(role) = self.role {
            parts.push(format!("role={role:?}"));
        }
        if !self.text.is_empty() {
            parts.push(format!(
                "text={:?}",
                self.text.iter().map(|t| t.as_str()).collect::<Vec<_>>()
            ));
        }
        if let Some(text) = &self.editable_text {
            parts.push(format!("editable_text={text:?}"));
        }
        if let Some(error) = &self.error {
            parts.push(format!("error={error:?}"));
        }
        if !self.content_description.is_empty() {
            parts.push(format!(
                "content_description={:?}",
                self.content_description.iter().map(|t| t.as_str()).collect::<Vec<_>>()
            ));
        }
        if let Some(tag) = &self.test_tag {
            parts.push(format!("tag={tag:?}"));
        }
        if let Some(state) = self.toggleable_state {
            parts.push(format!("toggle={state:?}"));
        }
        if let Some(selected) = self.selected {
            parts.push(format!("selected={selected}"));
        }
        if let Some(state) = &self.state_description {
            parts.push(format!("state={:?}", state.as_str()));
        }
        if let Some(info) = self.progress_bar_range_info {
            parts.push(format!(
                "progress={} in {}..{} steps {}",
                info.current, info.range.0, info.range.1, info.steps
            ));
        }
        for (range, axis) in [
            (self.horizontal_scroll_axis_range, "horizontal"),
            (self.vertical_scroll_axis_range, "vertical"),
        ] {
            if let Some(range) = range {
                let reverse = if range.reverse_scrolling { " reversed" } else { "" };
                parts.push(format!("{axis}_scroll={}/{}{reverse}", range.value, range.max_value));
            }
        }
        for (flag, name) in [
            (self.clickable, "clickable"),
            (self.long_clickable, "long_clickable"),
            (self.editable, "editable"),
            (self.password, "password"),
            (self.disabled, "disabled"),
            (self.focusable, "focusable"),
            (self.focused, "focused"),
            (self.merge_descendants, "merged"),
            (self.part_of_parent, "part_of_parent"),
            (self.is_dialog, "dialog"),
            (self.is_popup, "popup"),
            (self.clear_descendants, "cleared"),
            (self.selectable_group, "selectable_group"),
        ] {
            if flag {
                parts.push(name.to_string());
            }
        }
        formatter.write_str(&parts.join(" "))
    }
}

/// A node of the semantics tree: a layout node with semantics, its bounds in the root (in
/// pixels), and its configuration (merged, in the merged tree).
#[derive(Clone, Debug)]
pub struct SemanticsNode {
    pub id: NodeId,
    pub bounds: Rect,
    /// Laid out right to left, as Compose's `layoutInfo.layoutDirection`: a horizontal
    /// scroll's left and right go the other way (D484).
    pub rtl: bool,
    pub config: SemanticsConfiguration,
    pub children: Vec<SemanticsNode>,
}

impl SemanticsNode {
    /// The center of the node's bounds, where test clicks land.
    pub fn center(&self) -> Offset {
        Offset::new(
            (self.bounds.left + self.bounds.right) / 2.0,
            (self.bounds.top + self.bounds.bottom) / 2.0,
        )
    }

    /// This node and its descendants, depth-first.
    pub fn flatten(&self) -> Vec<&SemanticsNode> {
        let mut out = Vec::new();
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            out.push(node);
            stack.extend(node.children.iter().rev());
        }
        out
    }

    /// An indented dump, one node per line.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.dump_into(&mut out, 0);
        out
    }

    fn dump_into(&self, out: &mut String, depth: usize) {
        let _ = writeln!(
            out,
            "{}[{},{} {}x{}] {:?}",
            "  ".repeat(depth),
            self.bounds.left,
            self.bounds.top,
            self.bounds.width(),
            self.bounds.height(),
            self.config
        );
        for child in &self.children {
            child.dump_into(out, depth + 1);
        }
    }
}

// ---- The semantics tree ----------------------------------------------------------------------

use crate::tree::{UiTree, capability};

impl UiTree {
    /// The semantics a node's own modifiers set, and the layer of the first one (whose bounds
    /// are the node's semantic bounds).
    pub(crate) fn own_semantics(
        &mut self,
        node: NodeId,
    ) -> Option<(SemanticsConfiguration, usize)> {
        let value = self.get_mut(node)?;
        if value.all_capabilities & capability::SEMANTICS == 0 {
            return None;
        }
        let mut config = SemanticsConfiguration::default();
        let mut layers: SmallVec<[usize; 8]> = SmallVec::new();
        let mut layer = 0;
        for bits in &value.capabilities {
            layers.push(layer);
            if bits & capability::LAYOUT != 0 {
                layer += 1;
            }
        }
        let mut first_layer = None;
        let mut merging_layer = None;
        // From the innermost to the outermost, so an outer modifier's values win, as Compose's
        // `collapsedSemantics` (`tailToHead`).
        for index in (0..value.chain.len()).rev() {
            if value.capabilities[index] & capability::SEMANTICS != 0
                && let Some(semantics) = value.chain[index].as_semantics()
            {
                let mut own = SemanticsConfiguration::default();
                semantics.apply_semantics(&mut own);
                // Clearing drops what the modifiers after it set, as Compose's
                // `collapsedSemantics`.
                if own.clear_descendants {
                    config = SemanticsConfiguration::default();
                }
                config.apply(&own);
                first_layer = Some(layers[index]);
                // A merging node's bounds are those of the outermost modifier that merges.
                if own.merge_descendants {
                    merging_layer = Some(layers[index]);
                }
            }
        }
        Some((config, merging_layer.or(first_layer).unwrap_or(0)))
    }

    /// The semantics tree, as Compose's: merged (`merged`), a node merging its descendants
    /// takes their properties and hides them, unless they merge themselves; unmerged, every
    /// node with semantics appears. Nodes not placed are left out.
    pub fn semantics_tree(&mut self, merged: bool) -> SemanticsNode {
        let root = self.root_id();
        let mut children = Vec::new();
        let mut config = SemanticsConfiguration::default();
        let count = self.node(root).children.len();
        for index in 0..count {
            let child = self.node(root).children[index].node;
            self.semantics_of(child, merged, &mut config, &mut children);
        }
        // Overlays (popups, dialogs) are roots of their own, as Compose's windows: after the
        // content, never merged into what's around their anchor.
        for index in 0..self.overlays.len() {
            let overlay = self.overlays[index];
            if self.get(overlay).is_some_and(|value| value.overlay && value.placed) {
                let mut overlay_config = SemanticsConfiguration::default();
                self.semantics_of_overlay(overlay, merged, &mut overlay_config, &mut children);
            }
        }
        let rtl = self.node(root).layout_direction == crate::alignment::LayoutDirection::Rtl;
        SemanticsNode { id: root, bounds: self.bounds_in_root(root), rtl, config, children }
    }

    /// Adds `node`'s semantics nodes to `out`, the children of the node whose properties are
    /// `parent`.
    fn semantics_of_overlay(
        &mut self,
        node: NodeId,
        merged: bool,
        parent: &mut SemanticsConfiguration,
        out: &mut Vec<SemanticsNode>,
    ) {
        self.semantics_of_node(node, merged, parent, out);
    }

    fn semantics_of(
        &mut self,
        node: NodeId,
        merged: bool,
        parent: &mut SemanticsConfiguration,
        out: &mut Vec<SemanticsNode>,
    ) {
        // Overlays are visited as roots of their own.
        if self.node(node).overlay {
            return;
        }
        self.semantics_of_node(node, merged, parent, out);
    }

    fn semantics_of_node(
        &mut self,
        node: NodeId,
        merged: bool,
        parent: &mut SemanticsConfiguration,
        out: &mut Vec<SemanticsNode>,
    ) {
        if !self.node(node).placed {
            return;
        }
        let own = self.own_semantics(node).filter(|(config, _)| !config.is_empty());
        let children_of = |tree: &mut UiTree| {
            (0..tree.node(node).children.len())
                .map(|index| tree.node(node).children[index].node)
                .collect::<SmallVec<[NodeId; 8]>>()
        };
        match own {
            None => {
                for child in children_of(self) {
                    self.semantics_of(child, merged, parent, out);
                }
            }
            Some((own, _)) if own.part_of_parent => {
                parent.apply(&own);
                for child in children_of(self) {
                    self.semantics_of(child, merged, parent, out);
                }
            }
            Some((mut config, layer)) => {
                let mut children = Vec::new();
                if config.clear_descendants {
                    // Nothing inside reads.
                } else if merged && config.merge_descendants {
                    for child in children_of(self) {
                        self.merge_into(child, &mut config, &mut children);
                    }
                } else {
                    for child in children_of(self) {
                        self.semantics_of(child, merged, &mut config, &mut children);
                    }
                }
                let origin = self.layer_origin(node, layer);
                let size = self.node(node).layers[layer].measured;
                let rtl =
                    self.node(node).layout_direction == crate::alignment::LayoutDirection::Rtl;
                out.push(SemanticsNode {
                    id: node,
                    bounds: Rect::from_offset_size(origin, size.into()),
                    rtl,
                    config,
                    children,
                });
            }
        }
    }

    /// Merges `node`'s subtree into `config`, except descendants that merge themselves, which
    /// become `children`.
    fn merge_into(
        &mut self,
        node: NodeId,
        config: &mut SemanticsConfiguration,
        children: &mut Vec<SemanticsNode>,
    ) {
        if !self.node(node).placed || self.node(node).overlay {
            return;
        }
        if let Some((own, _)) = self.own_semantics(node) {
            if own.merge_descendants {
                self.semantics_of(node, true, config, children);
                return;
            }
            config.merge_child(&own);
            if own.clear_descendants {
                return;
            }
        }
        let count = self.node(node).children.len();
        for index in 0..count {
            let child = self.node(node).children[index].node;
            self.merge_into(child, config, children);
        }
    }
}
