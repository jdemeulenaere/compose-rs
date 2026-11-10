//! The shared-element tests of Compose's `LookaheadScopeTest`: `skip_to_lookahead_position`
//! keeps a content where the pass ahead put it in the `SharedTransitionLayout` while the shared
//! bounds around it move it, and only while it's enabled.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, IntOffset, IntSize, LayoutCoordinates, Offset, UnitExt};

/// A text of `size` sp, white, as the tests' `Text(…, fontSize, color = Color.White)`.
fn white_text(text: &'static str, size: f32) {
    BasicText!(
        text,
        style = TextStyle { font_size: size.sp(), color: Color::WHITE, ..TextStyle::DEFAULT }
    );
}

/// Notes each position in the root the content is placed at into `positions`.
fn noting_positions(positions: &Rc<RefCell<Vec<Offset>>>) -> Modifier {
    let positions = positions.clone();
    Modifier.on_globally_positioned(move |it: LayoutCoordinates| {
        positions.borrow_mut().push(it.position_in_root())
    })
}

/// Asserts each position in `positions` is within a pixel of the one before, on both axes.
#[track_caller]
fn assert_still(name: &str, positions: &[Offset]) {
    for pair in positions.windows(2) {
        let (before, after) = (pair[0], pair[1]);
        assert!(
            (before.x - after.x).abs() < 1.0,
            "{name}'s x changed: {} then {}, of {positions:?}",
            before.x,
            after.x
        );
        assert!(
            (before.y - after.y).abs() < 1.0,
            "{name}'s y changed: {} then {}, of {positions:?}",
            before.y,
            after.y
        );
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testSharedElementClipRevealWithskipToLookaheadPosition
/// A button's shared bounds revealing a column of texts, clipped to the bounds as they grow:
/// the column skipping to its position ahead stays there all through the transition, while the
/// bounds move from the button's to the column's.
#[test]
fn a_column_revealed_by_shared_bounds_stays_at_its_position_ahead() {
    let target = mutable_state_of(true);
    let positions: Rc<RefCell<Vec<Offset>>> = Rc::default();
    let (t, p) = (target.handle(), positions.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let p = p.clone();
        SharedTransitionLayout!(move |scope: SharedTransitionScope| {
            let (p, scope) = (p.clone(), scope.clone());
            AnimatedContent!(t.get(), move |content: AnimatedContentScope, shown: bool| {
                let (p, scope) = (p.clone(), scope.clone());
                if shown {
                    Box!(modifier = Modifier.fill_max_size(), move |box_scope| {
                        let clip = scope.remember_shared_content_state("clip");
                        Box!(
                            modifier = Modifier
                                .align(box_scope, Alignment::BOTTOM_CENTER)
                                .test_tag("toggle-button")
                                .then(scope.shared_bounds(&clip, &content))
                                .clickable(move || t.set(false))
                                .padding((24.dp(), 8.dp())),
                            move |_| BasicText!("Toggle State")
                        );
                    });
                } else {
                    let clip = scope.remember_shared_content_state("clip");
                    let options = SharedOptions {
                        resize_mode: ResizeMode::RemeasureToBounds,
                        ..Default::default()
                    };
                    Column!(
                        modifier = Modifier
                            .test_tag("shared-column")
                            .then(scope.shared_bounds_with(&clip, &content, options))
                            .clip_to_bounds()
                            .then(scope.skip_to_lookahead_position())
                            .then(noting_positions(&p))
                            .fill_max_size_with(0.5)
                            .background(Color::BLACK),
                        vertical_arrangement = Arrangement::SPACE_EVENLY,
                        horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
                        move |_| {
                            white_text("Hello", 80.0);
                            white_text("Shared", 80.0);
                            white_text("Clip", 80.0);
                            white_text("Bounds", 80.0);
                        }
                    );
                }
            });
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_text("Toggle State").assert_exists();

    ui.on_node_with_tag("toggle-button").perform_click();
    ui.wait_for_idle();

    ui.on_node_with_tag("shared-column").assert_exists();
    for text in ["Hello", "Shared", "Clip", "Bounds"] {
        ui.on_node_with_text(text).assert_exists();
    }
    let positions = positions.borrow();
    assert!(!positions.is_empty(), "the column was placed");
    assert_still("the column", &positions);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testMultipleSharedElementsWithskipToLookaheadPosition
/// Two shared bounds moving from a box's sides into a column, each skipping to its position
/// ahead, stay there all through the transition, as a shared element (the button) leaves.
#[test]
fn two_shared_bounds_skipping_to_their_positions_ahead_stay_there() {
    let target = mutable_state_of(true);
    let left: Rc<RefCell<Vec<Offset>>> = Rc::default();
    let right: Rc<RefCell<Vec<Offset>>> = Rc::default();
    let (t, l, r) = (target.handle(), left.clone(), right.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let (l, r) = (l.clone(), r.clone());
        SharedTransitionLayout!(
            modifier = Modifier.fill_max_size(),
            move |scope: SharedTransitionScope| {
                let (l, r, scope) = (l.clone(), r.clone(), scope.clone());
                AnimatedContent!(t.get(), move |content: AnimatedContentScope, shown: bool| {
                    let (l, r, scope) = (l.clone(), r.clone(), scope.clone());
                    if shown {
                        Box!(modifier = Modifier.fill_max_size(), move |box_scope| {
                            let left = scope.remember_shared_content_state("left");
                            Box!(
                                modifier = Modifier
                                    .align(box_scope, Alignment::CENTER_START)
                                    .test_tag("left-shared")
                                    .size(100.dp())
                                    .background(Color::RED)
                                    .then(scope.shared_bounds(&left, &content)),
                                content_alignment = Alignment::CENTER,
                                move |_| white_text("Left", 14.0)
                            );
                            let right = scope.remember_shared_content_state("right");
                            Box!(
                                modifier = Modifier
                                    .align(box_scope, Alignment::CENTER_END)
                                    .test_tag("right-shared")
                                    .size(100.dp())
                                    .background(Color::BLUE)
                                    .then(scope.shared_bounds(&right, &content)),
                                content_alignment = Alignment::CENTER,
                                move |_| white_text("Right", 14.0)
                            );
                            let toggle = scope.remember_shared_content_state("toggle-element");
                            Box!(
                                modifier = Modifier
                                    .align(box_scope, Alignment::BOTTOM_CENTER)
                                    .test_tag("multi-toggle")
                                    .then(scope.shared_element(&toggle, &content))
                                    .clickable(move || t.set(false))
                                    .padding((24.dp(), 8.dp())),
                                move |_| BasicText!("Toggle")
                            );
                        });
                    } else {
                        Column!(
                            modifier = Modifier.fill_max_size(),
                            horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
                            move |_| {
                                let left = scope.remember_shared_content_state("left");
                                Box!(
                                    modifier = Modifier
                                        .test_tag("left-anchored")
                                        .size(100.dp())
                                        .background(Color::RED)
                                        .then(scope.shared_bounds(&left, &content))
                                        .then(scope.skip_to_lookahead_position())
                                        .then(noting_positions(&l)),
                                    content_alignment = Alignment::CENTER,
                                    move |_| white_text("Left", 14.0)
                                );
                                Box!(
                                    modifier = Modifier
                                        .fill_max_width()
                                        .height(200.dp())
                                        .background(Color::GRAY),
                                    content_alignment = Alignment::CENTER,
                                    move |_| white_text("Content", 14.0)
                                );
                                let right = scope.remember_shared_content_state("right");
                                Box!(
                                    modifier = Modifier
                                        .test_tag("right-anchored")
                                        .size(100.dp())
                                        .background(Color::BLUE)
                                        .then(scope.shared_bounds(&right, &content))
                                        .then(scope.skip_to_lookahead_position())
                                        .then(noting_positions(&r)),
                                    content_alignment = Alignment::CENTER,
                                    move |_| white_text("Right", 14.0)
                                );
                            }
                        );
                    }
                });
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("left-shared").assert_exists();
    ui.on_node_with_tag("right-shared").assert_exists();

    ui.on_node_with_tag("multi-toggle").perform_click();
    ui.wait_for_idle();

    ui.on_node_with_tag("left-anchored").assert_exists();
    ui.on_node_with_tag("right-anchored").assert_exists();
    ui.on_node_with_text("Content").assert_exists();
    let (left, right) = (left.borrow(), right.borrow());
    assert!(!left.is_empty() && !right.is_empty(), "both were placed");
    assert_still("the left box", &left);
    assert_still("the right box", &right);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testConditionalskipToLookaheadPosition
/// An approach placing its content further than the pass ahead did: disabled,
/// `skip_to_lookahead_position_when` leaves the content where the approach put it; enabled, it
/// moves it back to its position ahead, as the approach's offset changes.
#[test]
fn skipping_to_the_position_ahead_only_while_enabled() {
    let is_enabled = mutable_state_of(false);
    let delta = mutable_state_of(10);
    let actual = Rc::new(Cell::new(Offset::ZERO));
    let expected_ahead = Rc::new(Cell::new(Offset::ZERO));
    let untempered = Rc::new(Cell::new(Offset::ZERO));
    let (e, d) = (is_enabled.handle(), delta.handle());
    let (a, x, u) = (actual.clone(), expected_ahead.clone(), untempered.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let (a, x, u) = (a.clone(), x.clone(), u.clone());
        SharedTransitionLayout!(move |scope: SharedTransitionScope| {
            let (a, x, u) = (a.clone(), x.clone(), u.clone());
            let lookahead = scope.lookahead_scope();
            let layout = scope.clone();
            Box!(
                modifier = Modifier
                    .fill_max_size()
                    .background(Color::GREEN)
                    .offset_with(move |_| IntOffset::new(d.get(), d.get()))
                    .approach_layout(
                        |_| true,
                        |_, _| true,
                        move |scope, measurable, constraints| {
                            let placeable = scope.measure(measurable, constraints);
                            scope.layout_with(placeable.width, placeable.height, move |at| {
                                at.place(&placeable, d.get(), d.get())
                            })
                        },
                    )
                    .on_globally_positioned(move |it: LayoutCoordinates| {
                        u.set(it.position_in_root())
                    })
                    .then(scope.skip_to_lookahead_position_when(move || e.get()))
                    .on_globally_positioned(move |it: LayoutCoordinates| {
                        a.set(it.position_in_root());
                        // `toLookaheadCoordinates().positionInRoot()`: its position ahead in the
                        // layout, which is placed ahead where it is.
                        let root = layout.root_for_test().expect("the layout is placed");
                        let ahead = lookahead.local_lookahead_position_of(root, it, Offset::ZERO);
                        x.set(root.position_in_root() + ahead);
                    }),
                content_alignment = Alignment::CENTER,
                move |_| white_text("Conditional Element", 14.0)
            );
        });
    });
    ui.wait_for_idle();

    for step in 1..=10 {
        delta.set(step * 10);
        ui.wait_for_idle();
        // Disabled: where the approach puts it.
        assert_eq!(untempered.get(), actual.get(), "at {}", step * 10);
        assert_ne!(expected_ahead.get(), actual.get(), "at {}", step * 10);
    }

    is_enabled.set(true);
    ui.wait_for_idle();

    for step in 1..=10 {
        delta.set(step * 10);
        ui.wait_for_idle();
        // Enabled: where the pass ahead put it.
        assert_ne!(untempered.get(), actual.get(), "at {}", step * 10);
        assert_eq!(expected_ahead.get(), actual.get(), "at {}", step * 10);
    }
}
