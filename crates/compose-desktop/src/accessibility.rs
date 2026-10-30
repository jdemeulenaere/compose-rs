//! The window's accessibility tree, for screen readers: AccessKit nodes built from the merged
//! semantics tree (docs/07-platforms.md), and their actions sent back as semantics actions.

use std::collections::HashMap;

use accesskit::{Action, ActionRequest, Node, NodeId as AccessNodeId, Rect, Toggled, TreeUpdate};
use compose_ui::{
    LiveRegionMode, Role, SemanticsActions as A, SemanticsProperties as P, ToggleableState,
    platform::{SemanticsSnapshot, UiHost},
};

/// The whole tree, as AccessKit asks for it: the window, then the merged semantics nodes, the
/// focus on the one holding the focused node.
pub(crate) fn tree_update(host: &UiHost, title: &str) -> TreeUpdate {
    host.with_tree(|tree| {
        let root = tree.accessibility_tree();
        let focused = tree.focused();
        let order = reading_order(&root);
        let mut update = TreeUpdate {
            nodes: Vec::new(),
            tree: Some(accesskit::TreeInfo::new(id(&root))),
            tree_id: accesskit::TreeId::ROOT,
            focus: id(&root),
        };
        let mut window = Node::new(accesskit::Role::Window);
        window.set_label(title);
        window.set_children(children_in_order(&root, &order));
        let mut focus = None;
        for child in &root.children {
            add(child, &order, &mut update.nodes, &mut |node| {
                // The innermost semantics node at or above the focused one.
                if focused.is_some_and(|focused| tree.is_at_or_below(focused, node.id)) {
                    focus = Some(id(node));
                }
            });
        }
        update.nodes.push((id(&root), window));
        if let Some(focus) = focus {
            update.focus = focus;
        }
        update
    })
}

/// A node's AccessKit id: its semantics id, which no other node or content had (D1262).
fn id(node: &SemanticsSnapshot) -> AccessNodeId {
    AccessNodeId(node.semantics_id as u64)
}

/// Each node's place in the order it's read in (`sorted_for_accessibility`).
fn reading_order(root: &SemanticsSnapshot) -> HashMap<i32, usize> {
    let sorted = compose_ui::sorted_for_accessibility(root);
    sorted.iter().enumerate().map(|(index, node)| (node.semantics_id, index)).collect()
}

/// When `node` is first read: its own place, or the earliest of what's below it.
fn first_read(node: &SemanticsSnapshot, order: &HashMap<i32, usize>) -> usize {
    let below = node.children.iter().map(|child| first_read(child, order)).min();
    order.get(&node.semantics_id).copied().into_iter().chain(below).min().unwrap_or(usize::MAX)
}

/// `node`'s children as they're read: AccessKit has no traversal order but its children's.
fn children_in_order(node: &SemanticsSnapshot, order: &HashMap<i32, usize>) -> Vec<AccessNodeId> {
    let mut children: Vec<_> = node.children.iter().collect();
    children.sort_by_key(|child| first_read(child, order));
    children.into_iter().map(id).collect()
}

/// The id of a node's dismiss action among its custom actions: AccessKit has no action of its own
/// for it, and the others' are never negative.
const DISMISS_CUSTOM_ACTION: i32 = -1;

/// A custom action's id, from its label: what a screen reader was shown it as finds it, or nothing
/// once it's gone, whatever came and went around it (D1295).
fn custom_action_id(label: &str) -> i32 {
    let hash = label
        .bytes()
        .fold(0x811c_9dc5_u32, |hash, byte| (hash ^ byte as u32).wrapping_mul(0x0100_0193));
    (hash & i32::MAX as u32) as i32
}

/// Adds `node` and its descendants to `nodes`, telling `visit` each id (parents first).
fn add(
    node: &SemanticsSnapshot,
    order: &HashMap<i32, usize>,
    nodes: &mut Vec<(AccessNodeId, Node)>,
    visit: &mut dyn FnMut(&SemanticsSnapshot),
) {
    visit(node);
    let config = &node.config;
    // Actions from the node's own configuration, as Compose's accessibility reads them.
    let own = node.unmerged_config();
    let join = |texts: Option<&compose_ui::StringList>| {
        texts.into_iter().flatten().map(|text| text.as_str()).collect::<Vec<_>>().join(" ")
    };
    let texts = join(config.get(&P::TEXT));
    let descriptions = join(config.get(&P::CONTENT_DESCRIPTION));
    // From its own configuration, as Compose's delegate.
    let pane_title = own.get(&P::PANE_TITLE);
    let disabled = config.contains(&P::DISABLED);
    let clickable = own.contains(&A::ON_CLICK);
    // As Compose's delegate, from the node's own configuration (`unmergedConfig`).
    let range = own.get(&P::PROGRESS_BAR_RANGE_INFO).copied();
    let role = match config.get(&P::ROLE).copied() {
        Some(Role::Button) => accesskit::Role::Button,
        Some(Role::Checkbox) => accesskit::Role::CheckBox,
        Some(Role::Switch) => accesskit::Role::Switch,
        Some(Role::RadioButton) => accesskit::Role::RadioButton,
        Some(Role::Tab) => accesskit::Role::Tab,
        Some(Role::Image) => accesskit::Role::Image,
        Some(Role::DropdownList) => accesskit::Role::ComboBox,
        // A role added later: a generic container.
        Some(_) => accesskit::Role::GenericContainer,
        None if config.contains(&P::IS_DIALOG) => accesskit::Role::Dialog,
        None if pane_title.is_some() => accesskit::Role::Pane,
        None if config.contains(&P::PASSWORD) => accesskit::Role::PasswordInput,
        None if config.contains(&P::EDITABLE_TEXT) => accesskit::Role::TextInput,
        None if range.is_some() => accesskit::Role::Slider,
        None if clickable => accesskit::Role::Button,
        None if !texts.is_empty() => accesskit::Role::Label,
        None => accesskit::Role::GenericContainer,
    };
    let mut access = Node::new(role);
    let bounds = node.bounds;
    access.set_bounds(Rect {
        x0: bounds.left as f64,
        y0: bounds.top as f64,
        x1: bounds.right as f64,
        y1: bounds.bottom as f64,
    });
    // What a screen reader says: its description, else its pane's title, else its text (a label's
    // own value).
    let label = match (descriptions.is_empty(), pane_title) {
        (false, _) => descriptions,
        (true, Some(title)) => title.to_string(),
        (true, None) => texts.clone(),
    };
    if role == accesskit::Role::Label {
        access.set_value(label);
    } else if !label.is_empty() {
        access.set_label(label);
    }
    if let Some(text) = config.get(&P::EDITABLE_TEXT) {
        access.set_value(text.as_str());
    } else if let Some(state) = config.get(&P::STATE_DESCRIPTION) {
        access.set_value(state.as_str());
    }
    if let Some(error) = config.get(&P::ERROR) {
        access.set_invalid(accesskit::Invalid::True);
        access.set_description(error.as_str());
    }
    if let Some(&state) = config.get(&P::TOGGLEABLE_STATE) {
        access.set_toggled(match state {
            ToggleableState::On => Toggled::True,
            ToggleableState::Off => Toggled::False,
            ToggleableState::Indeterminate => Toggled::Mixed,
        });
    }
    // A radio button's selection is its checked state, as ARIA's `aria-checked`.
    match config.get(&P::SELECTED).copied() {
        Some(selected) if role == accesskit::Role::RadioButton => {
            access.set_toggled(if selected { Toggled::True } else { Toggled::False });
        }
        Some(selected) => access.set_selected(selected),
        None => {}
    }
    if config.contains(&P::IS_DIALOG) {
        access.set_modal();
    }
    if let Some(info) = range {
        access.set_numeric_value(info.current as f64);
        access.set_min_numeric_value(info.range.0 as f64);
        access.set_max_numeric_value(info.range.1 as f64);
        access.set_numeric_value_step(progress_step(info) as f64);
        if !disabled {
            access.add_action(Action::Increment);
            access.add_action(Action::Decrement);
            access.add_action(Action::SetValue);
        }
    }
    if node.hidden {
        access.set_hidden();
    }
    match own.get(&P::LIVE_REGION) {
        Some(LiveRegionMode::Polite) => access.set_live(accesskit::Live::Polite),
        Some(LiveRegionMode::Assertive) => access.set_live(accesskit::Live::Assertive),
        None => {}
    }
    if disabled {
        access.set_disabled();
    } else {
        if clickable {
            access.add_action(Action::Click);
        }
        if own.contains(&P::FOCUSED) {
            access.add_action(Action::Focus);
        }
        if own.contains(&A::EXPAND) {
            access.add_action(Action::Expand);
        }
        if own.contains(&A::COLLAPSE) {
            access.add_action(Action::Collapse);
        }
        let mut custom = Vec::new();
        if let Some(dismiss) = own.get(&A::DISMISS) {
            let label = dismiss.label.as_ref().map_or("Dismiss", |label| label.as_str());
            custom.push(accesskit::CustomAction {
                id: DISMISS_CUSTOM_ACTION,
                description: label.into(),
            });
        }
        if let Some(actions) = own.get(&A::CUSTOM_ACTIONS) {
            for action in actions.iter() {
                custom.push(accesskit::CustomAction {
                    id: custom_action_id(action.label.as_str()),
                    description: action.label.as_str().into(),
                });
            }
        }
        if !custom.is_empty() {
            access.set_custom_actions(custom);
            access.add_action(Action::CustomAction);
        }
    }
    access.set_children(children_in_order(node, order));
    nodes.push((id(node), access));
    for child in &node.children {
        add(child, order, nodes, visit);
    }
}

/// How far a screen reader's increment moves a value: a step, or a twentieth of the range when
/// continuous, as Compose's accessibility delegate.
fn progress_step(info: compose_ui::ProgressBarRangeInfo) -> f32 {
    let length = info.range.1 - info.range.0;
    if info.steps > 0 { length / (info.steps + 1) as f32 } else { length / 20.0 }
}

/// Performs an assistive technology's request (a click, a focus, a value's increment or a new
/// value) as a semantics action. Returns whether a node did it. A disabled node's are refused,
/// as Compose's delegate refuses them (`if (!node.enabled()) return false`, D1255).
pub(crate) fn perform(host: &mut UiHost, request: &ActionRequest) -> bool {
    let (node, disabled, range, custom) = host.with_tree(|tree| {
        let root = tree.accessibility_tree();
        fn find(node: &SemanticsSnapshot, target: AccessNodeId) -> Option<&SemanticsSnapshot> {
            if id(node) == target {
                return Some(node);
            }
            node.children.iter().find_map(|child| find(child, target))
        }
        let found = find(&root, request.target_node);
        (
            found.map(|found| found.id),
            found.is_none_or(|found| found.config.contains(&P::DISABLED)),
            found.and_then(|found| {
                found.unmerged_config().get(&P::PROGRESS_BAR_RANGE_INFO).copied()
            }),
            found.and_then(|found| found.unmerged_config().get(&A::CUSTOM_ACTIONS).cloned()),
        )
    });
    let (Some(node), false) = (node, disabled) else { return false };
    let progress = || range;
    let progress_to = match request.action {
        Action::Click => return host.perform_semantics_action(node, &A::ON_CLICK),
        Action::Focus => return host.perform_semantics_action(node, &A::REQUEST_FOCUS),
        Action::Expand => return host.perform_semantics_action(node, &A::EXPAND),
        Action::Collapse => return host.perform_semantics_action(node, &A::COLLAPSE),
        Action::CustomAction => {
            let Some(accesskit::ActionData::CustomAction(action)) = request.data else {
                return false;
            };
            if action == DISMISS_CUSTOM_ACTION {
                return host.perform_semantics_action(node, &A::DISMISS);
            }
            let found = custom.as_ref().and_then(|all| {
                all.iter().find(|custom| custom_action_id(custom.label.as_str()) == action)
            });
            let Some(found) = found else { return false };
            return host.perform_custom_action(node, found.label.as_str());
        }
        Action::Increment | Action::Decrement => {
            let Some(info) = progress() else { return false };
            let step = progress_step(info);
            let delta = if request.action == Action::Increment { step } else { -step };
            info.current + delta
        }
        Action::SetValue => match &request.data {
            Some(accesskit::ActionData::NumericValue(value)) => *value as f32,
            Some(accesskit::ActionData::Value(text)) => match text.trim().parse::<f32>() {
                Ok(value) => value,
                Err(_) => return false,
            },
            _ => return false,
        },
        _ => return false,
    };
    host.perform_semantics_action_with(node, &A::SET_PROGRESS, |set| set(progress_to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose_ui::{
        ClickableOptions, Density, IntSize, Modifier, UnitExt, platform::FixedTextMeasurer,
    };

    /// A host of `content`, settled.
    fn host_of(content: impl Fn() + 'static) -> UiHost {
        let mut host = UiHost::new(
            IntSize::new(400, 400),
            Density::default(),
            Box::new(FixedTextMeasurer::default()),
        );
        host.set_content(content);
        host.settle(20);
        host
    }

    fn says(config: &mut compose_ui::SemanticsConfiguration) {
        config.set_content_description("says");
    }

    /// The node saying `tag` as its test tag, in a tree update.
    fn access_id(host: &UiHost, tag: &str) -> AccessNodeId {
        host.with_tree(|tree| {
            fn find(node: &SemanticsSnapshot, tag: &str) -> Option<AccessNodeId> {
                if node.config.get(&P::TEST_TAG).is_some_and(|it| it.as_str() == tag) {
                    return Some(id(node));
                }
                node.children.iter().find_map(|child| find(child, tag))
            }
            find(&tree.accessibility_tree(), tag).expect("tagged")
        })
    }

    fn node_of(update: &TreeUpdate, id: AccessNodeId) -> &Node {
        &update.nodes.iter().find(|(it, _)| *it == id).expect("in the update").1
    }

    #[test]
    fn a_nodes_children_are_given_in_the_order_they_are_read() {
        let host = host_of(|| {
            use compose_foundation::layout::*;
            let parent = Modifier.size(400.dp()).test_tag("parent");
            Box!(modifier = parent, {
                let lower = Modifier.offset(0.dp(), 100.dp()).size(10.dp());
                Box!(modifier = lower.semantics(says).test_tag("lower"), {});
                let upper = Modifier.size(10.dp());
                Box!(modifier = upper.semantics(says).test_tag("upper"), {});
            });
        });
        let update = tree_update(&host, "title");
        let children = node_of(&update, access_id(&host, "parent")).children();
        assert_eq!(children, [access_id(&host, "upper"), access_id(&host, "lower")]);
    }

    #[test]
    fn a_live_region_expand_collapse_and_custom_actions_reach_the_tree() {
        let host = host_of(|| {
            let node = Modifier.size(10.dp()).test_tag("node").semantics(|config| {
                config.set_live_region(LiveRegionMode::Assertive);
                config.expand(None, || true);
                config.collapse(None, || true);
                config.dismiss(Some("Close".into()), || true);
                config.set_custom_actions(vec![compose_ui::CustomAccessibilityAction::new(
                    "Archive",
                    || true,
                )]);
            });
            compose_foundation::layout::Box(
                node,
                compose_ui::Alignment::TOP_START,
                false,
                compose_runtime::callback(|_| {}),
            );
        });
        let update = tree_update(&host, "title");
        let node = node_of(&update, access_id(&host, "node"));
        assert_eq!(node.live(), Some(accesskit::Live::Assertive));
        assert!(node.supports_action(Action::Expand));
        assert!(node.supports_action(Action::Collapse));
        assert!(node.supports_action(Action::CustomAction));
        let labels: Vec<_> = node.custom_actions().iter().map(|it| &*it.description).collect();
        assert_eq!(labels, ["Close", "Archive"]);
    }

    #[test]
    fn a_custom_action_is_found_by_what_a_screen_reader_was_shown_or_not_at_all() {
        use std::cell::RefCell;
        let done: &'static RefCell<Vec<&'static str>> = Box::leak(Box::default());
        let labels = Box::leak(Box::new(compose_runtime::mutable_state_of(vec!["one", "two"])));
        let labels = labels.handle();
        let mut host = host_of(move || {
            use compose_foundation::layout::*;
            let node = Modifier.size(10.dp()).test_tag("node").semantics(move |config| {
                config.set_custom_actions(
                    labels
                        .get()
                        .into_iter()
                        .map(|label| {
                            compose_ui::CustomAccessibilityAction::new(label, move || {
                                done.borrow_mut().push(label);
                                true
                            })
                        })
                        .collect(),
                );
            });
            Box!(modifier = node, {});
        });
        let target = access_id(&host, "node");
        let update = tree_update(&host, "title");
        let shown = node_of(&update, target).custom_actions()[1].id;
        let ask = |host: &mut UiHost, action| {
            perform(
                host,
                &ActionRequest {
                    action: Action::CustomAction,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: target,
                    data: Some(accesskit::ActionData::CustomAction(action)),
                },
            )
        };
        labels.set(vec!["zero", "one", "two"]);
        host.settle(20);
        assert!(ask(&mut host, shown), "what it was shown, wherever that is now");
        assert_eq!(*done.borrow(), ["two"]);
        labels.set(vec!["zero"]);
        host.settle(20);
        assert!(!ask(&mut host, shown), "it's gone");
        assert_eq!(*done.borrow(), ["two"]);
    }

    #[test]
    fn the_tree_has_the_window_its_nodes_and_their_actions() {
        use std::cell::Cell;
        use std::rc::Rc;
        let clicks = Rc::new(Cell::new(0));
        let counted = clicks.clone();
        let mut host = UiHost::new(
            IntSize::new(200, 100),
            Density::default(),
            Box::new(FixedTextMeasurer::default()),
        );
        host.set_content(move || {
            let counted = counted.clone();
            compose_foundation::layout::Box(
                Modifier.size((80.dp(), 30.dp())).clickable_with(
                    ClickableOptions { role: Some(Role::Button), ..ClickableOptions::DEFAULT },
                    move || counted.set(counted.get() + 1),
                ),
                compose_ui::Alignment::TOP_START,
                false,
                compose_runtime::callback(|_| {
                    compose_foundation::BasicText(
                        "Save".into(),
                        Modifier,
                        compose_ui::TextStyle::DEFAULT,
                        None,
                        compose_ui::TextOverflow::Clip,
                        true,
                        u32::MAX,
                        1,
                    )
                }),
            );
        });
        host.frame();
        let update = tree_update(&host, "App");
        let root = update.tree.as_ref().unwrap().root;
        let (_, window) = update.nodes.iter().find(|(node, _)| *node == root).unwrap();
        assert_eq!(window.role(), accesskit::Role::Window);
        assert_eq!(window.label(), Some("App"));
        let button = window.children()[0];
        let (_, node) = update.nodes.iter().find(|(node, _)| *node == button).unwrap();
        assert_eq!(node.role(), accesskit::Role::Button);
        assert_eq!(node.label(), Some("Save"), "the merged text");
        assert!(node.supports_action(Action::Click));
        let bounds = node.bounds().unwrap();
        assert_eq!((bounds.x1, bounds.y1), (80.0, 30.0));
        // A screen reader's click.
        let request = ActionRequest {
            action: Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: button,
            data: None,
        };
        assert!(perform(&mut host, &request));
        assert_eq!(clicks.get(), 1);
        // Its focus, then the tree's focus on it.
        let focus = ActionRequest { action: Action::Focus, ..request };
        assert!(perform(&mut host, &focus));
        host.frame();
        assert_eq!(tree_update(&host, "App").focus, button);
    }

    fn host_with(content: impl Fn() + 'static) -> UiHost {
        let mut host = UiHost::new(
            IntSize::new(200, 100),
            Density::default(),
            Box::new(FixedTextMeasurer::default()),
        );
        host.set_content(content);
        host.frame();
        host
    }

    /// The window's first node and its id.
    fn first_node(host: &UiHost) -> (AccessNodeId, Node) {
        let update = tree_update(host, "App");
        let root = update.tree.as_ref().unwrap().root;
        let (_, window) = update.nodes.iter().find(|(node, _)| *node == root).unwrap();
        let first = window.children()[0];
        let (_, node) = update.nodes.iter().find(|(node, _)| *node == first).unwrap();
        (first, node.clone())
    }

    fn click(node: AccessNodeId) -> ActionRequest {
        ActionRequest {
            action: Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: node,
            data: None,
        }
    }

    /// A node reused is another AccessKit node: the old id's request does nothing (D1262).
    #[test]
    fn a_reused_node_has_another_id() {
        use compose_foundation::layout::*;
        use compose_runtime::{ReusableContent, mutable_state_of};
        let key = mutable_state_of(0);
        let handle = key.handle();
        let clicks = std::rc::Rc::new(std::cell::Cell::new(0));
        let counted = clicks.clone();
        let mut host = host_with(move || {
            let counted = counted.clone();
            ReusableContent(handle.get(), move || {
                let counted = counted.clone();
                Box!(
                    modifier = compose_ui::Modifier
                        .size(compose_ui::UnitExt::dp(10))
                        .clickable(move || counted.set(counted.get() + 1))
                );
            });
        });
        let (before, _) = first_node(&host);
        key.set(1);
        host.settle(10);
        let (after, _) = first_node(&host);
        assert_ne!(before, after);
        assert!(!perform(&mut host, &click(before)), "no node has the old id");
        assert_eq!(clicks.get(), 0);
        assert!(perform(&mut host, &click(after)));
        assert_eq!(clicks.get(), 1);
    }

    // Compose: compose/ui/ui/src/androidMain/kotlin/androidx/compose/ui/platform/AndroidComposeViewAccessibilityDelegateCompat.android.kt, the actions read from `unmergedConfig`
    /// A merging node offers its own actions, not those its descendants merged into it (D1252):
    /// a merging group holding a clickable part and a range offers no click and no increments,
    /// though Compose's policies merge both into its merged configuration.
    #[test]
    fn a_merging_node_offers_its_own_actions_only() {
        use compose_ui::{ProgressBarRangeInfo, SemanticsActions};
        let host = host_with(|| {
            compose_foundation::layout::Box(
                Modifier.size((80.dp(), 30.dp())).semantics_with(true, |_| {}),
                compose_ui::Alignment::TOP_START,
                false,
                compose_runtime::callback(|_| {
                    compose_foundation::layout::Box(
                        Modifier.size(10.dp()).semantics(|config| {
                            config.on_click(None, || true);
                            config.set_progress_bar_range_info(ProgressBarRangeInfo {
                                current: 0.5,
                                range: (0.0, 1.0),
                                steps: 0,
                            });
                            config.set_progress(None, |_| true);
                        }),
                        compose_ui::Alignment::TOP_START,
                        false,
                        compose_runtime::callback(|_| {}),
                    )
                }),
            );
        });
        let merged = host.semantics_tree(true);
        assert!(merged.children[0].config.contains(&SemanticsActions::ON_CLICK));
        let (_, node) = first_node(&host);
        assert!(!node.supports_action(Action::Click), "{node:?}");
        assert!(!node.supports_action(Action::Increment), "{node:?}");
        assert!(!node.supports_action(Action::ScrollRight), "{node:?}");
    }

    /// A card's click is its own `OnClick`, whatever its descendants merged.
    #[test]
    fn a_cards_click_is_its_own() {
        use std::cell::Cell;
        use std::rc::Rc;
        let (card, part) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let (card_count, part_count) = (card.clone(), part.clone());
        let mut host = host_with(move || {
            let (card_count, part_count) = (card_count.clone(), part_count.clone());
            compose_foundation::layout::Box(
                Modifier
                    .size((80.dp(), 30.dp()))
                    .clickable(move || card_count.set(card_count.get() + 1)),
                compose_ui::Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| {
                    let part_count = part_count.clone();
                    compose_foundation::layout::Box(
                        Modifier.size(10.dp()).semantics(move |config| {
                            let part_count = part_count.clone();
                            config.on_click(None, move || {
                                part_count.set(part_count.get() + 1);
                                true
                            });
                        }),
                        compose_ui::Alignment::TOP_START,
                        false,
                        compose_runtime::callback(|_| {}),
                    )
                }),
            );
        });
        let (id, node) = first_node(&host);
        assert!(node.supports_action(Action::Click));
        assert!(perform(&mut host, &click(id)));
        assert_eq!((card.get(), part.get()), (1, 0));
    }

    // Compose: compose/ui/ui/src/androidMain/kotlin/androidx/compose/ui/platform/AndroidComposeViewAccessibilityDelegateCompat.android.kt, performActionHelper's `if (!node.enabled()) return false`
    /// A disabled node's actions are refused by the bridge, as Compose's delegate refuses them:
    /// its click, which would call the callback, isn't called (D1255).
    #[test]
    fn a_disabled_nodes_actions_are_refused() {
        use std::cell::Cell;
        use std::rc::Rc;
        let clicks = Rc::new(Cell::new(0));
        let counted = clicks.clone();
        let mut host = host_with(move || {
            let counted = counted.clone();
            compose_foundation::layout::Box(
                Modifier.size((80.dp(), 30.dp())).clickable_with(
                    ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT },
                    move || counted.set(counted.get() + 1),
                ),
                compose_ui::Alignment::TOP_START,
                false,
                compose_runtime::callback(|_| {}),
            );
        });
        let (id, _) = first_node(&host);
        assert!(!perform(&mut host, &click(id)));
        assert_eq!(clicks.get(), 0);
    }

    #[test]
    fn progress_steps_are_a_stop_or_a_twentieth_of_the_range() {
        let info =
            |steps| compose_ui::ProgressBarRangeInfo { current: 0.0, range: (0.0, 10.0), steps };
        assert_eq!(progress_step(info(0)), 0.5);
        assert_eq!(progress_step(info(4)), 2.0);
    }
}
