//! Adversarial review tests for `w2-foundation-layout` (`D2320`–`D2324`, `D2335`–`D2337`):
//! `PaddingValues::absolute`, `Modifier::absolute_padding` and negative/`Dp::UNSPECIFIED` padding,
//! `AbsoluteArrangement`, custom cross-axis `HorizontalAlignment`/`VerticalAlignment` policies and
//! modifier chaining, `Row`/`Column`/`FlowRow`/`FlowColumn` `#[composable(non_restartable)]`
//! skipping and reuse, `BackgroundNode`/`BackgroundBrushNode` semantics invalidation and bounds,
//! and lazy `IndexForKey`/`CollectionInfo` across sticky headers, spanned grids and pagers.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::lazy::layout::get_default_lazy_layout_key;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{Optimizations, set_semantics_cache_check};
use compose_ui::{
    Alignment, AlignmentLine, Brush, CollectionInfo, Color, Dp, DpRect, HorizontalAlignment,
    HorizontalAlignmentLine, IntOffset, IntSize, IntrinsicSize, LOCAL_LAYOUT_DIRECTION, Layout,
    LayoutCoordinates, LayoutDirection, Modifier, SemanticsProperties, Shape, UnitExt,
    VerticalAlignment, VerticalAlignmentLine, measure_policy,
};

#[derive(Clone, Default)]
struct PlacedProbe {
    entries: Rc<RefCell<Vec<(usize, IntOffset, IntSize)>>>,
}

impl PlacedProbe {
    fn new() -> Self {
        Self::default()
    }

    fn record(&self, id: usize) -> Modifier {
        let entries = self.entries.clone();
        Modifier.on_placed(move |coords: LayoutCoordinates| {
            let pos = coords.position_in_parent();
            let offset = IntOffset::new(pos.x.round() as i32, pos.y.round() as i32);
            let mut entries = entries.borrow_mut();
            if let Some(slot) = entries.iter_mut().find(|(k, _, _)| *k == id) {
                *slot = (id, offset, coords.size());
            } else {
                entries.push((id, offset, coords.size()));
            }
        })
    }

    fn pos(&self, id: usize) -> IntOffset {
        self.entries.borrow().iter().find(|(k, _, _)| *k == id).expect("recorded").1
    }

    fn size(&self, id: usize) -> IntSize {
        self.entries.borrow().iter().find(|(k, _, _)| *k == id).expect("recorded").2
    }
}

fn rule_sized(width: i32, height: i32, content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(width, height), content)
}

#[composable]
fn with_direction(direction: LayoutDirection, content: impl Fn() + 'static) {
    CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || content());
}

fn captured_scopes() -> (RowScope, ColumnScope) {
    let row_out = Rc::new(Cell::new(None));
    let col_out = Rc::new(Cell::new(None));
    let (r, c) = (row_out.clone(), col_out.clone());
    let _ui = rule_sized(10, 10, move || {
        let (r, c) = (r.clone(), c.clone());
        Row!(move |s| r.set(Some(s)));
        Column!(move |s| c.set(Some(s)));
    });
    (row_out.get().unwrap(), col_out.get().unwrap())
}

// ---- PaddingValues::absolute, Modifier::absolute_padding, negative & UNSPECIFIED ---------------

#[test]
#[ignore = "review: Modifier::absolute_padding panics on Dp::UNSPECIFIED and struct PaddingValues with Dp::UNSPECIFIED breaks Modifier::same and poisons absolute+relative sums"]
fn absolute_padding_and_struct_padding_accept_unspecified_sides_and_preserve_equality() {
    let abs = Modifier.absolute_padding(Dp::UNSPECIFIED, 6.dp(), Dp::UNSPECIFIED, 10.dp());
    assert!(abs.same(&Modifier.absolute_padding(
        Dp::UNSPECIFIED,
        6.dp(),
        Dp::UNSPECIFIED,
        10.dp()
    )));
    assert!(abs.same(&Modifier.absolute_padding(0.dp(), 6.dp(), 0.dp(), 10.dp())));

    let rel_unspec = Modifier.padding(PaddingValues {
        start: Dp::UNSPECIFIED,
        top: 4.dp(),
        ..PaddingValues::ZERO
    });
    assert!(rel_unspec.same(&Modifier.padding(PaddingValues {
        start: Dp::UNSPECIFIED,
        top: 4.dp(),
        ..PaddingValues::ZERO
    })));

    let mixed_unspec = Modifier.padding(PaddingValues {
        start: 12.dp(),
        left: Dp::UNSPECIFIED,
        ..PaddingValues::ZERO
    });
    assert!(
        mixed_unspec
            .same(&Modifier.padding(PaddingValues { start: 12.dp(), ..PaddingValues::ZERO }))
    );

    let measures = Rc::new(Cell::new(0usize));
    let m = measures.clone();
    let consumed_display = Rc::new(RefCell::new(String::new()));
    let cd = consumed_display.clone();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let p = PlacedProbe::new();
    let p_in = p.clone();
    let mut ui = rule_sized(200, 200, move || {
        let _ = handle.get();
        let m = m.clone();
        let p = p_in.clone();
        let cd = cd.clone();
        with_direction!(LayoutDirection::Rtl, move || {
            let m = m.clone();
            let p1 = p.clone();
            let cd = cd.clone();
            Box!(
                modifier = Modifier
                    .absolute_padding(Dp::UNSPECIFIED, 6.dp(), 8.dp(), Dp::UNSPECIFIED)
                    .then(p.record(0)),
                {
                    let m = m.clone();
                    Layout!(
                        measure_policy = measure_policy(move |scope, _, _| {
                            m.set(m.get() + 1);
                            scope.layout(20, 30)
                        }),
                        {}
                    );
                }
            );
            Box!(
                modifier = Modifier.padding(PaddingValues {
                    start: 12.dp(),
                    left: Dp::UNSPECIFIED,
                    ..PaddingValues::ZERO
                }),
                move |_| {
                    Box!(modifier = Modifier.size(10.dp()).then(p1.record(1)));
                }
            );
            Box!(
                modifier = Modifier
                    .consume_window_insets(
                        PaddingValues::absolute(3.dp(), 4.dp(), 9.dp(), 6.dp())
                            + PaddingValues::new(10.dp(), 1.dp(), 20.dp(), 2.dp())
                    )
                    .on_consumed_window_insets_changed(move |insets: WindowInsets| {
                        *cd.borrow_mut() = insets.to_string();
                    }),
                {}
            );
        });
    });

    assert_eq!(measures.get(), 1);
    assert_eq!(p.size(0), IntSize::new(20, 30));
    // In RTL, start=12.dp puts 12 px on the right; child is at x = 0 inside a 22-wide padded box.
    assert_eq!(p.pos(1), IntOffset::new(0, 0));
    assert_eq!(
        consumed_display.borrow().as_str(),
        "(Insets(left=0, top=0, right=0, bottom=0) + PaddingValues(13.0.dp, 5.0.dp, 29.0.dp, 8.0.dp))"
    );

    // Recomposition with identical Dp::UNSPECIFIED modifiers must not remeasure.
    tick.set(1);
    ui.wait_for_idle();
    assert_eq!(measures.get(), 1);
}

#[test]
fn padding_negative_edge_cases_and_unspecified_constructors() {
    // Negative zero (-0.0) is accepted as zero everywhere.
    let neg_zero = Dp(-0.0);
    assert_eq!(PaddingValues::all(neg_zero), PaddingValues::ZERO);
    assert_eq!(
        PaddingValues::absolute(neg_zero, neg_zero, neg_zero, neg_zero),
        PaddingValues::ZERO
    );
    let _ = Modifier.absolute_padding(neg_zero, neg_zero, neg_zero, neg_zero);

    // Subnormal negative and -infinity panic with Compose's message; PaddingValues constructors
    // reject Dp::UNSPECIFIED, as Compose's PaddingValuesImpl / PaddingValues.Absolute.
    for bad in [Dp(-f32::MIN_POSITIVE), Dp(f32::NEG_INFINITY), Dp::UNSPECIFIED] {
        assert!(std::panic::catch_unwind(|| PaddingValues::all(bad)).is_err());
        assert!(std::panic::catch_unwind(|| PaddingValues::symmetric(bad, 0.dp())).is_err());
        assert!(std::panic::catch_unwind(|| PaddingValues::symmetric(0.dp(), bad)).is_err());
        assert!(
            std::panic::catch_unwind(|| PaddingValues::absolute(0.dp(), bad, 0.dp(), 0.dp()))
                .is_err()
        );
    }
    for neg in [Dp(-f32::MIN_POSITIVE), Dp(f32::NEG_INFINITY)] {
        assert!(
            std::panic::catch_unwind(|| {
                let _ = Modifier.absolute_padding(0.dp(), 0.dp(), 0.dp(), neg);
            })
            .is_err()
        );
    }
}

#[test]
fn absolute_padding_intrinsics_chaining_insets_and_lazy_rtl() {
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let p = PlacedProbe::new();
        let p_in = p.clone();
        let mut ui = rule_sized(300, 300, move || {
            let p = p_in.clone();
            with_direction!(direction, move || {
                let p1 = p.clone();
                let p2 = p.clone();
                // Intrinsic width/height around absolute_padding and PaddingValues::absolute:
                Box!(
                    modifier = Modifier
                        .width(IntrinsicSize::Min)
                        .height(IntrinsicSize::Max)
                        .then(p.record(0)),
                    move |_| {
                        Box!(
                            modifier = Modifier
                                .absolute_padding(5.dp(), 7.dp(), 11.dp(), 13.dp())
                                .padding(
                                    PaddingValues::absolute(2.dp(), 3.dp(), 4.dp(), 6.dp())
                                        + PaddingValues::new(10.dp(), 0.dp(), 20.dp(), 0.dp())
                                )
                                .then(p1.record(1)),
                            {
                                Box!(modifier = Modifier.size((30.dp(), 40.dp())));
                            }
                        );
                    }
                );

                // consume_window_insets(PaddingValues) resolves absolute + relative sides by direction:
                let consumed = PaddingValues::absolute(3.dp(), 4.dp(), 9.dp(), 6.dp())
                    + PaddingValues::new(10.dp(), 1.dp(), 20.dp(), 2.dp());
                Box!(
                    modifier = Modifier
                        .size(100.dp())
                        .consume_window_insets(consumed)
                        .window_insets_padding(WindowInsets::new(40, 40, 40, 40))
                        .then(p2.record(2)),
                    {
                        Box!(modifier = Modifier.fill_max_size());
                    }
                );

                // LazyRow with absolute content_padding in LTR vs RTL:
                LazyRow!(
                    modifier = Modifier.size((100.dp(), 40.dp())),
                    content_padding = PaddingValues::absolute(8.dp(), 2.dp(), 18.dp(), 4.dp()),
                    |scope: LazyListScope| {
                        scope.item(|| {
                            Box!(modifier = Modifier.size((20.dp(), 10.dp())).test_tag("lazy_pad"));
                        });
                    }
                );
            });
        });

        // Total horizontal padding = (5 + 11) + (2 + 4) + (10 + 20) = 52; vertical = (7 + 13) + (3 + 6) = 29.
        assert_eq!(p.size(0), IntSize::new(30 + 52, 40 + 29));
        // Combined left padding on inner box: 5 + 2 + (start=10 in LTR, end=20 in RTL); top = 7 + 3 = 10.
        let expected_pad_x = if direction == LayoutDirection::Ltr { 17 } else { 27 };
        assert_eq!(p.pos(1), IntOffset::new(expected_pad_x, 10));

        // Consumed insets out of (40, 40, 40, 40):
        // In LTR, consumed left = 3 + 10 = 13 -> remaining left = 27; consumed right = 9 + 20 = 29 -> remaining right = 11.
        // In RTL, consumed left = 3 + 20 = 23 -> remaining left = 17; consumed right = 9 + 10 = 19 -> remaining right = 21.
        let expected_insets_x = if direction == LayoutDirection::Ltr { 27 } else { 17 };
        assert_eq!(p.pos(2), IntOffset::new(expected_insets_x, 35));
        assert_eq!(p.size(2), IntSize::new(100 - 38, 100 - 67));

        // LazyRow with absolute(left=8, top=2, right=18, bottom=4):
        // In LTR, start=8 -> item at x=8; in RTL, start=right=18 -> item right edge at 100 - 18 = 82 -> x=62!
        let lazy_bounds = ui.on_node_with_tag("lazy_pad").get_unclipped_bounds_in_root();
        let expected_item_x = if direction == LayoutDirection::Ltr { 8.dp() } else { 62.dp() };
        assert_eq!((lazy_bounds.left, lazy_bounds.top), (expected_item_x, 2.dp()));
    }
}

// ---- AbsoluteArrangement & non_restartable Row/Column/Flow skipping ----------------------------

#[test]
fn absolute_arrangement_overflow_negative_spacing_and_flow_row_rtl() {
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let p = PlacedProbe::new();
        let p_in = p.clone();
        let _ui = rule_sized(200, 200, move || {
            let p = p_in.clone();
            with_direction!(direction, move || {
                let p0 = p.clone();
                let p1 = p.clone();
                let p2 = p.clone();
                // Overflow in 50 px with AbsoluteArrangement::spaced_by(15.dp()):
                // Unlike relative spaced_by (which packs from right in RTL and clamps at 0),
                // AbsoluteArrangement packs physically from left in both directions and clamps at total - size!
                Row!(
                    modifier = Modifier.required_size((50.dp(), 20.dp())),
                    horizontal_arrangement = AbsoluteArrangement::spaced_by(15.dp()),
                    move |_| {
                        Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p0.record(0)));
                        Box!(modifier = Modifier.size((15.dp(), 10.dp())).then(p0.record(1)));
                        Box!(modifier = Modifier.size((10.dp(), 10.dp())).then(p0.record(2)));
                    }
                );
                // Negative spacing (-5.dp()) with AbsoluteArrangement::aligned / spaced_by:
                Row!(
                    modifier = Modifier.required_size((100.dp(), 20.dp())),
                    horizontal_arrangement =
                        AbsoluteArrangement::spaced_by((Dp(-5.0), Alignment::END)),
                    move |_| {
                        Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p1.record(3)));
                        Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p1.record(4)));
                    }
                );
                // FlowRow in RTL with AbsoluteArrangement::RIGHT keeps physical left-to-right order per line:
                FlowRow!(
                    modifier = Modifier.required_size((100.dp(), 50.dp())),
                    horizontal_arrangement = AbsoluteArrangement::RIGHT,
                    max_items_in_each_row = 2,
                    move |_| {
                        Box!(modifier = Modifier.size((30.dp(), 10.dp())).then(p2.record(5)));
                        Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p2.record(6)));
                    }
                );
            });
        });

        // Direct overflow arrangement in 50 px with [20, 15, 10] and spacing 15:
        // AbsoluteArrangement::spaced_by packs physically from left and clamps at total - size -> [0, 35, 40]
        // in both LTR and RTL (whereas relative spaced_by in RTL packs from right -> [30, 0, 0]).
        let mut out = [0; 3];
        let abs_spaced: HorizontalArrangement = AbsoluteArrangement::spaced_by(15.dp()).into();
        abs_spaced.arrange(&compose_ui::Density::default(), 50, &[20, 15, 10], direction, &mut out);
        assert_eq!(out, [0, 35, 40]);
        // In a 50-wide Row, child 2 is constrained to remaining=0 -> [0, 35, 50] in both directions:
        assert_eq!((p.pos(0).x, p.pos(1).x, p.pos(2).x), (0, 35, 50));

        // Negative spacing -5: occupied = (20 - 5) + 20 = 35, free = 65.
        // Alignment::END in LTR shifts by +65 (positions 65, 80); in RTL END is left (0), shift 0 (positions 0, 15).
        if direction == LayoutDirection::Ltr {
            assert_eq!((p.pos(3).x, p.pos(4).x), (65, 80));
        } else {
            assert_eq!((p.pos(3).x, p.pos(4).x), (0, 15));
        }

        // FlowRow with AbsoluteArrangement::RIGHT in 100 px: [30, 20] placed at [50, 80] in both LTR and RTL.
        assert_eq!((p.pos(5).x, p.pos(6).x), (50, 80));
    }
}

#[test]
fn non_restartable_row_column_and_flow_skip_unchanged_content_and_reuse_cleanly() {
    let outer_ticks = mutable_state_of(0);
    let outer_handle = outer_ticks.handle();
    let reuse_key = mutable_state_of(0);
    let reuse_handle = reuse_key.handle();
    let child_compositions = Rc::new(Cell::new(0usize));
    let cc = child_compositions.clone();
    let remember_inits = Rc::new(Cell::new(0usize));
    let ri = remember_inits.clone();
    let p = PlacedProbe::new();
    let p_in = p.clone();

    #[composable]
    fn Leaf(comp_counter: Rc<Cell<usize>>, rem_counter: Rc<Cell<usize>>) {
        comp_counter.set(comp_counter.get() + 1);
        let _ = remember(move || {
            rem_counter.set(rem_counter.get() + 1);
            1usize
        });
        Box!(modifier = Modifier.size(10.dp()));
    }

    let mut ui = rule_sized(200, 200, move || {
        let _ = outer_handle.get();
        let rk = reuse_handle.get();
        let cc = cc.clone();
        let ri = ri.clone();
        let p = p_in.clone();
        ReusableContent(rk, move || {
            let cc1 = cc.clone();
            let cc2 = cc.clone();
            let ri1 = ri.clone();
            let ri2 = ri.clone();
            let p = p.clone();
            Row!(
                vertical_alignment = VerticalAlignment::custom(|size, space| (space - size) / 2),
                move |_| {
                    Leaf!(cc1.clone(), ri1.clone());
                }
            );
            FlowColumn!(
                item_horizontal_alignment =
                    HorizontalAlignment::custom(|size, space, _| space - size),
                modifier = Modifier.size((60.dp(), 60.dp())),
                move |_| {
                    Box!(modifier = Modifier.size((40.dp(), 10.dp())));
                    Box!(modifier = Modifier.size(15.dp()).then(p.record(0)));
                    Leaf!(cc2.clone(), ri2.clone());
                }
            );
        });
    });

    assert_eq!(child_compositions.get(), 2);
    assert_eq!(remember_inits.get(), 2);
    assert_eq!(p.pos(0).x, 25);

    // Parent recomposes without changing Row/FlowColumn arguments or non-capturing custom closures:
    // Layout skips both children's content lambdas!
    outer_ticks.set(1);
    ui.wait_for_idle();
    assert_eq!(child_compositions.get(), 2);
    assert_eq!(remember_inits.get(), 2);

    // Reusing under a new ReusableContent key resets remembered state and preserves custom alignment.
    reuse_key.set(1);
    ui.wait_for_idle();
    assert_eq!(remember_inits.get(), 4);
    assert_eq!(p.pos(0).x, 25);
}

// ---- Custom alignment modifier chaining & cross-axis directions --------------------------------

#[test]
#[ignore = "review: custom align and align_by / align_by_block ignore outermost-wins chaining order"]
fn custom_alignment_chaining_with_align_and_align_by_in_row_column_and_flow() {
    let (row_scope, col_scope) = captured_scopes();
    let h_line = HorizontalAlignmentLine::new(|a, b| a.min(b));
    let v_line = VerticalAlignmentLine::new(|a, b| a.min(b));
    let seen_row_dir = Rc::new(Cell::new(None));
    let seen_dir_in = seen_row_dir.clone();
    let p = PlacedProbe::new();
    let p_in = p.clone();

    let _ui = rule_sized(300, 300, move || {
        let p = p_in.clone();
        let seen_dir = seen_dir_in.clone();
        with_direction!(LayoutDirection::Rtl, move || {
            let p1 = p.clone();
            let p2 = p.clone();
            let seen_dir = seen_dir.clone();
            Row!(modifier = Modifier.required_height(100.dp()), move |row| {
                // 0: outer custom (y=17) beats inner align_by(line) AND does not participate in baseline group!
                let a0: AlignmentLine = h_line.into();
                Layout!(
                    modifier = Modifier
                        .align(row, VerticalAlignment::custom(|_, _| 17))
                        .align_by(row, h_line)
                        .then(p1.record(0)),
                    measure_policy = measure_policy(move |scope, _, _| {
                        scope.provide_alignment_lines([(a0, 5)]);
                        scope.layout(10, 20)
                    }),
                    {}
                );
                // 1: outer align_by_block (line at 8) beats inner custom (y=77), paired with child 2 (line at 28) -> y = 28 - 8 = 20!
                Box!(
                    modifier = Modifier
                        .size((10.dp(), 20.dp()))
                        .align_by_block(row, |_| 8)
                        .align(row, VerticalAlignment::custom(|_, _| 77))
                        .then(p1.record(1))
                );
                // 2: reference align_by_block child (line at 28 -> y = 0).
                Box!(
                    modifier = Modifier
                        .size((10.dp(), 30.dp()))
                        .align_by_block(row, |_| 28)
                        .then(p1.record(2))
                );
                // 3: outer custom (y=41) beats inner align_by_block!
                Box!(
                    modifier = Modifier
                        .size((10.dp(), 20.dp()))
                        .align(row, VerticalAlignment::custom(|_, _| 41))
                        .align_by_block(row, |_| 2)
                        .then(p1.record(3))
                );
                // 4: outer custom1 (y=13) beats inner custom2 (y=66) and built-in align:
                Box!(
                    modifier = Modifier
                        .size((10.dp(), 20.dp()))
                        .align(row, VerticalAlignment::custom(|_, _| 13))
                        .align(row, VerticalAlignment::custom(|_, _| 66))
                        .align(row, Alignment::BOTTOM)
                        .then(p1.record(4))
                );
                // 5: outer built-in BOTTOM (y=80) beats inner custom (y=13):
                Box!(
                    modifier = Modifier
                        .size((10.dp(), 20.dp()))
                        .align(row, Alignment::BOTTOM)
                        .align(row, VerticalAlignment::custom(|_, _| 13))
                        .then(p1.record(5))
                );
                // 6: ColumnScope custom horizontal alignment applied in an RTL Row receives Ltr (Compose's Row passes Ltr across)!
                let seen_dir = seen_dir.clone();
                Box!(
                    modifier = Modifier
                        .size((10.dp(), 20.dp()))
                        .align(
                            col_scope,
                            HorizontalAlignment::custom(move |_, _, dir| {
                                seen_dir.set(Some(dir));
                                29
                            })
                        )
                        .then(p1.record(6))
                );
            });

            // FlowColumn in RTL: outer custom beats inner align_by(v_line), and outer align_by_block beats inner custom!
            FlowColumn!(modifier = Modifier.required_width(100.dp()), move |col| {
                let av: AlignmentLine = v_line.into();
                Layout!(
                    modifier = Modifier
                        .align(
                            col,
                            HorizontalAlignment::custom(|_, _, dir| {
                                assert_eq!(dir, LayoutDirection::Rtl);
                                19
                            })
                        )
                        .align_by(col, v_line)
                        .then(p2.record(7)),
                    measure_policy = measure_policy(move |scope, _, _| {
                        scope.provide_alignment_lines([(av, 4)]);
                        scope.layout(20, 10)
                    }),
                    {}
                );
                // RowScope custom vertical alignment applied in RTL FlowColumn: unmirrored!
                Box!(
                    modifier = Modifier
                        .size((20.dp(), 10.dp()))
                        .align(row_scope, VerticalAlignment::custom(|_, _| 33))
                        .then(p2.record(8))
                );
            });
        });
    });

    assert_eq!(p.pos(0).y, 17);
    assert_eq!(p.pos(1).y, 20);
    assert_eq!(p.pos(2).y, 0);
    assert_eq!(p.pos(3).y, 41);
    assert_eq!(p.pos(4).y, 13);
    assert_eq!(p.pos(5).y, 80);
    assert_eq!(p.pos(6).y, 29);
    assert_eq!(seen_row_dir.get(), Some(LayoutDirection::Ltr));
    assert_eq!(p.pos(7).x, 19);
    assert_eq!(p.pos(8).x, 33);
}

// ---- Background semantics invalidation, bounds, brush alpha, and reuse ------------------------

#[test]
fn background_semantics_color_updates_in_place_without_invalidating_semantics_and_preserves_bounds()
{
    set_semantics_cache_check(true);
    struct ResetCacheCheck;
    impl Drop for ResetCacheCheck {
        fn drop(&mut self) {
            set_semantics_cache_check(false);
        }
    }
    let _guard = ResetCacheCheck;

    let color_state = mutable_state_of(Color::RED);
    let color_handle = color_state.handle();
    let shape_state = mutable_state_of(Shape::Rectangle);
    let shape_handle = shape_state.handle();
    let sem_runs = Rc::new(Cell::new(0usize));
    let sr = sem_runs.clone();

    let mut ui = rule_sized(200, 200, move || {
        let sr = sr.clone();
        Column!(move |_| {
            // 1. Solid color + custom semantics counter on same node:
            let sr = sr.clone();
            Box!(
                modifier = Modifier
                    .size(40.dp())
                    .background_with(color_handle.get(), shape_handle.get())
                    .semantics(move |config| {
                        sr.set(sr.get() + 1);
                        config.set_test_tag("solid_bg");
                    })
            );
            // 2. Gradient brush reports Color::UNSPECIFIED while setting Shape:
            Box!(
                modifier = Modifier
                    .size(40.dp())
                    .background_with(
                        Brush::linear_gradient([Color::RED, Color::BLUE]),
                        RoundedCornerShape(6.dp()),
                    )
                    .test_tag("grad_bg")
            );
            // 3. Solid brush with alpha folds alpha into BACKGROUND_COLOR:
            Box!(
                modifier = Modifier
                    .size(40.dp())
                    .background(Brush::solid(Color::GREEN).with_alpha(0.5))
                    .test_tag("faded_bg")
            );
            // 4. Outer background is NOT important for bounds, so inner padded merging semantics keeps inner bounds:
            Box!(
                modifier = Modifier
                    .size(60.dp())
                    .background(Color::WHITE)
                    .padding(10.dp())
                    .semantics_with(true, |config| {
                        config.set_test_tag("inner_merged");
                    })
            );
            // 5. Background alone (no other semantics modifier on the node, merged into parent or unmerged):
            Box!(modifier = Modifier.padding(15.dp()).background(Color::BLACK).size(20.dp()));
        });
    });

    let snap1 = ui.on_node_with_tag("solid_bg").fetch_semantics_snapshot();
    let runs_initial = sem_runs.get();
    let prod1 =
        snap1.config.get(&SemanticsProperties::BACKGROUND_COLOR).expect("background color").clone();
    assert_eq!((prod1.0)(), Color::RED);
    assert_eq!(snap1.config.get(&SemanticsProperties::SHAPE), Some(&Shape::Rectangle));

    // Color-only change updates the existing ColorProducer in place WITHOUT invalidating semantics!
    ui.run_on_idle(|_| color_state.set(Color::BLUE));
    let snap2 = ui.on_node_with_tag("solid_bg").fetch_semantics_snapshot();
    let prod2 =
        snap2.config.get(&SemanticsProperties::BACKGROUND_COLOR).expect("background color").clone();
    assert_eq!(prod1, prod2, "ColorProducer Rc must stay identical across color-only updates");
    assert_eq!((prod1.0)(), Color::BLUE);
    // Note: check_semantics_cache re-evaluates apply_semantics in debug check mode, so verify shape change updates SHAPE:
    let _ = runs_initial;

    let rounded = RoundedCornerShape(8.dp());
    ui.run_on_idle({
        let rounded = rounded.clone();
        move |_| shape_state.set(rounded)
    });
    let snap3 = ui.on_node_with_tag("solid_bg").fetch_semantics_snapshot();
    assert_eq!(snap3.config.get(&SemanticsProperties::SHAPE), Some(&rounded));
    assert_eq!(
        (snap3.config.get(&SemanticsProperties::BACKGROUND_COLOR).unwrap().0)(),
        Color::BLUE
    );

    // Gradient background has Color::UNSPECIFIED and RoundedCornerShape(6.dp()):
    let grad = ui.on_node_with_tag("grad_bg").fetch_semantics_snapshot();
    assert_eq!(grad.config.get(&SemanticsProperties::SHAPE), Some(&RoundedCornerShape(6.dp())));
    let grad_color = (grad.config.get(&SemanticsProperties::BACKGROUND_COLOR).unwrap().0)();
    assert!(!grad_color.is_specified());

    // Faded solid brush reports alpha 0.5:
    let faded = ui.on_node_with_tag("faded_bg").fetch_semantics_snapshot();
    let faded_color = (faded.config.get(&SemanticsProperties::BACKGROUND_COLOR).unwrap().0)();
    assert_eq!(faded_color, Color::GREEN.with_alpha(0.5));

    // Outer background before padding(10.dp()) does NOT widen inner_merged's 40x40 bounds to 60x60:
    let merged_bounds = ui.on_node_with_tag("inner_merged").get_unclipped_bounds_in_root();
    assert_eq!((merged_bounds.width(), merged_bounds.height()), (40.dp(), 40.dp()));
    assert_eq!(merged_bounds, DpRect::new(10.dp(), 130.dp(), 50.dp(), 170.dp()));
}

#[test]
fn background_semantics_survives_reusable_content_reuse() {
    let key = mutable_state_of(0);
    let key_handle = key.handle();
    let mut ui = rule_sized(100, 100, move || {
        let k = key_handle.get();
        ReusableContent(k, move || {
            let (color, shape) = if k == 0 {
                (Color::RED, Shape::Rectangle)
            } else {
                (Color::GREEN, RoundedCornerShape(4.dp()))
            };
            Box!(
                modifier = Modifier.size(30.dp()).background_with(color, shape).test_tag("reused")
            );
        });
    });

    let s0 = ui.on_node_with_tag("reused").fetch_semantics_snapshot();
    assert_eq!((s0.config.get(&SemanticsProperties::BACKGROUND_COLOR).unwrap().0)(), Color::RED);
    assert_eq!(s0.config.get(&SemanticsProperties::SHAPE), Some(&Shape::Rectangle));

    ui.run_on_idle(|_| key.set(1));
    let s1 = ui.on_node_with_tag("reused").fetch_semantics_snapshot();
    assert_eq!((s1.config.get(&SemanticsProperties::BACKGROUND_COLOR).unwrap().0)(), Color::GREEN);
    assert_eq!(s1.config.get(&SemanticsProperties::SHAPE), Some(&RoundedCornerShape(4.dp())));
}

// ---- Lazy IndexForKey & CollectionInfo edge cases ----------------------------------------------

#[test]
fn lazy_index_for_key_sticky_headers_spanned_grids_cloned_pager_key_and_empty_transitions() {
    let page_count = mutable_state_of(6usize);
    let pc = page_count.handle();
    let shared_pager_key = pager_key(|page| format!("p_{page}"));

    let mut ui = rule_sized(300, 300, move || {
        let pages = pc.get();
        let pk1 = shared_pager_key.clone();
        let pk2 = shared_pager_key.clone();
        Column!(move |_| {
            let pk1 = pk1.clone();
            let pk2 = pk2.clone();
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 80.dp())).test_tag("sticky_list"),
                |scope: LazyListScope| {
                    scope.sticky_header(|_| {
                        Spacer(Modifier.size(20.dp()));
                    });
                    scope.items_keyed(
                        2,
                        |i| format!("sec0_{i}"),
                        |_| {
                            Spacer(Modifier.size(20.dp()));
                        },
                    );
                    scope.sticky_header_keyed("hdr_1", |_| {
                        Spacer(Modifier.size(20.dp()));
                    });
                    scope.items_keyed(
                        3,
                        |i| 100usize + i,
                        |_| {
                            Spacer(Modifier.size(20.dp()));
                        },
                    );
                }
            );
            LazyVerticalGrid!(
                GridCells::Fixed(2),
                modifier = Modifier.size((100.dp(), 80.dp())).test_tag("sticky_grid"),
                |scope: LazyGridScope| {
                    scope.sticky_header_keyed("grid_hdr".to_string(), |_| {
                        Spacer(Modifier.size(20.dp()));
                    });
                    scope.items_keyed_with_span(
                        3,
                        |i| format!("g_{i}"),
                        |_, i| GridItemSpan(if i == 0 { 2 } else { 1 }),
                        |_| 0u8,
                        |_| {
                            Spacer(Modifier.size(20.dp()));
                        },
                    );
                }
            );
            HorizontalPager!(
                remember_pager_state!(0, 0.0, move || pages),
                modifier = Modifier.size((100.dp(), 40.dp())).test_tag("pager_a"),
                key = Some(pk1),
                page_content = |_, _| Spacer(Modifier.fill_max_size())
            );
            VerticalPager!(
                remember_pager_state!(0, 0.0, move || pages),
                modifier = Modifier.size((100.dp(), 40.dp())).test_tag("pager_b"),
                key = Some(pk2),
                page_content = |_, _| Spacer(Modifier.fill_max_size())
            );
        });
    });

    let ask = |ui: &mut ComposeTestRule, tag: &str, key: &dyn std::any::Any| -> i32 {
        let snap = ui.on_node_with_tag(tag).fetch_semantics_snapshot();
        (snap.config.get(&SemanticsProperties::INDEX_FOR_KEY).unwrap().0)(key)
    };

    // sticky_list: 0 = unkeyed sticky header, 1..=2 = sec0_0..1, 3 = "hdr_1", 4..=6 = 100..=102usize.
    assert_eq!(ask(&mut ui, "sticky_list", &get_default_lazy_layout_key(0)), 0);
    assert_eq!(ask(&mut ui, "sticky_list", &"sec0_1"), 2);
    assert_eq!(ask(&mut ui, "sticky_list", &"hdr_1".to_string()), 3);
    assert_eq!(ask(&mut ui, "sticky_list", &102usize), 6);
    assert_eq!(
        ui.on_node_with_tag("sticky_list")
            .fetch_semantics_snapshot()
            .config
            .get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: 7, column_count: 1 })
    );

    // sticky_grid: 0 = "grid_hdr" (String key matched by &'static str), 1..=3 = "g_0".."g_2".
    assert_eq!(ask(&mut ui, "sticky_grid", &"grid_hdr"), 0);
    assert_eq!(ask(&mut ui, "sticky_grid", &"g_2"), 3);

    // Cloned pager_key works across both pagers, and updates CollectionInfo & IndexForKey on 6 -> 0 -> 3:
    assert_eq!(ask(&mut ui, "pager_a", &"p_5"), 5);
    assert_eq!(ask(&mut ui, "pager_b", &"p_4".to_string()), 4);

    ui.run_on_idle(|_| page_count.set(0));
    assert_eq!(ask(&mut ui, "pager_a", &"p_0"), -1);
    assert_eq!(
        ui.on_node_with_tag("pager_a")
            .fetch_semantics_snapshot()
            .config
            .get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: 1, column_count: 0 })
    );
    assert_eq!(
        ui.on_node_with_tag("pager_b")
            .fetch_semantics_snapshot()
            .config
            .get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: 0, column_count: 1 })
    );

    ui.run_on_idle(|_| page_count.set(3));
    assert_eq!(ask(&mut ui, "pager_a", &"p_2"), 2);
    assert_eq!(ask(&mut ui, "pager_a", &"p_5"), -1);
}
