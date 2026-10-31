//! What a service does with a text (D1651): moving through it, selecting in a field, setting a
//! field's text, its clipboard and its action key.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use compose_android::accessibility::{
    ChangeKind, KeptCustomActions, KeptForChanges, TextCursor, action, actions_of, find,
    movement_granularities, perform_text_action, text_selection_of, virtual_id,
};
use compose_android::text_segments::movement;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{MutableState, mutable_state_of};
use compose_ui::platform::{SemanticsSnapshot, UiHost};
use compose_ui::*;

fn host_of(content: impl Fn() + 'static) -> UiHost {
    let mut host = UiHost::headless(400, 800);
    host.set_content(content);
    host.settle(20);
    host
}

fn by_tag<'a>(node: &'a SemanticsSnapshot, tag: &str) -> Option<&'a SemanticsSnapshot> {
    if node.config.get(&SemanticsProperties::TEST_TAG).is_some_and(|it| it.as_str() == tag) {
        return Some(node);
    }
    node.children.iter().find_map(|child| by_tag(child, tag))
}

/// A scene with a node tagged `node`, and what a bridge keeps of it.
struct Scene {
    host: UiHost,
    kept: KeptForChanges,
    id: i32,
}

impl Scene {
    fn new(content: impl Fn() + 'static) -> Scene {
        let host = host_of(content);
        let root = host.accessibility_tree();
        let id = virtual_id(by_tag(&root, "node").expect("tagged"));
        let mut kept = KeptForChanges::default();
        kept.changes(&host, &root);
        Scene { host, kept, id }
    }

    fn node(&self) -> SemanticsSnapshot {
        let root = self.host.accessibility_tree();
        find(&root, self.id).expect("there").clone()
    }

    fn perform(&mut self, code: u8, first: i32, second: i32, extend: bool, text: &str) -> bool {
        let root = self.host.accessibility_tree();
        let cursor = &mut self.kept.cursor;
        let done = perform_text_action(
            &mut self.host,
            &root,
            cursor,
            self.id,
            i32::from(code),
            first,
            second,
            extend,
            text,
        );
        self.host.settle(5);
        done
    }

    fn next(&mut self, granularity: i32) -> bool {
        self.perform(action::NEXT_AT_GRANULARITY, granularity, 0, false, "")
    }

    fn previous(&mut self, granularity: i32) -> bool {
        self.perform(action::PREVIOUS_AT_GRANULARITY, granularity, 0, false, "")
    }

    /// What a service hears now, without what's only about things changing around.
    fn told(&mut self) -> Vec<(ChangeKind, i32, i32, i32, String, bool)> {
        let root = self.host.accessibility_tree();
        self.kept
            .changes(&self.host, &root)
            .into_iter()
            .filter(|change| {
                matches!(
                    change.kind,
                    ChangeKind::TextSelection
                        | ChangeKind::TextChanged
                        | ChangeKind::TextMovedForward
                        | ChangeKind::TextMovedBackward
                )
            })
            .map(|it| (it.kind, it.from, it.removed, it.added, it.text, it.password))
            .collect()
    }

    fn selection(&self) -> (i32, i32) {
        text_selection_of(&self.node(), &self.kept.cursor)
    }
}

fn text_scene(text: &'static str) -> Scene {
    Scene::new(move || BasicText!(text, modifier = Modifier.test_tag("node")))
}

fn field_scene(value: MutableState<String>, password: bool) -> Scene {
    Scene::new(move || {
        BasicTextField!(
            value.get(),
            move |new| value.set(new),
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("node"),
            visual_transformation = if password {
                PasswordVisualTransformation::new().into()
            } else {
                VisualTransformation::NONE
            },
        );
    })
}

fn state(text: &str) -> MutableState<String> {
    Box::leak(Box::new(mutable_state_of(text.to_string()))).handle()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, selectionEventBeforeTraverseEvent_whenTraverseText
#[test]
fn moving_through_a_text_says_the_selection_then_the_move() {
    let mut scene = text_scene("h");
    assert!(scene.next(movement::CHARACTER));
    assert_eq!(
        scene.told(),
        [
            (ChangeKind::TextSelection, 1, 1, 1, "h".to_string(), false),
            (ChangeKind::TextMovedForward, 0, 1, movement::CHARACTER, "h".to_string(), false),
        ]
    );
    assert_eq!(scene.told(), []);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, selectionEventBeforeTraverseEvent_whenTraverseTextField
#[test]
fn moving_through_a_field_says_its_selection_then_the_move_and_never_the_letter() {
    let mut scene = field_scene(state("h"), true);
    scene.perform(action::SET_SELECTION, 0, 0, false, "");
    scene.told();
    assert!(scene.next(movement::CHARACTER));
    assert_eq!(
        scene.told(),
        [
            (ChangeKind::TextSelection, 1, 1, 1, "\u{2022}".to_string(), true),
            (ChangeKind::TextMovedForward, 0, 1, movement::CHARACTER, "\u{2022}".to_string(), true),
        ]
    );
}

#[test]
fn a_service_goes_on_from_where_it_is_and_back_and_stops_at_the_ends() {
    let mut scene = text_scene("ab cd");
    assert_eq!(scene.selection(), (-1, -1), "it isn't anywhere yet");
    assert!(scene.next(movement::WORD));
    assert_eq!(scene.selection(), (2, 2));
    assert!(scene.next(movement::WORD));
    assert_eq!(scene.selection(), (5, 5));
    assert!(!scene.next(movement::WORD), "the end");
    assert!(scene.previous(movement::WORD));
    assert_eq!(scene.selection(), (3, 3));
    assert!(scene.previous(movement::WORD));
    assert_eq!(scene.selection(), (0, 0));
    assert!(!scene.previous(movement::WORD), "the start");
}

#[test]
fn going_back_from_nowhere_starts_at_the_end() {
    let mut scene = text_scene("ab cd");
    assert!(scene.previous(movement::CHARACTER));
    assert_eq!(scene.selection(), (4, 4));
}

#[test]
fn a_cursor_stays_with_its_node() {
    let mut host = host_of(|| {
        Column!({
            BasicText!("one", modifier = Modifier.test_tag("one"));
            BasicText!("two", modifier = Modifier.test_tag("two"));
        });
    });
    let root = host.accessibility_tree();
    let (one, two) = (
        by_tag(&root, "one").expect("there").clone(),
        by_tag(&root, "two").expect("there").clone(),
    );
    let mut cursor = TextCursor::default();
    let next = |host: &mut UiHost, cursor: &mut TextCursor, id: i32| {
        perform_text_action(
            host,
            &root,
            cursor,
            id,
            i32::from(action::NEXT_AT_GRANULARITY),
            movement::CHARACTER,
            0,
            false,
            "",
        )
    };
    assert!(next(&mut host, &mut cursor, virtual_id(&one)));
    assert_eq!(text_selection_of(&one, &cursor), (1, 1));
    assert_eq!(text_selection_of(&two, &cursor), (-1, -1));
    assert!(next(&mut host, &mut cursor, virtual_id(&two)));
    assert_eq!(text_selection_of(&two, &cursor), (1, 1));
    assert_eq!(text_selection_of(&one, &cursor), (-1, -1), "it left");
}

#[test]
fn a_field_is_moved_through_by_its_own_selection_and_extended_from_it() {
    let value = state("ab cd");
    let mut scene = field_scene(value, false);
    assert_eq!(scene.selection(), (0, 0));
    assert!(scene.perform(action::NEXT_AT_GRANULARITY, movement::WORD, 0, true, ""));
    assert_eq!(scene.selection(), (0, 2), "selected from where it was");
    assert!(scene.perform(action::NEXT_AT_GRANULARITY, movement::WORD, 0, false, ""));
    assert_eq!(scene.selection(), (5, 5), "moved");
    assert_eq!(value.get(), "ab cd");
}

#[test]
fn selections_are_counted_in_what_android_counts_in() {
    // A flag is four units, and eight bytes.
    let mut scene = field_scene(state("\u{1f1eb}\u{1f1f7}x"), false);
    assert!(scene.perform(action::SET_SELECTION, 4, 5, false, ""));
    assert_eq!(scene.selection(), (4, 5));
    assert!(scene.perform(action::SET_SELECTION, 0, 0, false, ""));
    assert!(scene.next(movement::CHARACTER));
    assert_eq!(scene.selection(), (4, 4));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testTextField_performSetSelectionAction_succeedOnEnabledNode
#[test]
fn a_selection_is_set_in_the_field_and_said() {
    let mut scene = field_scene(state("hello"), false);
    assert!(scene.perform(action::SET_SELECTION, 1, 3, false, ""));
    assert_eq!(scene.selection(), (1, 3));
    assert_eq!(scene.told(), [(ChangeKind::TextSelection, 1, 3, 5, "hello".to_string(), false)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, sendTextEvents_whenSetText
#[test]
fn setting_a_text_changes_it_and_says_so() {
    let value = state("a");
    let mut scene = field_scene(value, false);
    assert!(scene.perform(action::SET_TEXT, 0, 0, false, "abc"));
    assert_eq!(value.get(), "abc");
    assert_eq!(scene.selection(), (3, 3));
    let told = scene.told();
    assert_eq!(told[0], (ChangeKind::TextChanged, 1, 0, 2, "abc".to_string(), false));
    assert_eq!(told[1], (ChangeKind::TextSelection, 3, 3, 3, "abc".to_string(), false));
}

#[test]
fn a_description_is_read_through_instead_of_the_text_and_has_no_lines() {
    let mut scene = Scene::new(|| {
        BasicText!(
            "long text",
            modifier = Modifier
                .test_tag("node")
                .semantics(|config| config.set_content_description("Short"))
        );
    });
    let all = movement::CHARACTER | movement::WORD | movement::PARAGRAPH;
    assert_eq!(movement_granularities(&scene.node()), all);
    assert!(scene.next(movement::WORD));
    assert_eq!(scene.selection(), (5, 5), "in the description");
}

#[test]
fn a_text_is_moved_through_by_lines_and_a_field_only_with_the_focus() {
    let scene = text_scene("words");
    let lines = movement::LINE | movement::PAGE;
    assert_eq!(movement_granularities(&scene.node()) & lines, lines);

    let mut scene = field_scene(state("words"), false);
    assert_eq!(movement_granularities(&scene.node()) & lines, 0);
    let node = scene.node();
    scene.host.request_focus(node.id);
    scene.host.settle(5);
    assert_eq!(movement_granularities(&scene.node()) & lines, lines);
    assert!(scene.next(movement::LINE));
}

#[test]
fn nothing_to_read_is_nothing_to_move_through() {
    let scene = Scene::new(|| {
        Box!(modifier = Modifier.size(10.dp()).test_tag("node").semantics(|_| {}));
    });
    assert_eq!(movement_granularities(&scene.node()), 0);
    let codes = |node: &SemanticsSnapshot| -> Vec<u8> {
        actions_of(node, &KeptCustomActions::default()).iter().map(|(code, _)| *code).collect()
    };
    assert!(!codes(&scene.node()).contains(&action::NEXT_AT_GRANULARITY));
}

#[test]
fn a_disabled_field_is_read_through_and_copied_from_but_not_changed() {
    let value = state("hello");
    let mut scene = Scene::new(move || {
        BasicTextField!(
            value.get(),
            move |new| value.set(new),
            modifier = Modifier.test_tag("node"),
            enabled = false,
        );
    });
    let codes: Vec<u8> = actions_of(&scene.node(), &KeptCustomActions::default())
        .iter()
        .map(|(code, _)| *code)
        .collect();
    assert!(codes.contains(&action::NEXT_AT_GRANULARITY));
    assert!(codes.contains(&action::SET_SELECTION));
    assert!(!codes.contains(&action::SET_TEXT));
    assert!(!codes.contains(&action::CLICK));
    assert!(!codes.contains(&action::IME_ENTER));
    // It has no selection of its own to set then: the service's cursor moves.
    assert!(scene.perform(action::SET_SELECTION, 2, 2, false, ""));
    assert_eq!(scene.selection(), (2, 2));
    assert_eq!(value.get(), "hello");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt, test_PasteAction_ifFocused, test_noPasteAction_ifUnfocused
#[test]
fn pasting_is_offered_only_with_the_focus() {
    let mut scene = field_scene(state("hello"), false);
    let offered = |scene: &Scene, code: u8| {
        actions_of(&scene.node(), &KeptCustomActions::default()).iter().any(|(it, _)| *it == code)
    };
    assert!(!offered(&scene, action::PASTE));
    assert!(offered(&scene, action::CLICK), "a service's double tap has something to do");
    assert!(offered(&scene, action::SET_TEXT));
    assert!(offered(&scene, action::IME_ENTER));
    let node = scene.node();
    scene.host.request_focus(node.id);
    scene.host.settle(5);
    assert!(offered(&scene, action::PASTE));

    assert!(!offered(&scene, action::COPY), "nothing's selected");
    scene.perform(action::SET_SELECTION, 0, 2, false, "");
    assert!(offered(&scene, action::COPY));
    assert!(offered(&scene, action::CUT));
    assert!(scene.perform(action::CUT, 0, 0, false, ""));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt, testTextField_performSetSelectionAction_succeedOnEnabledNode
#[test]
fn a_selection_past_the_text_is_refused_and_leaves_nothing_to_cut() {
    let value = state("hello");
    let mut scene = field_scene(value, false);
    assert!(!scene.perform(action::SET_SELECTION, 100, 100, false, ""));
    assert_eq!(scene.selection(), (0, 0));
    assert!(!scene.perform(action::SET_SELECTION, 0, 6, false, ""));
    assert!(scene.perform(action::SET_SELECTION, 0, 5, false, ""));
    assert!(scene.perform(action::CUT, 0, 0, false, ""));
    assert_eq!(value.get(), "");
}

/// `12-34` for `1234`.
struct Grouped;

impl VisualTransform for Grouped {
    fn filter(&self, text: &str) -> TransformedText {
        struct Mapping;
        impl OffsetMapping for Mapping {
            fn original_to_transformed(&self, offset: usize) -> usize {
                if offset <= 2 { offset } else { offset + 1 }
            }
            fn transformed_to_original(&self, offset: usize) -> usize {
                if offset <= 2 {
                    offset
                } else if offset == 3 {
                    2
                } else {
                    offset - 1
                }
            }
        }
        TransformedText {
            text: format!("{}-{}", &text[..2.min(text.len())], &text[2.min(text.len())..]),
            offset_mapping: Rc::new(Mapping),
        }
    }
}

#[test]
fn a_service_gets_past_what_a_transformation_adds() {
    let mut scene = Scene::new(|| {
        BasicTextField!(
            String::from("1234"),
            |_| {},
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("node"),
            visual_transformation = VisualTransformation::new(Grouped),
        );
    });
    let mut places = Vec::new();
    while scene.next(movement::CHARACTER) && places.len() < 10 {
        places.push(scene.selection().1);
    }
    assert_eq!(places, [1, 2, 4, 5]);
    let mut places = Vec::new();
    while scene.previous(movement::CHARACTER) && places.len() < 10 {
        places.push(scene.selection().1);
    }
    assert_eq!(places, [4, 2, 1, 0]);
}

#[test]
fn a_moves_telling_lasts_a_second() {
    let mut scene = field_scene(state("ab"), false);
    assert_eq!(scene.selection(), (0, 0));
    // Told after its selection changed, in the next `told`.
    assert!(scene.next(movement::CHARACTER));
    scene.kept.cursor.move_asked_ago(std::time::Duration::from_secs(2));
    assert_eq!(scene.told(), [(ChangeKind::TextSelection, 1, 1, 2, "ab".to_string(), false)]);
    // Younger than that, it is told.
    assert!(scene.next(movement::CHARACTER));
    scene.kept.cursor.move_asked_ago(std::time::Duration::from_millis(100));
    assert_eq!(scene.told().len(), 2);
}

#[test]
fn a_plain_texts_selection_is_not_extended() {
    let mut scene = text_scene("ab cd");
    assert!(scene.next(movement::WORD));
    assert!(scene.perform(action::NEXT_AT_GRANULARITY, movement::WORD, 0, true, ""));
    assert_eq!(scene.selection(), (5, 5));
}

#[test]
fn a_disabled_field_is_moved_through_by_the_services_place() {
    let mut scene = Scene::new(|| {
        BasicTextField!(
            String::from("hello"),
            |_| {},
            modifier = Modifier.test_tag("node"),
            enabled = false,
        );
    });
    assert!(scene.next(movement::CHARACTER));
    assert_eq!(scene.selection(), (1, 1));
    assert!(scene.next(movement::CHARACTER));
    assert_eq!(scene.selection(), (2, 2));
}

#[test]
fn a_page_of_a_field_in_a_scroller_is_what_is_shown_of_it() {
    let text: String = (0..500).map(|line| format!("Line: {line}")).collect::<Vec<_>>().join("\n");
    let value = state(&text);
    let mut scene = Scene::new(move || {
        let scroll = remember_scroll_state!();
        Column!(modifier = Modifier.fill_max_size().vertical_scroll(scroll), {
            BasicTextField!(
                value.get(),
                move |new| value.set(new),
                modifier = Modifier.test_tag("node"),
            );
        });
    });
    let node = scene.node();
    scene.host.request_focus(node.id);
    scene.host.settle(5);
    assert!(scene.next(movement::PAGE));
    let end = scene.selection().1 as usize;
    let mut layouts = Vec::new();
    scene.host.perform_semantics_action_with(
        node.id,
        &SemanticsActions::GET_TEXT_LAYOUT_RESULT,
        |get| get(&mut layouts),
    );
    let layout = &layouts[0];
    // Up to the window's 800, not the text's whole height.
    let last = layout.get_line_for_offset(end - 1);
    let line_height = layout.get_line_bottom(last) - layout.get_line_top(last);
    assert!((layout.get_line_bottom(last) - 800.0).abs() < 2.0 * line_height);
}

#[test]
fn a_long_text_is_moved_through_at_once() {
    let text: String = "word ".repeat(40_000);
    let segments = compose_android::text_segments::TextSegments::new(&text);
    let start = std::time::Instant::now();
    use compose_android::text_segments::Granularity;
    assert_eq!(segments.following(Granularity::Character, 100_000), Some((100_000, 100_001)));
    assert_eq!(segments.following(Granularity::Word, 100_000), Some((100_000, 100_004)));
    // Quadratic, this took half a minute unoptimized.
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "{:?}", start.elapsed());
}

#[test]
fn a_read_only_field_is_a_field_that_can_not_be_edited() {
    use compose_android::accessibility::flag;
    let scene = Scene::new(|| {
        BasicTextField!(
            String::from("hello"),
            |_| {},
            modifier = Modifier.test_tag("node"),
            read_only = true,
        );
    });
    let node = scene.node();
    assert_eq!(node.config.get(&SemanticsProperties::IS_EDITABLE), Some(&false));
    assert!(!record_has_flag(&scene.host, flag::CAN_EDIT));
}

/// Whether the one tagged record has `bit`: what's written, read back from the front.
fn record_has_flag(host: &UiHost, bit: u32) -> bool {
    let root = host.accessibility_tree();
    let mut out = compose_android::serialize::Writer::default();
    compose_android::accessibility::write_tree(&root, &mut KeptCustomActions::default(), &mut out);
    // Id, parent and four bounds, then the flags.
    let flags = u32::from_le_bytes(out.bytes[24..28].try_into().unwrap());
    flags & bit != 0
}

#[test]
fn a_selection_on_a_plain_text_is_in_that_text() {
    // A text with a selection of its own, in bytes; a flag is eight of them, four units.
    let scene = Scene::new(|| {
        BasicText!(
            "\u{1f1eb}\u{1f1f7}x",
            modifier = Modifier
                .test_tag("node")
                .semantics(|config| config.set_text_selection_range(TextRange::new(8, 9)))
        );
    });
    assert_eq!(scene.selection(), (4, 5));
}

#[test]
fn what_is_sent_is_cut_to_what_is_safe_a_pair_whole() {
    use compose_android::accessibility::parcel_safe;
    assert_eq!(parcel_safe("short"), "short");
    let long = "x".repeat(100_001);
    assert_eq!(parcel_safe(&long).len(), 100_000);
    // A pair across the limit stays out.
    let pair = format!("{}\u{1f600}x", "y".repeat(99_999));
    assert_eq!(parcel_safe(&pair).len(), 99_999);
    let fits = format!("{}\u{1f600}", "y".repeat(99_998));
    assert_eq!(parcel_safe(&fits), fits);
}

#[test]
fn a_description_read_through_extends_nothing() {
    let value = state("hello world");
    let mut scene = Scene::new(move || {
        BasicTextField!(
            value.get(),
            move |new| value.set(new),
            modifier = Modifier
                .size((200.dp(), 40.dp()))
                .test_tag("node")
                .semantics(|config| config.set_content_description("Long description")),
        );
    });
    assert!(scene.perform(action::NEXT_AT_GRANULARITY, movement::WORD, 0, true, ""));
    let node = scene.node();
    let range = node.config.get(&SemanticsProperties::TEXT_SELECTION_RANGE).copied();
    assert_eq!(range, Some(TextRange::new(4, 4)), "collapsed, at the word's end");
}
