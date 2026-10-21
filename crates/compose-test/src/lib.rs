//! Testing compose-rs UIs, as Compose's `ComposeTestRule` (docs/09-testing.md): a headless host
//! running frames synchronously, finders through the semantics tree, actions that inject real
//! input events, assertions, dumps, and event scripts.
//!
//! ```ignore
//! let mut ui = compose_test(|| Counter());
//! ui.on_node_with_text("Count: 0").perform_click();
//! ui.on_node_with_text("Count: 1").assert_exists();
//! ```

#![forbid(unsafe_code)]

mod matcher;
mod script;

use compose_ui::{
    Density, FixedTextMeasurer, IntSize, Key, KeyEvent, NodeId, PointerEvent, SemanticsNode,
    TextMeasurer, UiHost,
};

pub use matcher::{
    Matcher, has_click_action, has_role, has_test_tag, has_text, has_text_containing, is_enabled,
    is_focusable, is_focused, is_not_enabled,
};
pub use script::{check_script, run_script};

/// The default test root: a phone-sized window at density 1.
pub const DEFAULT_SIZE: IntSize = IntSize::new(400, 800);

/// A UI under test: owns a headless host, and runs a frame after every action, as Compose's
/// test rule waits for idle.
pub struct ComposeTestRule {
    host: UiHost,
}

/// A test rule composing `content` in the default root, measuring text with the fixed-advance
/// measurer.
pub fn compose_test(content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::with_host(UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height), content)
}

/// A test rule with a root of `size` pixels at `density`, measuring text with `text_measurer`.
pub fn compose_test_with(
    size: IntSize,
    density: Density,
    text_measurer: Box<dyn TextMeasurer>,
    content: impl Fn() + 'static,
) -> ComposeTestRule {
    ComposeTestRule::with_host(UiHost::new(size, density, text_measurer), content)
}

impl ComposeTestRule {
    pub fn with_host(host: UiHost, content: impl Fn() + 'static) -> ComposeTestRule {
        let mut rule = ComposeTestRule { host };
        rule.host.set_content(content);
        rule.wait_for_idle();
        rule
    }

    /// A rule over the fixed-advance measurer at density 1, in a root of `size`.
    pub fn sized(size: IntSize, content: impl Fn() + 'static) -> ComposeTestRule {
        compose_test_with(size, Density::default(), Box::new(FixedTextMeasurer::new()), content)
    }

    pub fn host(&mut self) -> &mut UiHost {
        &mut self.host
    }

    /// Runs frames until nothing is left to do, as `waitForIdle`.
    pub fn wait_for_idle(&mut self) {
        let frames = self.host.settle(100);
        assert!(frames < 100, "compose-test: the UI never became idle (100 frames)");
    }

    // ---- Finders ----------------------------------------------------------------------------

    /// The node matching `matcher` in the merged semantics tree, as `onNode`.
    pub fn on_node(&mut self, matcher: Matcher) -> SemanticsNodeInteraction<'_> {
        SemanticsNodeInteraction { rule: self, matcher, merged: true, index: None }
    }

    /// The same, in the unmerged tree (`useUnmergedTree = true`).
    pub fn on_node_unmerged(&mut self, matcher: Matcher) -> SemanticsNodeInteraction<'_> {
        SemanticsNodeInteraction { rule: self, matcher, merged: false, index: None }
    }

    /// The node whose text is exactly `text`, as `onNodeWithText`.
    pub fn on_node_with_text(&mut self, text: &str) -> SemanticsNodeInteraction<'_> {
        self.on_node(has_text(text))
    }

    /// The node with test tag `tag`, as `onNodeWithTag`.
    pub fn on_node_with_tag(&mut self, tag: &str) -> SemanticsNodeInteraction<'_> {
        self.on_node(has_test_tag(tag))
    }

    /// The `index`th node matching `matcher`, in tree order, as `onAllNodes(…)[index]`.
    pub fn on_all_nodes(&mut self, matcher: Matcher, index: usize) -> SemanticsNodeInteraction<'_> {
        SemanticsNodeInteraction { rule: self, matcher, merged: true, index: Some(index) }
    }

    /// How many nodes match `matcher` in the merged tree.
    pub fn count_nodes(&mut self, matcher: &Matcher) -> usize {
        let tree = self.host.semantics_tree(true);
        tree.flatten().iter().filter(|node| matcher.matches(&node.config)).count()
    }

    // ---- Input ------------------------------------------------------------------------------

    /// Sends a key press and release to the focused node.
    pub fn press_key(&mut self, key: Key) {
        self.host.send_key(KeyEvent::down(key));
        self.host.send_key(KeyEvent::up(key));
        self.wait_for_idle();
    }

    /// Types `text` into the focused node.
    pub fn type_text(&mut self, text: &str) {
        self.host.send_text(text);
        self.wait_for_idle();
    }

    // ---- Dumps ------------------------------------------------------------------------------

    /// The node tree with layout, modifiers and semantics.
    pub fn dump_tree(&self) -> String {
        self.host.dump_tree()
    }

    /// The merged semantics tree.
    pub fn dump_semantics(&self) -> String {
        self.host.semantics_tree(true).dump()
    }

    /// The display list of the last frame.
    pub fn dump_display_list(&self) -> String {
        self.host.display_list().dump()
    }

    /// The slot table.
    pub fn dump_slots(&self) -> String {
        self.host.composition().dump()
    }
}

/// A node found by a matcher, acted on and asserted about, as `SemanticsNodeInteraction`. It
/// finds its node again for every call, in the tree as it is then.
pub struct SemanticsNodeInteraction<'a> {
    rule: &'a mut ComposeTestRule,
    matcher: Matcher,
    merged: bool,
    index: Option<usize>,
}

impl SemanticsNodeInteraction<'_> {
    fn find(&self) -> Result<SemanticsNode, String> {
        let tree = self.rule.host.semantics_tree(self.merged);
        let found: Vec<&SemanticsNode> =
            tree.flatten().into_iter().filter(|node| self.matcher.matches(&node.config)).collect();
        match self.index {
            Some(index) => found.get(index).map(|node| (*node).clone()).ok_or_else(|| {
                format!(
                    "no node #{index} {}: found {} matching nodes in\n{}",
                    self.matcher.description,
                    found.len(),
                    tree.dump()
                )
            }),
            None if found.len() == 1 => Ok(found[0].clone()),
            None => Err(format!(
                "expected exactly 1 node {}, found {} in\n{}",
                self.matcher.description,
                found.len(),
                tree.dump()
            )),
        }
    }

    /// The node, as `fetchSemanticsNode`. Panics unless exactly one matches.
    pub fn fetch_semantics_node(&self) -> SemanticsNode {
        self.find().unwrap_or_else(|message| panic!("compose-test: {message}"))
    }

    pub fn node_id(&self) -> NodeId {
        self.fetch_semantics_node().id
    }

    // ---- Actions ----------------------------------------------------------------------------

    /// Clicks the node's center with a press and a release, then waits for idle, as
    /// `performClick` (a touch input on the node).
    pub fn perform_click(&mut self) -> &mut Self {
        let center = self.fetch_semantics_node().center();
        let host = &mut self.rule.host;
        host.send_pointer(PointerEvent::press(center));
        host.send_pointer(PointerEvent::release(center));
        self.rule.wait_for_idle();
        self
    }

    /// Focuses the node and types `text` into it, as `performTextInput`.
    pub fn perform_text_input(&mut self, text: &str) -> &mut Self {
        let node = self.fetch_semantics_node();
        assert!(
            node.config.editable,
            "compose-test: performTextInput on a node that isn't editable: {:?}",
            node.config
        );
        self.rule.host.request_focus(node.id);
        self.rule.host.send_text(text);
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

    // ---- Assertions -------------------------------------------------------------------------

    #[track_caller]
    pub fn assert_exists(&self) -> &Self {
        if let Err(message) = self.find() {
            panic!("compose-test: assert_exists failed: {message}");
        }
        self
    }

    #[track_caller]
    pub fn assert_does_not_exist(&self) -> &Self {
        let tree = self.rule.host.semantics_tree(self.merged);
        let count = tree.flatten().iter().filter(|node| self.matcher.matches(&node.config)).count();
        assert!(
            count == 0,
            "compose-test: assert_does_not_exist failed: {count} nodes {} in\n{}",
            self.matcher.description,
            tree.dump()
        );
        self
    }

    #[track_caller]
    fn assert_config(
        &self,
        what: &str,
        check: impl Fn(&compose_ui::SemanticsConfiguration) -> bool,
    ) -> &Self {
        let node = self.fetch_semantics_node();
        assert!(
            check(&node.config),
            "compose-test: expected the node {} to be {what}: {:?}",
            self.matcher.description,
            node.config
        );
        self
    }

    #[track_caller]
    pub fn assert_is_enabled(&self) -> &Self {
        self.assert_config("enabled", |config| !config.disabled)
    }

    #[track_caller]
    pub fn assert_is_not_enabled(&self) -> &Self {
        self.assert_config("not enabled", |config| config.disabled)
    }

    #[track_caller]
    pub fn assert_is_on(&self) -> &Self {
        self.assert_config("on", |config| {
            config.toggleable_state == Some(compose_ui::ToggleableState::On)
        })
    }

    #[track_caller]
    pub fn assert_is_off(&self) -> &Self {
        self.assert_config("off", |config| {
            config.toggleable_state == Some(compose_ui::ToggleableState::Off)
        })
    }

    #[track_caller]
    pub fn assert_is_focused(&self) -> &Self {
        self.assert_config("focused", |config| config.focused)
    }

    /// The node's text, merged and in order, is exactly `texts`, as `assertTextEquals`.
    #[track_caller]
    pub fn assert_text_equals(&self, texts: &[&str]) -> &Self {
        let node = self.fetch_semantics_node();
        let actual: Vec<&str> = node.config.all_text().collect();
        assert_eq!(
            actual, texts,
            "compose-test: the text of the node {}",
            self.matcher.description
        );
        self
    }
}
