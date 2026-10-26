//! Interactions: a node (or nodes) found by a selector, acted on and asserted about, as
//! Compose's `SemanticsNodeInteraction` and `SemanticsNodeInteractionCollection`. They find
//! their nodes again for every call, in the tree as it is then, after waiting for idle.

use compose_ui::ImeAction;
use compose_ui::platform::{NodeId, SemanticsNode};
use compose_ui::{Key, ProgressBarRangeInfo, SemanticsAction};

use crate::matcher::{Matcher, NodeRef, TreeView, same_set};
use crate::print::print_node;
use crate::rule::ComposeTestRule;
use crate::selector::{Relative, Selector};

/// A node found by a selector, as `SemanticsNodeInteraction`.
pub struct SemanticsNodeInteraction<'a> {
    pub(crate) rule: &'a mut ComposeTestRule,
    pub(crate) selector: Selector,
    pub(crate) merged: bool,
}

/// Nodes found by a selector, as `SemanticsNodeInteractionCollection`.
pub struct SemanticsNodeInteractionCollection<'a> {
    rule: &'a mut ComposeTestRule,
    selector: Selector,
    merged: bool,
}

impl<'a> SemanticsNodeInteraction<'a> {
    pub(crate) fn new(
        rule: &'a mut ComposeTestRule,
        selector: Selector,
        merged: bool,
    ) -> SemanticsNodeInteraction<'a> {
        SemanticsNodeInteraction { rule, selector, merged }
    }

    /// What the selector describes, for messages.
    pub fn description(&self) -> &str {
        &self.selector.description
    }

    /// Waits for idle (unless told not to), then runs `f` with the tree and the nodes the
    /// selector picks in it (or why it can't pick).
    pub(crate) fn with_selection<R>(
        &mut self,
        f: impl FnOnce(&TreeView<'_>, Result<Vec<NodeRef<'_>>, String>) -> R,
    ) -> R {
        self.rule.implicit_wait();
        let tree = self.rule.host.semantics_tree(self.merged);
        let view = TreeView::new(&tree);
        let selection = self.selector.select(&view).and_then(|selection| {
            if selection.nodes.is_empty()
                && let Some(error) = selection.custom_error
            {
                return Err(error);
            }
            Ok(selection.nodes)
        });
        f(&view, selection)
    }

    /// Runs `f` with the one node selected, in its tree; an error message otherwise.
    pub(crate) fn with_node<R>(&mut self, f: impl FnOnce(NodeRef<'_>) -> R) -> Result<R, String> {
        let description = self.selector.description.clone();
        let tree_name = if self.merged { "merged" } else { "unmerged" };
        self.with_selection(|view, selection| match selection {
            Ok(nodes) if nodes.len() == 1 => Ok(f(nodes[0])),
            Ok(nodes) => Err(format!(
                "expected exactly 1 node {description}, found {} in the {tree_name} tree:\n{}",
                nodes.len(),
                view.node(0).node().dump()
            )),
            Err(error) => {
                Err(format!("{error}\nin the {tree_name} tree:\n{}", view.node(0).node().dump()))
            }
        })
    }

    /// The node, or an error message.
    fn find(&mut self) -> Result<SemanticsNode, String> {
        self.with_node(|node| node.node().clone())
    }

    /// The node, as `fetchSemanticsNode`. Panics unless exactly one matches.
    #[track_caller]
    pub fn fetch_semantics_node(&mut self) -> SemanticsNode {
        self.find().unwrap_or_else(|message| panic!("compose-test: {message}"))
    }

    pub fn node_id(&mut self) -> NodeId {
        self.fetch_semantics_node().id
    }

    // ---- Navigation -------------------------------------------------------------------------

    fn step(self, relative: Relative) -> SemanticsNodeInteraction<'a> {
        let selector = self.selector.relative(relative);
        SemanticsNodeInteraction { rule: self.rule, selector, merged: self.merged }
    }

    fn steps(self, relative: Relative) -> SemanticsNodeInteractionCollection<'a> {
        let selector = self.selector.relative(relative);
        SemanticsNodeInteractionCollection { rule: self.rule, selector, merged: self.merged }
    }

    /// The node's parent, as `onParent`.
    pub fn on_parent(self) -> SemanticsNodeInteraction<'a> {
        self.step(Relative::Parent)
    }

    /// Its one child, as `onChild`.
    pub fn on_child(self) -> SemanticsNodeInteraction<'a> {
        self.step(Relative::Child)
    }

    /// Its children, as `onChildren`.
    pub fn on_children(self) -> SemanticsNodeInteractionCollection<'a> {
        self.steps(Relative::Children)
    }

    /// Its child at `index`, as `onChildAt`.
    pub fn on_child_at(self, index: usize) -> SemanticsNodeInteraction<'a> {
        self.on_children().get(index)
    }

    /// Its one sibling, as `onSibling`.
    pub fn on_sibling(self) -> SemanticsNodeInteraction<'a> {
        self.step(Relative::Sibling)
    }

    /// Its siblings, as `onSiblings`.
    pub fn on_siblings(self) -> SemanticsNodeInteractionCollection<'a> {
        self.steps(Relative::Siblings)
    }

    /// Its ancestors, the parent first, as `onAncestors`.
    pub fn on_ancestors(self) -> SemanticsNodeInteractionCollection<'a> {
        self.steps(Relative::Ancestors)
    }

    /// Its descendants, as `onDescendants`.
    pub fn on_descendants(self) -> SemanticsNodeInteractionCollection<'a> {
        self.steps(Relative::Descendants)
    }

    // ---- Actions ----------------------------------------------------------------------------

    /// Clicks the node's center, as `performClick`: a touch down, a move one event period
    /// (16 ms) later, and an up, as `performTouchInput { click() }`, then waits for idle.
    pub fn perform_click(&mut self) -> &mut Self {
        self.perform_touch_input(|touch| touch.click())
    }

    /// Performs `action` on the node, as `performSemanticsAction` (an accessibility service's
    /// request, not input), then waits for idle. Panics if the node doesn't offer it.
    #[track_caller]
    pub fn perform_semantics_action(&mut self, action: SemanticsAction) -> &mut Self {
        let node = self.fetch_semantics_node();
        let performed = self.rule.host.perform_semantics_action(node.id, action);
        assert!(performed, "compose-test: the node doesn't offer {action:?}: {:?}", node.config);
        self.rule.wait_for_idle();
        self
    }

    /// Focuses the node and checks it took the focus, for the text actions.
    #[track_caller]
    fn focus_for_text(&mut self, what: &str, require_editable: bool) -> SemanticsNode {
        let node = self.fetch_semantics_node();
        assert!(
            !node.config.disabled,
            "compose-test: {what} on a node that isn't enabled: {:?}",
            node.config
        );
        assert!(
            !require_editable || node.config.editable,
            "compose-test: {what} on a node that isn't editable: {:?}",
            node.config
        );
        self.rule.host.request_focus(node.id);
        let focused = self.rule.host.with_tree(|tree| {
            tree.focused().is_some_and(|focused| tree.is_at_or_below(focused, node.id))
        });
        assert!(focused, "compose-test: {what} on a node that can't take focus: {:?}", node.config);
        node
    }

    /// Focuses the node and types `text` into it, as `performTextInput`.
    #[track_caller]
    pub fn perform_text_input(&mut self, text: &str) -> &mut Self {
        self.focus_for_text("performTextInput", true);
        self.rule.host.send_text(text);
        self.rule.wait_for_idle();
        self
    }

    /// Focuses the node and performs its IME action (`KeyboardOptions.ime_action`), as
    /// `performImeAction`: fails if it has the default action, or if nothing took it.
    #[track_caller]
    pub fn perform_ime_action(&mut self) -> &mut Self {
        let node = self.focus_for_text("performImeAction", false);
        let options = self.rule.host.text_input_options();
        let action = options.map(|options| options.ime_action);
        assert!(
            action.is_some_and(|action| action != ImeAction::Default),
            "compose-test: Failed to perform IME action: the node has no IME action ({action:?}): \
             {:?}",
            node.config
        );
        let taken = self.rule.host.perform_ime_action(action.unwrap_or_default());
        assert!(
            taken,
            "compose-test: Failed to perform IME action, handler returned false: {:?}",
            node.config
        );
        self.rule.wait_for_idle();
        self
    }

    /// Focuses the node, as `requestFocus`.
    pub fn request_focus(&mut self) -> &mut Self {
        let node = self.node_id();
        self.rule.host.request_focus(node);
        self.rule.wait_for_idle();
        self
    }

    /// Focuses the node and presses and releases `key`, as `performKeyPress`.
    pub fn perform_key_press(&mut self, key: Key) -> &mut Self {
        self.request_focus();
        self.rule.press_key(key);
        self
    }

    /// Scrolls the scrolling containers around the node until it's in view, as
    /// `performScrollTo`: through each one's `ScrollBy` action, the nearest first.
    #[track_caller]
    pub fn perform_scroll_to(&mut self) -> &mut Self {
        for _ in 0..1000 {
            let scroll = self
                .with_node(|node| {
                    let scrollable = node.ancestors().find(|ancestor| {
                        let config = ancestor.config();
                        config.horizontal_scroll_axis_range.is_some()
                            || config.vertical_scroll_axis_range.is_some()
                    })?;
                    let viewport = scrollable.bounds();
                    let target = node.bounds();
                    let delta = |a: f32, b: f32| {
                        if a.signum() == b.signum() {
                            if a.abs() < b.abs() { a } else { b }
                        } else {
                            0.0
                        }
                    };
                    let config = scrollable.config();
                    let mut dx = delta(target.left - viewport.left, target.right - viewport.right);
                    if config.horizontal_scroll_axis_range.is_some_and(|r| r.reverse_scrolling) {
                        dx = -dx;
                    }
                    if scrollable.node().rtl {
                        dx = -dx;
                    }
                    let mut dy = delta(target.top - viewport.top, target.bottom - viewport.bottom);
                    if config.vertical_scroll_axis_range.is_some_and(|r| r.reverse_scrolling) {
                        dy = -dy;
                    }
                    let scrolls = (dx != 0.0 && config.horizontal_scroll_axis_range.is_some())
                        || (dy != 0.0 && config.vertical_scroll_axis_range.is_some());
                    Some((scrollable.id(), scrolls.then_some((dx, dy))))
                })
                .unwrap_or_else(|message| panic!("compose-test: performScrollTo: {message}"));
            let Some((scrollable, delta)) = scroll else {
                panic!(
                    "compose-test: performScrollTo: the node {} has no parent with a scroll \
                     action",
                    self.selector.description
                );
            };
            let Some((x, y)) = delta else { return self };
            assert!(
                self.rule.auto_advance(),
                "compose-test: performScrollTo can't scroll the node into view with the clock \
                 advanced by hand: scrolls take frames"
            );
            self.rule.host.perform_semantics_action(scrollable, SemanticsAction::ScrollBy { x, y });
        }
        panic!("compose-test: performScrollTo never brought the node into view");
    }

    // ---- Assertions -------------------------------------------------------------------------

    #[track_caller]
    pub fn assert_exists(&mut self) -> &mut Self {
        if let Err(message) = self.find() {
            panic!("compose-test: assert_exists failed: {message}");
        }
        self
    }

    #[track_caller]
    pub fn assert_does_not_exist(&mut self) -> &mut Self {
        let description = self.selector.description.clone();
        let count = self.with_selection(|view, selection| match selection {
            Ok(nodes) => Ok((nodes.len(), view.node(0).node().dump())),
            // An index past the nodes selects nothing.
            Err(error) if error.starts_with("can't retrieve node at index") => {
                Ok((0, String::new()))
            }
            Err(error) => Err(error),
        });
        match count {
            Ok((0, _)) => {}
            Ok((count, dump)) => panic!(
                "compose-test: assert_does_not_exist failed: {count} nodes {description} in\n{dump}"
            ),
            Err(error) => panic!("compose-test: assert_does_not_exist failed: {error}"),
        }
        self
    }

    /// Checks `matcher` on the node, in its tree, as `assert(matcher)`.
    #[track_caller]
    pub fn assert(&mut self, matcher: Matcher) -> &mut Self {
        let description = self.selector.description.clone();
        let result = self.with_node(|node| {
            matcher.matches_node(node).then_some(()).ok_or_else(|| print_node(node, 0))
        });
        match result {
            Ok(Ok(())) => self,
            Ok(Err(node)) => panic!(
                "compose-test: Assert failed: the node {description} doesn't satisfy ({}):\n{node}",
                matcher.description
            ),
            Err(message) => panic!("compose-test: Assert failed: {message}"),
        }
    }

    #[track_caller]
    pub fn assert_is_enabled(&mut self) -> &mut Self {
        self.assert(crate::is_enabled())
    }

    #[track_caller]
    pub fn assert_is_not_enabled(&mut self) -> &mut Self {
        self.assert(crate::is_not_enabled())
    }

    #[track_caller]
    pub fn assert_is_on(&mut self) -> &mut Self {
        self.assert(crate::is_on())
    }

    #[track_caller]
    pub fn assert_is_off(&mut self) -> &mut Self {
        self.assert(crate::is_off())
    }

    #[track_caller]
    pub fn assert_is_toggleable(&mut self) -> &mut Self {
        self.assert(crate::is_toggleable())
    }

    #[track_caller]
    pub fn assert_is_selectable(&mut self) -> &mut Self {
        self.assert(crate::is_selectable())
    }

    #[track_caller]
    pub fn assert_is_selected(&mut self) -> &mut Self {
        self.assert(crate::is_selected())
    }

    #[track_caller]
    pub fn assert_is_not_selected(&mut self) -> &mut Self {
        self.assert(crate::is_not_selected())
    }

    #[track_caller]
    pub fn assert_is_focused(&mut self) -> &mut Self {
        self.assert(crate::is_focused())
    }

    #[track_caller]
    pub fn assert_is_not_focused(&mut self) -> &mut Self {
        self.assert(crate::is_not_focused())
    }

    #[track_caller]
    pub fn assert_has_click_action(&mut self) -> &mut Self {
        self.assert(crate::has_click_action())
    }

    #[track_caller]
    pub fn assert_has_no_click_action(&mut self) -> &mut Self {
        self.assert(crate::has_no_click_action())
    }

    /// The node's text, merged, is `texts` (its editable text included), compared as sets, as
    /// `assertTextEquals`.
    #[track_caller]
    pub fn assert_text_equals(&mut self, texts: &[&str]) -> &mut Self {
        self.assert_text_equals_with(texts, true)
    }

    /// The same, leaving the editable text out unless `include_editable_text`, as
    /// `assertTextEquals(…, includeEditableText)`.
    #[track_caller]
    pub fn assert_text_equals_with(
        &mut self,
        texts: &[&str],
        include_editable_text: bool,
    ) -> &mut Self {
        self.assert(crate::has_text_exactly(texts, include_editable_text))
    }

    /// One of the node's texts (or its editable text) is `value`, or contains it with
    /// `substring`, as `assertTextContains(value, substring, ignoreCase)`.
    #[track_caller]
    pub fn assert_text_contains(
        &mut self,
        value: &str,
        substring: bool,
        ignore_case: bool,
    ) -> &mut Self {
        self.assert(crate::has_text_with(value, substring, ignore_case))
    }

    /// The node's content descriptions are `descriptions`, as sets, as
    /// `assertContentDescriptionEquals`.
    #[track_caller]
    pub fn assert_content_description_equals(&mut self, descriptions: &[&str]) -> &mut Self {
        let expected: Vec<String> = descriptions.iter().map(|d| d.to_string()).collect();
        self.assert(Matcher::new(format!("content descriptions = {expected:?}"), move |config| {
            let actual: Vec<&str> = config.content_description.iter().map(|d| d.as_str()).collect();
            same_set(&actual, &expected)
        }))
    }

    /// As `assertContentDescriptionContains(value, substring, ignoreCase)`.
    #[track_caller]
    pub fn assert_content_description_contains(
        &mut self,
        value: &str,
        substring: bool,
        ignore_case: bool,
    ) -> &mut Self {
        self.assert(crate::has_content_description_with(value, substring, ignore_case))
    }

    /// The node's state description is `value`, as `assertValueEquals`.
    #[track_caller]
    pub fn assert_value_equals(&mut self, value: &str) -> &mut Self {
        self.assert(crate::has_state_description(value))
    }

    /// As `assertRangeInfoEquals`.
    #[track_caller]
    pub fn assert_range_info_equals(&mut self, info: ProgressBarRangeInfo) -> &mut Self {
        self.assert(crate::has_progress_bar_range_info(info))
    }

    // ---- Displayed ----------------------------------------------------------------------------

    /// Whether the node shows: exactly one matches (none: no), it's placed, and its bounds
    /// clipped by the clips around it are non-empty and inside the window (in part, or wholly
    /// with `fully`), as Compose's `checkIsDisplayed`.
    #[track_caller]
    fn check_is_displayed(&mut self, fully: bool) -> bool {
        let description = self.selector.description.clone();
        let found = self.with_selection(|view, selection| {
            let window = view.node(0).bounds();
            match selection {
                Ok(nodes) if nodes.is_empty() => Ok(None),
                Ok(nodes) if nodes.len() > 1 => Err(format!(
                    "Failed to perform checkIsDisplayed check: expected at most 1 node but found \
                     {} nodes that satisfy ({description})",
                    nodes.len()
                )),
                Ok(nodes) => Ok(Some((nodes[0].id(), nodes[0].is_root(), window))),
                Err(_) => Ok(None),
            }
        });
        let (id, is_root, window) = match found {
            Ok(Some(found)) => found,
            Ok(None) => return false,
            Err(message) => panic!("compose-test: {message}"),
        };
        let clipped = if is_root {
            window
        } else {
            match self.rule.host.with_tree(|tree| tree.semantics_bounds_in_root(id)) {
                Some((_, clipped)) => clipped,
                None => return false,
            }
        };
        if clipped.width() <= 0.0 || clipped.height() <= 0.0 {
            return false;
        }
        if fully {
            clipped.left >= window.left
                && clipped.top >= window.top
                && clipped.right <= window.right
                && clipped.bottom <= window.bottom
        } else {
            clipped.left.max(window.left) < clipped.right.min(window.right)
                && clipped.top.max(window.top) < clipped.bottom.min(window.bottom)
        }
    }

    /// Whether the node shows, at least in part, as `isDisplayed`.
    pub fn is_displayed(&mut self) -> bool {
        self.check_is_displayed(false)
    }

    /// Whether the node doesn't show wholly, as `isNotDisplayed`: missing, unplaced, clipped
    /// away or empty, or partly outside the window.
    pub fn is_not_displayed(&mut self) -> bool {
        !self.check_is_displayed(true)
    }

    /// As `assertIsDisplayed`.
    #[track_caller]
    pub fn assert_is_displayed(&mut self) -> &mut Self {
        assert!(
            self.is_displayed(),
            "compose-test: Assert failed: the component {} is not displayed!",
            self.selector.description
        );
        self
    }

    /// As `assertIsNotDisplayed`.
    #[track_caller]
    pub fn assert_is_not_displayed(&mut self) -> &mut Self {
        assert!(
            self.is_not_displayed(),
            "compose-test: Assert failed: the component {} is displayed!",
            self.selector.description
        );
        self
    }

    // ---- Dumps ------------------------------------------------------------------------------

    /// The node and its descendants to `max_depth` (`usize::MAX`: all), as `printToString`.
    #[track_caller]
    pub fn print_to_string(&mut self, max_depth: usize) -> String {
        self.with_node(|node| print_node(node, max_depth))
            .unwrap_or_else(|message| panic!("compose-test: printToString: {message}"))
    }

    /// Prints [`print_to_string`](Self::print_to_string) to stderr under `tag`, as
    /// `printToLog`.
    pub fn print_to_log(&mut self, tag: &str, max_depth: usize) -> &mut Self {
        let printed = self.print_to_string(max_depth);
        eprintln!("{tag}: {printed}");
        self
    }
}

impl<'a> SemanticsNodeInteractionCollection<'a> {
    pub(crate) fn new(
        rule: &'a mut ComposeTestRule,
        selector: Selector,
        merged: bool,
    ) -> SemanticsNodeInteractionCollection<'a> {
        SemanticsNodeInteractionCollection { rule, selector, merged }
    }

    /// Runs `f` with the nodes selected (after waiting for idle), or panics with why there
    /// are none.
    #[track_caller]
    fn with_nodes<R>(&mut self, f: impl FnOnce(&[NodeRef<'_>]) -> R) -> R {
        self.rule.implicit_wait();
        let tree = self.rule.host.semantics_tree(self.merged);
        let view = TreeView::new(&tree);
        match self.selector.select(&view) {
            Ok(selection) => f(&selection.nodes),
            Err(error) => panic!("compose-test: {error}\nin\n{}", tree.dump()),
        }
    }

    /// The nodes, as `fetchSemanticsNodes`.
    #[track_caller]
    pub fn fetch_semantics_nodes(&mut self) -> Vec<SemanticsNode> {
        self.with_nodes(|nodes| nodes.iter().map(|node| node.node().clone()).collect())
    }

    /// The node at `index`, as `get(index)`.
    pub fn get(self, index: usize) -> SemanticsNodeInteraction<'a> {
        let selector = self.selector.index(index);
        SemanticsNodeInteraction { rule: self.rule, selector, merged: self.merged }
    }

    /// The first node, as `onFirst`.
    pub fn on_first(self) -> SemanticsNodeInteraction<'a> {
        self.get(0)
    }

    /// The last node, as `onLast`.
    pub fn on_last(self) -> SemanticsNodeInteraction<'a> {
        let selector = self.selector.last();
        SemanticsNodeInteraction { rule: self.rule, selector, merged: self.merged }
    }

    /// The nodes matching `matcher`, as `filter`.
    pub fn filter(self, matcher: Matcher) -> SemanticsNodeInteractionCollection<'a> {
        let selector = self.selector.filter("filter", matcher);
        SemanticsNodeInteractionCollection { rule: self.rule, selector, merged: self.merged }
    }

    /// The one node matching `matcher`, as `filterToOne`.
    pub fn filter_to_one(self, matcher: Matcher) -> SemanticsNodeInteraction<'a> {
        let selector = self.selector.filter("filterToOne", matcher);
        SemanticsNodeInteraction { rule: self.rule, selector, merged: self.merged }
    }

    /// As `assertCountEquals`.
    #[track_caller]
    pub fn assert_count_equals(&mut self, expected: usize) -> &mut Self {
        let description = self.selector.description.clone();
        let (count, printed) = self.with_nodes(|nodes| {
            (nodes.len(), nodes.iter().map(|node| print_node(*node, 0)).collect::<Vec<_>>())
        });
        assert!(
            count == expected,
            "compose-test: Failed to assert count of nodes: expected {expected} but found \
             {count} nodes {description}:\n{}",
            printed.join("\n")
        );
        self
    }

    /// Every node satisfies `matcher`, as `assertAll`.
    #[track_caller]
    pub fn assert_all(&mut self, matcher: Matcher) -> &mut Self {
        let description = self.selector.description.clone();
        let failing = self.with_nodes(|nodes| {
            nodes
                .iter()
                .filter(|node| !matcher.matches_node(**node))
                .map(|node| print_node(*node, 0))
                .collect::<Vec<_>>()
        });
        assert!(
            failing.is_empty(),
            "compose-test: Failed to assertAll({}) on the nodes {description}: these don't:\n{}",
            matcher.description,
            failing.join("\n")
        );
        self
    }

    /// At least one node satisfies `matcher`, as `assertAny`.
    #[track_caller]
    pub fn assert_any(&mut self, matcher: Matcher) -> &mut Self {
        let description = self.selector.description.clone();
        let (any, count) = self.with_nodes(|nodes| {
            (nodes.iter().any(|node| matcher.matches_node(*node)), nodes.len())
        });
        assert!(
            any,
            "compose-test: Failed to assertAny({}) on the {count} nodes {description}",
            matcher.description
        );
        self
    }

    /// Each node and its descendants to `max_depth`, as `printToString`.
    pub fn print_to_string(&mut self, max_depth: usize) -> String {
        self.with_nodes(|nodes| {
            nodes
                .iter()
                .enumerate()
                .map(|(index, node)| format!("{index}) {}", print_node(*node, max_depth)))
                .collect::<Vec<_>>()
                .join("\n")
        })
    }
}
