//! Adversarial review of the API round D (lazy list API, layout modifiers, layout callbacks,
//! z_index, combined_clickable, draw_with_cache, brushes).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Constraints, GraphicsLayerScope, IntSize, Layout, LayoutCoordinates, Offset, PointerEvent,
    Rect, UnitExt, measure_policy,
};

type Log = Rc<RefCell<Vec<String>>>;

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

fn click(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.wait_for_idle();
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
    ui.wait_for_idle();
}

// ---- z_index ---------------------------------------------------------------------------------

/// A node whose `z_index` goes away along with all its other modifiers keeps a z of `-0.0`
/// (the sum of no elements), which `total_cmp` sorts before its siblings' `0.0`: it's drawn
/// (and hit) under a sibling placed before it.
#[test]
fn a_node_losing_its_z_index_and_all_modifiers_keeps_its_placement_order() {
    let raised = mutable_state_of(true);
    let handle = raised.handle();
    let log: Log = Rc::default();
    let (under, over) = (log.clone(), log.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (under, over) = (under.clone(), over.clone());
        Box!({
            let (under, over) = (under.clone(), over.clone());
            Box!(
                modifier = Modifier
                    .size(50.dp())
                    .background(Color::BLUE)
                    .clickable(move || under.borrow_mut().push("first".into()))
            );
            // Placed second: over the first, whatever its z (1, then none).
            let modifier = if handle.get() { Modifier.z_index(1.0) } else { Modifier };
            Box!(modifier = modifier, {
                let over = over.clone();
                Box!(
                    modifier = Modifier
                        .size(50.dp())
                        .background(Color::RED)
                        .clickable(move || over.borrow_mut().push("second".into()))
                );
            });
        });
    });
    raised.set(false);
    ui.wait_for_idle();
    let display = ui.dump_display_list();
    let red = display.find("#FFFF0000").expect("red");
    let blue = display.find("#FF0000FF").expect("blue");
    assert!(red > blue, "the second sibling is drawn under the first:\n{display}");
    click(&mut ui, 10.0, 10.0);
    assert_eq!(*log.borrow(), vec!["second"]);
}

// ---- Layout callbacks ------------------------------------------------------------------------

/// An intrinsic probe (`max_intrinsic_height`, which measures to ask: Material's tabs and
/// dialogs use it) runs `on_size_changed`'s measure with the probe's constraints: the callback
/// hears the probe's size, then the real one, at every measurement of the parent. Compose's
/// intrinsics don't measure, so it hears the real size once.
#[test]
fn on_size_changed_is_not_told_an_intrinsic_probes_size() {
    let log: Log = Rc::default();
    let tick = mutable_state_of(0);
    let (seen, ticks) = (log.clone(), tick.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let seen = seen.clone();
        Layout!(
            measure_policy = measure_policy(move |scope, measurables, _constraints| {
                // Measured again when `tick` changes.
                let _ = ticks.get();
                let child = measurables[0];
                let _ = scope.max_intrinsic_height(child, 40);
                let placeable = scope.measure(child, Constraints::fixed(60, 30));
                scope.place(&placeable, 0, 0);
                scope.layout(60, 30)
            }),
            {
                let seen = seen.clone();
                Box!(
                    modifier = Modifier.on_size_changed(move |size: IntSize| {
                        seen.borrow_mut().push(format!("{}x{}", size.width, size.height))
                    }),
                    {
                        Box!(modifier = Modifier.size(10.dp()));
                    }
                );
            }
        );
    });
    assert_eq!(*log.borrow(), vec!["60x30"]);
    tick.set(1);
    ui.wait_for_idle();
    // The size never changed.
    assert_eq!(*log.borrow(), vec!["60x30"]);
}

/// D172 says what a layout callback writes "recomposes and lays out within the frame"; a write
/// read only while measuring (not composing) is left to the next frame: the host's rounds after
/// the callbacks only look at the composition's invalidations.
#[test]
fn a_size_written_by_on_size_changed_and_read_by_a_measurement_is_laid_out_in_the_same_frame() {
    let width = mutable_state_of(10.0f32);
    let handle = width.handle();
    let measured = mutable_state_of(0);
    let follow = measured.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let follower = remember(|| {
            Modifier.layout(move |scope, measurable, _constraints: Constraints| {
                let width = follow.get();
                let placeable = scope.measure(measurable, Constraints::fixed(width, 10));
                scope.place(&placeable, 0, 0);
                scope.layout(width, 10)
            })
        });
        Column!({
            Box!(
                modifier = Modifier
                    .size((handle.get().dp(), 10.dp()))
                    .on_size_changed(move |size: IntSize| follow.set(size.width))
            );
            Box!(modifier = follower.clone().test_tag("follower"));
        });
    });
    assert_eq!(bounds(&mut ui, "follower").width(), 10.0);
    width.set(30.0);
    ui.host().frame();
    // All laid out in that frame: the next one has nothing to measure.
    let next = ui.host().frame();
    assert!(!next.measured, "the follower was measured a frame late");
}

/// Compose's `positionInRoot` (and `onGloballyPositioned`) include the graphics layers'
/// transforms (`translationX`); the snapshot here only sums the layout offsets.
#[test]
fn on_globally_positioned_includes_a_graphics_layer_translation() {
    let log: Rc<RefCell<Vec<LayoutCoordinates>>> = Rc::default();
    let seen = log.clone();
    let _ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.graphics_layer_with(GraphicsLayerScope {
                translation_x: 40.0,
                ..GraphicsLayerScope::default()
            }),
            {
                let seen = seen.clone();
                Box!(
                    modifier = Modifier
                        .on_globally_positioned(move |coordinates: LayoutCoordinates| {
                            seen.borrow_mut().push(coordinates)
                        })
                        .size(20.dp())
                );
            }
        );
    });
    let last = *log.borrow().last().expect("positioned");
    assert_eq!(last.position_in_root(), Offset::new(40.0, 0.0));
}

// ---- Lazy lists ------------------------------------------------------------------------------

/// A user key that equals the hashed default key of an unkeyed item (`("index", i)`) shares its
/// state in release builds, and panics as a duplicate in debug ones. Compose's default keys are
/// a private type no user key equals.
#[test]
fn a_user_key_never_equals_an_unkeyed_items_default_key() {
    let _ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            scope.items(2, |_| {
                Box!(modifier = Modifier.size(10.dp()));
            });
            scope.items_keyed(
                2,
                |index| ("index", index),
                |_| {
                    Box!(modifier = Modifier.size(10.dp()));
                },
            );
        });
    });
}

#[composable]
fn ReversedSections(state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        reverse_layout = true,
        |scope: LazyListScope| {
            for section in ["A", "B"] {
                scope.sticky_header(move |_| {
                    Box!(modifier = Modifier.size((100.dp(), 20.dp())).test_tag(section));
                });
                scope.items(10, move |index| {
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), 30.dp()))
                            .test_tag(format!("{section}{index}"))
                    );
                });
            }
        }
    );
}

/// In a reversed list the start is the bottom: a sticky header sticks there.
#[test]
fn a_sticky_header_of_a_reversed_list_sticks_to_the_bottom() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || ReversedSections(given));
    assert_eq!(bounds(&mut ui, "A").bottom, 100.0);
    state.request_scroll_to_item(5, 10);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "A").bottom, 100.0);
    // B's header 10 px from the bottom pushes A's down.
    state.request_scroll_to_item(10, 20);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "B").bottom, 90.0);
    assert_eq!(bounds(&mut ui, "A").bottom, 110.0);
}

/// A sticky header kept across prefetching: idle prefetch between frames recomposes the items
/// with a new range, and the header (outside it) must stay composed and drawn.
#[test]
fn a_sticky_header_survives_prefetch_between_frames() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.sticky_header(|_| {
                    Box!(modifier = Modifier.size((100.dp(), 20.dp())).test_tag("header"));
                });
                scope.items(50, |index| {
                    Box!(
                        modifier = Modifier.size((100.dp(), 30.dp())).test_tag(format!("{index}"))
                    );
                });
            }
        );
    });
    for _ in 0..6 {
        ui.host()
            .send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 45.0)));
        ui.wait_for_idle();
        ui.host().prefetch(std::time::Instant::now() + Duration::from_millis(50));
        ui.wait_for_idle();
        assert_eq!(bounds(&mut ui, "header").top, 0.0);
    }
    assert!(state.first_visible_item_index() > 3);
}

// ---- combined_clickable ----------------------------------------------------------------------

/// A press in a scroll that drags past the slop is the scroll's: no long click once held.
#[test]
fn a_combined_clickable_in_a_scroll_doesnt_long_click_once_dragged() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        Column!(
            modifier =
                Modifier.size((100.dp(), 100.dp())).vertical_scroll(remember_scroll_state!()),
            {
                let (click, long) = (seen.clone(), seen.clone());
                Box!(
                    modifier = Modifier.size((100.dp(), 300.dp())).combined_clickable(
                        Some(callback(move || long.borrow_mut().push("long".into()))),
                        None,
                        move || click.borrow_mut().push("click".into())
                    )
                );
            }
        );
    });
    let host = ui.host();
    host.send_pointer(
        PointerEvent::press(Offset::new(50.0, 80.0))
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    for y in [70.0, 60.0, 50.0, 40.0] {
        host.send_pointer(
            PointerEvent::moved(Offset::new(50.0, y), true)
                .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
        );
    }
    ui.advance_time_by(Duration::from_millis(600));
    ui.host().send_pointer(
        PointerEvent::release(Offset::new(50.0, 40.0))
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    ui.advance_time_by(Duration::from_millis(400));
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
}

// ---- draw_with_cache -------------------------------------------------------------------------

/// The cache builds once per size, not at each draw: a redraw of what the drawing (not the
/// build) reads keeps it.
#[test]
fn draw_with_cache_keeps_its_cache_over_draws_reading_other_state() {
    let builds = Rc::new(Cell::new(0));
    let color = mutable_state_of(Color::RED);
    let (counted, read) = (builds.clone(), color.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let counted = counted.clone();
        let modifier = remember(move || {
            let counted = counted.clone();
            Modifier.size(20.dp()).draw_with_cache(move |cache| {
                counted.set(counted.get() + 1);
                let size = cache.size();
                cache.on_draw_behind(move |scope| {
                    scope.draw_rect(
                        Rect::new(0.0, 0.0, size.width, size.height),
                        compose_ui::Paint::fill(read.get()),
                    );
                })
            })
        });
        Box!(modifier = modifier.clone());
    });
    assert_eq!(builds.get(), 1);
    color.set(Color::BLUE);
    ui.wait_for_idle();
    assert!(ui.dump_display_list().contains("#FF0000FF"), "{}", ui.dump_display_list());
    assert_eq!(builds.get(), 1);
}
