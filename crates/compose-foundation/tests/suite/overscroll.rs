//! `OverscrollEffect` around scrollables, lists and anchored drags: scenarios of Compose's
//! `OverscrollTest` and `AnchoredDraggableOverscrollTest`, rewritten. Android's stretch and glow,
//! which the rest of those files draw, aren't in compose-rs yet (D1464).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::gestures::{
    AnchoredDraggableModifier, AnchoredDraggableOptions, AnchoredDraggableState, DraggableAnchors,
    LocalBoxFuture, ScrollBlock,
};
use compose_foundation::layout::*;
use compose_foundation::lazy::LazyListScope;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, NestedScrollConnection, NestedScrollSource, Offset, UnitExt, Velocity};

/// What Compose's `TestOverscrollEffect` notes.
#[derive(Default)]
struct Effect {
    consumes_a_tenth: bool,
    running: Cell<bool>,
    in_progress_calls: Cell<u32>,
    placed: Rc<Cell<u32>>,
    applied_to_scroll: Cell<u32>,
    applied_to_fling: Cell<u32>,
    delta: Cell<Offset>,
    left: Cell<Offset>,
    source: Cell<Option<NestedScrollSource>>,
    pre_fling: Cell<Velocity>,
    fling_left: Cell<Velocity>,
    fling_ended: Cell<bool>,
}

impl Effect {
    fn taking_a_tenth() -> Rc<Effect> {
        Rc::new(Effect { consumes_a_tenth: true, ..Effect::default() })
    }
}

impl OverscrollEffect for Effect {
    fn apply_to_scroll(
        &self,
        delta: Offset,
        source: NestedScrollSource,
        perform_scroll: &mut dyn FnMut(Offset) -> Offset,
    ) -> Offset {
        self.applied_to_scroll.set(self.applied_to_scroll.get() + 1);
        self.delta.set(delta);
        self.source.set(Some(source));
        let own = if self.consumes_a_tenth {
            Offset::new(delta.x / 10.0, delta.y / 10.0)
        } else {
            Offset::ZERO
        };
        let used = perform_scroll(delta - own);
        self.left.set(delta - used - own);
        delta - self.left.get()
    }

    fn apply_to_fling<'a>(
        &'a self,
        velocity: Velocity,
        perform_fling: PerformFling,
    ) -> LocalBoxFuture<'a, ()> {
        self.applied_to_fling.set(self.applied_to_fling.get() + 1);
        self.pre_fling.set(velocity);
        let own = if self.consumes_a_tenth {
            Velocity::new(velocity.x / 10.0, velocity.y / 10.0)
        } else {
            Velocity::ZERO
        };
        Box::pin(async move {
            let used = perform_fling(velocity - own).await;
            self.fling_left.set(velocity - own - used);
            self.fling_ended.set(true);
        })
    }

    fn is_in_progress(&self) -> bool {
        self.in_progress_calls.set(self.in_progress_calls.get() + 1);
        self.running.get()
    }

    fn modifier(&self) -> Modifier {
        let placed = self.placed.clone();
        Modifier.on_size_changed(move |_| placed.set(placed.get() + 1))
    }
}

/// Deltas summed, a part used.
fn counting(part: Rc<Cell<f32>>) -> (DefaultScrollableState, Rc<Cell<f32>>) {
    let total = Rc::new(Cell::new(0.0f32));
    let into = total.clone();
    let state = scrollable_state(move |delta| {
        into.set(into.get() + delta);
        delta * part.get()
    });
    (state, total)
}

fn all() -> Rc<Cell<f32>> {
    Rc::new(Cell::new(1.0))
}

const SLOP: f32 = 18.0;

struct Setup<S> {
    state: S,
    effect: Option<Rc<dyn OverscrollEffect>>,
    fling: Option<Rc<dyn FlingBehavior>>,
    reverse: bool,
    orientation: Orientation,
    around: Option<Rc<dyn NestedScrollConnection>>,
}

impl<S: ScrollableState + ComposeArg + Clone + 'static> Setup<S> {
    fn new(state: S, effect: Rc<dyn OverscrollEffect>) -> Setup<S> {
        Setup {
            state,
            effect: Some(effect),
            fling: None,
            reverse: false,
            orientation: Orientation::Horizontal,
            around: None,
        }
    }

    fn rule(self) -> ComposeTestRule {
        let around = self.around.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
            let mut modifier = Modifier;
            if let Some(around) = &around {
                modifier = modifier.nested_scroll(around.clone());
            }
            let state = self.state.clone();
            let (effect, fling) = (self.effect.clone(), self.fling.clone());
            let (orientation, reverse) = (self.orientation, self.reverse);
            Box!(modifier = modifier, {
                Box!(
                    modifier =
                        Modifier.size(300.dp()).overscroll(effect.clone()).scrollable_overscroll(
                            state.clone(),
                            orientation,
                            effect.clone(),
                            true,
                            reverse,
                            fling.clone(),
                            None,
                        )
                );
            });
        });
        ui.wait_for_idle();
        ui.host().set_view_configuration(Some(compose_ui::ViewConfiguration {
            touch_slop: SLOP,
            ..compose_ui::ViewConfiguration::default()
        }));
        ui.advance_time_by_frame();
        ui
    }
}

const CENTER: Offset = Offset::new(150.0, 150.0);

/// Down, 1000 px to the right, no velocity left, up.
fn drag_and_rest(ui: &mut ComposeTestRule) {
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(1000.0, 0.0));
        touch.advance_event_time(3000);
        touch.up();
    });
    ui.wait_for_idle();
}

fn flick(ui: &mut ComposeTestRule, velocity: f32) {
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(CENTER, Offset::new(300.0, 150.0), velocity, None);
    });
    ui.wait_for_idle();
}

/// Notes the velocity it's flung at; none of it used.
struct Noted(Rc<Cell<f32>>);

impl FlingBehavior for Noted {
    fn perform_fling<'a>(
        &'a self,
        _scope: ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32> {
        self.0.set(initial_velocity);
        Box::pin(async move { initial_velocity })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, modifierAttachesNode, modifierUpdatesToNewNode
#[test]
fn the_modifier_is_what_the_effect_draws_with() {
    let effects = [Rc::new(Effect::default()), Rc::new(Effect::default())];
    let which = mutable_state_of(Some(0usize));
    let read = which.handle();
    let given = effects.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let which = read.get();
        let effect = which.map(|index| given[index].clone() as Rc<dyn OverscrollEffect>);
        // A new size, for the node updated in place to say so.
        let size = 100 + which.unwrap_or(2) * 10;
        Box!(modifier = Modifier.size((size as i32).dp()).overscroll(effect));
    });
    ui.wait_for_idle();
    assert_eq!((effects[0].placed.get(), effects[1].placed.get()), (1, 0));
    which.set(Some(1));
    ui.wait_for_idle();
    assert_eq!((effects[0].placed.get(), effects[1].placed.get()), (1, 1));
    which.set(None);
    ui.wait_for_idle();
    assert_eq!((effects[0].placed.get(), effects[1].placed.get()), (1, 1));
}

struct Factory(Rc<Cell<u32>>);

impl OverscrollFactory for Factory {
    fn create_overscroll_effect(&self) -> Rc<dyn OverscrollEffect> {
        self.0.set(self.0.get() + 1);
        Rc::new(Effect::default())
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, rememberOverscrollEffect_defaultValue, rememberOverscrollEffect_nullOverscrollFactory, rememberOverscrollEffect_ChangeOverscrollFactory
#[test]
fn an_effect_is_remembered_until_the_factory_changes() {
    let made = Rc::new(Cell::new(0));
    let factories: [Rc<dyn OverscrollFactory>; 2] =
        [Rc::new(Factory(made.clone())), Rc::new(Factory(made.clone()))];
    let which = mutable_state_of(None::<usize>);
    let read = which.handle();
    type Effects = Vec<Option<Rc<dyn OverscrollEffect>>>;
    let seen: Rc<RefCell<Effects>> = Rc::default();
    let noted = seen.clone();
    let tick = mutable_state_of(0);
    let ticks = tick.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let factory = read.get().map(|index| factories[index].clone());
        let noted = noted.clone();
        CompositionLocalProvider(LOCAL_OVERSCROLL_FACTORY.provides(factory), move || {
            ticks.get();
            noted.borrow_mut().push(remember_overscroll_effect());
        });
    });
    ui.wait_for_idle();
    // Compose's default is the platform's; compose-rs has none yet.
    assert!(seen.borrow().last().expect("composed").is_none());
    which.set(Some(0));
    ui.wait_for_idle();
    let first = seen.borrow().last().expect("composed").clone().expect("from the factory");
    assert_eq!(made.get(), 1);
    tick.set(1);
    ui.wait_for_idle();
    let again = seen.borrow().last().expect("composed").clone().expect("from the factory");
    assert!(Rc::ptr_eq(&first, &again));
    assert_eq!(made.get(), 1);
    which.set(Some(1));
    ui.wait_for_idle();
    let other = seen.borrow().last().expect("composed").clone().expect("from the factory");
    assert!(!Rc::ptr_eq(&first, &other));
    assert_eq!(made.get(), 2);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, overscrollEffect_scrollable_drag, overscrollEffect_scrollable_drag_reverseDirection
#[test]
fn a_drag_tells_the_effect_what_is_left_whatever_the_direction() {
    for reverse in [false, true] {
        let part = all();
        let effect = Rc::new(Effect::default());
        let (state, _) = counting(part.clone());
        let mut setup = Setup::new(state, effect.clone());
        setup.reverse = reverse;
        let mut ui = setup.rule();
        ui.on_root().perform_touch_input(|touch| {
            touch.down(CENTER);
            touch.move_by(Offset::new(1000.0, 0.0));
        });
        ui.wait_for_idle();
        assert_eq!(effect.delta.get(), Offset::new(1000.0 - SLOP, 0.0));
        assert_eq!(effect.left.get(), Offset::ZERO, "all used");
        ui.on_root().perform_touch_input(|touch| {
            touch.advance_event_time(3000);
            touch.up();
        });
        ui.wait_for_idle();
        part.set(0.5);
        ui.on_root().perform_touch_input(|touch| {
            touch.down(CENTER);
            touch.move_by(Offset::new(1000.0, 0.0));
        });
        ui.wait_for_idle();
        assert_eq!(effect.delta.get(), Offset::new(1000.0 - SLOP, 0.0));
        assert_eq!(effect.left.get(), Offset::new((1000.0 - SLOP) / 2.0, 0.0));
        assert_eq!(effect.source.get(), Some(NestedScrollSource::UserInput));
        ui.on_root().perform_touch_input(|touch| {
            touch.advance_event_time(3000);
            touch.up();
        });
        ui.wait_for_idle();
        assert_eq!(effect.pre_fling.get(), Velocity::ZERO);
        assert!(effect.fling_ended.get());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, overscrollEffect_scrollable_fling
#[test]
fn a_fling_goes_through_it_with_the_velocity_and_its_deltas() {
    let effect = Rc::new(Effect::default());
    let (state, _) = counting(all());
    let mut ui = Setup::new(state, effect.clone()).rule();
    flick(&mut ui, 3000.0);
    assert!(effect.pre_fling.get().x > 0.0);
    assert_eq!(effect.source.get(), Some(NestedScrollSource::SideEffect));
    assert!(effect.fling_ended.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, overscrollEffect_scrollable_preDrag_respectsConsumption
#[test]
fn what_it_takes_of_a_drag_doesnt_scroll() {
    let effect = Effect::taking_a_tenth();
    let (state, total) = counting(all());
    let mut ui = Setup::new(state, effect.clone()).rule();
    drag_and_rest(&mut ui);
    assert!((total.get().abs() - (1000.0 - SLOP) * 0.9).abs() < 0.1, "{}", total.get());
    assert_eq!(effect.delta.get(), Offset::new(1000.0 - SLOP, 0.0));
    assert_eq!(effect.source.get(), Some(NestedScrollSource::UserInput));
}

/// A state saying whether it can scroll.
#[derive(Clone)]
struct Able(DefaultScrollableState, Rc<Cell<bool>>);

impl ComposeArg for Able {
    fn same(&self, other: &Self) -> bool {
        self.0.same(&other.0)
    }
}

impl ScrollableState for Able {
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        self.0.scroll_boxed(priority, block)
    }
    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.0.dispatch_raw_delta(delta)
    }
    fn is_scroll_in_progress(&self) -> bool {
        self.0.is_scroll_in_progress()
    }
    fn last_scrolled_forward(&self) -> bool {
        self.0.last_scrolled_forward()
    }
    fn last_scrolled_backward(&self) -> bool {
        self.0.last_scrolled_backward()
    }
    fn can_scroll_forward(&self) -> bool {
        self.1.get()
    }
    fn can_scroll_backward(&self) -> bool {
        self.1.get()
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, overscrollEffect_scrollable_skipsDeltasIfCannotScroll
#[test]
fn what_cannot_scroll_tells_it_nothing_unless_it_is_showing() {
    let effect = Effect::taking_a_tenth();
    let able = Rc::new(Cell::new(true));
    let (state, total) = counting(all());
    let mut ui = Setup::new(Able(state, able.clone()), effect.clone()).rule();
    drag_and_rest(&mut ui);
    assert!((total.get().abs() - (1000.0 - SLOP) * 0.9).abs() < 0.1, "{}", total.get());
    assert_eq!(effect.delta.get(), Offset::new(1000.0 - SLOP, 0.0));
    effect.delta.set(Offset::ZERO);
    able.set(false);
    let flings = effect.applied_to_fling.get();
    drag_and_rest(&mut ui);
    assert_eq!(effect.delta.get(), Offset::ZERO, "a scrollable with nowhere to go");
    assert_eq!(effect.applied_to_fling.get(), flings);
    // Showing, it's told: to let the pointer pull it back.
    effect.running.set(true);
    drag_and_rest(&mut ui);
    assert_eq!(effect.delta.get(), Offset::new(1000.0, 0.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, overscrollEffect_scrollable_preFling_respectsConsumption
#[test]
fn what_it_takes_of_a_fling_doesnt_fling() {
    let effect = Effect::taking_a_tenth();
    let noted = Rc::new(Cell::new(0.0));
    let (state, _) = counting(all());
    let mut setup = Setup::new(state, effect.clone());
    setup.fling = Some(Rc::new(Noted(noted.clone())));
    let mut ui = setup.rule();
    flick(&mut ui, 3000.0);
    assert!((effect.pre_fling.get().x.abs() - 3000.0).abs() < 0.1, "{:?}", effect.pre_fling.get());
    assert!((noted.get().abs() - 2700.0).abs() < 0.1, "{}", noted.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, overscrollEffect_scrollable_attemptsToStopAnimation
#[test]
fn a_press_while_it_shows_takes_the_drag_without_a_slop() {
    let effect = Rc::new(Effect::default());
    let (state, total) = counting(all());
    let mut ui = Setup::new(state, effect.clone()).rule();
    assert_eq!(effect.in_progress_calls.get(), 0, "no press yet");
    drag_and_rest(&mut ui);
    assert_eq!(effect.in_progress_calls.get(), 1);
    assert_eq!(total.get(), 1000.0 - SLOP);
    effect.running.set(true);
    let before = total.get();
    drag_and_rest(&mut ui);
    assert_eq!(total.get() - before, 1000.0);
    assert_eq!(effect.in_progress_calls.get(), 2);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, horizontalOverscrollEnabled_verifyOverscrollReceivedSingleAxisValues, verticalOverscrollEnabled_verifyOverscrollReceivedSingleAxisValues
#[test]
fn it_is_told_of_its_axis_only() {
    for (orientation, at) in [
        (Orientation::Horizontal, Offset::new(290.0, 200.0)),
        (Orientation::Vertical, Offset::new(200.0, 290.0)),
    ] {
        let effect = Rc::new(Effect::default());
        let (state, _) = counting(Rc::new(Cell::new(0.0)));
        let mut setup = Setup::new(state, effect.clone());
        setup.orientation = orientation;
        let mut ui = setup.rule();
        ui.on_root().perform_touch_input(|touch| {
            touch.swipe_with_velocity(CENTER, at, 3000.0, None);
        });
        ui.wait_for_idle();
        let horizontal = orientation == Orientation::Horizontal;
        let (delta, left, velocity) =
            (effect.delta.get(), effect.left.get(), effect.pre_fling.get());
        if horizontal {
            assert!(delta.x != 0.0 && delta.y == 0.0, "{delta:?}");
            assert!(left.x != 0.0 && left.y == 0.0, "{left:?}");
            assert!(velocity.x != 0.0 && velocity.y == 0.0, "{velocity:?}");
        } else {
            assert!(delta.y != 0.0 && delta.x == 0.0, "{delta:?}");
            assert!(left.y != 0.0 && left.x == 0.0, "{left:?}");
            assert!(velocity.y != 0.0 && velocity.x == 0.0, "{velocity:?}");
        }
    }
}

#[derive(Default)]
struct Inspected {
    pre_scroll: Cell<Offset>,
    post_fling: Cell<Offset>,
}

impl NestedScrollConnection for Inspected {
    fn on_pre_scroll(&self, available: Offset, _: NestedScrollSource) -> Offset {
        self.pre_scroll.set(available);
        Offset::ZERO
    }

    fn on_post_fling(&self, _: Offset, available: Offset) -> LocalBoxFuture<'_, Offset> {
        self.post_fling.set(available);
        Box::pin(async { Offset::ZERO })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, verticalOverscrollEnabled_notTriggered_verifyCrossAxisIsCorrectlyPropagated, verticalOverscrollEnabled_notTriggered_verifyCrossAxisVelocityIsCorrectlyPropagated, horizontalOverscrollEnabled_notTriggered_verifyCrossAxisIsCorrectlyPropagated, horizontalOverscrollEnabled_notTriggered_verifyCrossAxisVelocityIsCorrectlyPropagated
#[test]
fn the_connections_around_hear_nothing_across_its_axis() {
    for orientation in [Orientation::Vertical, Orientation::Horizontal] {
        let inspected = Rc::new(Inspected::default());
        let effect = Rc::new(Effect::default());
        let (state, _) = counting(Rc::new(Cell::new(0.0)));
        let mut setup = Setup::new(state, effect.clone());
        setup.orientation = orientation;
        setup.around = Some(inspected.clone());
        let mut ui = setup.rule();
        ui.on_root().perform_touch_input(|touch| {
            touch.swipe_with_velocity(CENTER, Offset::new(290.0, 290.0), 3000.0, None);
        });
        ui.wait_for_idle();
        let (scrolled, flung) = (inspected.pre_scroll.get(), inspected.post_fling.get());
        if orientation == Orientation::Vertical {
            assert!(scrolled.x == 0.0 && scrolled.y != 0.0, "{scrolled:?}");
            assert!(flung.x == 0.0 && flung.y != 0.0, "{flung:?}");
        } else {
            assert!(scrolled.y == 0.0 && scrolled.x != 0.0, "{scrolled:?}");
            assert!(flung.y == 0.0 && flung.x != 0.0, "{flung:?}");
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, overscrollEffect_withoutVisualEffect_preDrag, overscrollEffect_withoutVisualEffect_preFling
#[test]
fn without_its_visual_effect_it_handles_the_events_and_draws_nothing() {
    let effect = Effect::taking_a_tenth();
    let noted = Rc::new(Cell::new(0.0));
    let (state, total) = counting(all());
    let mut setup = Setup::new(state, without_visual_effect(effect.clone()));
    setup.fling = Some(Rc::new(Noted(noted.clone())));
    let mut ui = setup.rule();
    drag_and_rest(&mut ui);
    assert!((total.get().abs() - (1000.0 - SLOP) * 0.9).abs() < 0.1, "{}", total.get());
    assert_eq!(effect.source.get(), Some(NestedScrollSource::UserInput));
    flick(&mut ui, 3000.0);
    assert!((noted.get().abs() - 2700.0).abs() < 0.1, "{}", noted.get());
    assert_eq!(effect.placed.get(), 0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, overscrollEffect_withoutEventHandling_drag, overscrollEffect_withoutEventHandling_fling
#[test]
fn without_its_events_it_draws_and_hears_nothing() {
    let effect = Effect::taking_a_tenth();
    let noted = Rc::new(Cell::new(0.0));
    let (state, total) = counting(all());
    let mut setup = Setup::new(state, without_event_handling(effect.clone()));
    setup.fling = Some(Rc::new(Noted(noted.clone())));
    let mut ui = setup.rule();
    assert_eq!(effect.placed.get(), 1);
    drag_and_rest(&mut ui);
    assert!((total.get().abs() - (1000.0 - SLOP)).abs() < 0.1, "{}", total.get());
    assert_eq!(effect.delta.get(), Offset::ZERO);
    assert_eq!(effect.source.get(), None);
    flick(&mut ui, 3000.0);
    assert!((noted.get().abs() - 3000.0).abs() < 0.1, "{}", noted.get());
    assert_eq!(effect.pre_fling.get(), Velocity::ZERO);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, testOverscrollCallbacks_verticalSwipeUp_shouldTriggerCallbacks
#[test]
fn a_swipe_calls_it_for_the_scroll_and_the_fling() {
    let effect = Rc::new(Effect::default());
    let (state, _) = counting(all());
    let mut setup = Setup::new(state, effect.clone());
    setup.orientation = Orientation::Vertical;
    let mut ui = setup.rule();
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(150.0, 290.0), Offset::new(150.0, 150.0), 200);
    });
    ui.wait_for_idle();
    assert!(effect.applied_to_scroll.get() > 0);
    assert!(effect.applied_to_fling.get() > 0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt, testOverscrollCallbacks_verticalScrollMouse_shouldNotTriggerCallbacks
#[test]
fn a_wheel_doesnt_call_it() {
    let effect = Rc::new(Effect::default());
    let (state, total) = counting(all());
    let mut setup = Setup::new(state, effect.clone());
    setup.orientation = Orientation::Vertical;
    let mut ui = setup.rule();
    ui.on_root().perform_mouse_input(|mouse| {
        mouse.move_to(CENTER);
        mouse.scroll(100.0, ScrollWheel::Vertical);
    });
    ui.wait_for_idle();
    assert_ne!(total.get(), 0.0);
    assert_eq!(effect.applied_to_scroll.get(), 0);
    assert_eq!(effect.applied_to_fling.get(), 0);
}

/// A swipe that goes past the edge: `effect` is told of a scroll and a velocity left, the way of
/// `sign`.
fn assert_told_of_the_rest(ui: &mut ComposeTestRule, effect: &Effect, from: f32, to: f32) {
    assert_eq!(effect.placed.get(), 1, "drawn");
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(150.0, from), Offset::new(150.0, to), 200);
    });
    ui.wait_for_idle();
    let sign = (to - from).signum();
    assert!(effect.applied_to_scroll.get() > 1);
    assert_eq!(effect.applied_to_fling.get(), 1);
    assert!(effect.left.get().y * sign > 0.0, "{:?}", effect.left.get());
    assert!(effect.fling_left.get().y * sign > 0.0, "{:?}", effect.fling_left.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, customOverscroll
#[test]
fn a_scroll_modifier_draws_it_and_tells_it_what_is_past_the_end() {
    let effect = Rc::new(Effect::default());
    let given: Rc<dyn OverscrollEffect> = effect.clone();
    let state = ScrollState::new(0);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        Box!(
            modifier = Modifier.size(300.dp()).vertical_scroll_overscroll(
                handle,
                Some(given.clone()),
                true,
                None,
                false
            ),
            { Box!(modifier = Modifier.size(310.dp())) }
        );
    });
    ui.wait_for_idle();
    assert_told_of_the_rest(&mut ui, &effect, 290.0, 10.0);
    assert_eq!(state.value(), 10);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, customOverscroll
#[test]
fn a_list_draws_it_and_tells_it_what_is_past_the_start() {
    let effect = Rc::new(Effect::default());
    let given: Rc<dyn OverscrollEffect> = effect.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        LazyColumn!(
            modifier = Modifier.size(200.dp()),
            overscroll_effect = Some(given.clone()),
            content = |scope: LazyListScope| {
                scope.items(4, |_| Box!(modifier = Modifier.size(101.dp())));
            }
        );
    });
    ui.wait_for_idle();
    assert_told_of_the_rest(&mut ui, &effect, 10.0, 190.0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, customOverscroll
#[test]
fn a_grid_draws_it_and_tells_it_what_is_past_the_start() {
    let effect = Rc::new(Effect::default());
    let given: Rc<dyn OverscrollEffect> = effect.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.size(200.dp()),
            overscroll_effect = Some(given.clone()),
            content = |scope: LazyGridScope| {
                scope.items(4, |_| Box!(modifier = Modifier.size(101.dp())));
            }
        );
    });
    ui.wait_for_idle();
    assert_told_of_the_rest(&mut ui, &effect, 10.0, 190.0);
}

/// Takes all the velocity a fling leaves.
struct TakesThePostFling;

impl NestedScrollConnection for TakesThePostFling {
    fn on_post_fling(&self, _: Offset, available: Offset) -> LocalBoxFuture<'_, Offset> {
        Box::pin(async move { available })
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt, ScrollingLogic.onScrollStopped
#[test]
fn what_the_connections_take_of_a_fling_is_consumed_for_it() {
    let effect = Rc::new(Effect::default());
    let noted = Rc::new(Cell::new(0.0));
    let (state, _) = counting(all());
    let mut setup = Setup::new(state, effect.clone());
    setup.fling = Some(Rc::new(Noted(noted.clone())));
    setup.around = Some(Rc::new(TakesThePostFling));
    let mut ui = setup.rule();
    flick(&mut ui, 3000.0);
    assert!(noted.get().abs() > 2900.0, "{}", noted.get());
    assert_eq!(effect.fling_left.get(), Velocity::ZERO);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt, ScrollingLogic.onScrollStopped
#[test]
fn a_cancelled_drag_ends_on_it_at_no_velocity() {
    let effect = Rc::new(Effect::default());
    let (state, _) = counting(all());
    let mut ui = Setup::new(state, effect.clone()).rule();
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(100.0, 0.0));
        touch.cancel();
    });
    ui.wait_for_idle();
    assert_eq!(effect.applied_to_fling.get(), 1);
    assert_eq!(effect.pre_fling.get(), Velocity::ZERO);
    assert!(effect.fling_ended.get());
}

// An anchored drag: `A` at 0, `B` at 250, no slop; the effect given.
fn anchored(
    effect: Rc<Effect>,
    orientation: Orientation,
    positions: &[(u8, f32)],
) -> (ComposeTestRule, AnchoredDraggableState<u8>) {
    let positions = positions.to_vec();
    let state = AnchoredDraggableState::with_anchors(
        1,
        DraggableAnchors::new(|config| {
            for (anchor, position) in &positions {
                config.at(*anchor, *position);
            }
        }),
    );
    let given = state.clone();
    let as_effect: Rc<dyn OverscrollEffect> = effect;
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        Box!(
            modifier = Modifier.size(200.dp()).anchored_draggable(
                given.clone(),
                orientation,
                AnchoredDraggableOptions {
                    overscroll_effect: Some(as_effect.clone()),
                    ..AnchoredDraggableOptions::default()
                }
            )
        );
    });
    ui.wait_for_idle();
    ui.host().set_view_configuration(Some(compose_ui::ViewConfiguration {
        touch_slop: 0.0,
        ..compose_ui::ViewConfiguration::default()
    }));
    ui.advance_time_by_frame();
    (ui, state)
}

const TWO: &[(u8, f32)] = &[(1, 0.0), (2, 250.0)];

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableOverscrollTest.kt, anchoredDraggable_scrollOutOfBounds_haveDeltaForOverscroll, anchoredDraggable_scrollOutOfBounds_dispatchesToOverscroll_inOrientationOnly_horizontal, anchoredDraggable_scrollOutOfBounds_dispatchesToOverscroll_inOrientationOnly_vertical
#[test]
fn a_drag_past_the_anchors_leaves_the_effect_the_rest_on_its_axis() {
    for orientation in [Orientation::Horizontal, Orientation::Vertical] {
        let effect = Rc::new(Effect::default());
        let (mut ui, state) = anchored(effect.clone(), orientation, TWO);
        ui.on_root().perform_touch_input(|touch| {
            touch.down(Offset::new(1.0, 1.0));
            touch.move_by(Offset::new(350.0, 350.0));
            touch.up();
        });
        ui.wait_for_idle();
        assert_eq!(state.current_value(), 2);
        assert_eq!(state.require_offset(), 250.0);
        assert_eq!(effect.applied_to_scroll.get(), 1);
        let left = if orientation == Orientation::Horizontal {
            Offset::new(100.0, 0.0)
        } else {
            Offset::new(0.0, 100.0)
        };
        assert_eq!(effect.left.get(), left);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableOverscrollTest.kt, anchoredDraggable_swipeWithVelocity_haveVelocityForOverscroll_horizontal, anchoredDraggable_swipeWithVelocity_haveVelocityForOverscroll_vertical
#[test]
fn a_fling_ending_on_the_last_anchor_leaves_the_effect_its_velocity() {
    for orientation in [Orientation::Horizontal, Orientation::Vertical] {
        let effect = Rc::new(Effect::default());
        let (mut ui, state) = anchored(effect.clone(), orientation, TWO);
        let end = if orientation == Orientation::Horizontal {
            Offset::new(199.0, 1.0)
        } else {
            Offset::new(1.0, 199.0)
        };
        ui.on_root().perform_touch_input(|touch| {
            touch.swipe_with_velocity(Offset::new(1.0, 1.0), end, 4000.0, None);
        });
        ui.wait_for_idle();
        assert_eq!(state.current_value(), 2);
        assert_eq!(state.require_offset(), 250.0);
        assert_eq!(effect.applied_to_fling.get(), 1);
        let left = effect.fling_left.get();
        let (along, across) = if orientation == Orientation::Horizontal {
            (left.x, left.y)
        } else {
            (left.y, left.x)
        };
        assert!((along.abs() - 4000.0).abs() < 4000.0 * 0.05, "{left:?}");
        assert_eq!(across, 0.0);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableOverscrollTest.kt, anchoredDraggable_swipeWithVelocity_notAtBounds_noOverscroll
#[test]
fn a_fling_ending_between_the_anchors_leaves_it_nothing() {
    let effect = Rc::new(Effect::default());
    let three = [(1, 0.0), (2, 250.0), (3, 500.0)];
    let (mut ui, state) = anchored(effect.clone(), Orientation::Horizontal, &three);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(Offset::new(1.0, 1.0), Offset::new(138.0, 1.0), 4000.0, None);
    });
    ui.wait_for_idle();
    assert_eq!(state.current_value(), 2);
    assert_eq!(state.require_offset(), 250.0);
    assert_eq!(effect.applied_to_fling.get(), 1);
    assert!(effect.fling_left.get().x.abs() < 1.0, "{:?}", effect.fling_left.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableOverscrollTest.kt, anchoredDraggable_swipeWithVelocity_notEnoughVelocityForOverscroll
#[test]
fn a_release_without_velocity_leaves_it_nothing() {
    let effect = Rc::new(Effect::default());
    let (mut ui, state) = anchored(effect.clone(), Orientation::Horizontal, TWO);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(1.0, 1.0));
        touch.move_by(Offset::new(198.0, 0.0));
        touch.advance_event_time(3000);
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(state.current_value(), 2);
    assert_eq!(effect.applied_to_fling.get(), 1);
    assert!(effect.fling_left.get().x.abs() < 1.0, "{:?}", effect.fling_left.get());
}
