//! Adversarial review tests for `layout-partials` (D1927–D1930) and `input-layout-ports`
//! (D1481, D1926):
//! - `Arrangement::spaced_by` in LTR and RTL (`Row` vs `Column`, overflow clamping, odd free-space
//!   rounding, negative spacing, 0 and 1 child; D1928)
//! - Cross-axis alignments of the other axis on `Row`, `Column`, `FlowRow` and `FlowColumn`,
//!   RTL mirroring rules, and `align_by` / chaining interactions (D1927)
//! - `Modifier::match_parent_size` and `required_*_in` bounds, chaining and intrinsics (D1929)
//! - `PaddingValues` `+`, `-` clamping and directional side queries in layout (D1930)
//! - `LazyGrid` and `LazyColumn` pinned items out of view: unequal slot widths, variable spans,
//!   negative spacing, RTL, `reverse_layout`, `animate_item` reorders/scrolls, and directional
//!   focus search by arrow and Tab keys (D1481, D2024)
//! - `PlatformTextToolbar` `MenuItemOption` ordering across subsets and prepares (D1926)

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::time::Duration;

use compose_animation::tween;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, AlignmentLine, Dp, FocusRequester, HorizontalAlignmentLine, IntOffset, IntSize,
    IntrinsicSize, Key, KeyEvent, KeyModifiers, LOCAL_LAYOUT_DIRECTION, Layout, LayoutCoordinates,
    LayoutDirection, PinnableContainer, PinnedHandle, PlatformTextToolbar, Rect, TextActionMenu,
    TextActionMode, TextActionModeCallback, TextActionModes, TextToolbar, TextToolbarAction,
    UnitExt, measure_policy, text_action_ids,
};

use crate::lazy_animate_support::paused;
use crate::scroll_support::run;

fn rule_sized(width: i32, height: i32, content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(width, height), content);
    ui.wait_for_idle();
    ui
}

#[derive(Clone, Copy)]
struct PlacedProbe(&'static RefCell<BTreeMap<usize, (IntOffset, IntSize)>>);

impl PlacedProbe {
    fn new() -> Self {
        Self(Box::leak(Box::default()))
    }

    fn record(self, id: usize) -> Modifier {
        let rects = self.0;
        Modifier.on_globally_positioned(move |coords: LayoutCoordinates| {
            let root = coords.position_in_root();
            rects.borrow_mut().insert(
                id,
                (IntOffset::new(root.x.round() as i32, root.y.round() as i32), coords.size()),
            );
        })
    }

    fn pos(self, id: usize) -> IntOffset {
        self.0.borrow().get(&id).unwrap_or_else(|| panic!("missing probe {id}")).0
    }

    fn size(self, id: usize) -> IntSize {
        self.0.borrow().get(&id).unwrap_or_else(|| panic!("missing probe {id}")).1
    }
}

fn with_direction(direction: LayoutDirection, content: impl Fn() + 'static) {
    CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), content);
}

// ---- D1928: Arrangement::spaced_by in LTR and RTL --------------------------------------------

#[test]
fn spaced_by_in_rtl_column_packs_from_top_and_overflows_at_bottom() {
    // A `Column` is vertical: `LayoutDirection::Rtl` must never reverse its `spaced_by`
    // arrangement, whether children fit (aligned or default) or overflow.
    let p = PlacedProbe::new();
    let _ui = rule_sized(200, 200, move || {
        with_direction(LayoutDirection::Rtl, move || {
            Row!({
                // Column 1: room enough (100px tall, children 20 and 15, space = 10 -> y = 0, 30).
                Column!(
                    modifier = Modifier.size((40.dp(), 100.dp())),
                    vertical_arrangement = Arrangement::spaced_by(10.dp()),
                    {
                        Box!(modifier = Modifier.size((20.dp(), 20.dp())).then(p.record(0)));
                        Box!(modifier = Modifier.size((20.dp(), 15.dp())).then(p.record(1)));
                    }
                );
                // Column 2: room enough with BOTTOM alignment (free = 100 - 45 = 55 -> y = 55, 85).
                Column!(
                    modifier = Modifier.size((40.dp(), 100.dp())),
                    vertical_arrangement = Arrangement::spaced_by((10.dp(), Alignment::BOTTOM)),
                    {
                        Box!(modifier = Modifier.size((20.dp(), 20.dp())).then(p.record(2)));
                        Box!(modifier = Modifier.size((20.dp(), 15.dp())).then(p.record(3)));
                    }
                );
                // Column 3: insufficient space (50px tall, space = 15, children 20, 15, 10 ->
                // measured heights 20, 15, 0 -> packed from top: y = 0, 35, 50).
                Column!(
                    modifier = Modifier.size((40.dp(), 50.dp())),
                    vertical_arrangement = Arrangement::spaced_by(15.dp()),
                    {
                        Box!(modifier = Modifier.size((20.dp(), 20.dp())).then(p.record(4)));
                        Box!(modifier = Modifier.size((20.dp(), 15.dp())).then(p.record(5)));
                        Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p.record(6)));
                    }
                );
            });
        });
    });

    assert_eq!((p.pos(0).y, p.pos(1).y), (0, 30));
    assert_eq!((p.pos(2).y, p.pos(3).y), (55, 85));
    assert_eq!((p.pos(4).y, p.pos(5).y, p.pos(6).y), (0, 35, 50));
}

#[test]
fn spaced_by_in_rtl_row_with_alignment_clamps_overflow_at_left_without_shifting() {
    // When children + gaps fill or exceed the row width in RTL, `free <= 0`, so `SpacedAligned`'s
    // alignment branch must not shift them, and overflowing items clamp at x = 0.
    for alignment in [Alignment::START, Alignment::CENTER_HORIZONTALLY, Alignment::END] {
        let p = PlacedProbe::new();
        let _ui = rule_sized(200, 100, move || {
            with_direction(LayoutDirection::Rtl, move || {
                Row!(
                    modifier = Modifier.width(50.dp()).height(20.dp()),
                    horizontal_arrangement = Arrangement::spaced_by((15.dp(), alignment)),
                    {
                        Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p.record(0)));
                        Box!(modifier = Modifier.size((15.dp(), 10.dp())).then(p.record(1)));
                        Box!(modifier = Modifier.size((10.dp(), 10.dp())).then(p.record(2)));
                    }
                );
            });
        });
        assert_eq!((p.pos(0).x, p.pos(1).x, p.pos(2).x), (30, 0, 0), "alignment {alignment:?}");
        assert_eq!((p.size(0).width, p.size(1).width, p.size(2).width), (20, 15, 0));
    }
}

#[test]
fn spaced_by_odd_free_space_rounding_and_negative_spacing_match_compose() {
    // Total = 101px, children = [20, 20], space = 10 -> occupied = 50, free = 51.
    // Center alignment (`bias = 0.0`):
    // - LTR: `align(0, 51, Ltr) = round(25.5) = 26` -> [26, 56].
    // - RTL: packed from right -> [81, 51]; `shift = round(25.5) - 51 = -25` -> [56, 26].
    let p = PlacedProbe::new();
    let _ui = rule_sized(200, 200, move || {
        Column!({
            Row!(
                modifier = Modifier.width(101.dp()).height(20.dp()),
                horizontal_arrangement =
                    Arrangement::spaced_by((10.dp(), Alignment::CENTER_HORIZONTALLY)),
                {
                    Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p.record(0)));
                    Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p.record(1)));
                }
            );
            with_direction(LayoutDirection::Rtl, move || {
                Row!(
                    modifier = Modifier.width(101.dp()).height(20.dp()),
                    horizontal_arrangement =
                        Arrangement::spaced_by((10.dp(), Alignment::CENTER_HORIZONTALLY)),
                    {
                        Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p.record(2)));
                        Box!(modifier = Modifier.size((20.dp(), 10.dp())).then(p.record(3)));
                    }
                );
            });
            // Negative spacing (-10px) in LTR and RTL, plus 1-child alignment in RTL:
            Row!(
                modifier = Modifier.width(100.dp()).height(20.dp()),
                horizontal_arrangement = Arrangement::spaced_by((-10.0).dp()),
                {
                    Box!(modifier = Modifier.size((30.dp(), 10.dp())).then(p.record(4)));
                    Box!(modifier = Modifier.size((30.dp(), 10.dp())).then(p.record(5)));
                }
            );
            with_direction(LayoutDirection::Rtl, move || {
                Column!({
                    Row!(
                        modifier = Modifier.width(100.dp()).height(20.dp()),
                        horizontal_arrangement = Arrangement::spaced_by((-10.0).dp()),
                        {
                            Box!(modifier = Modifier.size((30.dp(), 10.dp())).then(p.record(6)));
                            Box!(modifier = Modifier.size((30.dp(), 10.dp())).then(p.record(7)));
                        }
                    );
                    // Lone child in RTL with `END` (left in RTL): x = 0; empty row also valid.
                    Row!(
                        modifier = Modifier.width(100.dp()).height(20.dp()),
                        horizontal_arrangement = Arrangement::spaced_by((15.dp(), Alignment::END)),
                        {
                            Box!(modifier = Modifier.size((30.dp(), 10.dp())).then(p.record(8)));
                        }
                    );
                    Row!(
                        modifier = Modifier.width(100.dp()).height(10.dp()).then(p.record(9)),
                        horizontal_arrangement = Arrangement::spaced_by(15.dp()),
                        {}
                    );
                });
            });
        });
    });

    assert_eq!((p.pos(0).x, p.pos(1).x), (26, 56));
    assert_eq!((p.pos(2).x, p.pos(3).x), (56, 26));
    assert_eq!((p.pos(4).x, p.pos(5).x), (0, 20));
    assert_eq!((p.pos(6).x, p.pos(7).x), (70, 50));
    assert_eq!(p.pos(8).x, 0);
    assert_eq!(p.size(9), IntSize::new(100, 10));
}

// ---- D1927: Cross-axis alignment of the other axis -------------------------------------------

fn captured_scopes() -> (RowScope, ColumnScope) {
    let scopes: &'static Cell<Option<(RowScope, ColumnScope)>> = Box::leak(Box::default());
    let _ui = rule_sized(10, 10, move || {
        Row!(move |row| {
            Column!(move |col| {
                scopes.set(Some((row, col)));
            });
        });
    });
    scopes.get().unwrap()
}

#[test]
fn row_and_column_cross_axis_alignments_of_other_axis_respect_rtl_rules_and_chaining() {
    let (row_scope, col_scope) = captured_scopes();
    let p = PlacedProbe::new();
    let _ui = rule_sized(300, 300, move || {
        with_direction(LayoutDirection::Rtl, move || {
            Column!({
                // 1. RTL Row (height 60) with ColumnScope alignments: Row always passes Ltr for
                // cross-axis alignment, so START -> y=0, CENTER -> y=25, END -> y=50 (NOT mirrored!).
                // Also test outermost-wins chaining: `col_scope END` outside `row_scope TOP` -> y=50.
                Row!(
                    modifier = Modifier.size((200.dp(), 60.dp())),
                    vertical_alignment = Alignment::CENTER_VERTICALLY,
                    move |row| {
                        Box!(
                            modifier = Modifier
                                .align(col_scope, Alignment::START)
                                .size(10.dp())
                                .then(p.record(0))
                        );
                        Box!(
                            modifier = Modifier
                                .align(col_scope, Alignment::CENTER_HORIZONTALLY)
                                .size(10.dp())
                                .then(p.record(1))
                        );
                        Box!(
                            modifier = Modifier
                                .align(col_scope, Alignment::END)
                                .size(10.dp())
                                .then(p.record(2))
                        );
                        Box!(
                            modifier = Modifier
                                .align(col_scope, Alignment::END)
                                .align(row, Alignment::TOP)
                                .size(10.dp())
                                .then(p.record(3))
                        );
                    }
                );
                // 2. RTL Column (width 80, default CENTER_HORIZONTALLY) with RowScope alignments
                // vs ColumnScope alignments:
                // - RowScope.TOP -> x=0 (NOT mirrored in RTL!)
                // - RowScope.CENTER_VERTICALLY -> x=35
                // - RowScope.BOTTOM -> x=70 (NOT mirrored in RTL!)
                // - ColumnScope.START -> x=70 (MIRRORED to right in RTL!)
                // - ColumnScope.END -> x=0 (MIRRORED to left in RTL!)
                Column!(
                    modifier = Modifier.size((80.dp(), 120.dp())).then(p.record(10)),
                    horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
                    move |col| {
                        Box!(
                            modifier = Modifier
                                .align(row_scope, Alignment::TOP)
                                .size(10.dp())
                                .then(p.record(4))
                        );
                        Box!(
                            modifier = Modifier
                                .align(row_scope, Alignment::CENTER_VERTICALLY)
                                .size(10.dp())
                                .then(p.record(5))
                        );
                        Box!(
                            modifier = Modifier
                                .align(row_scope, Alignment::BOTTOM)
                                .size(10.dp())
                                .then(p.record(6))
                        );
                        Box!(
                            modifier = Modifier
                                .align(col, Alignment::START)
                                .size(10.dp())
                                .then(p.record(7))
                        );
                        Box!(
                            modifier =
                                Modifier.align(col, Alignment::END).size(10.dp()).then(p.record(8))
                        );
                    }
                );
            });
        });
    });

    assert_eq!((p.pos(0).y, p.pos(1).y, p.pos(2).y, p.pos(3).y), (0, 25, 50, 50));
    let cx = p.pos(10).x;
    assert_eq!(
        (p.pos(4).x - cx, p.pos(5).x - cx, p.pos(6).x - cx, p.pos(7).x - cx, p.pos(8).x - cx),
        (0, 35, 70, 70, 0)
    );
}

#[test]
fn row_mixes_align_by_line_with_other_axis_align() {
    let (_, col_scope) = captured_scopes();
    let line = HorizontalAlignmentLine::new(|a, b| a.min(b));
    let p = PlacedProbe::new();
    let _ui = rule_sized(200, 200, move || {
        Row!(move |row| {
            let l0 = line;
            let a0: AlignmentLine = l0.into();
            Layout!(
                modifier = Modifier.align_by(row, l0).then(p.record(0)),
                measure_policy = measure_policy(move |scope, _, _| {
                    scope.provide_alignment_lines([(a0, 30)]);
                    scope.layout(20, 40)
                }),
                {}
            );
            let l1 = line;
            let a1: AlignmentLine = l1.into();
            Layout!(
                modifier = Modifier.align_by(row, l1).then(p.record(1)),
                measure_policy = measure_policy(move |scope, _, _| {
                    scope.provide_alignment_lines([(a1, 5)]);
                    scope.layout(20, 10)
                }),
                {}
            );
            // Sibling with `ColumnScope.align(END)` goes to bottom of 40px row: y = 30.
            Box!(
                modifier = Modifier
                    .align(col_scope, Alignment::END)
                    .size((20.dp(), 10.dp()))
                    .then(p.record(2))
            );
        });
    });

    assert_eq!((p.pos(0).y, p.pos(1).y, p.pos(2).y), (0, 25, 30));
}

#[test]
fn flow_row_and_flow_column_honor_other_axis_alignment_in_ltr_and_rtl() {
    let (row_scope, col_scope) = captured_scopes();
    let p = PlacedProbe::new();
    let _ui = rule_sized(300, 300, move || {
        with_direction(LayoutDirection::Rtl, move || {
            Column!({
                // RTL FlowRow: line height 40, second item 30x10 with `ColumnScope.align(END)`
                // must go to bottom of line (y = 30), NOT top (y = 0).
                FlowRow!(modifier = Modifier.width(100.dp()).then(p.record(10)), move |_| {
                    Box!(modifier = Modifier.size((30.dp(), 40.dp())).then(p.record(0)));
                    Box!(
                        modifier = Modifier
                            .align(col_scope, Alignment::END)
                            .size((30.dp(), 10.dp()))
                            .then(p.record(1))
                    );
                });
                // RTL FlowColumn: line width 50, second item 10x30 with `RowScope.align(BOTTOM)`
                // must go to right of column (dx = 40, NOT mirrored to 0!), while third item
                // with `FlowColumnScope.align(START)` mirrors to right (dx = 40).
                FlowColumn!(
                    modifier = Modifier.height(100.dp()).width(50.dp()).then(p.record(11)),
                    move |col| {
                        Box!(modifier = Modifier.size((50.dp(), 30.dp())).then(p.record(2)));
                        Box!(
                            modifier = Modifier
                                .align(row_scope, Alignment::BOTTOM)
                                .size((10.dp(), 30.dp()))
                                .then(p.record(3))
                        );
                        Box!(
                            modifier = Modifier
                                .align(col, Alignment::START)
                                .size((10.dp(), 30.dp()))
                                .then(p.record(4))
                        );
                    }
                );
            });
        });
    });

    let ry = p.pos(10).y;
    assert_eq!((p.pos(0).y - ry, p.pos(1).y - ry), (0, 30));
    let cx = p.pos(11).x;
    assert_eq!((p.pos(2).x - cx, p.pos(3).x - cx, p.pos(4).x - cx), (0, 40, 40));
}

// ---- D1929: match_parent_size and required_*_in ----------------------------------------------

#[test]
fn box_lone_and_all_match_parent_size_children_use_min_constraints_and_center() {
    let p = PlacedProbe::new();
    let _ui = rule_sized(300, 300, move || {
        Column!({
            // 1. Lone `match_parent_size` child with `required_size(20)` inside a Box of min 50x40:
            Box!(
                modifier = Modifier
                    .size_in(SizeSpec::bounds(50.dp(), 40.dp(), 100.dp(), 100.dp()))
                    .then(p.record(0)),
                content_alignment = Alignment::TOP_START,
                move |scope| {
                    Box!(
                        modifier = Modifier
                            .match_parent_size(scope)
                            .required_size(20.dp())
                            .then(p.record(1))
                    );
                }
            );
            // 2. Lone `match_parent_size` child with `required_size(80)` (oversized!) inside min 40x40:
            Box!(
                modifier = Modifier
                    .size_in(SizeSpec::bounds(40.dp(), 40.dp(), 100.dp(), 100.dp()))
                    .then(p.record(2)),
                move |scope| {
                    Box!(
                        modifier = Modifier
                            .match_parent_size(scope)
                            .required_size(80.dp())
                            .then(p.record(3))
                    );
                }
            );
            // 3. Multiple `match_parent_size` children with NO normal children: Box sizes to
            // min constraints (30x24), and both children match 30x24.
            Box!(
                modifier = Modifier
                    .size_in(SizeSpec::bounds(30.dp(), 24.dp(), 100.dp(), 100.dp()))
                    .then(p.record(4)),
                move |scope| {
                    Box!(modifier = Modifier.match_parent_size(scope).then(p.record(5)));
                    Box!(modifier = Modifier.match_parent_size(scope).then(p.record(6)));
                }
            );
        });
    });

    assert_eq!(p.size(0), IntSize::new(50, 40));
    assert_eq!(p.size(1), IntSize::new(20, 20));
    assert_eq!((p.pos(1).x - p.pos(0).x, p.pos(1).y - p.pos(0).y), (15, 10));

    assert_eq!(p.size(2), IntSize::new(40, 40));
    assert_eq!(p.size(3), IntSize::new(80, 80));
    assert_eq!((p.pos(3).x - p.pos(2).x, p.pos(3).y - p.pos(2).y), (-20, -20));

    assert_eq!(p.size(4), IntSize::new(30, 24));
    assert_eq!(p.size(5), IntSize::new(30, 24));
    assert_eq!(p.size(6), IntSize::new(30, 24));
}

#[test]
fn box_match_parent_size_chaining_and_intrinsics_ignore_matching_children() {
    let p = PlacedProbe::new();
    let _ui = rule_sized(300, 300, move || {
        Column!({
            // Chaining: outermost wins.
            Box!(modifier = Modifier.size(60.dp()).then(p.record(0)), move |scope| {
                Box!(
                    modifier = Modifier
                        .align(scope, Alignment::BOTTOM_END)
                        .match_parent_size(scope)
                        .default_min_size(10.dp(), 10.dp())
                        .then(p.record(1))
                );
                Box!(
                    modifier = Modifier
                        .match_parent_size(scope)
                        .align(scope, Alignment::BOTTOM_END)
                        .required_size(20.dp())
                        .then(p.record(2))
                );
            });
            // Intrinsics: ignores `match_parent_size` child wanting 90x90, sizing to 25x18.
            Box!(
                modifier =
                    Modifier.width(IntrinsicSize::Min).height(IntrinsicSize::Max).then(p.record(3)),
                move |scope| {
                    Box!(modifier = Modifier.size((25.dp(), 18.dp())));
                    Box!(
                        modifier = Modifier
                            .match_parent_size(scope)
                            .default_min_size(90.dp(), 90.dp())
                            .then(p.record(4))
                    );
                }
            );
            // Intrinsics with ONLY `match_parent_size` children: 0x0.
            Box!(
                modifier =
                    Modifier.width(IntrinsicSize::Max).height(IntrinsicSize::Min).then(p.record(5)),
                |scope| {
                    Box!(
                        modifier =
                            Modifier.match_parent_size(scope).default_min_size(70.dp(), 70.dp())
                    );
                    Box!(
                        modifier =
                            Modifier.match_parent_size(scope).default_min_size(80.dp(), 80.dp())
                    );
                }
            );
        });
    });

    assert_eq!(p.size(1), IntSize::new(10, 10));
    assert_eq!((p.pos(1).x - p.pos(0).x, p.pos(1).y - p.pos(0).y), (50, 50));
    assert_eq!(p.size(2), IntSize::new(20, 20));
    assert_eq!((p.pos(2).x - p.pos(0).x, p.pos(2).y - p.pos(0).y), (20, 20));
    assert_eq!(p.size(3), IntSize::new(25, 18));
    assert_eq!(p.size(4), IntSize::new(25, 18));
    assert_eq!(p.size(5), IntSize::new(0, 0));
}

#[test]
fn box_with_constraints_scope_supports_align_and_match_parent_size() {
    let p = PlacedProbe::new();
    let _ui = rule_sized(200, 200, move || {
        BoxWithConstraints!(modifier = p.record(0), move |scope: BoxWithConstraintsScope| {
            Box!(modifier = Modifier.size((80.dp(), 50.dp())).then(p.record(1)));
            Box!(
                modifier =
                    Modifier.align(scope, Alignment::BOTTOM_END).size(20.dp()).then(p.record(2))
            );
            Box!(modifier = Modifier.match_parent_size(scope).padding(10.dp()).then(p.record(3)));
        });
    });

    assert_eq!(p.size(0), IntSize::new(80, 50));
    assert_eq!((p.pos(2).x - p.pos(0).x, p.pos(2).y - p.pos(0).y), (60, 30));
    assert_eq!(p.size(3), IntSize::new(60, 30));
    assert_eq!((p.pos(3).x - p.pos(0).x, p.pos(3).y - p.pos(0).y), (10, 10));
}

#[test]
fn required_size_in_modifiers_override_incoming_and_clamp_intrinsics() {
    let p = PlacedProbe::new();
    let _ui = rule_sized(300, 300, move || {
        Column!({
            Box!(modifier = Modifier.size(40.dp()).then(p.record(0)), move |_| {
                Box!(
                    modifier = Modifier
                        .fill_max_size()
                        .required_size_in(SizeSpec::bounds(60.dp(), 10.dp(), 80.dp(), 20.dp()))
                        .then(p.record(1)),
                    |_| {
                        Box!(modifier = Modifier.size((70.dp(), 30.dp())));
                    }
                );
            });
            Box!(modifier = Modifier.size((50.dp(), 40.dp())).then(p.record(2)), move |_| {
                Box!(
                    modifier = Modifier
                        .size_in(SizeSpec::bounds(40.dp(), 30.dp(), 50.dp(), 40.dp()))
                        .required_width_in(90.dp(), Dp::UNSPECIFIED)
                        .required_height_in(Dp::UNSPECIFIED, 12.dp())
                        .then(p.record(3))
                );
            });
            Box!(
                modifier =
                    Modifier.width(IntrinsicSize::Min).height(IntrinsicSize::Max).then(p.record(4)),
                |_| {
                    Box!(
                        modifier = Modifier
                            .required_width_in(45.dp(), 80.dp())
                            .required_height_in(10.dp(), 22.dp())
                            .size((20.dp(), 50.dp()))
                    );
                }
            );
        });
    });

    assert_eq!(p.size(1), IntSize::new(70, 20));
    assert_eq!((p.pos(1).x - p.pos(0).x, p.pos(1).y - p.pos(0).y), (-15, 10));
    assert_eq!(p.size(3), IntSize::new(90, 12));
    assert_eq!(p.size(4), IntSize::new(45, 22));
}

// ---- D1930: PaddingValues +, - and directional resolution ------------------------------------

#[test]
fn padding_values_add_sub_clamp_and_apply_in_ltr_and_rtl() {
    let a = PaddingValues::new(10.dp(), 20.dp(), 5.dp(), 12.dp());
    let b = PaddingValues::new(4.dp(), 25.dp(), 15.dp(), 2.dp());

    let sum = a + b;
    assert_eq!((sum.start, sum.top, sum.end, sum.bottom), (14.dp(), 45.dp(), 20.dp(), 14.dp()));
    assert_eq!(sum.calculate_left_padding(LayoutDirection::Ltr), 14.dp());
    assert_eq!(sum.calculate_right_padding(LayoutDirection::Ltr), 20.dp());
    assert_eq!(sum.calculate_left_padding(LayoutDirection::Rtl), 20.dp());
    assert_eq!(sum.calculate_right_padding(LayoutDirection::Rtl), 14.dp());
    assert_eq!((sum.calculate_top_padding(), sum.calculate_bottom_padding()), (45.dp(), 14.dp()));

    let diff = a - b;
    assert_eq!((diff.start, diff.top, diff.end, diff.bottom), (6.dp(), 0.dp(), 0.dp(), 10.dp()));
    assert_eq!(diff.calculate_left_padding(LayoutDirection::Ltr), 6.dp());
    assert_eq!(diff.calculate_right_padding(LayoutDirection::Ltr), 0.dp());
    assert_eq!(diff.calculate_left_padding(LayoutDirection::Rtl), 0.dp());
    assert_eq!(diff.calculate_right_padding(LayoutDirection::Rtl), 6.dp());

    let p = PlacedProbe::new();
    let _ui = rule_sized(200, 200, move || {
        Column!({
            Box!(modifier = Modifier.size(100.dp()), move |_| {
                Box!(modifier = Modifier.fill_max_size().padding(sum).then(p.record(0)));
            });
            with_direction(LayoutDirection::Rtl, move || {
                Box!(modifier = Modifier.size(100.dp()), move |_| {
                    Box!(modifier = Modifier.fill_max_size().padding(diff).then(p.record(1)));
                });
            });
        });
    });

    assert_eq!(p.pos(0), IntOffset::new(14, 45));
    assert_eq!(p.size(0), IntSize::new(66, 41));
    assert_eq!(p.pos(1), IntOffset::new(0, 100));
    assert_eq!(p.size(1), IntSize::new(94, 90));
}

// ---- D1481 & D2024: LazyGrid / LazyColumn pinned items, animate_item & focus search ----------

fn wait_until(ui: &mut ComposeTestRule, done: impl Fn() -> bool) {
    let later = std::time::Instant::now() + std::time::Duration::from_secs(60);
    for _ in 0..100 {
        ui.wait_for_idle();
        if done() {
            return;
        }
        ui.host().prefetch(later);
    }
    panic!("condition never became true");
}

#[test]
fn pinned_grid_item_reordered_out_of_view_with_animate_item_does_not_double_place() {
    for in_lookahead in [false, true] {
        let initial: Vec<i32> = (0..12).collect();
        let items = mutable_state_of(initial);
        let items_h = items.handle();
        let containers: Rc<RefCell<BTreeMap<i32, Rc<dyn PinnableContainer>>>> = Rc::default();
        let containers_c = containers.clone();
        let state = LazyGridState::new(4, 0);
        let grid_state = *state;
        std::mem::forget(state);

        let mut ui = paused(IntSize::new(200, 200), move || {
            let containers_c = containers_c.clone();
            let body = move || {
                let list = items_h.get();
                let containers_c = containers_c.clone();
                LazyVerticalGrid!(
                    GridCells::Fixed(2),
                    modifier = Modifier.size((100.dp(), 40.dp())),
                    state = grid_state,
                    move |scope: LazyGridScope| {
                        let item_scope = scope.item_scope();
                        let containers_c = containers_c.clone();
                        scope.items_list_keyed(
                            list.clone(),
                            |it| *it,
                            move |&item| {
                                if let Some(c) = compose_ui::LOCAL_PINNABLE_CONTAINER.current() {
                                    containers_c.borrow_mut().insert(item, c);
                                }
                                let spec = tween(160).into();
                                Box!(
                                    modifier = Modifier
                                        .animate_item_with(&item_scope, None, Some(spec), None)
                                        .size((50.dp(), 20.dp()))
                                        .test_tag(format!("g-{item}"))
                                );
                            },
                        );
                    }
                );
            };
            if in_lookahead {
                compose_ui::LookaheadScope(callback(move |_| body()));
            } else {
                body();
            }
        });

        ui.wait_for_idle();
        let pin_6: PinnedHandle = containers.borrow().get(&6).unwrap().pin();
        let pin_7: PinnedHandle = containers.borrow().get(&7).unwrap().pin();
        ui.wait_for_idle();

        // Anchor item 4 stays at index 5 (line 2), while pinned item 6 moves before `lead`
        // (index 0 -> `pinned_before` + `moved_to_start`) and pinned item 7 moves after `end_index`
        // (index 11 -> `pinned_after` + `moved_to_end`).
        items.set(vec![6, 0, 1, 2, 3, 4, 5, 8, 9, 10, 11, 7]);
        for _ in 0..14 {
            ui.advance_time_by(Duration::from_millis(16));
            ui.wait_for_idle();
        }
        ui.on_node_with_tag("g-6").assert_exists();
        ui.on_node_with_tag("g-7").assert_exists();
        drop((pin_6, pin_7));
    }
}

#[test]
fn pinned_grid_item_scrolled_out_of_view_while_animating_placement_does_not_double_place() {
    for in_lookahead in [false, true] {
        let initial: Vec<i32> = (0..20).collect();
        let items = mutable_state_of(initial);
        let items_h = items.handle();
        let containers: Rc<RefCell<BTreeMap<i32, Rc<dyn PinnableContainer>>>> = Rc::default();
        let containers_c = containers.clone();
        let state = LazyGridState::new(0, 0);
        let grid_state = *state;
        std::mem::forget(state);

        let mut ui = paused(IntSize::new(200, 200), move || {
            let containers_c = containers_c.clone();
            let body = move || {
                let list = items_h.get();
                let containers_c = containers_c.clone();
                LazyVerticalGrid!(
                    GridCells::Fixed(2),
                    modifier = Modifier.size((100.dp(), 40.dp())),
                    state = grid_state,
                    move |scope: LazyGridScope| {
                        let item_scope = scope.item_scope();
                        let containers_c = containers_c.clone();
                        scope.items_list_keyed(
                            list.clone(),
                            |it| *it,
                            move |&item| {
                                if let Some(c) = compose_ui::LOCAL_PINNABLE_CONTAINER.current() {
                                    containers_c.borrow_mut().insert(item, c);
                                }
                                let spec = tween(200).into();
                                Box!(
                                    modifier = Modifier
                                        .animate_item_with(&item_scope, None, Some(spec), None)
                                        .size((50.dp(), 20.dp()))
                                        .test_tag(format!("gs-{item}"))
                                );
                            },
                        );
                    }
                );
            };
            if in_lookahead {
                compose_ui::LookaheadScope(callback(move |_| body()));
            } else {
                body();
            }
        });

        ui.wait_for_idle();
        let pin_1: PinnedHandle = containers.borrow().get(&1).unwrap().pin();
        ui.wait_for_idle();

        // Swap 1 and 3 in view to start a placement animation on pinned item 1.
        let mut reordered: Vec<i32> = (0..20).collect();
        reordered.swap(1, 3);
        items.set(reordered);
        ui.advance_time_by_frame();
        ui.advance_time_by_frame();
        ui.advance_time_by(Duration::from_millis(16));
        ui.wait_for_idle();

        // Scroll past item 1 while its placement animation is still in progress!
        let _ = run(&mut ui, async move { grid_state.scroll_by(120.0).await });
        ui.advance_time_by(Duration::from_millis(16));
        ui.wait_for_idle();
        ui.on_node_with_tag("gs-1").assert_exists();
        drop(pin_1);
    }
}

#[test]
fn pinned_lazy_list_item_reordered_before_view_with_animate_item_does_not_double_place() {
    for in_lookahead in [false, true] {
        let initial: Vec<i32> = (0..10).collect();
        let items = mutable_state_of(initial);
        let items_h = items.handle();
        let containers: Rc<RefCell<BTreeMap<i32, Rc<dyn PinnableContainer>>>> = Rc::default();
        let containers_c = containers.clone();
        let state = LazyListState::new(3, 0);
        let list_state = *state;
        std::mem::forget(state);

        let mut ui = paused(IntSize::new(200, 200), move || {
            let containers_c = containers_c.clone();
            let body = move || {
                let list = items_h.get();
                let containers_c = containers_c.clone();
                LazyColumn!(
                    modifier = Modifier.size((100.dp(), 40.dp())),
                    state = list_state,
                    move |scope: LazyListScope| {
                        let containers_c = containers_c.clone();
                        scope.items_list_keyed(
                            list.clone(),
                            |it| *it,
                            move |item_scope, &item| {
                                if let Some(c) = compose_ui::LOCAL_PINNABLE_CONTAINER.current() {
                                    containers_c.borrow_mut().insert(item, c);
                                }
                                let spec = tween(160).into();
                                Box!(
                                    modifier = Modifier
                                        .animate_item_with(&item_scope, None, Some(spec), None)
                                        .size((100.dp(), 20.dp()))
                                        .test_tag(format!("l-{item}"))
                                );
                            },
                        );
                    }
                );
            };
            if in_lookahead {
                compose_ui::LookaheadScope(callback(move |_| body()));
            } else {
                body();
            }
        });

        ui.wait_for_idle();
        let pin_4: PinnedHandle = containers.borrow().get(&4).unwrap().pin();
        ui.wait_for_idle();

        items.set(vec![4, 0, 1, 2, 3, 5, 6, 7, 8, 9]);
        for _ in 0..14 {
            ui.advance_time_by(Duration::from_millis(16));
            ui.wait_for_idle();
        }
        ui.on_node_with_tag("l-4").assert_exists();
        drop(pin_4);
    }
}

#[test]
fn lazy_grid_pinned_items_keep_cells_across_unequal_widths_spans_rtl_reverse_and_padding() {
    // 3-column vertical grid in 100px width with content_padding (start=4, top=6, end=5, bottom=8 ->
    // cross_room = 91px across 3 cols = `[31, 30, 30]`, unequal!).
    let composed: Rc<RefCell<BTreeSet<usize>>> = Rc::default();
    let containers: Rc<RefCell<BTreeMap<usize, Rc<dyn PinnableContainer>>>> = Rc::default();
    let pr = PlacedProbe::new();
    let measured_widths: Rc<RefCell<Vec<(usize, i32)>>> = Rc::default();

    let (comp, cont, mw) = (composed.clone(), containers.clone(), measured_widths.clone());
    let state = LazyGridState::new(0, 0);
    let grid_state = *state;
    std::mem::forget(state);

    let direction = mutable_state_of(LayoutDirection::Ltr);
    let dir_h = direction.handle();
    let reverse = mutable_state_of(false);
    let rev_h = reverse.handle();

    let mut ui = rule_sized(200, 200, move || {
        let (comp, cont, mw) = (comp.clone(), cont.clone(), mw.clone());
        with_direction(dir_h.get(), move || {
            let (comp, cont, mw) = (comp.clone(), cont.clone(), mw.clone());
            LazyVerticalGrid!(
                GridCells::Fixed(3),
                modifier = Modifier.size((100.dp(), 50.dp())),
                state = grid_state,
                content_padding = PaddingValues::new(4.dp(), 6.dp(), 5.dp(), 8.dp()),
                reverse_layout = rev_h.get(),
                vertical_arrangement = Arrangement::spaced_by(5.dp()),
                move |scope: LazyGridScope| {
                    let (comp, cont, mw) = (comp.clone(), cont.clone(), mw.clone());
                    scope.items_with_span(
                        25,
                        |_, i| {
                            GridItemSpan(if i == 1 || (2..18).contains(&i) && i % 3 == 2 {
                                2
                            } else {
                                1
                            })
                        },
                        |_| 0usize,
                        move |i| {
                            if let Some(c) = compose_ui::LOCAL_PINNABLE_CONTAINER.current() {
                                cont.borrow_mut().insert(i, c);
                            }
                            let comp2 = comp.clone();
                            DisposableEffect(i, move |effects| {
                                comp2.borrow_mut().insert(i);
                                let comp3 = comp2.clone();
                                effects.on_dispose(move || {
                                    comp3.borrow_mut().remove(&i);
                                })
                            });
                            let mw2 = mw.clone();
                            Layout!(
                                modifier = pr.record(i),
                                measure_policy = measure_policy(move |scope, _, constraints| {
                                    mw2.borrow_mut().push((i, constraints.max_width));
                                    scope.layout(constraints.max_width, 15)
                                }),
                                {}
                            );
                        },
                    );
                }
            );
        });
    });

    assert_eq!((pr.pos(0), pr.size(0)), (IntOffset::new(4, 6), IntSize::new(31, 15)));
    assert_eq!((pr.pos(1), pr.size(1)), (IntOffset::new(35, 6), IntSize::new(60, 15)));

    let pin_0 = containers.borrow().get(&0).unwrap().pin();
    let pin_1 = containers.borrow().get(&1).unwrap().pin();
    measured_widths.borrow_mut().clear();

    let _ = run(&mut ui, async move { grid_state.scroll_to_item(20, 0).await });
    wait_until(&mut ui, || !composed.borrow().contains(&5));

    // Pinned items 0 and 1 are out of view before `lead` (`content_padding.top = 6` lays out
    // one leading line at `6 - 20 = -14`, so `pinned_before` steps back by 20 from -14: -34, -54):
    assert_eq!((pr.pos(0), pr.size(0)), (IntOffset::new(4, -54), IntSize::new(31, 15)));
    assert_eq!((pr.pos(1), pr.size(1)), (IntOffset::new(35, -34), IntSize::new(60, 15)));
    assert!(
        !measured_widths.borrow().iter().any(|(i, _)| *i == 0 || *i == 1),
        "pinned items kept their exact cell widths without remeasuring: {:?}",
        *measured_widths.borrow()
    );

    // Pin item 21 (beyond the variable-span section), scroll back to 0, check `pinned_after`:
    let (visible_x_21, visible_w_21) = (pr.pos(21).x, pr.size(21).width);
    let pin_21 = containers.borrow().get(&21).unwrap().pin();
    drop((pin_0, pin_1));

    let _ = run(&mut ui, async move { grid_state.scroll_to_item(0, 0).await });
    wait_until(&mut ui, || !composed.borrow().contains(&15));

    assert_eq!(pr.pos(21).x, visible_x_21);
    assert_eq!(pr.size(21).width, visible_w_21);
    assert!(pr.pos(21).y >= 50 - 8, "placed after visible lines: {:?}", pr.pos(21));

    // Flip to RTL and `reverse_layout = true` and verify `pinned_after` mirrors both axes!
    direction.set(LayoutDirection::Rtl);
    reverse.set(true);
    ui.wait_for_idle();
    assert_eq!(pr.pos(21).x, 100 - visible_x_21 - visible_w_21);
    assert!(pr.pos(21).y < 0, "reversed `pinned_after` is above top: {:?}", pr.pos(21));

    drop(pin_21);
}

#[test]
fn lazy_horizontal_grid_pinned_items_with_negative_spacing() {
    let containers: Rc<RefCell<BTreeMap<usize, Rc<dyn PinnableContainer>>>> = Rc::default();
    let pr = PlacedProbe::new();
    let cont = containers.clone();
    let state = LazyGridState::new(0, 0);
    let grid_state = *state;
    std::mem::forget(state);

    let mut ui = rule_sized(200, 200, move || {
        let cont = cont.clone();
        LazyHorizontalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.size((60.dp(), 40.dp())),
            state = grid_state,
            horizontal_arrangement = Arrangement::spaced_by((-25.0).dp()),
            move |scope: LazyGridScope| {
                let cont = cont.clone();
                scope.items(20, move |i| {
                    if let Some(c) = compose_ui::LOCAL_PINNABLE_CONTAINER.current() {
                        cont.borrow_mut().insert(i, c);
                    }
                    Box!(modifier = Modifier.size((20.dp(), 20.dp())).then(pr.record(i)));
                });
            }
        );
    });

    let pin_0 = containers.borrow().get(&0).unwrap().pin();
    let pin_1 = containers.borrow().get(&1).unwrap().pin();
    let _ = run(&mut ui, async move { grid_state.scroll_to_item(14, 0).await });
    ui.wait_for_idle();

    assert_eq!((pr.pos(0).y, pr.pos(1).y), (0, 20));
    assert_eq!(pr.pos(0).x, pr.pos(1).x);
    drop((pin_0, pin_1));
}

#[test]
fn focused_grid_item_scrolled_out_of_view_moves_focus_by_arrow_and_tab_keys() {
    let focused_index: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let requesters: Rc<RefCell<BTreeMap<usize, FocusRequester>>> = Rc::default();
    let composed: Rc<RefCell<BTreeSet<usize>>> = Rc::default();

    let (fi, reqs, comp) = (focused_index.clone(), requesters.clone(), composed.clone());
    let state = LazyGridState::new(0, 0);
    let grid_state = *state;
    std::mem::forget(state);

    let mut ui = rule_sized(200, 200, move || {
        let (fi, reqs, comp) = (fi.clone(), reqs.clone(), comp.clone());
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((90.dp(), 40.dp())),
            state = grid_state,
            move |scope: LazyGridScope| {
                let (fi, reqs, comp) = (fi.clone(), reqs.clone(), comp.clone());
                scope.items(30, move |i| {
                    let requester = remember(FocusRequester::new);
                    reqs.borrow_mut().insert(i, requester);
                    let comp2 = comp.clone();
                    DisposableEffect(i, move |effects| {
                        comp2.borrow_mut().insert(i);
                        let comp3 = comp2.clone();
                        effects.on_dispose(move || {
                            comp3.borrow_mut().remove(&i);
                        })
                    });
                    let fi2 = fi.clone();
                    Box!(
                        modifier = Modifier
                            .size((30.dp(), 20.dp()))
                            .focus_requester(requester)
                            .on_focus_changed(move |state: compose_ui::FocusState| {
                                if state.has_focus() {
                                    fi2.set(Some(i));
                                }
                            })
                            .focusable()
                    );
                });
            }
        );
    });
    ui.host().set_keys_navigate(true);

    let focus_and_scroll = |ui: &mut ComposeTestRule, item: usize, scroll_target: usize| {
        let _ = run(ui, async move { grid_state.scroll_to_item(item / 3 * 3, 0).await });
        ui.wait_for_idle();
        let req = *requesters.borrow().get(&item).expect("target composed");
        assert!(req.request_focus());
        ui.wait_for_idle();
        assert_eq!(focused_index.get(), Some(item));
        let _ = run(ui, async move { grid_state.scroll_to_item(scroll_target, 0).await });
        let mid = (item + scroll_target) / 2;
        wait_until(ui, || !composed.borrow().contains(&mid));
        assert!(composed.borrow().contains(&item), "focused item {item} stays pinned");
    };

    let press = |ui: &mut ComposeTestRule, key: Key, shift: bool| {
        let mut event = KeyEvent::down(key);
        if shift {
            event.modifiers = KeyModifiers { shift: true, ..KeyModifiers::default() };
        }
        ui.host().send_key(event);
        ui.host().send_key(KeyEvent::up(key));
        ui.wait_for_idle();
    };

    // 1. Focus item 4 (row 1, col 1), scroll to 24 (rows 8..9 visible: 24,25,26 / 27,28,29):
    // `DirectionDown` stays in col 1 -> item 25!
    focus_and_scroll(&mut ui, 4, 24);
    press(&mut ui, Key::DirectionDown, false);
    assert_eq!(focused_index.get(), Some(25));

    // 2. Focus item 4 (row 1, col 1), scroll to 24, press `DirectionUp`:
    // Steps `ahead_lines` back to row 0 -> item 1 (row 0, col 1)!
    focus_and_scroll(&mut ui, 4, 24);
    press(&mut ui, Key::DirectionUp, false);
    assert_eq!(focused_index.get(), Some(1));

    // 3. Focus item 1 (row 0, col 1), scroll to 24:
    // - `Tab` (`Next`) moves in placement order from `pinned_before` [1] to first visible item 24!
    focus_and_scroll(&mut ui, 1, 24);
    press(&mut ui, Key::Tab, false);
    assert_eq!(focused_index.get(), Some(24));

    // - `Shift+Tab` (`Previous`) steps `ahead_lines` back to row 0 ([0, 1, 2]) -> item 0!
    focus_and_scroll(&mut ui, 1, 24);
    press(&mut ui, Key::Tab, true);
    assert_eq!(focused_index.get(), Some(0));

    // 4. Symmetric `pinned_after` cases: focus item 25 (row 8, col 1), scroll back to 0:
    // - `DirectionUp` stays in col 1 -> item 4 (row 1, col 1)!
    focus_and_scroll(&mut ui, 25, 0);
    press(&mut ui, Key::DirectionUp, false);
    assert_eq!(focused_index.get(), Some(4));

    // - `DirectionDown` steps `beyond_lines` forward to row 9 -> item 28 (row 9, col 1)!
    focus_and_scroll(&mut ui, 25, 0);
    press(&mut ui, Key::DirectionDown, false);
    assert_eq!(focused_index.get(), Some(28));

    // - `Shift+Tab` (`Previous`) moves from `pinned_after` [25] to last visible item 5!
    focus_and_scroll(&mut ui, 25, 0);
    press(&mut ui, Key::Tab, true);
    assert_eq!(focused_index.get(), Some(5));

    // - `Tab` (`Next`) steps `beyond_lines` forward to row 8 ([24, 25, 26]) -> item 26!
    focus_and_scroll(&mut ui, 25, 0);
    press(&mut ui, Key::Tab, false);
    assert_eq!(focused_index.get(), Some(26));
}

#[test]
fn focused_reversed_rtl_grid_item_scrolled_out_of_view_moves_focus_by_keys() {
    let focused_index: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let requesters: Rc<RefCell<BTreeMap<usize, FocusRequester>>> = Rc::default();
    let composed: Rc<RefCell<BTreeSet<usize>>> = Rc::default();

    let (fi, reqs, comp) = (focused_index.clone(), requesters.clone(), composed.clone());
    let state = LazyGridState::new(0, 0);
    let grid_state = *state;
    std::mem::forget(state);

    let mut ui = rule_sized(200, 200, move || {
        let (fi, reqs, comp) = (fi.clone(), reqs.clone(), comp.clone());
        with_direction(LayoutDirection::Rtl, move || {
            let (fi, reqs, comp) = (fi.clone(), reqs.clone(), comp.clone());
            LazyVerticalGrid!(
                GridCells::Fixed(3),
                modifier = Modifier.size((90.dp(), 40.dp())),
                state = grid_state,
                reverse_layout = true,
                move |scope: LazyGridScope| {
                    let (fi, reqs, comp) = (fi.clone(), reqs.clone(), comp.clone());
                    scope.items(30, move |i| {
                        let requester = remember(FocusRequester::new);
                        reqs.borrow_mut().insert(i, requester);
                        let comp2 = comp.clone();
                        DisposableEffect(i, move |effects| {
                            comp2.borrow_mut().insert(i);
                            let comp3 = comp2.clone();
                            effects.on_dispose(move || {
                                comp3.borrow_mut().remove(&i);
                            })
                        });
                        let fi2 = fi.clone();
                        Box!(
                            modifier = Modifier
                                .size((30.dp(), 20.dp()))
                                .focus_requester(requester)
                                .on_focus_changed(move |state: compose_ui::FocusState| {
                                    if state.has_focus() {
                                        fi2.set(Some(i));
                                    }
                                })
                                .focusable()
                        );
                    });
                }
            );
        });
    });
    ui.host().set_keys_navigate(true);

    let focus_and_scroll = |ui: &mut ComposeTestRule, item: usize, scroll_target: usize| {
        let _ = run(ui, async move { grid_state.scroll_to_item(item / 3 * 3, 0).await });
        ui.wait_for_idle();
        let req = *requesters.borrow().get(&item).expect("target composed");
        assert!(req.request_focus());
        ui.wait_for_idle();
        assert_eq!(focused_index.get(), Some(item));
        let _ = run(ui, async move { grid_state.scroll_to_item(scroll_target, 0).await });
        let mid = (item + scroll_target) / 2;
        wait_until(ui, || !composed.borrow().contains(&mid));
    };

    // In `reverse_layout = true` + `Rtl`:
    // Logical col 2 is on the left (`x = 0..30`), col 0 on the right (`x = 60..90`).
    // Focus item 5 (row 1, col 2 = left column), scroll to 24 (row 8 at y=20..40, row 9 at y=0..20):
    // Item 5 is in `pinned_before` below the viewport (`y = 40..60`, `x = 0..30`):
    // - `DirectionUp` stays in the left column (col 2) and lands on item 26 (row 8, col 2)!
    focus_and_scroll(&mut ui, 5, 24);
    ui.host().send_key(KeyEvent::down(Key::DirectionUp));
    ui.host().send_key(KeyEvent::up(Key::DirectionUp));
    ui.wait_for_idle();
    assert_eq!(focused_index.get(), Some(26));

    // - `DirectionDown` steps `ahead_lines` toward row 0 (below row 1) and lands on item 2 (row 0, col 2)!
    focus_and_scroll(&mut ui, 5, 24);
    ui.host().send_key(KeyEvent::down(Key::DirectionDown));
    ui.host().send_key(KeyEvent::up(Key::DirectionDown));
    ui.wait_for_idle();
    assert_eq!(focused_index.get(), Some(2));
}

// ---- D1926: PlatformTextToolbar subset ordering and callback updates -------------------------

#[derive(Default)]
struct ToolbarModes {
    callback: RefCell<Option<Rc<dyn TextActionModeCallback>>>,
}

impl TextActionModes for ToolbarModes {
    fn start(&self, callback: Rc<dyn TextActionModeCallback>) -> Option<Rc<dyn TextActionMode>> {
        *self.callback.borrow_mut() = Some(callback);
        struct NoopMode;
        impl TextActionMode for NoopMode {
            fn invalidate(&self) {}
            fn invalidate_content_rect(&self) {}
            fn finish(&self) {}
        }
        Some(Rc::new(NoopMode))
    }
}

#[test]
fn platform_text_toolbar_subsets_and_prepare_keep_menu_item_option_orders() {
    let modes = Rc::new(ToolbarModes::default());
    let toolbar = PlatformTextToolbar::new(Some(modes.clone()));
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let mk = |tag: &'static str| -> Option<TextToolbarAction> {
        let log = log.clone();
        Some(Rc::new(move || log.borrow_mut().push(tag)))
    };

    // Show only Paste (order 1) and Autofill (order 4).
    toolbar.show_menu(Rect::ZERO, None, mk("paste1"), None, None, mk("autofill1"));
    let cb = modes.callback.borrow().clone().expect("started");
    let mut menu = TextActionMenu::new();
    assert!(cb.on_create_action_mode(&mut menu));
    let orders: Vec<(i32, i32)> = menu.items().iter().map(|i| (i.id, i.order)).collect();
    assert_eq!(orders, [(text_action_ids::PASTE, 1), (text_action_ids::AUTOFILL, 4)]);

    // Update toolbar to Cut (order 2) and SelectAll (order 3) and re-prepare the same menu.
    toolbar.show_menu(Rect::ZERO, None, None, mk("cut2"), mk("select_all2"), None);
    assert!(cb.on_prepare_action_mode(&mut menu));
    let orders: Vec<(i32, i32)> = menu.items().iter().map(|i| (i.id, i.order)).collect();
    assert_eq!(orders, [(text_action_ids::CUT, 2), (text_action_ids::SELECT_ALL, 3)]);

    // Paste is no longer in the menu; clicking Cut and SelectAll runs the updated callbacks.
    assert!(menu.items().iter().all(|item| item.id != text_action_ids::PASTE));
    for item in menu.items() {
        (item.on_click)();
    }
    assert_eq!(*log.borrow(), ["cut2", "select_all2"]);
}
