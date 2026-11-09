//! Review of Navigation 3's completion (D2170, D2171): an entry's movable content across pops,
//! moves, gestures and re-decoration, the shared-entry decorator around it, and nested displays.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::{SharedTransitionLayout, SharedTransitionScope};
use compose_foundation::BasicText;
use compose_navigation::*;
use compose_navigation_event::NavigationEvent;
use compose_runtime::snapshots::mutable_state_list_from;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{BackEventCompat, DialogProperties, IntSize, Rect};

use crate::nav_support::*;

const DIALOG: &str = "dialog";

/// How often each probe was made and disposed, and its last remembered number.
#[derive(Default)]
struct Probes {
    made: HashMap<String, u32>,
    disposed: HashMap<String, u32>,
    states: HashMap<String, MutableState<i32>>,
}

thread_local! {
    static PROBES: RefCell<Probes> = RefCell::default();
}

fn reset() {
    PROBES.with_borrow_mut(|probes| *probes = Probes::default());
}

fn made(name: &str) -> u32 {
    PROBES.with_borrow(|probes| probes.made.get(name).copied().unwrap_or(0))
}

fn disposed(name: &str) -> u32 {
    PROBES.with_borrow(|probes| probes.disposed.get(name).copied().unwrap_or(0))
}

fn state(name: &str) -> MutableState<i32> {
    PROBES.with_borrow(|probes| *probes.states.get(name).expect("a probe of that name"))
}

/// A number kept by plain `remember` (no saved state to hide a lost slot), shown as `name: n`.
#[composable]
fn Probe(name: String) {
    let made_name = name.clone();
    let count = remember(move || {
        PROBES.with_borrow_mut(|probes| *probes.made.entry(made_name).or_default() += 1);
        mutable_state_of(0)
    });
    let keyed = name.clone();
    PROBES.with_borrow_mut(|probes| {
        probes.states.insert(keyed, count);
    });
    let gone = name.clone();
    DisposableEffect((), move |scope| {
        scope.on_dispose(move || {
            PROBES.with_borrow_mut(|probes| *probes.disposed.entry(gone).or_default() += 1);
        })
    });
    BasicText!("{name}: {}", count.get());
}

fn probe_entry(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |key| Probe!(key.to_string()))
}

/// `FIRST` a list, `SECOND` and `THIRD` details, `DIALOG` a dialog, all probes.
fn list_detail_probe_entry(key: &'static str) -> NavEntry<&'static str> {
    let entry = probe_entry(key);
    match key {
        FIRST => entry.with_metadata(ListDetailScene::list_pane()),
        SECOND | THIRD => entry.with_metadata(ListDetailScene::detail_pane()),
        DIALOG => entry.with_metadata(DialogSceneStrategy::dialog(DialogProperties::default())),
        other => panic!("Invalid key passed: {other}"),
    }
}

fn event(progress: f32) -> BackEventCompat {
    BackEventCompat {
        swipe_edge: NavigationEvent::EDGE_LEFT,
        progress,
        touch_x: 0.1,
        touch_y: 0.1,
        ..Default::default()
    }
}

fn frames(ui: &mut ComposeTestRule, millis: u64) {
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(millis));
}

/// An entry popped, then pushed back while its exit still shows it, is the same content: Compose's
/// `remember { movableContentMap.getOrPut(contentKey) }` site is still composed, `onPop` not called.
#[test]
fn an_entry_pushed_back_while_still_leaving_keeps_its_state() {
    reset();
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || NavDisplay!(stack, entry_provider = probe_entry));
    ui.wait_for_idle();
    state(SECOND).set(5);
    ui.wait_for_idle();

    ui.set_auto_advance(false);
    stack.remove(stack.len() - 1);
    frames(&mut ui, 200);
    ui.on_node_with_text("second: 5").assert_exists();
    stack.push(SECOND);
    frames(&mut ui, 100);
    ui.set_auto_advance(true);
    ui.wait_for_idle();

    ui.on_node_with_text("second: 5").assert_is_displayed();
    assert_eq!((made(SECOND), disposed(SECOND)), (1, 0));
}

/// Popped and gone, an entry's movable content is evicted: pushed again, it starts anew.
#[test]
fn an_entry_pushed_back_after_leaving_starts_anew() {
    reset();
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || NavDisplay!(stack, entry_provider = probe_entry));
    ui.wait_for_idle();
    state(SECOND).set(5);
    ui.wait_for_idle();

    stack.remove(stack.len() - 1);
    ui.wait_for_idle();
    ui.on_node_with_text("second: 5").assert_does_not_exist();
    assert_eq!(disposed(SECOND), 1);

    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text("second: 0").assert_is_displayed();
    assert_eq!((made(SECOND), disposed(SECOND)), (2, 1));
}

fn list_detail_probes(
    initial: &[&'static str],
    shared: bool,
) -> (ComposeTestRule, compose_runtime::snapshots::OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from(initial.to_vec());
    let stack = owner.handle();
    let ui = ComposeTestRule::sized(IntSize::new(1280, 800), move || {
        SharedTransitionLayout!(move |scope: SharedTransitionScope| {
            let back_stack = stack;
            let list_detail = remember_list_detail_scene_strategy();
            NavDisplay!(
                back_stack,
                on_back = Some(move || {
                    back_stack.remove(back_stack.len() - 1);
                }),
                scene_strategies = vec![scene_strategy(DialogSceneStrategy), list_detail],
                shared_transition_scope = if shared { Some(scope) } else { None },
                entry_provider = list_detail_probe_entry
            );
        });
    });
    (ui, owner)
}

/// The list entry stays in both scenes of a predictive back (`entriesToExclude` moves it to the
/// destination at the gesture's start): cancelled, it moves back; committed, it stays, its state,
/// and effects one throughout, the detail's kept until it's popped.
fn gesture_cancelled_then_committed(shared: bool) {
    reset();
    let (mut ui, stack) = list_detail_probes(&[FIRST, SECOND], shared);
    ui.wait_for_idle();
    state(FIRST).set(3);
    state(SECOND).set(4);
    ui.wait_for_idle();

    for _ in 0..2 {
        ui.host().on_back_started(event(0.1));
        ui.host().on_back_progressed(event(0.5));
        ui.wait_for_idle();
        ui.on_node_with_text("first: 3").assert_is_displayed();
        ui.on_node_with_text("second: 4").assert_exists();
        ui.host().on_back_cancelled();
        ui.wait_for_idle();
        ui.on_node_with_text("first: 3").assert_is_displayed();
        ui.on_node_with_text("second: 4").assert_is_displayed();
        assert_eq!(stack.len(), 2);
    }
    if shared {
        let first = ui.on_node_with_text("first: 3").fetch_semantics_node().bounds_in_root();
        let second = ui.on_node_with_text("second: 4").fetch_semantics_node().bounds_in_root();
        assert_eq!((first.left, first.top), (0.0, 0.0), "back in the list pane: {first:?}");
        assert_eq!((second.left, second.top), (512.0, 0.0), "back in the detail pane: {second:?}");
    }
    assert_eq!((made(FIRST), disposed(FIRST)), (1, 0));
    assert_eq!((made(SECOND), disposed(SECOND)), (1, 0));

    ui.host().on_back_started(event(0.1));
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    assert_eq!(stack.len(), 1);
    ui.on_node_with_text("first: 3").assert_is_displayed();
    ui.on_node_with_text("second: 4").assert_does_not_exist();
    if shared {
        let first = ui.on_node_with_text("first: 3").fetch_semantics_node().bounds_in_root();
        assert_eq!((first.left, first.top), (0.0, 0.0));
    }
    assert_eq!((made(FIRST), disposed(FIRST)), (1, 0));
    assert_eq!((made(SECOND), disposed(SECOND)), (1, 1));
}

#[test]
fn a_shared_entry_survives_a_gesture_cancelled_then_committed() {
    gesture_cancelled_then_committed(false);
}

#[test]
fn a_shared_entry_survives_a_gesture_cancelled_then_committed_with_a_shared_scope() {
    gesture_cancelled_then_committed(true);
}

/// Pushing and popping a detail faster than the transitions run moves the list entry between the
/// single pane and the list-detail scene mid-animation, keeping it.
fn rapid_back_and_forth(shared: bool) {
    reset();
    let (mut ui, stack) = list_detail_probes(&[FIRST], shared);
    ui.wait_for_idle();
    state(FIRST).set(9);
    ui.wait_for_idle();

    ui.set_auto_advance(false);
    for _ in 0..3 {
        stack.push(SECOND);
        frames(&mut ui, 120);
        stack.remove(stack.len() - 1);
        frames(&mut ui, 120);
    }
    stack.push(SECOND);
    frames(&mut ui, 60);
    ui.set_auto_advance(true);
    ui.wait_for_idle();

    ui.on_node_with_text("first: 9").assert_is_displayed();
    ui.on_node_with_text("second: 0").assert_is_displayed();
    assert_eq!((made(FIRST), disposed(FIRST)), (1, 0));
    if shared {
        let first = ui.on_node_with_text("first: 9").fetch_semantics_node().bounds_in_root();
        assert_eq!((first.left, first.top), (0.0, 0.0));
    }

    stack.remove(stack.len() - 1);
    ui.wait_for_idle();
    ui.on_node_with_text("first: 9").assert_is_displayed();
    assert_eq!((made(FIRST), disposed(FIRST)), (1, 0));
}

#[test]
fn an_entry_moved_back_and_forth_mid_transition_keeps_its_state() {
    rapid_back_and_forth(false);
}

#[test]
fn an_entry_moved_back_and_forth_mid_transition_keeps_its_state_with_a_shared_scope() {
    rapid_back_and_forth(true);
}

/// An entry re-provided with the same content key but new content shows the newest content, in
/// place and when it moves to another scene in the same pass, its remembered state kept.
#[test]
fn a_re_decorated_entry_shows_its_latest_content_where_it_moves() {
    reset();
    let label = mutable_state_of("one");
    let label_read = label.handle();
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let text = label_read.get();
        let entries: Vec<NavEntry<&'static str>> = stack
            .with(Vec::clone)
            .into_iter()
            .map(|key| {
                if key == FIRST {
                    NavEntry::new(FIRST, move |_| Probe!(format!("{FIRST}-{text}")))
                        .with_metadata(ListDetailScene::list_pane())
                } else {
                    list_detail_probe_entry(key)
                }
            })
            .collect();
        let list_detail = remember_list_detail_scene_strategy();
        NavDisplayOfEntries!(entries, scene_strategies = vec![list_detail], on_back = || {});
    });
    ui.wait_for_idle();
    ui.on_node_with_text("first-one: 0").assert_is_displayed();

    label.handle().set("two");
    ui.wait_for_idle();
    ui.on_node_with_text("first-two: 0").assert_is_displayed();
    ui.on_node_with_text("first-one: 0").assert_does_not_exist();

    // Re-decorated and moved into the list-detail scene in one pass.
    label.handle().set("three");
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text("first-three: 0").assert_is_displayed();
    ui.on_node_with_text("second: 0").assert_is_displayed();
    ui.on_node_with_text("first-two: 0").assert_does_not_exist();

    // And back.
    label.handle().set("four");
    stack.remove(stack.len() - 1);
    ui.wait_for_idle();
    ui.on_node_with_text("first-four: 0").assert_is_displayed();
}

/// A display in an entry of a display, both with the shared scope, moving entries in both and
/// going back by a gesture in the outer one: no `RefCell` is borrowed across what an entry
/// composes. `distinct` gives the inner entries content keys of their own; without, the inner
/// `FIRST` and the outer one are shared elements of one key, as they would be in AndroidX.
fn nested(distinct: bool) {
    reset();
    let outer_owner = mutable_state_list_from([FIRST]);
    let inner_owner = mutable_state_list_from([FIRST]);
    let (outer, inner) = (outer_owner.handle(), inner_owner.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(1280, 800), move || {
        SharedTransitionLayout!(move |scope: SharedTransitionScope| {
            let back_stack = outer;
            let list_detail = remember_list_detail_scene_strategy();
            let inner_scope = scope.clone();
            NavDisplay!(
                back_stack,
                on_back = Some(move || {
                    back_stack.remove(back_stack.len() - 1);
                }),
                scene_strategies = vec![list_detail],
                shared_transition_scope = Some(scope),
                entry_provider = move |key: &'static str| {
                    if key == FIRST {
                        let scope = inner_scope.clone();
                        NavEntry::new(FIRST, move |_| {
                            let inner_list_detail = remember_list_detail_scene_strategy();
                            NavDisplay!(
                                inner,
                                scene_strategies = vec![inner_list_detail],
                                shared_transition_scope = Some(scope.clone()),
                                entry_provider = move |key: &'static str| {
                                    let entry = if distinct {
                                        NavEntry::keyed(key, format!("inner-{key}"), move |key| {
                                            Probe!(format!("inner-{key}"))
                                        })
                                    } else {
                                        NavEntry::new(key, move |key| {
                                            Probe!(format!("inner-{key}"))
                                        })
                                    };
                                    match key {
                                        FIRST => entry.with_metadata(ListDetailScene::list_pane()),
                                        _ => entry.with_metadata(ListDetailScene::detail_pane()),
                                    }
                                }
                            );
                        })
                        .with_metadata(ListDetailScene::list_pane())
                    } else {
                        list_detail_probe_entry(key)
                    }
                }
            );
        });
    });
    ui.wait_for_idle();
    state("inner-first").set(1);
    ui.wait_for_idle();
    inner.push(SECOND);
    ui.wait_for_idle();
    outer.push(SECOND);
    ui.wait_for_idle();
    inner.push(THIRD);
    ui.wait_for_idle();
    ui.on_node_with_text("inner-first: 1").assert_is_displayed();
    ui.on_node_with_text("inner-third: 0").assert_is_displayed();
    ui.on_node_with_text("second: 0").assert_is_displayed();

    ui.host().on_back_started(event(0.1));
    ui.host().on_back_progressed(event(0.4));
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    ui.on_node_with_text("inner-first: 1").assert_is_displayed();
    assert_eq!((made("inner-first"), disposed("inner-first")), (1, 0));
}

#[test]
fn a_nested_display_with_the_same_shared_scope_composes() {
    nested(true);
}

/// The same with the inner and outer `FIRST` sharing a content key, so a shared-element key: two
/// visible shared elements of one key in a transition, Compose's "error case" (`SharedElement`'s
/// `_allEntries` doc: no single target, nothing animates), here never settles. compose-animation's,
/// as [`two_visible_shared_elements_of_one_key_settle`] shows without a display.
#[test]
#[ignore = "review: two visible shared elements of one key never settle (compose-animation)"]
fn a_nested_display_with_the_same_shared_scope_and_keys_settles() {
    nested(false);
}

/// A dialog over a list-detail scene with the shared scope: the dialog's entry isn't a shared
/// element (it's in an overlay), the panes are, and both panes keep their state through the
/// dialog's coming and going, and the list through the detail's pop.
#[test]
fn a_dialog_over_a_list_detail_scene_with_a_shared_scope() {
    reset();
    let (mut ui, stack) = list_detail_probes(&[FIRST, SECOND], true);
    ui.wait_for_idle();
    state(FIRST).set(1);
    state(SECOND).set(2);
    ui.wait_for_idle();

    stack.push(DIALOG);
    ui.wait_for_idle();
    ui.on_node_with_text("first: 1").assert_is_displayed();
    ui.on_node_with_text("second: 2").assert_is_displayed();
    ui.on_node_with_text("dialog: 0").assert_is_displayed();
    let first = ui.on_node_with_text("first: 1").fetch_semantics_node().bounds_in_root();
    assert_eq!((first.left, first.top), (0.0, 0.0));
    let second = ui.on_node_with_text("second: 2").fetch_semantics_node().bounds_in_root();
    assert_eq!((second.left, second.top), (512.0, 0.0));

    ui.host().on_back_pressed();
    ui.wait_for_idle();
    ui.on_node_with_text("dialog: 0").assert_does_not_exist();
    ui.on_node_with_text("second: 2").assert_is_displayed();

    ui.host().on_back_pressed();
    ui.wait_for_idle();
    ui.on_node_with_text("first: 1").assert_is_displayed();
    let first = ui.on_node_with_text("first: 1").fetch_semantics_node().bounds_in_root();
    assert_eq!((first.left, first.top), (0.0, 0.0));
    assert_eq!((made(FIRST), disposed(FIRST)), (1, 0));
    assert_eq!((made(SECOND), disposed(SECOND)), (1, 1));
    assert_eq!((made(DIALOG), disposed(DIALOG)), (1, 1));
}

/// The scope switching between `None` and `Some` puts the shared-entry decorator in and out
/// around the movable content: Compose's movable content moves into the new `Box` and out, the
/// entries' state and effects kept, as they're kept when moving between scenes.
#[test]
fn switching_the_shared_scope_on_and_off_keeps_entries() {
    reset();
    let shared = mutable_state_of(false);
    let shared_read = shared.handle();
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let seen: Rc<Cell<bool>> = Rc::default();
    let seen_in = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1280, 800), move || {
        let seen = seen_in.clone();
        SharedTransitionLayout!(move |scope: SharedTransitionScope| {
            let use_scope = shared_read.get();
            seen.set(use_scope);
            let list_detail = remember_list_detail_scene_strategy();
            NavDisplay!(
                stack,
                scene_strategies = vec![list_detail],
                shared_transition_scope = if use_scope { Some(scope) } else { None },
                entry_provider = list_detail_probe_entry
            );
        });
    });
    ui.wait_for_idle();
    state(FIRST).set(1);
    state(SECOND).set(2);
    ui.wait_for_idle();

    for on in [true, false, true] {
        shared.handle().set(on);
        ui.wait_for_idle();
        assert_eq!(seen.get(), on);
        ui.on_node_with_text("first: 1").assert_is_displayed();
        ui.on_node_with_text("second: 2").assert_is_displayed();
        let first = ui.on_node_with_text("first: 1").fetch_semantics_node().bounds_in_root();
        assert_eq!((first.left, first.top), (0.0, 0.0), "{first:?}");
    }
    assert_eq!((made(FIRST), disposed(FIRST)), (1, 0));
    assert_eq!((made(SECOND), disposed(SECOND)), (1, 0));

    // And still moves with the scope on.
    stack.remove(stack.len() - 1);
    ui.wait_for_idle();
    ui.on_node_with_text("first: 1").assert_is_displayed();
    let first = ui.on_node_with_text("first: 1").fetch_semantics_node().bounds_in_root();
    assert_eq!(first, Rect::new(0.0, 0.0, first.width(), first.height()));
    assert_eq!((made(FIRST), disposed(FIRST)), (1, 0));
}

/// `remember_scene_state` used by hand, the current and previous scenes both shown with nothing
/// excluded: one movable content called twice in a pass, which Compose composes as two instances.
#[test]
fn a_scene_state_shown_twice_by_hand_composes_both() {
    reset();
    let mut ui = compose_test(|| {
        let entries =
            remember_decorated_nav_entries!(vec![FIRST, SECOND], entry_provider = probe_entry);
        let state = remember_scene_state!(
            entries,
            vec![scene_strategy(SinglePaneSceneStrategy)],
            on_back = || {}
        );
        state.current_scene().content();
        for scene in state.previous_scenes() {
            scene.content();
        }
        // The same entry again.
        state.current_scene().content();
    });
    ui.wait_for_idle();
    ui.on_node_with_text("first: 0").assert_is_displayed();
    ui.on_all_nodes_with_text("second: 0").assert_count_equals(2);
}

/// Two shared elements of one key both visible, one of them changing content: Compose finds no
/// single target and animates neither, settling; compose-rs never settles.
#[test]
#[ignore = "review: two visible shared elements of one key never settle (compose-animation)"]
fn two_visible_shared_elements_of_one_key_settle() {
    use compose_animation::{AnimatedContent, AnimatedContentScope};
    use compose_foundation::layout::*;
    let flag = mutable_state_of(false);
    let flag_read = flag.handle();
    let mut ui = compose_test(move || {
        SharedTransitionLayout!(move |scope: SharedTransitionScope| {
            let (a, b) = (scope.clone(), scope.clone());
            Column!({
                let a = a.clone();
                AnimatedContent!(
                    0,
                    content = move |content: AnimatedContentScope, _: i32| {
                        let state = a.remember_shared_content_state(1);
                        BasicText!("fixed", modifier = a.shared_element(&state, &content));
                    }
                );
                let b = b.clone();
                AnimatedContent!(
                    flag_read.get(),
                    content = move |content: AnimatedContentScope, on: bool| {
                        let state = b.remember_shared_content_state(1);
                        BasicText!("moving {on}", modifier = b.shared_element(&state, &content));
                    }
                );
            });
        });
    });
    ui.wait_for_idle();
    flag.handle().set(true);
    ui.wait_for_idle();
    ui.on_node_with_text("moving true").assert_is_displayed();
    ui.on_node_with_text("fixed").assert_is_displayed();
}

/// Where each `DrawRect` of `color` lands on the screen, through the translations and transforms
/// around it (no rotations here).
fn drawn_rects(ops: &[compose_ui::platform::DrawOp], color: compose_ui::Color) -> Vec<Rect> {
    use compose_ui::platform::DrawOp;
    // `p -> (sx * p.x + tx, sy * p.y + ty)`.
    type Affine = (f32, f32, f32, f32);
    let then = |m: Affine, l: Affine| (m.0 * l.0, m.1 * l.1, m.0 * l.2 + m.2, m.1 * l.3 + m.3);
    let (mut current, mut saved): (Affine, Vec<Affine>) = ((1.0, 1.0, 0.0, 0.0), Vec::new());
    let mut rects = Vec::new();
    for op in ops {
        match op {
            DrawOp::Save
            | DrawOp::SaveLayerAlpha { .. }
            | DrawOp::SaveLayer { .. }
            | DrawOp::SaveLayerEffect { .. } => saved.push(current),
            DrawOp::Restore => current = saved.pop().expect("a save to restore"),
            DrawOp::Translate(offset) => current = then(current, (1.0, 1.0, offset.x, offset.y)),
            DrawOp::Transform { pivot, scale, rotation, translation } => {
                assert_eq!(*rotation, 0.0);
                let local = (
                    scale.0,
                    scale.1,
                    pivot.x * (1.0 - scale.0) + translation.x,
                    pivot.y * (1.0 - scale.1) + translation.y,
                );
                current = then(current, local);
            }
            DrawOp::DrawRect(rect, paint) if paint.color == color => {
                let (sx, sy, tx, ty) = current;
                rects.push(Rect::new(
                    sx * rect.left + tx,
                    sy * rect.top + ty,
                    sx * rect.right + tx,
                    sy * rect.bottom + ty,
                ));
            }
            _ => {}
        }
    }
    rects
}

/// `NavDisplayScreenshotTest.testSharedElementCorrectlyDisplayedOnPredictiveBack`'s golden, by
/// where things land: during the gesture the list entry, a shared element whose config doesn't
/// permit the deferred transform (`SharedEntryInSceneNavEntryDecorator`), is drawn where it was,
/// unscaled, while the detail pane, in the leaving scene, is drawn scaled by 0.94 around the
/// centre. `nav_predictive_back.rs`'s port checks the red rectangle's own coordinates and that some
/// transform scales by 0.94, both true with the transform permitted too.
#[test]
fn a_shared_entry_is_drawn_where_it_was_during_a_predictive_back() {
    use compose_foundation::layout::*;
    use compose_foundation::*;
    use compose_ui::{Alignment, Color, Modifier};

    const RED_FILL: Color = Color::from_rgba(255, 77, 77, 255);
    const BLUE_FILL: Color = Color::from_rgba(51, 51, 255, 255);

    #[composable]
    fn ColoredBox(text: &'static str, fill: Color) {
        Box!(
            modifier = Modifier.fill_max_size().background(fill),
            content_alignment = Alignment::CENTER,
            { BasicText!(text) }
        );
    }

    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(1280, 800), move || {
        SharedTransitionLayout!(move |scope: SharedTransitionScope| {
            let back_stack = stack;
            let list_detail = remember_list_detail_scene_strategy();
            NavDisplay!(
                back_stack,
                on_back = Some(move || {
                    back_stack.remove(back_stack.len() - 1);
                }),
                scene_strategies = vec![list_detail],
                shared_transition_scope = Some(scope),
                entry_provider = |key: &'static str| match key {
                    FIRST => NavEntry::new(FIRST, |_| ColoredBox!(FIRST, RED_FILL))
                        .with_metadata(ListDetailScene::list_pane()),
                    _ => NavEntry::new(SECOND, |_| ColoredBox!(SECOND, BLUE_FILL))
                        .with_metadata(ListDetailScene::detail_pane()),
                }
            );
        });
    });
    ui.wait_for_idle();
    let ops = ui.host().display_list().ops;
    assert_eq!(drawn_rects(&ops, RED_FILL), vec![Rect::new(0.0, 0.0, 512.0, 800.0)]);
    assert_eq!(drawn_rects(&ops, BLUE_FILL), vec![Rect::new(512.0, 0.0, 1280.0, 800.0)]);

    ui.host().on_back_started(event(0.1));
    ui.host().on_back_progressed(event(0.2));
    ui.wait_for_idle();
    let ops = ui.host().display_list().ops;
    assert_eq!(drawn_rects(&ops, RED_FILL), vec![Rect::new(0.0, 0.0, 512.0, 800.0)], "{ops:?}");
    let blue = drawn_rects(&ops, BLUE_FILL);
    assert_eq!(blue.len(), 1, "{blue:?}");
    let expected = Rect::new(
        640.0 - 128.0 * 0.94,
        400.0 - 400.0 * 0.94,
        640.0 + 640.0 * 0.94,
        400.0 + 400.0 * 0.94,
    );
    for (got, want) in [
        (blue[0].left, expected.left),
        (blue[0].top, expected.top),
        (blue[0].right, expected.right),
        (blue[0].bottom, expected.bottom),
    ] {
        assert!((got - want).abs() < 1.0, "{blue:?} against {expected:?}");
    }
}

/// An entry moving from a scene into a dialog isn't a shared element there (an overlay's entries
/// aren't wrapped, `currentScene !is OverlayScene`): nothing matches the box it leaves behind, and
/// no shared transition runs while the scene under the dialog changes.
#[test]
fn an_entry_moving_into_a_dialog_is_not_a_shared_element() {
    let as_dialog = mutable_state_of(false);
    let as_dialog_read = as_dialog.handle();
    let seen_scope: Rc<RefCell<Option<SharedTransitionScope>>> = Rc::default();
    let scope_in = seen_scope.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1280, 800), move || {
        let scope_out = scope_in.clone();
        SharedTransitionLayout!(move |scope: SharedTransitionScope| {
            *scope_out.borrow_mut() = Some(scope.clone());
            let dialog = as_dialog_read.get();
            let second = probe_entry(SECOND);
            let second = if dialog {
                second.with_metadata(DialogSceneStrategy::dialog(DialogProperties::default()))
            } else {
                second
            };
            NavDisplayOfEntries!(
                vec![probe_entry(FIRST), second],
                scene_strategies = vec![
                    scene_strategy(DialogSceneStrategy),
                    scene_strategy(SinglePaneSceneStrategy)
                ],
                shared_transition_scope = Some(scope),
                on_back = || {}
            );
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_text("second: 0").assert_is_displayed();

    ui.set_auto_advance(false);
    as_dialog.handle().set(true);
    let mut active = Vec::new();
    for _ in 0..12 {
        frames(&mut ui, 50);
        active.push(seen_scope.borrow().as_ref().unwrap().is_transition_active());
    }
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert!(active.iter().all(|active| !active), "{active:?}");
    ui.on_node_with_text("first: 0").assert_is_displayed();
    ui.on_node_with_text("second: 0").assert_is_displayed();
}

/// What moves with an entry is its own state and effects, not its lifecycle owner: the
/// back-stack-aware decorator's `rememberLifecycleOwner` is `remember(parent)`, and the parent is
/// the scene's owner, so an entry moving to another scene gets an owner anew and the old one is
/// destroyed, as in AndroidX (D2170 says the owner is kept; D2190 corrects it).
#[test]
fn an_entry_moving_to_another_scene_gets_a_new_lifecycle_owner() {
    use compose_lifecycle::{LOCAL_LIFECYCLE_OWNER, Lifecycle, LifecycleState};

    #[composable]
    fn OwnerProbe(seen: Rc<RefCell<Vec<Lifecycle>>>) {
        let owner = LOCAL_LIFECYCLE_OWNER.current();
        if !seen.borrow().contains(&owner) {
            seen.borrow_mut().push(owner);
        }
        Probe!(FIRST.to_string());
    }

    reset();
    let seen: Rc<RefCell<Vec<Lifecycle>>> = Rc::default();
    let seen_in = seen.clone();
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let list_detail = remember_list_detail_scene_strategy();
        let seen = seen_in.clone();
        NavDisplay!(
            stack,
            scene_strategies = vec![list_detail],
            entry_provider = move |key: &'static str| {
                if key == FIRST {
                    let seen = seen.clone();
                    NavEntry::new(FIRST, move |_| OwnerProbe!(seen.clone()))
                        .with_metadata(ListDetailScene::list_pane())
                } else {
                    list_detail_probe_entry(key)
                }
            }
        );
    });
    ui.wait_for_idle();
    state(FIRST).set(6);
    ui.wait_for_idle();
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0].current_state(), LifecycleState::Resumed);

    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text("first: 6").assert_is_displayed();
    assert_eq!((made(FIRST), disposed(FIRST)), (1, 0));
    let owners = seen.borrow().clone();
    assert_eq!(owners.len(), 2, "an owner anew in the list-detail scene");
    assert_eq!(owners[0].current_state(), LifecycleState::Destroyed);
    assert_eq!(owners[1].current_state(), LifecycleState::Resumed);
}
