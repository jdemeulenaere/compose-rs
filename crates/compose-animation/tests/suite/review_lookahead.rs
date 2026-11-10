//! Adversarial review of `w2-lookahead`'s animation changes (`D2350`–`D2353`):
//! - `AnimatedContent` in a `LookaheadScope` with `BOTTOM_END` alignment, three-state mid-flight
//!   interruption, and lookahead-vs-approach child size divergence (`EnterExitMeasurePolicy`'s
//!   `has_lookahead`);
//! - `animate_content_size` moved via `movable_content_of` from inside a `LookaheadScope` to
//!   outside (`ContentSizeNode::on_attach` clearing `lookahead_size` and `lookahead_constraints`);
//! - `CapturedAnimatedVisibility` intrinsics (both lookahead and approach passes while visible
//!   and while exiting) and mid-exit interruption in a `LookaheadScope`.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, Constraints, IntOffset, IntSize, IntrinsicMeasurable, IntrinsicMeasureScope,
    IntrinsicSize, LayoutModifierNode, LookaheadScope, Measurable, MeasureResult, MeasureScope,
    Modifier, ModifierNode, ModifierNodeElement, Offset, UnitExt,
};

fn by_pass_size(ahead: IntSize, now: IntSize) -> Modifier {
    Modifier.layout(move |scope, measurable, _| {
        let size = if scope.is_looking_ahead() { ahead } else { now };
        let placeable = scope.measure(measurable, Constraints::fixed(size.width, size.height));
        scope.layout_with(size.width, size.height, move |placement| {
            placement.place(&placeable, 0, 0);
        })
    })
}

/// `AnimatedContent` in a `LookaheadScope` with `Alignment::BOTTOM_END` and a linear
/// `SizeTransform`:
/// 1. State `0` (`100 x 80`) -> State `1` (`40 x 30` ahead, `20 x 15` now):
///    - Ahead, container is immediately `(40, 30)`, State `1` is at `(0, 0)`, and leaving State `0`
///      (`100 x 80` aligned `BOTTOM_END` in `40 x 30`) is at `(-60, -50)`.
///    - Now, `SizeNode` animates toward State `1`'s lookahead `target_size` `(40, 30)` (not its
///      approach `(20, 15)`), so at 50 ms of 100 ms the container is `(70, 55)`.
/// 2. Interrupted at 50 ms to State `2` (`160 x 120`):
///    - Ahead, container jumps immediately to `(160, 120)` with State `2` at `(0, 0)` and
///      State `0` at `(60, 40)`.
///    - Now, `SizeNode` continues smoothly from `(70, 55)` toward `(160, 120)` without jumping.
#[test]
fn animated_content_bottom_end_and_interrupted_size_transform_in_lookahead() {
    let target = MutableTransitionState::new(0i32);
    let container_ahead = Rc::new(Cell::new(IntSize::ZERO));
    let container_now = Rc::new(Cell::new(IntSize::ZERO));
    let intr_ahead = Rc::new(Cell::new(IntSize::ZERO));
    let intr_now = Rc::new(Cell::new(IntSize::ZERO));
    let pos_ahead: Rc<RefCell<[Option<IntOffset>; 3]>> = Default::default();
    let (st, ca, cn, ia, inw, pa) = (
        target.clone(),
        container_ahead.clone(),
        container_now.clone(),
        intr_ahead.clone(),
        intr_now.clone(),
        pos_ahead.clone(),
    );

    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let (st, ca, cn, pa) = (st.clone(), ca.clone(), cn.clone(), pa.clone());
        let probe = IntrinsicProbe { ahead: ia.clone(), now: inw.clone() };
        let note_container = Modifier.layout(move |measure, measurable, constraints| {
            let placeable = measure.measure(measurable, constraints);
            if measure.is_looking_ahead() {
                ca.set(placeable.size());
            } else {
                cn.set(placeable.size());
            }
            measure.layout_with(placeable.width, placeable.height, move |placement| {
                placement.place(&placeable, 0, 0);
            })
        });
        LookaheadScope(callback(move |scope: LookaheadScope| {
            let (st, pa, probe) = (st.clone(), pa.clone(), probe.clone());
            Box!(modifier = Modifier.then_custom(probe).then(note_container.clone()), move |_| {
                let transition = remember_transition(&st, None);
                let pa = pa.clone();
                transition.AnimatedContent(
                    Modifier,
                    |_| {
                        fade_in_with(tween(100), 0.0)
                            .together_with(fade_out_with(tween(100), 0.0))
                            .using(Some(size_transform(true, |_, _| {
                                tween(100).easing(LINEAR_EASING).into()
                            })))
                    },
                    Alignment::BOTTOM_END,
                    ContentKey::state(),
                    move |_, which: i32| {
                        let pa = pa.clone();
                        let index = which as usize;
                        let note_pos = Modifier.layout(move |measure, measurable, constraints| {
                            let placeable = measure.measure(measurable, constraints);
                            let pa = pa.clone();
                            measure.layout_with(
                                placeable.width,
                                placeable.height,
                                move |placement| {
                                    if placement.is_looking_ahead()
                                        && let (Some(frame), Some(own)) = (
                                            scope.lookahead_scope_coordinates(placement),
                                            placement.coordinates(),
                                        )
                                    {
                                        pa.borrow_mut()[index] = Some(
                                            scope
                                                .local_lookahead_position_of(
                                                    frame,
                                                    own,
                                                    Offset::ZERO,
                                                )
                                                .round(),
                                        );
                                    }
                                    placement.place(&placeable, 0, 0);
                                },
                            )
                        });
                        let sized = match which {
                            0 => Modifier.size((100.dp(), 80.dp())),
                            1 => by_pass_size(IntSize::new(40, 30), IntSize::new(20, 15)),
                            _ => Modifier.size((160.dp(), 120.dp())),
                        };
                        Box!(modifier = note_pos.then(sized));
                    },
                );
            });
        }));
    });

    ui.wait_for_idle();
    assert_eq!(container_ahead.get(), IntSize::new(100, 80));
    assert_eq!(container_now.get(), IntSize::new(100, 80));
    assert_eq!(pos_ahead.borrow()[0], Some(IntOffset::ZERO));

    ui.set_auto_advance(false);
    target.set_target_state(1);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(container_ahead.get(), IntSize::new(40, 30));
    assert_eq!(intr_ahead.get(), IntSize::new(100, 80));
    assert_eq!(intr_now.get(), IntSize::new(100, 80));
    assert_eq!(pos_ahead.borrow()[1], Some(IntOffset::ZERO));
    assert_eq!(pos_ahead.borrow()[0], Some(IntOffset::new(-60, -50)));

    // Two frames above started at t = 0 and stepped 16 ms; +34 ms reaches t = 50 ms (midpoint).
    ui.advance_time_by(Duration::from_millis(34));
    ui.wait_for_idle();
    let mid = container_now.get();
    assert!((mid.width - 70).abs() <= 2 && (mid.height - 55).abs() <= 2, "at 50ms: {mid:?}");

    // Interrupt mid-flight to State 2 (`160 x 120`): State 1 (`40 x 30` ahead) is now leaving,
    // aligned `BOTTOM_END` in `160 x 120` at `(120, 90)`.
    target.set_target_state(2);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(container_ahead.get(), IntSize::new(160, 120));
    assert_eq!(pos_ahead.borrow()[2], Some(IntOffset::ZERO));
    assert_eq!(pos_ahead.borrow()[1], Some(IntOffset::new(120, 90)));
    // Continuous from ~`(70, 55)`, not jumping back to `(40, 30)` or `(20, 15)`.
    let right_after = container_now.get();
    assert!(
        right_after.width >= 68 && right_after.width < 120,
        "continuous from {mid:?}, got {right_after:?}"
    );

    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(container_now.get(), IntSize::new(160, 120));
}

/// `ContentSizeNode::on_attach` clears `lookahead_size` and `lookahead_constraints` (`D2351`):
/// when a `movable_content_of` box with `animate_content_size` moves from inside a
/// `LookaheadScope` (where its child reported `300 x 220` ahead under `400 x 400` max constraints)
/// to a container outside any `LookaheadScope` (with `500 x 500` constraints, where its child
/// measures `180 x 140` or fills max width `500`), it neither animates toward the stale
/// `300 x 220` lookahead size nor measures its child with the stale `400 x 400` lookahead
/// constraints.
#[test]
fn animate_content_size_moved_out_of_lookahead_clears_cached_lookahead_state() {
    let in_scope = mutable_state_of(true);
    let fill = mutable_state_of(false);
    let seen_now = Rc::new(Cell::new(IntSize::ZERO));
    let (is_in, fl, sn) = (in_scope.handle(), fill.handle(), seen_now.clone());

    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let sn = sn.clone();
        let movable = remember(move || {
            let sn = sn.clone();
            movable_content_of(move || {
                let sn = sn.clone();
                let note_now = Modifier.layout(move |measure, measurable, constraints| {
                    let placeable = measure.measure(measurable, constraints);
                    if !measure.is_looking_ahead() {
                        sn.set(placeable.size());
                    }
                    measure.layout_with(placeable.width, placeable.height, move |placement| {
                        placement.place(&placeable, 0, 0);
                    })
                });
                let fill_now = fl.get();
                let child_mod = if fill_now {
                    Modifier.fill_max_size()
                } else {
                    by_pass_size(IntSize::new(300, 220), IntSize::new(180, 140))
                };
                Box!(
                    modifier = note_now.animate_content_size_with(
                        tween(80).easing(LINEAR_EASING),
                        Alignment::TOP_START,
                        None::<fn(_, _)>,
                    ),
                    { Box!(modifier = child_mod.clone()) }
                );
            })
        });

        let (m_in, m_out) = (movable.clone(), movable);
        if is_in.get() {
            LookaheadScope(callback(move |_| {
                let m_in = m_in.clone();
                Box!(modifier = Modifier.size(400.dp()), move |_| m_in());
            }));
        } else {
            Box!(modifier = Modifier.size(500.dp()), move |_| m_out());
        }
    });

    ui.wait_for_idle();
    // Inside LookaheadScope: animated to the child's lookahead size `(300, 220)`.
    assert_eq!(seen_now.get(), IntSize::new(300, 220));

    // Move outside LookaheadScope: `on_attach` clears `lookahead_size` & `lookahead_constraints`,
    // so it animates to the non-lookahead size `(180, 140)`, NOT staying at `(300, 220)`.
    in_scope.set(false);
    ui.wait_for_idle();
    assert_eq!(seen_now.get(), IntSize::new(180, 140));

    // Switch child to `fill_max_size()` under the outer `500 x 500` box: if `lookahead_constraints`
    // (`0..400`) had leaked, the child would measure `400 x 400` instead of `500 x 500`.
    fill.set(true);
    ui.wait_for_idle();
    assert_eq!(seen_now.get(), IntSize::new(500, 500));
}

#[derive(Clone)]
struct IntrinsicProbe {
    ahead: Rc<Cell<IntSize>>,
    now: Rc<Cell<IntSize>>,
}

impl ModifierNodeElement for IntrinsicProbe {
    type Node = Self;
    fn create(&self) -> Self {
        self.clone()
    }
    fn update(&self, node: &mut Self) {
        *node = self.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.ahead, &other.ahead) && Rc::ptr_eq(&self.now, &other.now)
    }
}

impl ModifierNode for IntrinsicProbe {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for IntrinsicProbe {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let w = scope.max_intrinsic_width(content, Constraints::INFINITY);
        let h = scope.max_intrinsic_height(content, Constraints::INFINITY);
        if scope.is_looking_ahead() {
            self.ahead.set(IntSize::new(w, h));
        } else {
            self.now.set(IntSize::new(w, h));
        }
        let placeable = scope.measure(content, constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, 0, 0);
        })
    }

    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        height: i32,
    ) -> i32 {
        scope.max_intrinsic_width(content, height)
    }

    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        width: i32,
    ) -> i32 {
        scope.max_intrinsic_height(content, width)
    }
}

/// `CapturedAnimatedVisibility` in a `LookaheadScope` (`D2352`):
/// - While visible with a child `240 x 120` ahead and `160 x 80` now, intrinsics report `(240, 120)`
///   ahead and `(160, 80)` now.
/// - Once exiting, intrinsics and measured size ahead immediately report `(0, 0)`, while approach
///   intrinsics keep `(160, 80)` (before the shrink node) and approach size shrinks along the exit.
/// - Interrupted mid-exit (`visible = true`) with a larger child (`300 x 140` ahead, `200 x 100`
///   now), then hidden again: the second exit uses the newly kept `(200, 100)` rather than `(160, 80)`.
#[test]
fn captured_visibility_intrinsics_and_interrupted_exit_in_lookahead() {
    let visible = mutable_state_of(true);
    let large = mutable_state_of(false);
    let intr_ahead = Rc::new(Cell::new(IntSize::ZERO));
    let intr_now = Rc::new(Cell::new(IntSize::ZERO));
    let size_now = Rc::new(Cell::new(IntSize::ZERO));
    let (vis, lg, ia, inw, sn) =
        (visible.handle(), large.handle(), intr_ahead.clone(), intr_now.clone(), size_now.clone());

    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let probe = IntrinsicProbe { ahead: ia.clone(), now: inw.clone() };
        let sn = sn.clone();
        let note_size = Modifier.layout(move |measure, measurable, constraints| {
            let placeable = measure.measure(measurable, constraints);
            if !measure.is_looking_ahead() {
                sn.set(placeable.size());
            }
            measure.layout_with(placeable.width, placeable.height, move |placement| {
                placement.place(&placeable, 0, 0);
            })
        });
        LookaheadScope(callback(move |_| {
            let probe = probe.clone();
            let note_size = note_size.clone();
            Box!(
                modifier = Modifier.width(IntrinsicSize::Max).then_custom(probe).then(note_size),
                {
                    let linear = || tween(160).easing(LINEAR_EASING);
                    CapturedAnimatedVisibility!(
                        vis.get(),
                        enter = expand_vertically_with(linear(), Alignment::BOTTOM, true, |_| 0),
                        exit = shrink_vertically_with(linear(), Alignment::BOTTOM, true, |_| 0),
                        {
                            let modifier = if lg.get() {
                                by_pass_size(IntSize::new(300, 140), IntSize::new(200, 100))
                            } else {
                                by_pass_size(IntSize::new(240, 120), IntSize::new(160, 80))
                            };
                            Box!(modifier = modifier);
                        }
                    );
                }
            );
        }));
    });

    ui.wait_for_idle();
    assert_eq!(intr_ahead.get(), IntSize::new(240, 120));
    assert_eq!(intr_now.get(), IntSize::new(160, 80));
    assert_eq!(size_now.get(), IntSize::new(160, 80));

    ui.set_auto_advance(false);
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(intr_ahead.get(), IntSize::ZERO, "exiting ahead has 0 intrinsic size");
    assert_eq!(intr_now.get(), IntSize::new(160, 80), "approach keeps captured intrinsic size");

    // `EnterExitNode` shrinks vertically from `lookahead_size.height` (120 -> 60 at 80 ms), while
    // `IntrinsicSize::Max` constrains width to the approach intrinsic width 160.
    ui.advance_time_by(Duration::from_millis(80));
    ui.wait_for_idle();
    assert_eq!(size_now.get(), IntSize::new(160, 60));

    // Interrupt mid-exit with larger content.
    large.set(true);
    visible.set(true);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(intr_ahead.get(), IntSize::new(300, 140));
    assert_eq!(intr_now.get(), IntSize::new(200, 100));
    assert_eq!(size_now.get(), IntSize::new(200, 100));

    // Hide again: approach width is kept at 200 and exit shrinks from new lookahead height 140
    // (midpoint at t = 80 ms is 70).
    ui.set_auto_advance(false);
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(80));
    ui.wait_for_idle();
    assert_eq!(intr_ahead.get(), IntSize::ZERO);
    assert_eq!(intr_now.get(), IntSize::new(200, 100));
    assert_eq!(size_now.get(), IntSize::new(200, 70));
}
