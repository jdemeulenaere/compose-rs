//! A stale accessibility tree is a bug users meet: what each change must show in the semantics
//! kept per layout node (D1257). Every rule here checks the kept configurations at each tree as
//! well (D1261); the assertions say what a service would read, actions included, which that
//! check compares only by presence.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{FixedTextMeasurer, SemanticsNode, UiHost};
use compose_ui::*;

fn own_text(node: &SemanticsNode) -> Vec<String> {
    node.config
        .get(&SemanticsProperties::TEXT)
        .into_iter()
        .flatten()
        .map(|text| text.as_str().to_string())
        .collect()
}

// ---- State changes and modifier updates ---------------------------------------------------------

/// A text changed by state, seen on its node and on the node merging it.
#[test]
fn a_text_changed_shows_in_its_node_and_the_merging_one() {
    let text = mutable_state_of("one");
    let handle = text.handle();
    let mut ui = compose_test(move || {
        Box!(modifier = Modifier.clickable(|| {}).test_tag("merging"), {
            BasicText!(handle.get());
        });
    });
    ui.on_node_with_text("one").assert_exists();
    assert_eq!(own_text(&ui.on_node_with_tag("merging").fetch_semantics_node()), ["one"]);
    text.set("two");
    ui.on_node_with_text("two").assert_exists();
    ui.on_node_with_text("one").assert_does_not_exist();
    assert_eq!(own_text(&ui.on_node_with_tag("merging").fetch_semantics_node()), ["two"]);
}

/// `Modifier.semantics` with another block, an image's other description, a test tag changed.
#[test]
fn updated_modifiers_show_their_new_values() {
    let generation = mutable_state_of(0);
    let handle = generation.handle();
    let mut ui = compose_test(move || {
        let generation = handle.get();
        Column!({
            Box!(
                modifier = Modifier
                    .size(10.dp())
                    .semantics(move |config| {
                        config.set_content_description(if generation == 0 {
                            "before"
                        } else {
                            "after"
                        });
                    })
                    .test_tag(if generation == 0 { "tag0" } else { "tag1" })
            );
        });
    });
    ui.on_node_with_content_description("before").assert_exists();
    ui.on_node_with_tag("tag0").assert_exists();
    generation.set(1);
    ui.on_node_with_content_description("after").assert_exists();
    ui.on_node_with_content_description("before").assert_does_not_exist();
    ui.on_node_with_tag("tag1").assert_exists();
    ui.on_node_with_tag("tag0").assert_does_not_exist();
}

/// A clickable disabled, and given another callback: the node's enabled state and its action are
/// the new ones.
#[test]
fn a_clickable_updated_offers_its_new_state_and_callback() {
    let generation = mutable_state_of(0);
    let handle = generation.handle();
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let log = clicks.clone();
    let mut ui = compose_test(move || {
        let generation = handle.get();
        let log = log.clone();
        Box!(
            modifier = Modifier
                .size(10.dp())
                .clickable_with(
                    ClickableOptions { enabled: generation < 2, ..ClickableOptions::DEFAULT },
                    move || log.borrow_mut().push(generation)
                )
                .test_tag("button")
        );
    });
    ui.on_node_with_tag("button").perform_semantics_action(&SemanticsActions::ON_CLICK);
    generation.set(1);
    ui.on_node_with_tag("button").perform_semantics_action(&SemanticsActions::ON_CLICK);
    assert_eq!(*clicks.borrow(), [0, 1]);
    ui.on_node_with_tag("button").assert_is_enabled();
    generation.set(2);
    ui.on_node_with_tag("button").assert_is_not_enabled();
}

/// A toggle's action says the opposite of its value now, not of the value kept with an old
/// configuration.
#[test]
fn a_toggle_shows_and_flips_its_value_now() {
    let checked = mutable_state_of(false);
    let handle = checked.handle();
    let mut ui = compose_test(move || {
        Box!(
            modifier = Modifier
                .size(10.dp())
                .toggleable(handle.get(), move |value| handle.set(value))
                .test_tag("toggle")
        );
    });
    ui.on_node_with_tag("toggle").assert_is_off();
    ui.on_node_with_tag("toggle").perform_semantics_action(&SemanticsActions::ON_CLICK);
    ui.on_node_with_tag("toggle").assert_is_on();
    ui.on_node_with_tag("toggle").perform_semantics_action(&SemanticsActions::ON_CLICK);
    ui.on_node_with_tag("toggle").assert_is_off();
    checked.set(true);
    ui.on_node_with_tag("toggle").assert_is_on();
}

/// A modifier added before the semantics modifier moves the layer that gives the node's bounds.
#[test]
fn a_modifier_added_before_moves_the_bounds() {
    let padded = mutable_state_of(false);
    let handle = padded.handle();
    let mut ui = compose_test(move || {
        let outer = if handle.get() { Modifier.padding(5.dp()) } else { Modifier };
        Box!(modifier = Modifier.size(50.dp()).then(outer).test_tag("tag"));
    });
    ui.on_node_with_tag("tag").assert_width_is_equal_to(50.dp());
    padded.set(true);
    ui.on_node_with_tag("tag").assert_width_is_equal_to(40.dp());
    ui.on_node_with_tag("tag").assert_position_in_root_is_equal_to(5.dp(), 5.dp());
    padded.set(false);
    ui.on_node_with_tag("tag").assert_width_is_equal_to(50.dp());
}

// ---- Focus --------------------------------------------------------------------------------------

/// Focused and not, by request, by another node's taking it, by being disabled while focused.
#[test]
fn focus_shows_in_the_semantics_at_each_change() {
    let enabled = mutable_state_of(true);
    let handle = enabled.handle();
    let mut ui = compose_test(move || {
        let on = handle.get();
        Column!({
            Box!(
                modifier = Modifier
                    .size(10.dp())
                    .clickable_with(
                        ClickableOptions { enabled: on, ..ClickableOptions::DEFAULT },
                        || {}
                    )
                    .test_tag("first")
            );
            Box!(modifier = Modifier.size(10.dp()).clickable(|| {}).test_tag("second"));
        });
    });
    ui.on_node_with_tag("first").assert_is_not_focused();
    ui.on_node_with_tag("first").request_focus();
    ui.on_node_with_tag("first").assert_is_focused();
    ui.on_node_with_tag("second").assert_is_not_focused();
    ui.on_node_with_tag("second").request_focus();
    ui.on_node_with_tag("first").assert_is_not_focused();
    ui.on_node_with_tag("second").assert_is_focused();
    ui.on_node_with_tag("first").request_focus();
    ui.on_node_with_tag("first").assert_is_focused();
    enabled.set(false);
    ui.on_node_with_tag("first").assert_is_not_enabled();
    ui.on_node_with_tag("first").assert(key_not_defined(&SemanticsProperties::FOCUSED));
    ui.on_node_with_tag("second").assert_is_not_focused();
    enabled.set(true);
    ui.on_node_with_tag("first").assert_is_not_focused();
}

// ---- Scrolling ----------------------------------------------------------------------------------

fn vertical_range(node: &SemanticsNode) -> ScrollAxisRange {
    *node.unmerged_config().get(&SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE).expect("a range")
}

/// A plain scroll's offset and largest offset (states its semantics read) follow the scroll and
/// the content.
#[test]
fn a_scrolls_range_follows_its_offset_and_content() {
    let scroll = ScrollState::new(0);
    let given = *scroll;
    let height = mutable_state_of(100);
    let handle = height.handle();
    let mut ui = compose_test(move || {
        Column!(modifier = Modifier.size(20.dp()).vertical_scroll(given).test_tag("column"), {
            Box!(modifier = Modifier.size((20.dp(), handle.get().dp())));
        });
    });
    let range = vertical_range(&ui.on_node_with_tag("column").fetch_semantics_node());
    assert_eq!(range.value, 0.0);
    let largest = range.max_value;
    assert!(largest > 0.0);
    scroll.dispatch_raw_delta(30.0);
    let range = vertical_range(&ui.on_node_with_tag("column").fetch_semantics_node());
    assert_eq!(range.value, 30.0);
    height.set(200);
    let range = vertical_range(&ui.on_node_with_tag("column").fetch_semantics_node());
    assert!(range.max_value > largest);
    // Performed through the semantics, the range moves too.
    ui.on_node_with_tag("column").perform_semantics_action_with(
        &SemanticsActions::SCROLL_BY,
        |action| {
            assert!(action(0.0, 10.0));
        },
    );
    let range = vertical_range(&ui.on_node_with_tag("column").fetch_semantics_node());
    assert_eq!(range.value, 40.0);
}

/// A lazy list's range (cells its measurement fills, read in its semantics) follows the scroll
/// and the items.
#[test]
fn a_lazy_lists_range_follows_its_scroll_and_items() {
    let state = LazyListState::new(0, 0);
    let list = *state;
    let count = mutable_state_of(50usize);
    let handle = count.handle();
    let mut ui = compose_test(move || {
        LazyColumn!(
            modifier = Modifier.size((20.dp(), 50.dp())).test_tag("list"),
            state = list,
            |scope: LazyListScope| {
                scope.items(handle.get(), |_| {
                    Box!(modifier = Modifier.size(20.dp()));
                });
            }
        );
    });
    let first = vertical_range(&ui.on_node_with_tag("list").fetch_semantics_node());
    assert_eq!(first.value, 0.0);
    assert_ne!(state.dispatch_raw_delta(30.0), 0.0);
    ui.wait_for_idle();
    let scrolled = vertical_range(&ui.on_node_with_tag("list").fetch_semantics_node());
    assert!(scrolled.value > first.value, "{scrolled:?}");
    state.request_scroll_to_item(20, 0);
    ui.wait_for_idle();
    let far = vertical_range(&ui.on_node_with_tag("list").fetch_semantics_node());
    assert!(far.value > scrolled.value, "{far:?}");
    count.set(100);
    ui.wait_for_idle();
    let more = vertical_range(&ui.on_node_with_tag("list").fetch_semantics_node());
    assert!(more.max_value > far.max_value, "{more:?}");
    // To the end: no room left that way.
    state.request_scroll_to_item(99, 0);
    ui.wait_for_idle();
    let end = vertical_range(&ui.on_node_with_tag("list").fetch_semantics_node());
    assert_eq!(end.value, end.max_value, "{end:?}");
}

// ---- Text fields --------------------------------------------------------------------------------

fn editable_text(node: &SemanticsNode) -> String {
    node.config
        .get(&SemanticsProperties::EDITABLE_TEXT)
        .map_or(String::new(), |text| text.as_str().to_string())
}

/// What a text field shows, whichever way it changes: the value, what the input method is
/// composing, an edit the application refused, the mask, and its enabled state.
#[test]
fn a_text_field_shows_what_it_shows() {
    let text = mutable_state_of(String::from("ab"));
    let accepting = mutable_state_of(true);
    let masked = mutable_state_of(false);
    let enabled = mutable_state_of(true);
    let (text_handle, accepting_handle, masked_handle, enabled_handle) =
        (text.handle(), accepting.handle(), masked.handle(), enabled.handle());
    let mut ui = compose_test(move || {
        let transformation: VisualTransformation = if masked_handle.get() {
            PasswordVisualTransformation::new().into()
        } else {
            VisualTransformation::NONE
        };
        BasicTextField!(
            text_handle.get(),
            move |new: String| {
                if accepting_handle.get() {
                    text_handle.set(new);
                }
            },
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
            enabled = enabled_handle.get(),
            visual_transformation = transformation
        );
    });
    let field = |ui: &mut ComposeTestRule| ui.on_node_with_tag("field").fetch_semantics_node();
    assert_eq!(editable_text(&field(&mut ui)), "ab");
    // The value, from the application.
    text.set(String::from("abc"));
    assert_eq!(editable_text(&field(&mut ui)), "abc");
    // Focused, it says so.
    ui.on_node_with_tag("field").request_focus();
    ui.on_node_with_tag("field").assert_is_focused();
    // Typed (the cursor at its start).
    ui.on_node_with_tag("field").perform_text_input("d");
    ui.wait_for_idle();
    assert_eq!(editable_text(&field(&mut ui)), "dabc");
    // Composing, then cancelled.
    ui.host().send_preedit("xy", Some((2, 2)));
    ui.wait_for_idle();
    assert!(editable_text(&field(&mut ui)).contains("xy"), "{}", editable_text(&field(&mut ui)));
    ui.host().send_preedit("", None);
    ui.wait_for_idle();
    assert_eq!(editable_text(&field(&mut ui)), "dabc");
    // An edit the application doesn't take is undone in the field, and in its semantics.
    accepting.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("field").perform_text_input("e");
    ui.wait_for_idle();
    assert_eq!(editable_text(&field(&mut ui)), "dabc");
    // The mask.
    masked.set(true);
    ui.wait_for_idle();
    let node = field(&mut ui);
    assert!(node.config.contains(&SemanticsProperties::PASSWORD));
    assert!(!editable_text(&node).contains("dabc"), "{}", editable_text(&node));
    masked.set(false);
    ui.wait_for_idle();
    assert!(!field(&mut ui).config.contains(&SemanticsProperties::PASSWORD));
    assert_eq!(editable_text(&field(&mut ui)), "dabc");
    // Disabled: not focused, not editable.
    enabled.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_is_not_enabled();
    assert_eq!(field(&mut ui).config.get(&SemanticsProperties::IS_EDITABLE), Some(&false));
}

// ---- Reuse and deactivation ---------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, testRegenerateSemanticsId
/// Reused, a node's semantics id is another: a service never sees one id for two contents.
#[test]
fn reuse_gives_a_new_semantics_id() {
    let key = mutable_state_of(0);
    let handle = key.handle();
    let mut ui = compose_test(move || {
        ReusableContent(handle.get(), || {
            Box!(modifier = Modifier.size(10.dp()).test_tag("tag"));
        });
    });
    let before = ui.on_node_with_tag("tag").fetch_semantics_node();
    key.set(1);
    let after = ui.on_node_with_tag("tag").fetch_semantics_node();
    assert_eq!(before.id, after.id, "the same layout node, reused");
    assert_ne!(before.semantics_id, after.semantics_id);
    // Nothing else changes it.
    ui.wait_for_idle();
    let again = ui.on_node_with_tag("tag").fetch_semantics_node();
    assert_eq!(after.semantics_id, again.semantics_id);
}

/// Deactivated and reused (a `ReusableContentHost` turned off and on), the id is another too, and
/// two nodes never share one.
#[test]
fn deactivating_and_reusing_gives_a_new_semantics_id() {
    let active = mutable_state_of(true);
    let handle = active.handle();
    let mut ui = compose_test(move || {
        Column!({
            ReusableContentHost(handle.get(), || {
                Box!(modifier = Modifier.size(10.dp()).test_tag("hosted"));
            });
            Box!(modifier = Modifier.size(10.dp()).test_tag("other"));
        });
    });
    let before = ui.on_node_with_tag("hosted").fetch_semantics_node();
    let other = ui.on_node_with_tag("other").fetch_semantics_node();
    assert_ne!(before.semantics_id, other.semantics_id);
    active.set(false);
    ui.on_node_with_tag("hosted").assert_does_not_exist();
    active.set(true);
    let after = ui.on_node_with_tag("hosted").fetch_semantics_node();
    assert_eq!(before.id, after.id);
    assert_ne!(before.semantics_id, after.semantics_id);
    assert_eq!(
        other.semantics_id,
        ui.on_node_with_tag("other").fetch_semantics_node().semantics_id
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt, testSemanticsUpdatedWhenReused
/// Reused with other modifiers' values: the bounds and the properties are the new content's.
#[test]
fn reuse_shows_the_new_contents_semantics() {
    let key = mutable_state_of(true);
    let handle = key.handle();
    let mut ui = compose_test(move || {
        Box!({
            ReusableContent(handle.get(), || {
                let on = handle.get();
                Box!(
                    modifier = Modifier
                        .offset(
                            if on { 0.dp() } else { 20.dp() },
                            if on { 0.dp() } else { 30.dp() }
                        )
                        .size(if on { 50.dp() } else { 100.dp() })
                        .semantics(|config| config.set_content_description("Reusable Box"))
                        .test_tag("semantics_child")
                );
            });
        });
    });
    ui.on_node_with_tag("semantics_child")
        .assert_left_position_in_root_is_equal_to(0.dp())
        .assert_top_position_in_root_is_equal_to(0.dp())
        .assert_width_is_equal_to(50.dp())
        .assert_height_is_equal_to(50.dp());
    ui.on_node_with_content_description("Reusable Box").assert_exists();
    key.set(false);
    ui.on_node_with_tag("semantics_child")
        .assert_left_position_in_root_is_equal_to(20.dp())
        .assert_top_position_in_root_is_equal_to(30.dp())
        .assert_width_is_equal_to(100.dp())
        .assert_height_is_equal_to(100.dp());
    ui.on_node_with_content_description("Reusable Box").assert_exists();
}

/// Scrolling reuses items' nodes: each shows its new text and tag, and its click is the new
/// item's, even where the element compares the same but for its closure.
#[test]
fn a_lazy_item_reused_shows_and_does_what_the_new_item_does() {
    let state = LazyListState::new(0, 0);
    let list = *state;
    let clicked = Rc::new(Cell::new(usize::MAX));
    let log = clicked.clone();
    let mut ui = compose_test(move || {
        let log = log.clone();
        LazyColumn!(
            modifier = Modifier.size((50.dp(), 20.dp())),
            state = list,
            |scope: LazyListScope| {
                let log = log.clone();
                scope.items(100, move |index| {
                    let log = log.clone();
                    Box!(
                        modifier = Modifier
                            .size((50.dp(), 20.dp()))
                            .clickable(move || log.set(index))
                            .test_tag(format!("item{index}")),
                        {
                            BasicText!(format!("Item {index}"));
                        }
                    );
                });
            }
        );
    });
    let first = ui.on_node_with_tag("item0").fetch_semantics_node();
    assert_eq!(own_text(&first), ["Item 0"]);
    let mut seen = vec![first.semantics_id];
    for index in 1..12 {
        state.request_scroll_to_item(index, 0);
        ui.wait_for_idle();
        let tag = format!("item{index}");
        let node = ui.on_node_with_tag(&tag).fetch_semantics_node();
        assert_eq!(own_text(&node), [format!("Item {index}")]);
        ui.on_node_with_text(&format!("Item {}", index - 1)).assert_does_not_exist();
        ui.on_node_with_tag(&tag).perform_semantics_action(&SemanticsActions::ON_CLICK);
        assert_eq!(clicked.get(), index);
        assert!(!seen.contains(&node.semantics_id), "a new item's id is new");
        seen.push(node.semantics_id);
    }
}

/// A state a custom node's semantics read, written while the node is deactivated: reused, the
/// node shows the value now, and writing it afterwards still shows.
#[test]
fn a_state_written_while_deactivated_shows_once_reused() {
    let tag = mutable_state_of("tag1");
    let read = tag.handle();
    let active = mutable_state_of(true);
    let handle = active.handle();
    let mut ui = compose_test(move || {
        ReusableContentHost(handle.get(), move || {
            Box!(modifier = Modifier.size(10.dp()).then_custom(ReadsTag(read)));
        });
    });
    ui.on_node_with_tag("tag1").assert_exists();
    active.set(false);
    ui.on_node_with_tag("tag1").assert_does_not_exist();
    tag.set("tag2");
    ui.wait_for_idle();
    ui.on_node_with_tag("tag2").assert_does_not_exist();
    active.set(true);
    ui.on_node_with_tag("tag2").assert_exists();
    tag.set("tag3");
    ui.on_node_with_tag("tag3").assert_exists();
}

#[derive(Clone, Copy)]
struct ReadsTag(MutableState<&'static str>);

impl ModifierNodeElement for ReadsTag {
    type Node = ReadsTagNode;
    fn create(&self) -> ReadsTagNode {
        ReadsTagNode(self.0)
    }
    fn update(&self, node: &mut ReadsTagNode) {
        node.0 = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

struct ReadsTagNode(MutableState<&'static str>);

impl ModifierNode for ReadsTagNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for ReadsTagNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_test_tag(self.0.get());
    }
}

// ---- What a host sees between frames -----------------------------------------------------------

fn host(content: impl Fn() + 'static) -> UiHost {
    let mut host =
        UiHost::new(IntSize::new(400, 400), Density::new(1.0), Box::new(FixedTextMeasurer::new()));
    host.set_content(content);
    host.settle(10);
    host
}

fn find_by_tag<'a>(node: &'a SemanticsNode, tag: &str) -> Option<&'a SemanticsNode> {
    if node.config.get(&SemanticsProperties::TEST_TAG).is_some_and(|found| found.as_str() == tag) {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_by_tag(child, tag))
}

/// A state only `apply_semantics` read, written with nothing else to do: the host asks for a
/// frame, that frame says the semantics changed, and the tree after it is the new one.
#[test]
fn a_write_only_semantics_read_asks_for_a_frame_and_tells_it() {
    let tag = mutable_state_of("tag1");
    let read = tag.handle();
    let mut host = host(move || {
        Box!(modifier = Modifier.size(10.dp()).then_custom(ReadsTag(read)));
    });
    assert!(find_by_tag(&host.semantics_tree(true), "tag1").is_some());
    assert!(!host.needs_frame());
    tag.set("tag2");
    assert!(host.needs_frame());
    let stats = host.frame();
    assert!(stats.semantics_changed);
    assert!(find_by_tag(&host.semantics_tree(true), "tag2").is_some());
    // Nothing since: nothing to tell.
    assert!(!host.needs_frame());
    assert!(!host.frame().semantics_changed);
}

/// A node's `invalidate_semantics` from outside a frame asks for one, which tells it.
#[test]
fn an_invalidation_from_outside_a_frame_asks_for_one_and_tells_it() {
    let tag = Rc::new(Cell::new("tag1"));
    let handle = Rc::new(Cell::new(None));
    let (read, given) = (tag.clone(), handle.clone());
    let mut host = host(move || {
        let (read, given) = (read.clone(), given.clone());
        Box!(modifier = Modifier.size(10.dp()).then_custom(ReadsCell(read, given)));
    });
    assert!(find_by_tag(&host.semantics_tree(true), "tag1").is_some());
    tag.set("tag2");
    assert!(find_by_tag(&host.semantics_tree(true), "tag1").is_some(), "not said yet");
    let node: NodeHandle = handle.get().expect("attached");
    node.invalidate_semantics();
    assert!(host.needs_frame());
    assert!(host.frame().semantics_changed);
    assert!(find_by_tag(&host.semantics_tree(true), "tag2").is_some());
}

#[derive(Clone)]
struct ReadsCell(Rc<Cell<&'static str>>, Rc<Cell<Option<NodeHandle>>>);

impl ModifierNodeElement for ReadsCell {
    type Node = ReadsCellNode;
    fn create(&self) -> ReadsCellNode {
        ReadsCellNode(self.0.clone(), self.1.clone())
    }
    fn update(&self, _node: &mut ReadsCellNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
}

struct ReadsCellNode(Rc<Cell<&'static str>>, Rc<Cell<Option<NodeHandle>>>);

impl ModifierNode for ReadsCellNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.1.set(Some(context.handle()));
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for ReadsCellNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_test_tag(self.0.get());
    }
}

/// A state only `apply_semantics` read, written by an item a prefetch composes: nothing is drawn
/// or placed, but the host still asks for the frame that says the semantics changed.
#[test]
fn a_write_during_a_prefetch_only_semantics_read_asks_for_a_frame() {
    let tag = mutable_state_of("before");
    let read = tag.handle();
    let state = LazyListState::new(0, 0);
    let list = *state;
    let armed = Rc::new(Cell::new(false));
    let given = armed.clone();
    let mut host = host(move || {
        let given = given.clone();
        Box!(modifier = Modifier.size(10.dp()).then_custom(ReadsTag(read)));
        LazyColumn!(
            modifier = Modifier.size((20.dp(), 50.dp())),
            state = list,
            |scope: LazyListScope| {
                let given = given.clone();
                scope.items(100, move |_| {
                    if given.get() {
                        SideEffect(move || read.set("after"));
                    }
                    Box!(modifier = Modifier.size(20.dp()));
                });
            }
        );
    });
    assert!(find_by_tag(&host.semantics_tree(true), "before").is_some());
    assert_ne!(state.dispatch_raw_delta(30.0), 0.0);
    host.settle(10);
    assert!(host.has_prefetch_work());
    armed.set(true);
    while host.prefetch(std::time::Instant::now() + std::time::Duration::from_secs(1)) {}
    assert_eq!(tag.get(), "after", "the prefetch composed the item");
    assert!(host.needs_frame());
    assert!(host.frame().semantics_changed);
    assert!(find_by_tag(&host.semantics_tree(true), "after").is_some());
}

/// An action reads the same answer the tree would give: none for a node the tree leaves out
/// (under a parent not placed, under a clearing one, itself gone), though its own configuration
/// is kept. Gone, it answers rather than panics: a panic across JNI aborts the app.
#[test]
fn a_node_out_of_the_tree_offers_no_action() {
    let placed = mutable_state_of(true);
    let cleared = mutable_state_of(false);
    let present = mutable_state_of(true);
    let (placed_handle, cleared_handle, present_handle) =
        (placed.handle(), cleared.handle(), present.handle());
    let mut host = host(move || {
        let (placed, cleared) = (placed_handle.get(), cleared_handle.get());
        Layout!(
            modifier = Modifier.size(100.dp()),
            measure_policy = measure_policy(move |scope, measurables, constraints| {
                for &measurable in measurables {
                    let placeable = scope.measure(measurable, constraints.loosen());
                    if placed {
                        scope.place(&placeable, 0, 0);
                    }
                }
                scope.layout(50, 50)
            }),
            {
                let clearing =
                    if cleared { Modifier.clear_and_set_semantics(|_| {}) } else { Modifier };
                Box!(modifier = clearing.size(50.dp()), {
                    if present_handle.get() {
                        Box!(modifier = Modifier.size(10.dp()).clickable(|| {}).test_tag("inner"));
                    }
                });
            }
        );
    });
    let inner = find_by_tag(&host.semantics_tree(false), "inner").expect("in the tree").id;
    assert!(host.semantics_action(inner, &SemanticsActions::ON_CLICK).is_some());
    placed.set(false);
    host.settle(10);
    assert!(find_by_tag(&host.semantics_tree(false), "inner").is_none());
    assert!(host.semantics_action(inner, &SemanticsActions::ON_CLICK).is_none(), "not placed");
    placed.set(true);
    host.settle(10);
    assert!(host.semantics_action(inner, &SemanticsActions::ON_CLICK).is_some());
    cleared.set(true);
    host.settle(10);
    assert!(find_by_tag(&host.semantics_tree(false), "inner").is_none());
    assert!(host.semantics_action(inner, &SemanticsActions::ON_CLICK).is_none(), "cleared");
    cleared.set(false);
    host.settle(10);
    assert!(host.semantics_action(inner, &SemanticsActions::ON_CLICK).is_some());
    present.set(false);
    host.settle(10);
    assert!(find_by_tag(&host.semantics_tree(false), "inner").is_none());
    assert!(host.semantics_action(inner, &SemanticsActions::ON_CLICK).is_none(), "gone");
    assert!(!host.perform_semantics_action(inner, &SemanticsActions::ON_CLICK), "gone");
}

/// A focus target whose change draws nothing but says it in its semantics: the frame telling
/// that is asked for, as D1263 says a frame tells a semantics change (D1266).
#[test]
fn a_focus_change_drawing_nothing_asks_for_a_frame() {
    let mut host = host(|| {
        Box!(modifier = Modifier.size(10.dp()).then_custom(FocusWithoutDrawing).test_tag("target"));
    });
    let wakes = Rc::new(Cell::new(0));
    let counter = wakes.clone();
    host.set_frame_waker(move || counter.set(counter.get() + 1));
    let tree = host.semantics_tree(true);
    let target = find_by_tag(&tree, "target").expect("the target");
    assert_eq!(target.config.get(&SemanticsProperties::FOCUSED), Some(&false));
    let target = target.id;
    assert!(!host.needs_frame());
    host.request_focus(target);
    assert!(host.needs_frame(), "a frame tells it");
    assert!(wakes.get() > 0, "the platform is asked for that frame");
    assert!(host.frame().semantics_changed);
    let tree = host.semantics_tree(true);
    let target = find_by_tag(&tree, "target").expect("the target");
    assert_eq!(target.config.get(&SemanticsProperties::FOCUSED), Some(&true));
    assert!(!host.needs_frame());
}

#[derive(Clone, Copy)]
struct FocusWithoutDrawing;

impl ModifierNodeElement for FocusWithoutDrawing {
    type Node = FocusWithoutDrawingNode;
    fn create(&self) -> FocusWithoutDrawingNode {
        FocusWithoutDrawingNode(false)
    }
    fn update(&self, _node: &mut FocusWithoutDrawingNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
}

struct FocusWithoutDrawingNode(bool);

impl ModifierNode for FocusWithoutDrawingNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for FocusWithoutDrawingNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_focused(self.0);
    }
}

impl FocusTargetModifierNode for FocusWithoutDrawingNode {
    fn on_focus_changed(&mut self, focused: bool, _context: &mut EventContext<'_>) {
        self.0 = focused;
    }
}

/// A node whose `apply_semantics` invalidates itself behaves the same under a rule's check as on
/// a host: an invalidation asked while computing doesn't count, so the frame after a read has no
/// change to tell (D1261).
#[test]
fn the_rules_check_doesnt_count_invalidations_while_computing() {
    let slot: Rc<Cell<Option<NodeHandle>>> = Rc::default();
    let inside = slot.clone();
    let mut ui = compose_test(move || {
        Box!(modifier = Modifier.size(10.dp()).then_custom(InvalidatesItself(inside.clone())));
    });
    ui.on_node_with_tag("itself").assert_exists();
    assert!(slot.get().is_some());
    // The check runs at this read: the kept configuration is compared with one computed again.
    ui.host().semantics_tree(true);
    assert!(!ui.host().frame().semantics_changed, "the check's computing counted");
}

#[derive(Clone)]
struct InvalidatesItself(Rc<Cell<Option<NodeHandle>>>);

impl ModifierNodeElement for InvalidatesItself {
    type Node = InvalidatesItselfNode;
    fn create(&self) -> InvalidatesItselfNode {
        InvalidatesItselfNode(self.0.clone())
    }
    fn update(&self, node: &mut InvalidatesItselfNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

struct InvalidatesItselfNode(Rc<Cell<Option<NodeHandle>>>);

impl ModifierNode for InvalidatesItselfNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.set(Some(context.handle()));
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for InvalidatesItselfNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_test_tag("itself");
        if let Some(handle) = self.0.get() {
            handle.invalidate_semantics();
        }
    }
}

/// A deactivated node's semantics read (`assert_is_deactivated`'s tree computes them), what they
/// read changed, then the node reused with its element unchanged: attaching forgets what the read
/// kept, as no update does.
#[test]
fn attaching_forgets_what_a_read_of_a_deactivated_node_kept() {
    let tag = Rc::new(Cell::new("tag1"));
    let handle = Rc::new(Cell::new(None));
    let active = mutable_state_of(true);
    let active_handle = active.handle();
    let (read, given) = (tag.clone(), handle.clone());
    let mut ui = compose_test(move || {
        let (read, given) = (read.clone(), given.clone());
        ReusableContentHost(active_handle.get(), move || {
            Box!(
                modifier =
                    Modifier.size(10.dp()).then_custom(ReadsCell(read.clone(), given.clone()))
            );
        });
    });
    ui.on_node_with_tag("tag1").assert_exists();
    active.set(false);
    ui.on_node_with_tag("tag1").assert_is_deactivated();
    tag.set("tag2");
    active.set(true);
    ui.on_node_with_tag("tag2").assert_exists();
    ui.on_node_with_tag("tag1").assert_does_not_exist();
}
