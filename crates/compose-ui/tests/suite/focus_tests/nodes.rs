//! Targets as nodes: their focusability, what they're told, their focused rectangle, a node of
//! several focus kinds, requests for a child by rectangle, and focus groups.

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::callback;

use super::support::*;

const ALWAYS: Focusability = Focusability::ALWAYS;
const NEVER: Focusability = Focusability::NEVER;
const SYSTEM: Focusability = Focusability::SYSTEM_DEFINED;

/// `held`, alone in a window of `mode`.
fn alone(held: HeldTarget, mode: InputMode) -> Composed {
    let scene = scene(move |_| Leaf(held.modifier()));
    scene.with_host(|host| host.set_input_mode(mode));
    scene.idle();
    scene
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetModifierNodeTest.kt, requestFocus, requestFocus_notFocusable_noContent, requestFocus_focusabilitySystemDefined_touchMode, requestFocus_focusabilitySystemDefined_nonTouchMode
#[test]
fn a_target_takes_the_focus_by_its_focusability_and_the_input_mode() {
    for (focusability, mode, state) in [
        (ALWAYS, InputMode::Touch, Active),
        (ALWAYS, InputMode::Keyboard, Active),
        (NEVER, InputMode::Keyboard, Inactive),
        (SYSTEM, InputMode::Touch, Inactive),
        (SYSTEM, InputMode::Keyboard, Active),
    ] {
        let held = HeldTarget::new(focusability);
        let scene = alone(held, mode);
        assert_eq!(held.state(), Inactive);
        held.request();
        scene.idle();
        assert_eq!(held.state(), state, "{focusability:?} in {mode:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetModifierNodeTest.kt, requestFocus_notFocusable_noFocusableContent, requestFocus_notFocusable_focusableContent
#[test]
fn a_target_that_never_takes_the_focus_gives_it_to_a_focusable_child() {
    for (with_focusable, state) in [(false, Inactive), (true, ActiveParent)] {
        let held = HeldTarget::new(NEVER);
        let scene = scene(move |scene| {
            Stack!(modifier = held.modifier(), {
                if with_focusable {
                    Leaf(Modifier.size(10.dp()).then(scene.named("child")).focusable());
                } else {
                    Leaf(Modifier.size(10.dp()));
                }
            });
        });
        held.request();
        scene.idle();
        assert_eq!(held.state(), state);
        assert_eq!(scene.state("child").is_focused(), with_focusable);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetModifierNodeTest.kt, focused_focusabilitySetToNotFocusableWhileFocused, focused_focusabilitySetToSystemDefinedWhileFocusedInTouchMode, focused_focusabilitySetToSystemDefinedWhileFocusedInNonTouchMode
#[test]
fn a_target_with_the_focus_keeps_it_only_if_it_can_still_take_it() {
    for (to, mode, state) in [
        (NEVER, InputMode::Keyboard, Inactive),
        (SYSTEM, InputMode::Touch, Inactive),
        (SYSTEM, InputMode::Keyboard, Active),
    ] {
        let held = HeldTarget::new(ALWAYS);
        let scene = alone(held, mode);
        assert!(held.request());
        scene.idle();
        assert_eq!(held.state(), Active);
        held.set_focusability(to);
        scene.idle();
        assert_eq!(held.state(), state, "{to:?} in {mode:?}");
    }
}

/// As Compose's observation of the mode where a target's properties are fetched.
#[test]
fn going_to_touch_mode_takes_the_focus_from_a_target_the_system_defines() {
    let held = HeldTarget::new(SYSTEM);
    let scene = alone(held, InputMode::Keyboard);
    assert!(held.request());
    scene.idle();
    scene.with_host(|host| host.set_input_mode(InputMode::Touch));
    scene.idle();
    assert_eq!(held.state(), Inactive);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetModifierNodeTest.kt, onFocusChange, onFocusChange_requestingChildFocus, onFocusChange_requestingChildFocusAfterParent, onFocusChange_updatingFocusabilityBeforeAttach
#[test]
fn a_target_is_told_each_change_of_its_state_as_previous_and_current() {
    // Nothing at first, a change of focusability before it's attached included.
    let held = HeldTarget::new(ALWAYS);
    held.set_focusability(NEVER);
    held.set_focusability(ALWAYS);
    assert!(held.take_changes().is_empty());

    let scene = alone(held, InputMode::Keyboard);
    assert!(held.take_changes().is_empty(), "attached");
    assert!(held.request());
    scene.idle();
    assert_eq!(held.take_changes(), [(Inactive, Active)]);
    held.set_focusability(NEVER);
    scene.idle();
    assert_eq!(held.take_changes(), [(Active, Inactive)]);

    // A child's request, and a request for the parent first.
    let (parent, child) = (HeldTarget::new(ALWAYS), HeldTarget::new(ALWAYS));
    let scene = self::scene(move |_| {
        Stack!(modifier = parent.modifier(), {
            Leaf(child.modifier());
        });
    });
    assert!(child.request());
    scene.idle();
    assert_eq!(parent.take_changes(), [(Inactive, ActiveParent)]);
    assert_eq!(child.take_changes(), [(Inactive, Active)]);

    let (parent, child) = (HeldTarget::new(ALWAYS), HeldTarget::new(ALWAYS));
    let scene = self::scene(move |_| {
        Stack!(modifier = parent.modifier(), {
            Leaf(child.modifier());
        });
    });
    assert!(parent.request());
    scene.idle();
    assert_eq!(parent.take_changes(), [(Inactive, Active)]);
    assert!(child.take_changes().is_empty());
    assert!(child.request());
    scene.idle();
    assert_eq!(parent.take_changes(), [(Active, ActiveParent)]);
    assert_eq!(child.take_changes(), [(Inactive, Active)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetModifierNodeTest.kt, focusPropertiesNodeInHierarchy
#[test]
fn properties_can_let_a_target_that_never_takes_the_focus_take_it_while_they_are_there() {
    let held = HeldTarget::new(NEVER);
    let scene = scene(move |scene| {
        let gone = scene.flag("gone").get();
        let chain = if gone { Modifier } else { properties(|p| p.can_focus = true) };
        Leaf(chain.then(held.modifier()));
    });
    assert!(held.request());
    scene.idle();
    assert_eq!(held.state(), Active);
    scene.set("gone", true);
    assert_eq!(held.state(), Inactive);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetModifierNodeTest.kt, calculateFocusArea_findsTheFocusedChildOrSelf, calculateFocusArea_canFindSelf, calculateFocusArea_isNotClipped
#[test]
fn a_targets_focused_rectangle_is_of_what_has_the_focus_below_it_in_its_own_coordinates() {
    let square = |side: f32| Modifier.required_size((side.dp(), side.dp()));

    let nodes: [HeldTarget; 4] = std::array::from_fn(|_| HeldTarget::new(ALWAYS));
    let [top, first, second, third] = nodes;
    let scene = scene(move |_| {
        Stack!(modifier = top.modifier(), {
            Row!({
                Leaf(square(12.0).then(first.modifier()));
                Leaf(square(12.0).then(second.modifier()));
                Leaf(square(12.0).then(third.modifier()));
            });
        });
    });
    assert!(second.request());
    scene.idle();
    assert_eq!(top.handle().focused_rect(), Some(Rect::new(12.0, 0.0, 24.0, 12.0)));
    assert_eq!(first.handle().focused_rect(), None);
    assert_eq!(second.handle().focused_rect(), Some(Rect::new(0.0, 0.0, 12.0, 12.0)));
    assert_eq!(third.handle().focused_rect(), None);

    // Its own, the focus not being below it.
    let (parent, child) = (HeldTarget::new(ALWAYS), HeldTarget::new(ALWAYS));
    let scene = self::scene(move |_| {
        Stack!(modifier = square(36.0).then(parent.modifier()), {
            Leaf(square(12.0).then(child.modifier()));
        });
    });
    assert!(parent.request());
    scene.idle();
    assert_eq!(parent.handle().focused_rect(), Some(Rect::new(0.0, 0.0, 36.0, 36.0)));
    assert_eq!(child.handle().focused_rect(), None);

    // Out of its bounds: not cut to them.
    let (parent, child) = (HeldTarget::new(ALWAYS), HeldTarget::new(ALWAYS));
    let scene = self::scene(move |_| {
        Stack!(modifier = square(90.0).then(parent.modifier()), {
            Leaf(square(40.0).offset(150.dp(), 150.dp()).then(child.modifier()));
        });
    });
    assert!(child.request());
    scene.idle();
    assert_eq!(parent.handle().focused_rect(), Some(Rect::new(150.0, 150.0, 190.0, 190.0)));
}

// ---- One node of several focus kinds ---------------------------------------------------------------------

/// A node that's a requester and a properties node, a target through a delegate or the element
/// after it.
#[derive(Clone, Copy)]
struct Several {
    can_focus: compose_runtime::MutableState<bool>,
    handle: &'static Cell<Option<NodeHandle>>,
}

impl Several {
    fn modifier(self, delegated: bool) -> Modifier {
        let element = SeveralElement(self, delegated);
        if delegated {
            Modifier.then_custom(element)
        } else {
            Modifier.then_custom(element).focus_target()
        }
    }
    fn handle(self) -> NodeHandle {
        self.handle.get().expect("attached")
    }
}

struct SeveralElement(Several, bool);

struct SeveralNode {
    several: Several,
    delegates: Delegates,
}

impl ModifierNodeElement for SeveralElement {
    type Node = SeveralNode;
    fn create(&self) -> SeveralNode {
        let mut delegates = Delegates::new();
        if self.1 {
            delegates.delegate(focus_target_modifier_node(ALWAYS, None));
        }
        SeveralNode { several: self.0, delegates }
    }
    fn update(&self, _node: &mut SeveralNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "several"
    }
}

impl ModifierNode for SeveralNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.several.handle.set(Some(context.handle()));
    }
    fn as_focus_requester(&mut self) -> Option<&mut dyn FocusRequesterModifierNode> {
        Some(self)
    }
    fn as_focus_properties(&mut self) -> Option<&mut dyn FocusPropertiesModifierNode> {
        Some(self)
    }
}

impl FocusRequesterModifierNode for SeveralNode {}

impl FocusPropertiesModifierNode for SeveralNode {
    fn apply_focus_properties(&self, properties: &mut FocusProperties) {
        properties.can_focus = self.several.can_focus.get();
    }
}

/// `check` on the node, a target through a delegate and through the next element.
fn with_several(check: impl Fn(Several, &Composed)) {
    for delegated in [false, true] {
        let several = Several { can_focus: shared(true), handle: cell(None) };
        let scene = scene(move |_| Leaf(several.modifier(delegated)));
        check(several, &scene);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CombinedFocusModifierNodeTest.kt, requestFocus, captureFocus, freeFocus
#[test]
fn a_node_of_several_kinds_requests_captures_and_frees() {
    with_several(|several, scene| {
        assert!(several.handle().request_focus(Enter));
        scene.idle();
        assert!(scene.root_has_focus());
        assert!(several.handle().capture_focus());
        scene.clear(false);
        assert!(scene.root_has_focus(), "captured: a clear that isn't forced is refused");
        assert!(several.handle().free_focus());
        scene.clear(false);
        assert!(!scene.root_has_focus());
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CombinedFocusModifierNodeTest.kt, requestFocusWhenCanFocusIsTrue, requestFocusWhenCanFocusIsFalse, losesFocusWhenCanFocusChangesToFalse, doesNotGainFocusWhenCanFocusChangesToTrue
#[test]
fn a_node_of_several_kinds_says_itself_whether_it_can_take_the_focus() {
    with_several(|several, scene| {
        several.can_focus.set(false);
        scene.idle();
        assert!(!several.handle().request_focus(Enter));
        assert!(!scene.root_has_focus());
        several.can_focus.set(true);
        scene.idle();
        assert!(!scene.root_has_focus(), "it doesn't gain it alone");
        assert!(several.handle().request_focus(Enter));
        assert!(scene.root_has_focus());
        several.can_focus.set(false);
        scene.idle();
        assert!(!scene.root_has_focus(), "it lost it");
    });
}

// ---- A child by a rectangle --------------------------------------------------------------------------------

/// `left, top, right, bottom` of what's asked for.
type Asked = (i32, i32, i32, i32);

/// Six boxes of 200 by 100 in a column, in a container that holds the handle asked.
fn six_boxes(scene: Scene, container: HeldTarget) {
    Column!(modifier = Modifier.required_size((200.dp(), 600.dp())).then(container.modifier()), {
        for name in ["0", "1", "2", "3", "4", "5"] {
            Leaf(Modifier.required_size((200.dp(), 100.dp())).then(scene.named(name)).focusable());
        }
    });
}

/// Places its children 100 apart, each 200 high: the middle of it is both's.
struct Overlapping;

impl MeasurePolicy for Overlapping {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<_> =
            measurables.iter().map(|&each| scope.measure(each, constraints)).collect();
        for (index, placeable) in placeables.iter().enumerate() {
            scope.place(placeable, 0, 100 * index as i32);
        }
        scope.layout(100, 300)
    }
}

/// Two boxes overlapping, in a container that holds the handle asked.
fn overlapping(scene: Scene, container: HeldTarget) {
    Layout(
        container.modifier(),
        Rc::new(Overlapping),
        callback(move || {
            for name in ["a", "b"] {
                Leaf(
                    Modifier
                        .required_size((100.dp(), 200.dp()))
                        .then(scene.named(name))
                        .focusable(),
                );
            }
        }),
    );
}

/// A row of a request: `content`, the first request, then this one's answer and who has the focus.
struct Child {
    what: &'static str,
    content: fn(Scene, HeldTarget),
    first: Option<Asked>,
    asked: Asked,
    answer: bool,
    focused: &'static [&'static str],
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestChildFocusTest.kt, requestedRect_is_insideTheNode, requestedRect_is_onePixelRect, requestedRect_is_theSameAsTheNodeBounds, requestedRect_belongsToAnotherContainer, containerItself_doesNotGetFocused, nonOverlappingRect_doesNotRequestFocus, focusIsNotRequested_if_layoutNode_is_alreadyFocused, highestLayoutNode_getsFocused, firstFocusableModifier_getsFocused_with_innerMostBounds, firstFocusableModifier_getsFocused_with_intermediateBounds, firstFocusableModifier_getsFocused_with_outermostBounds, internalLogic_respects_modifierOffsets_focusIsOutsideFocusableBounds, layoutNode_thatIs_outside_of_containerLayoutNodeBounds_willBeFocused, modifierNode_that_doesNotOverlap_with_layoutNode_willNotBeFocused, modifierNode_that_overlaps_with_layoutNode_willBeFocused, overlappingSibling_focusTheFirstOne_andThen_focusOverlapArea, overlappingSibling_focusTheSecondOne_andThen_focusOverlapArea, overlappingSiblings_when_requestFocusForOverlapArea_focusesTheFirstPlacedChild
#[test]
fn a_rectangle_gives_the_focus_to_the_first_child_it_is_over_and_only_if_the_focus_isnt_there() {
    fn three_on_a_chain(scene: Scene, container: HeldTarget) {
        Stack!(
            modifier = Modifier.required_size((300.dp(), 300.dp())).then(container.modifier()),
            {
                Leaf(
                    Modifier
                        .required_size((300.dp(), 300.dp()))
                        .then(scene.named("outer"))
                        .focusable()
                        .padding(10.dp())
                        .then(scene.named("middle"))
                        .focusable()
                        .padding(10.dp())
                        .then(scene.named("inner"))
                        .focusable(),
                );
            }
        );
    }
    fn beside_its_container(scene: Scene, container: HeldTarget, inside: bool) {
        Stack!(modifier = Modifier.required_size((300.dp(), 100.dp())), {
            Stack!(
                modifier = Modifier.required_size((100.dp(), 100.dp())).then(container.modifier()),
                {
                    let at = if inside { 90.0 } else { 110.0 };
                    Leaf(
                        Modifier
                            .required_size((100.dp(), 100.dp()))
                            .offset(at.dp(), 0.dp())
                            .then(scene.named("beside"))
                            .focusable(),
                    );
                }
            );
        });
    }
    let rows = [
        Child {
            what: "inside a box",
            content: six_boxes,
            first: None,
            asked: (50, 240, 60, 250),
            answer: true,
            focused: &["2"],
        },
        Child {
            what: "a pixel",
            content: six_boxes,
            first: None,
            asked: (10, 150, 10, 150),
            answer: true,
            focused: &["1"],
        },
        Child {
            what: "a box's bounds",
            content: six_boxes,
            first: None,
            asked: (0, 100, 200, 200),
            answer: true,
            focused: &["1"],
        },
        Child {
            what: "another container's child",
            content: |scene, container| {
                Row!(modifier = Modifier.required_size((400.dp(), 200.dp())), {
                    Stack!(
                        modifier =
                            Modifier.required_size((200.dp(), 200.dp())).then(container.modifier()),
                        {
                            Leaf(
                                Modifier
                                    .required_size((200.dp(), 200.dp()))
                                    .then(scene.named("mine"))
                                    .focusable(),
                            );
                        }
                    );
                    Stack!(modifier = Modifier.required_size((200.dp(), 200.dp())), {
                        Leaf(
                            Modifier
                                .required_size((200.dp(), 200.dp()))
                                .then(scene.named("other"))
                                .focusable(),
                        );
                    });
                });
            },
            first: None,
            asked: (250, 0, 350, 200),
            answer: false,
            focused: &[],
        },
        Child {
            what: "the container itself",
            content: |scene, container| {
                Leaf(
                    Modifier
                        .required_size((100.dp(), 100.dp()))
                        .then(container.modifier())
                        .then(scene.named("itself"))
                        .focusable(),
                );
            },
            first: None,
            asked: (0, 0, 100, 100),
            answer: false,
            focused: &[],
        },
        Child {
            what: "between two",
            content: |scene, container| {
                Column!(
                    modifier =
                        Modifier.required_size((200.dp(), 600.dp())).then(container.modifier()),
                    {
                        Leaf(
                            Modifier
                                .required_size((200.dp(), 100.dp()))
                                .then(scene.named("a"))
                                .focusable(),
                        );
                        Leaf(Modifier.required_size((200.dp(), 50.dp())));
                        Leaf(
                            Modifier
                                .required_size((200.dp(), 100.dp()))
                                .then(scene.named("b"))
                                .focusable(),
                        );
                    }
                );
            },
            first: None,
            asked: (50, 125, 100, 130),
            answer: false,
            focused: &[],
        },
        Child {
            what: "over what has the focus",
            content: six_boxes,
            first: Some((0, 200, 200, 300)),
            asked: (50, 250, 50, 250),
            answer: false,
            focused: &["2"],
        },
        Child {
            what: "the highest of nested ones",
            content: |scene, container| {
                Stack!(
                    modifier =
                        Modifier.required_size((300.dp(), 300.dp())).then(container.modifier()),
                    {
                        Stack!(
                            modifier = Modifier
                                .required_size((300.dp(), 300.dp()))
                                .then(scene.named("outer"))
                                .focusable(),
                            {
                                Stack!(
                                    modifier = Modifier
                                        .required_size((300.dp(), 300.dp()))
                                        .then(scene.named("middle"))
                                        .focusable(),
                                    {
                                        Leaf(
                                            Modifier
                                                .required_size((300.dp(), 300.dp()))
                                                .then(scene.named("inner"))
                                                .focusable(),
                                        );
                                    }
                                );
                            }
                        );
                    }
                );
            },
            first: None,
            asked: (140, 140, 160, 160),
            answer: true,
            focused: &["outer"],
        },
        Child {
            what: "the chain's innermost bounds: its first",
            content: three_on_a_chain,
            first: None,
            asked: (150, 150, 150, 150),
            answer: true,
            focused: &["outer"],
        },
        Child {
            what: "the chain's middle bounds: its first",
            content: three_on_a_chain,
            first: None,
            asked: (150, 15, 150, 15),
            answer: true,
            focused: &["outer"],
        },
        Child {
            what: "the chain's outer bounds: its first",
            content: three_on_a_chain,
            first: None,
            asked: (150, 5, 150, 5),
            answer: true,
            focused: &["outer"],
        },
        Child {
            what: "a target moved away from the point",
            content: |scene, container| {
                Stack!(
                    modifier =
                        Modifier.required_size((300.dp(), 300.dp())).then(container.modifier()),
                    {
                        Leaf(
                            Modifier
                                .required_size((100.dp(), 100.dp()))
                                .offset(100.dp(), 100.dp())
                                .then(scene.named("moved"))
                                .focusable(),
                        );
                    }
                );
            },
            first: None,
            asked: (50, 50, 50, 50),
            answer: false,
            focused: &[],
        },
        Child {
            what: "a child out of the container's bounds",
            content: |scene, container| {
                Stack!(modifier = Modifier.required_size((300.dp(), 100.dp())), {
                    Stack!(
                        modifier =
                            Modifier.required_size((100.dp(), 100.dp())).then(container.modifier()),
                        {
                            Stack!(
                                modifier = Modifier
                                    .required_size((100.dp(), 100.dp()))
                                    .offset(110.dp(), 0.dp())
                                    .focusable(),
                                {
                                    Leaf(
                                        Modifier
                                            .required_size((100.dp(), 100.dp()))
                                            .then(scene.named("out"))
                                            .focusable(),
                                    );
                                }
                            );
                        }
                    );
                });
            },
            first: None,
            asked: (150, 50, 150, 50),
            answer: true,
            focused: &["out"],
        },
        Child {
            what: "a node with its target's chain moved out of its layout's bounds, not over the point",
            content: |scene, container| beside_its_container(scene, container, false),
            first: None,
            asked: (150, 50, 150, 50),
            answer: false,
            focused: &[],
        },
        Child {
            what: "a node over the point by its layout's bounds",
            content: |scene, container| beside_its_container(scene, container, true),
            first: None,
            asked: (95, 50, 95, 50),
            answer: true,
            focused: &["beside"],
        },
        Child {
            what: "from the first, the overlap",
            content: overlapping,
            first: Some((50, 50, 50, 50)),
            asked: (50, 150, 50, 150),
            answer: false,
            focused: &["a"],
        },
        Child {
            what: "from the second, the overlap",
            content: overlapping,
            first: Some((50, 250, 50, 250)),
            asked: (50, 150, 50, 150),
            answer: false,
            focused: &["b"],
        },
        Child {
            what: "the overlap: the first placed",
            content: overlapping,
            first: None,
            asked: (50, 150, 50, 150),
            answer: true,
            focused: &["a"],
        },
    ];
    for row in rows {
        let container = HeldTarget::new(NEVER);
        let content = row.content;
        let scene = scene(move |scene| content(scene, container));
        let ask = |(left, top, right, bottom): Asked| {
            let answer =
                container.handle().request_focus_for_child_in_root_bounds(left, top, right, bottom);
            scene.idle();
            answer
        };
        if let Some(first) = row.first {
            assert!(ask(first), "{}", row.what);
        }
        assert_eq!(ask(row.asked), row.answer, "{}", row.what);
        assert_eq!(scene.focused(), row.focused, "{}", row.what);
    }
}

/// By its own bounds, the same as by the root's with the container's origin taken away.
#[test]
fn a_rectangle_in_local_bounds_counts_from_the_container() {
    let container = HeldTarget::new(NEVER);
    let scene = scene(move |scene| {
        Stack!(modifier = Modifier.offset(50.dp(), 50.dp()), {
            six_boxes(scene, container);
        });
    });
    assert!(container.handle().request_focus_for_child_in_local_bounds(10, 150, 10, 150));
    scene.idle();
    assert_eq!(scene.focused(), ["1"]);
}

// ---- Focus groups ----------------------------------------------------------------------------------------------

/// A group of two focusables, the group listened to and focusable if asked.
fn group(scene: Scene, name: &'static str, focusable: bool, children: [&'static str; 2]) {
    let chain = scene.req(name).then(scene.listen(name));
    let chain = if focusable { chain.then(properties(|p| p.can_focus = true)) } else { chain };
    Column!(modifier = chain.focus_group(), {
        for child in children {
            Leaf(Modifier.size(10.dp()).then(scene.named(child)).focusable());
        }
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusGroupTest.kt, focusGroup_withNonFocusableContent_isNotFocusable, focusGroup_withFocusableContent_isNotFocusable, focusGroup_canBeMadeFocusableUsingFocusProperties, itemWithFocusGroup_canBeMadeFocusable
#[test]
fn a_group_takes_the_focus_only_if_its_properties_say_so() {
    let rows: [(fn(Scene), bool); 3] = [
        (
            |scene| {
                Stack!(modifier = scene.named("group").focus_group(), {
                    Text!("not a target");
                });
            },
            false,
        ),
        (
            |scene| {
                Stack!(modifier = scene.named("group").focus_group(), {
                    Leaf(Modifier.focusable());
                });
            },
            false,
        ),
        (
            |scene| {
                Stack!(
                    modifier =
                        scene.named("group").then(properties(|p| p.can_focus = true)).focus_group(),
                    {
                        Text!("not a target");
                    }
                );
            },
            true,
        ),
    ];
    for (content, takes) in rows {
        let scene = scene(content);
        scene.request("group");
        assert_eq!(scene.state("group").is_focused(), takes);
    }

    // A group inside a component made focusable from outside it: the outer target has it.
    let scene = self::scene(|scene| {
        Stack!(
            modifier = scene.named("outer").focusable().then(scene.listen("inside")).focus_group(),
            {
                Text!("not a target");
            }
        );
    });
    assert!(scene.request("outer"));
    assert!(scene.state("outer").is_focused());
    assert!(!scene.state("inside").is_focused());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusGroupTest.kt, oneDimensionalFocusSearch_traversesCurrentFocusGroupBeforeNextGroup, oneDimensionalFocusSearch_flowsOverToNextGroup, oneDimensionalFocusSearch_flowsOverToNextFocusGroupParent, oneDimensionalFocusSearch_wrapsAroundToFirstGroup, twoDimensionalFocusSearch_skipsChildrenIfTheyAreNotInDirectionOfSearch, twoDimensionalFocusSearch_returnsNextFocusGroupParentIfItIsFocusable, twoDimensionalFocusSearch_acrossFocusableFocusGroups_skipsChildren, twoDimensionalFocusSearch_acrossFocusableFocusGroups_whenChildIsInitiallyFocused
#[test]
fn a_search_finishes_a_group_before_the_next_and_stops_on_a_group_that_can_take_the_focus() {
    // (the first group's children, whether each group can take the focus, from, direction, to)
    let rows = [
        (false, false, "a1", Next, "a2"),
        (false, false, "a2", Next, "b1"),
        (false, true, "a2", Next, "b"),
        (false, false, "b2", Next, "a1"),
        (false, false, "a1", Right, "b1"),
        (false, true, "a1", Right, "b"),
        (true, true, "a", Right, "b"),
        (true, true, "a1", Right, "b"),
    ];
    for (first_focusable, second_focusable, from, direction, to) in rows {
        let scene = scene(move |scene| {
            Row!({
                group(scene, "a", first_focusable, ["a1", "a2"]);
                group(scene, "b", second_focusable, ["b1", "b2"]);
            });
        });
        assert!(scene.request(from));
        assert!(scene.move_focus(direction));
        assert_eq!(scene.focused(), [to], "{direction:?} from {from}");
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusGroupTest.kt, twoDimensionalFocusSearch_Enter, twoDimensionalFocusSearch_Exit
#[test]
fn entering_and_exiting_go_between_a_group_that_can_take_the_focus_and_its_child() {
    for (from, direction, to) in [("group", Enter, "child"), ("child", Exit, "group")] {
        let scene = scene(|scene| {
            let chain = scene.named("group").then(properties(|p| p.can_focus = true));
            Stack!(modifier = chain.focus_group(), {
                Leaf(scene.named("child").focusable());
            });
        });
        assert!(scene.request(from));
        assert!(scene.move_focus(direction));
        assert_eq!(scene.focused(), [to]);
    }
}
