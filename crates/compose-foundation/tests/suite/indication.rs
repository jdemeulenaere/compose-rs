//! `Modifier.indication` and `IndicationNodeFactory`: a factory's node draws what a source's
//! interactions say, made again only for a new source or a factory not the same.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    ClickableOptions, Color, ContentDrawScope, DrawModifierNode, Indication, IndicationNodeFactory,
    Interaction, InteractionSource, Modifier, ModifierNode, MutableInteractionSource, NodeContext,
    Offset, Paint, Press, PressInteraction, ReleaseContext, UnitExt,
};

type Hook = Option<Rc<dyn Fn()>>;

/// A factory whose node draws black at 30 % while pressed, calling `on_create` when it makes a
/// node and `on_draw` when its node draws, as Compose's `TestPressIndicationNodeFactory`.
#[derive(Clone)]
pub(crate) struct TestPressIndication {
    pub(crate) on_create: Hook,
    pub(crate) on_draw: Hook,
}

fn same_hook(a: &Hook, b: &Hook) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl IndicationNodeFactory for TestPressIndication {
    fn create(&self, interaction_source: InteractionSource) -> Box<dyn ModifierNode> {
        if let Some(on_create) = &self.on_create {
            on_create();
        }
        let pressed = mutable_state_of(false);
        let handle = pressed.handle();
        let interactions = interaction_source.interactions();
        let job = CoroutineScope::new().launch(async move {
            let mut presses: Vec<Press> = Vec::new();
            interactions
                .collect(move |interaction| {
                    match interaction {
                        Interaction::Press(PressInteraction::Press(press)) => presses.push(press),
                        Interaction::Press(
                            PressInteraction::Release(press) | PressInteraction::Cancel(press),
                        ) => presses.retain(|kept| *kept != press),
                        _ => {}
                    }
                    handle.set(!presses.is_empty());
                })
                .await
        });
        Box::new(TestPressNode { pressed, job, on_draw: self.on_draw.clone() })
    }

    fn same(&self, other: &dyn IndicationNodeFactory) -> bool {
        other.downcast_ref::<TestPressIndication>().is_some_and(|other| {
            same_hook(&self.on_create, &other.on_create) && same_hook(&self.on_draw, &other.on_draw)
        })
    }
}

struct TestPressNode {
    pressed: OwnedState<bool>,
    job: Job,
    on_draw: Hook,
}

impl Drop for TestPressNode {
    fn drop(&mut self) {
        self.job.cancel();
    }
}

impl ModifierNode for TestPressNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for TestPressNode {
    fn draws_content(&self) -> bool {
        true
    }

    fn draw_with_content(&mut self, scope: &mut ContentDrawScope<'_, '_>) {
        if let Some(on_draw) = &self.on_draw {
            on_draw();
        }
        scope.draw_content();
        if self.pressed.get() {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(Color::BLACK.with_alpha(0.3)));
        }
    }
}

fn counter() -> (Rc<Cell<u32>>, Hook) {
    let count = Rc::new(Cell::new(0));
    let counting = count.clone();
    (count, Some(Rc::new(move || counting.set(counting.get() + 1))))
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/IndicationTest.kt, indicationNodeFactory_drawIsCalled
#[test]
fn an_indications_node_draws() {
    let (draws, on_draw) = counter();
    let indication = Indication::new(TestPressIndication { on_create: None, on_draw });
    let source = MutableInteractionSource::new();
    let _ui = compose_test(move || {
        Box!(modifier = Modifier.size(100.dp()).indication(source.clone(), indication.clone()));
    });
    assert_eq!(draws.get(), 1);
}

/// Each interaction that changes what it shows draws it again: once at first, once pressed,
/// once released.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/IndicationTest.kt, indicationNodeFactory_receivesUpdates
#[test]
fn an_indications_node_draws_again_for_each_interaction() {
    let (draws, on_draw) = counter();
    let indication = Indication::new(TestPressIndication { on_create: None, on_draw });
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = compose_test(move || {
        Box!(modifier = Modifier.size(100.dp()).indication(given.clone(), indication.clone()));
    });
    assert_eq!(draws.get(), 1);
    let press = Press::new(Offset::ZERO);
    source.try_emit(PressInteraction::Press(press));
    ui.wait_for_idle();
    assert_eq!(draws.get(), 2);
    assert!(ui.dump_display_list().contains("#4D000000"), "{}", ui.dump_display_list());
    source.try_emit(PressInteraction::Release(press));
    ui.wait_for_idle();
    assert_eq!(draws.get(), 3);
    assert!(!ui.dump_display_list().contains("#4D000000"));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/IndicationTest.kt, indicationNodeFactory_newInteractionSource
#[test]
fn a_new_interaction_source_makes_a_new_node() {
    let (creates, on_create) = counter();
    let (draws, on_draw) = counter();
    let indication = Indication::new(TestPressIndication { on_create, on_draw });
    let source = mutable_state_of(MutableInteractionSource::new());
    let handle = source.handle();
    let mut ui = compose_test(move || {
        Box!(modifier = Modifier.size(100.dp()).indication(handle.get(), indication.clone()));
    });
    assert_eq!((creates.get(), draws.get()), (1, 1));
    source.set(MutableInteractionSource::new());
    ui.wait_for_idle();
    assert_eq!((creates.get(), draws.get()), (2, 2));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/IndicationTest.kt, indicationNodeFactory_reuse_sameIndicationInstance
#[test]
fn a_recomposition_with_the_same_indication_keeps_its_node() {
    let (creates, on_create) = counter();
    let indication = Indication::new(TestPressIndication { on_create, on_draw: None });
    let source = MutableInteractionSource::new();
    let state = mutable_state_of(false);
    let handle = state.handle();
    let mut ui = compose_test(move || {
        let _ = handle.get();
        Box!(modifier = Modifier.size(100.dp()).indication(source.clone(), indication.clone()));
    });
    assert_eq!(creates.get(), 1);
    state.set(true);
    ui.wait_for_idle();
    assert_eq!(creates.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/IndicationTest.kt, indicationNodeFactory_reuse_differentIndicationInstance_comparesEqual
#[test]
fn an_equal_factory_keeps_the_node() {
    let (creates, on_create) = counter();
    let factory = TestPressIndication { on_create, on_draw: None };
    let indication = mutable_state_of(Indication::new(factory.clone()));
    let handle = indication.handle();
    let source = MutableInteractionSource::new();
    let mut ui = compose_test(move || {
        Box!(modifier = Modifier.size(100.dp()).indication(source.clone(), handle.get()));
    });
    assert_eq!(creates.get(), 1);
    indication.set(Indication::new(factory));
    ui.wait_for_idle();
    assert_eq!(creates.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/IndicationTest.kt, indicationNodeFactory_recreation
#[test]
fn a_factory_not_the_same_makes_a_new_node_that_draws() {
    let (creates, on_create) = counter();
    let drawn = Rc::new(Cell::new(0));
    let first = drawn.clone();
    let indication = mutable_state_of(Indication::new(TestPressIndication {
        on_create: on_create.clone(),
        on_draw: Some(Rc::new(move || first.set(1))),
    }));
    let handle = indication.handle();
    let source = MutableInteractionSource::new();
    let mut ui = compose_test(move || {
        Box!(modifier = Modifier.size(100.dp()).indication(source.clone(), handle.get()));
    });
    assert_eq!((creates.get(), drawn.get()), (1, 1));
    drawn.set(0);
    let second = drawn.clone();
    indication.set(Indication::new(TestPressIndication {
        on_create,
        on_draw: Some(Rc::new(move || second.set(2))),
    }));
    ui.wait_for_idle();
    assert_eq!((creates.get(), drawn.get()), (2, 2));
}

/// `Indication::NONE` draws nothing and adds no modifier, as `indication(source, null)`.
#[test]
fn no_indication_is_no_modifier() {
    let source = MutableInteractionSource::new();
    let ui = compose_test(move || {
        Box!(modifier = Modifier.size(100.dp()).indication(source.clone(), Indication::NONE));
    });
    assert!(!ui.dump_tree().contains("indication"), "{}", ui.dump_tree());
}

// ---- The hosted node's lifecycle -----------------------------------------------------------------

type Log = Rc<std::cell::RefCell<Vec<&'static str>>>;

/// A factory whose nodes log their lifecycle.
struct Logging(Log);
struct LoggingNode(Log);

impl IndicationNodeFactory for Logging {
    fn create(&self, _: InteractionSource) -> Box<dyn ModifierNode> {
        Box::new(LoggingNode(self.0.clone()))
    }
    fn same(&self, other: &dyn IndicationNodeFactory) -> bool {
        other.downcast_ref::<Logging>().is_some_and(|other| Rc::ptr_eq(&self.0, &other.0))
    }
}

impl ModifierNode for LoggingNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, _: &mut NodeContext<'_>) {
        self.0.borrow_mut().push("attach");
    }
    fn on_detach(&mut self, _: &mut NodeContext<'_>) {
        self.0.borrow_mut().push("detach");
    }
    fn on_reset(&mut self, _: &mut NodeContext<'_>) {
        self.0.borrow_mut().push("reset");
    }
    fn on_release(&mut self, _: &mut ReleaseContext<'_>) {
        self.0.borrow_mut().push("release");
    }
}

impl DrawModifierNode for LoggingNode {}

fn clickable_hosting(log: &Log, source: &MutableInteractionSource) -> Modifier {
    let options = ClickableOptions {
        interaction_source: Some(source.clone()),
        indication: Indication::new(Logging(log.clone())),
        ..ClickableOptions::DEFAULT
    };
    Modifier.size(10.dp()).clickable_with(options, || {})
}

/// A clickable's indication node, its host reused: reset, detached (a new one made when needed,
/// as `onDetach`'s `undelegate`) and released, then the next one attached.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, reset
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, onDetach
#[test]
fn a_reused_clickable_resets_detaches_and_releases_its_indication_node() {
    let log = Log::default();
    let source = MutableInteractionSource::new();
    let key = mutable_state_of(0);
    let (handle, logged, given) = (key.handle(), log.clone(), source.clone());
    let mut ui = compose_test(move || {
        let (logged, given) = (logged.clone(), given.clone());
        ReusableContent(handle.get(), move || {
            Box!(modifier = clickable_hosting(&logged, &given));
        });
    });
    assert_eq!(*log.borrow(), ["attach"]);
    key.set(1);
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["attach", "reset", "detach", "release", "attach"]);
}

/// Removed, it's detached and released, not reset.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, onDetach
#[test]
fn a_removed_clickable_detaches_and_releases_its_indication_node() {
    let log = Log::default();
    let source = MutableInteractionSource::new();
    let shown = mutable_state_of(true);
    let (handle, logged, given) = (shown.handle(), log.clone(), source.clone());
    let mut ui = compose_test(move || {
        if handle.get() {
            Box!(modifier = clickable_hosting(&logged, &given));
        }
    });
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["attach", "detach", "release"]);
}

/// Replaced by an update, the old one is detached and released, as `undelegate`, not reset.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, undelegate
#[test]
fn a_clickable_s_replaced_indication_node_is_detached_and_released() {
    let (first, second) = (Log::default(), Log::default());
    let source = MutableInteractionSource::new();
    let swapped = mutable_state_of(false);
    let (handle, a, b, given) = (swapped.handle(), first.clone(), second.clone(), source.clone());
    let mut ui = compose_test(move || {
        let log = if handle.get() { &b } else { &a };
        Box!(modifier = clickable_hosting(log, &given));
    });
    assert_eq!(*first.borrow(), ["attach"]);
    swapped.set(true);
    ui.wait_for_idle();
    assert_eq!(*first.borrow(), ["attach", "detach", "release"]);
    assert_eq!(*second.borrow(), ["attach"]);
}

/// `Modifier.indication`'s node stays with its host, as `IndicationModifierNode`'s delegate: reset
/// and detached with it when it's reused, attached again, released only when it goes.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatingNode.kt, reset
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/IndicationTest.kt, indicationNodeFactory_reuse_sameIndicationInstance
#[test]
fn a_reused_indication_modifier_keeps_its_node_through_reset_and_detach() {
    let log = Log::default();
    let source = MutableInteractionSource::new();
    let key = mutable_state_of(0);
    let shown = mutable_state_of(true);
    let (handle, is_shown, logged, given) =
        (key.handle(), shown.handle(), log.clone(), source.clone());
    let mut ui = compose_test(move || {
        if is_shown.get() {
            let (logged, given) = (logged.clone(), given.clone());
            ReusableContent(handle.get(), move || {
                let indication = Indication::new(Logging(logged.clone()));
                Box!(modifier = Modifier.size(10.dp()).indication(given.clone(), indication));
            });
        }
    });
    assert_eq!(*log.borrow(), ["attach"]);
    key.set(1);
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["attach", "reset", "detach", "attach"]);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["attach", "reset", "detach", "attach", "detach", "release"]);
}
