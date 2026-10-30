//! The window's view and the content's focus, and the keys that move it where the host says so.

use super::support::*;

fn press(scene: Scene, key: Key) -> bool {
    scene.idle();
    let consumed = scene.with_host(|host| host.send_key(KeyEvent::down(key)));
    scene.idle();
    consumed
}

fn on_a_phone(content: fn(Scene)) -> Composed {
    let scene = scene(content);
    scene.with_host(|host| host.set_keys_navigate(true));
    scene
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/KeyEventToFocusDirectionTest.kt, left, right, up, down, page_up, page_down, tab_next, shiftTab_previous, dpadCenter_enter, enter_enter, numPadEnter_enter, back_exit, esc_exit
#[test]
fn a_key_says_the_direction_it_moves_the_focus_in() {
    for (key, shift, direction) in [
        (Key::DirectionLeft, false, Left),
        (Key::DirectionRight, false, Right),
        (Key::DirectionUp, false, Up),
        (Key::DirectionDown, false, Down),
        (Key::PageUp, false, Up),
        (Key::PageDown, false, Down),
        (Key::Tab, false, Next),
        (Key::Tab, true, Previous),
        (Key::NavigateNext, false, Next),
        (Key::NavigatePrevious, false, Previous),
        (Key::DirectionCenter, false, Enter),
        (Key::Enter, false, Enter),
        (Key::NumPadEnter, false, Enter),
        (Key::Back, false, Exit),
        (Key::Escape, false, Exit),
    ] {
        let mut event = KeyEvent::down(key);
        event.modifiers.shift = shift;
        assert_eq!(event.to_focus_direction(), Some(direction), "{key:?}");
    }
    assert_eq!(KeyEvent::down(Key::Space).to_focus_direction(), None);
}

/// Three in a column over a child of the middle one.
fn column_with_a_child(scene: Scene) {
    Column!({
        Leaf(scene.target("top").size(10.dp()));
        Stack!(modifier = scene.target("middle").size(10.dp()), {
            Leaf(scene.target("child").size(10.dp()));
        });
        Leaf(scene.target("bottom").size(10.dp()));
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ComposeViewKeyEventInteropTest.kt, composeView_consumesKeyEvent_ifFocusIsMoved, composeView_doesNotConsumesKeyEvent_ifFocusIsNotMoved, composeView_consumesBackKeyEvent_ifFocusMovesToNonRoot, composeView_doesNotConsumeBackKeyEvent_ifFocusMovesToRoot, composeView_doesNotConsumesKeyEvent_ifTheContentIsNotFocusable
#[test]
fn on_a_phone_a_key_nothing_takes_moves_the_focus_and_is_consumed_if_it_did() {
    // (from, the key, whether it's consumed, where it ends)
    for (from, key, consumed, to) in [
        ("top", Key::DirectionDown, true, "middle"),
        ("middle", Key::DirectionDown, true, "bottom"),
        ("middle", Key::DirectionUp, true, "top"),
        ("top", Key::PageDown, true, "middle"),
        ("bottom", Key::PageUp, true, "middle"),
        // Nowhere to go.
        ("top", Key::DirectionUp, false, "top"),
        ("top", Key::DirectionLeft, false, "top"),
        ("top", Key::Back, false, "top"),
        // Into a child and out of it.
        ("middle", Key::Enter, true, "child"),
        ("middle", Key::DirectionCenter, true, "child"),
        ("middle", Key::NumPadEnter, true, "child"),
        ("child", Key::Back, true, "middle"),
        ("child", Key::Escape, true, "middle"),
        ("top", Key::NavigateNext, true, "middle"),
        ("middle", Key::NavigatePrevious, true, "top"),
    ] {
        let scene = on_a_phone(column_with_a_child);
        assert!(scene.request(from));
        assert_eq!(press(*scene, key), consumed, "{key:?} from {from}");
        assert_eq!(scene.focused(), [to], "{key:?} from {from}");
    }

    let scene = on_a_phone(|_| Leaf(Modifier.size(10.dp())));
    assert!(!press(*scene, Key::DirectionDown));
}

/// Elsewhere only Tab does (D230).
#[test]
fn without_a_phones_keys_only_tab_moves_the_focus() {
    let scene = scene(column_with_a_child);
    assert!(scene.request("top"));
    assert!(!press(*scene, Key::DirectionDown));
    assert_eq!(scene.focused(), ["top"]);
    assert!(press(*scene, Key::Tab));
    assert_eq!(scene.focused(), ["middle"]);
}

#[test]
fn a_node_taking_a_key_keeps_it_from_moving_the_focus() {
    let scene = on_a_phone(|scene| {
        Column!({
            let takes = Modifier.on_key_event(move |_: KeyEvent| true);
            Leaf(takes.then(scene.target("top")).size(10.dp()));
            Leaf(scene.target("bottom").size(10.dp()));
        });
    });
    assert!(scene.request("top"));
    assert!(press(*scene, Key::DirectionDown));
    assert_eq!(scene.focused(), ["top"]);
}

fn take(scene: Scene, direction: FocusDirection) -> bool {
    scene.idle();
    let taken = scene.with_host(|host| host.owner_requested_focus(direction, None));
    scene.idle();
    taken
}

fn asked_of_the_view(scene: Scene) -> Option<bool> {
    scene.idle();
    scene.with_host(|host| host.take_owner_focus_request())
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OwnerFocusTest.kt, requestFocus_bringsViewInFocus
#[test]
fn a_request_in_the_content_asks_the_view_for_the_focus_and_a_clear_gives_it_up() {
    let scene = scene(column_with_a_child);
    assert_eq!(asked_of_the_view(*scene), None);
    assert!(scene.request("top"));
    assert_eq!(asked_of_the_view(*scene), Some(true));
    assert!(scene.request("bottom"));
    assert_eq!(asked_of_the_view(*scene), None, "it has it already");
    scene.clear(false);
    assert_eq!(asked_of_the_view(*scene), Some(false));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OwnerFocusTest.kt, whenOwnerGainsFocus_focusModifiersAreUpdated, viewDoesNotTakeFocus_whenThereAreNoFocusableItems, callingRequestFocusDownWhenOwnerAlreadyHasFocus, callingRequestFocusUpWhenOwnerAlreadyHasFocus
#[test]
fn the_view_asking_gives_the_focus_to_the_first_target_found_or_changes_nothing() {
    // Nothing focused: a search, from its end.
    for (direction, expected) in [(Down, "top"), (Next, "top"), (Up, "bottom")] {
        let scene = scene(column_with_a_child);
        assert!(take(*scene, direction));
        assert_eq!(scene.focused(), [expected], "{direction:?}");
    }

    // Something focused: nothing changes, whatever the direction.
    for direction in [Down, Up] {
        let scene = scene(column_with_a_child);
        assert!(scene.request("middle"));
        assert!(take(*scene, direction));
        assert_eq!(scene.focused(), ["middle"], "{direction:?}");
    }

    // Nothing to take it: it says so, and the view doesn't have it.
    let scene = self::scene(|_| Leaf(Modifier.size(10.dp())));
    assert!(!take(*scene, Enter));
    assert!(!scene.root_has_focus());
}

/// A rectangle the focus comes from is where the search starts.
#[test]
fn the_view_asking_from_a_rectangle_finds_the_target_nearest_it() {
    let scene = scene(|scene| {
        Column!({
            Leaf(scene.target("left").size(10.dp()));
            Leaf(Modifier.offset(100.dp(), 0.dp()).size(10.dp()).then(scene.target("right")));
        });
    });
    let from_the_right = Rect::new(100.0, -20.0, 110.0, -10.0);
    assert!(scene.with_host(|host| host.owner_requested_focus(Down, Some(from_the_right))));
    assert_eq!(scene.focused(), ["right"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OwnerFocusTest.kt, whenOwnerLosesFocus_focusModifiersAreUpdated
#[test]
fn the_view_losing_the_focus_takes_it_from_the_draw_captured_and_with_exits_cancelling() {
    let scene = scene(|scene| {
        let cancels = properties(|p| p.set_on_exit(|scope| scope.cancel_focus_change()));
        Stack!(modifier = cancels.then(scene.target("parent")), {
            Leaf(scene.target("x"));
        });
    });
    assert!(scene.request("x"));
    assert!(scene.capture("x"));
    scene.with_host(|host| host.owner_lost_focus());
    scene.idle();
    assert_eq!(scene.states(["parent", "x"]), [Inactive, Inactive]);
    assert!(!scene.root_has_focus());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OwnerFocusTest.kt, clickingOnNonClickableSpaceInAppWhenViewIsFocused_doesNotChangeViewFocus, clickingOnNonClickableSpaceInAppWhenViewIsNotFocused_doesNotChangeViewFocus
#[test]
fn a_click_on_nothing_asks_nothing_of_the_view() {
    let scene = scene(|scene| Leaf(scene.req("x").size(100.dp()).focus_target()));
    scene.with_host(|host| click(host, 50.0, 50.0));
    assert_eq!(asked_of_the_view(*scene), None);
    assert!(scene.request("x"));
    assert_eq!(asked_of_the_view(*scene), Some(true));
    scene.with_host(|host| click(host, 50.0, 50.0));
    assert_eq!(asked_of_the_view(*scene), None);
    assert!(scene.root_has_focus());
}

/// As the view system gives the focus back to a view in keyboard mode as it's cleared
/// (`FocusManagerCompositionLocalTest`'s keyboard arm, D1281).
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusManagerCompositionLocalTest.kt, clearFocus_singleLayout_focusIsRestoredAfterClear, clearFocus_whenRootIsActiveParent, clearFocus_forced_whenHierarchyHasCapturedFocus
#[test]
fn a_clear_in_keyboard_mode_comes_back_through_the_view() {
    for (captures, force) in [(false, false), (true, true)] {
        let scene = scene(|scene| Leaf(scene.target("x")));
        assert!(scene.request("x"));
        if captures {
            assert!(scene.capture("x"));
        }
        assert_eq!(asked_of_the_view(*scene), Some(true));
        scene.forget();
        scene.clear(force);
        assert_eq!(asked_of_the_view(*scene), Some(false), "the view gives it up");
        // The view system gives it back to the only view, which asks the content.
        assert!(take(*scene, Down));
        assert_eq!(scene.heard("x"), [Inactive, Active]);
        assert!(scene.root_has_focus());
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/InitialFocusTest.kt, noFocusable, singleFocusedItem, topItemIsInitiallyFocused, leftItemIsInitiallyFocused
#[test]
fn a_windows_first_focus_goes_to_the_top_left_item_and_to_nothing_with_nothing_to_focus() {
    // What the view system asks of the view as a window opens in keyboard mode.
    let scene1 = scene(|_| Leaf(Modifier.size(100.dp())));
    assert!(!take(*scene1, Next));
    assert!(!scene1.root_has_focus());

    let rows: [(fn(Scene), &str); 3] = [
        (|scene| Leaf(scene.named("box").size(100.dp()).focusable()), "box"),
        (
            |scene| {
                Column!({
                    Leaf(scene.named("top").size(100.dp()).focusable());
                    Leaf(scene.named("below").size(100.dp()).focusable());
                });
            },
            "top",
        ),
        (
            |scene| {
                Row!({
                    Leaf(scene.named("left").size(100.dp()).focusable());
                    Leaf(scene.named("right").size(100.dp()).focusable());
                });
            },
            "left",
        ),
    ];
    for (content, expected) in rows {
        let scene = scene(content);
        assert!(take(*scene, Next));
        assert_eq!(scene.focused(), [expected]);
    }
}

/// The first navigation key on a phone leaves touch mode as the view system moves the focus to
/// the view: what's clickable can take it then.
#[test]
fn leaving_touch_mode_the_view_asking_lights_the_first_clickable() {
    let scene = scene(|scene| {
        Column!({
            Leaf(scene.named("first").size(10.dp()).clickable(|| {}));
            Leaf(scene.named("second").size(10.dp()).clickable(|| {}));
        });
    });
    scene.with_host(|host| host.set_system_input_mode(InputMode::Touch));
    assert!(!take(*scene, Down), "in touch mode no clickable takes it");
    scene.with_host(|host| host.set_system_input_mode(InputMode::Keyboard));
    assert!(take(*scene, Down));
    assert_eq!(scene.focused(), ["first"]);
}

// ---- A node whose being a target comes and goes -------------------------------------------------------------

/// A node holding a delegate that clips hits, a focus target while it's enabled.
struct Flipping(bool);

struct ClipsHits;

impl ModifierNode for ClipsHits {
    fn hit_clip(&self) -> Option<Shape> {
        Some(RECTANGLE_SHAPE)
    }
}

struct FlippingNode {
    enabled: bool,
    is_target: bool,
    delegates: Delegates,
}

impl ModifierNodeElement for Flipping {
    type Node = FlippingNode;
    fn create(&self) -> FlippingNode {
        let mut delegates = Delegates::new();
        delegates.delegate(ClipsHits);
        FlippingNode { enabled: self.0, is_target: self.0, delegates }
    }
    fn update(&self, node: &mut FlippingNode) {
        node.enabled = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn name(&self) -> &'static str {
        "flipping"
    }
}

impl ModifierNode for FlippingNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        if self.is_target { Some(self) } else { None }
    }
    fn focus_target_wanted(&self) -> Option<bool> {
        Some(self.enabled)
    }
    fn set_is_focus_target(&mut self, is_target: bool) {
        self.is_target = is_target;
    }
}

impl FocusTargetModifierNode for FlippingNode {}

/// What its update changes is the target alone: the delegate's clip stays heard.
#[test]
fn a_delegating_node_turning_into_a_target_and_back_keeps_its_delegates_clip() {
    let clicks = cell(0);
    let scene = scene(move |scene| {
        let enabled = !scene.flag("off").get();
        // Its child is placed past its bounds: only outside the clip does a press miss it.
        Stack!(
            modifier = Modifier
                .required_size((50.dp(), 50.dp()))
                .then_custom(Flipping(enabled))
                .then(scene.req("x")),
            {
                Leaf(
                    Modifier
                        .offset(40.dp(), 0.dp())
                        .required_size((40.dp(), 40.dp()))
                        .clickable(move || clicks.set(clicks.get() + 1)),
                );
            }
        );
    });
    let press_at = |x: f32| {
        scene.with_host(|host| click(host, x, 10.0));
        scene.idle();
    };
    press_at(45.0);
    assert_eq!(clicks.get(), 1, "inside");
    press_at(70.0);
    assert_eq!(clicks.get(), 1, "outside its clip");
    scene.set("off", true);
    press_at(70.0);
    assert_eq!(clicks.get(), 1, "still clipped, not a target");
    press_at(45.0);
    assert_eq!(clicks.get(), 2);
    scene.set("off", false);
    press_at(70.0);
    assert_eq!(clicks.get(), 2, "still clipped, a target again");
}
