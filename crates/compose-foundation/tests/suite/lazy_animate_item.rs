//! `Modifier::animate_item` on lazy lists: places, moves out of view, fades in and out.

#![allow(non_snake_case)]

use compose_animation::{AnimationSpec, LINEAR_EASING, tween};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, Color, Dp, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, PaddingValues, UnitExt,
};

use crate::lazy_animate_support::*;
use crate::scroll_support::run;

const ITEM: i32 = 40;

/// Scrolls `by` pixels and lays out what that changes, the clock standing.
fn scroll_list(ui: &mut ComposeTestRule, state: LazyListState, by: i32) {
    ui.run_on_ui_thread(move |_| state.dispatch_raw_delta(by as f32));
    ui.host().frame();
}
const LIST_TAG: &str = "list";

fn dp(px: i32) -> Dp {
    px.dp()
}

fn f(px: i32) -> f32 {
    px as f32
}

/// What Compose's placement test parameterizes.
#[derive(Clone, Copy, Debug)]
struct Params {
    vertical: bool,
    reverse: bool,
}

const PARAMS: [Params; 4] = [
    Params { vertical: true, reverse: false },
    Params { vertical: false, reverse: false },
    Params { vertical: true, reverse: true },
    Params { vertical: false, reverse: true },
];

/// Compose's tests' `LazyList`.
struct ListOptions {
    /// The arrangement: from the end when reversed, unless one of these says otherwise.
    centered: bool,
    space_between: bool,
    from_start: bool,
    arrangement_spacing: Option<i32>,
    min_size: i32,
    max_size: i32,
    start_index: usize,
    cross_size: Option<i32>,
    alignment: Alignment,
    start_padding: i32,
    end_padding: i32,
}

impl Default for ListOptions {
    fn default() -> ListOptions {
        ListOptions {
            centered: false,
            space_between: false,
            from_start: false,
            arrangement_spacing: None,
            min_size: 0,
            max_size: ITEM * 2,
            start_index: 0,
            cross_size: None,
            alignment: Alignment::TOP_START,
            start_padding: 0,
            end_padding: 0,
        }
    }
}

fn lazy_list(
    params: Params,
    options: ListOptions,
    state: LazyListState,
    content: impl Fn(LazyListScope) + Clone + 'static,
) {
    let Params { vertical, reverse } = params;
    let main = |modifier: Modifier| {
        if vertical {
            modifier.height_in(dp(options.min_size), dp(options.max_size))
        } else {
            modifier.width_in(dp(options.min_size), dp(options.max_size))
        }
    };
    let cross = |modifier: Modifier| match (options.cross_size, vertical) {
        (Some(size), true) => modifier.required_width(dp(size)),
        (Some(size), false) => modifier.required_height(dp(size)),
        (None, true) => modifier.fill_max_width(),
        (None, false) => modifier.fill_max_height(),
    };
    let modifier = cross(main(Modifier)).test_tag(LIST_TAG);
    let padding = |start: i32, end: i32| {
        if vertical {
            PaddingValues { top: dp(start), bottom: dp(end), ..PaddingValues::ZERO }
        } else {
            PaddingValues { start: dp(start), end: dp(end), ..PaddingValues::ZERO }
        }
    };
    let content_padding = padding(options.start_padding, options.end_padding);
    let spacing = options.arrangement_spacing;
    let alignment = options.alignment;
    if vertical {
        let arrangement: VerticalArrangement = match spacing {
            Some(spacing) => Arrangement::spaced_by(dp(spacing)).into(),
            None if options.centered => Arrangement::CENTER.into(),
            None if options.space_between => Arrangement::SPACE_BETWEEN.into(),
            None if reverse && !options.from_start => Arrangement::BOTTOM,
            None => Arrangement::TOP,
        };
        let horizontal = if alignment == Alignment::CENTER {
            Alignment::CENTER_HORIZONTALLY
        } else if alignment == Alignment::BOTTOM_END {
            Alignment::END
        } else {
            Alignment::START
        };
        LazyColumn!(
            modifier = modifier,
            state = state,
            content_padding = content_padding,
            reverse_layout = reverse,
            vertical_arrangement = arrangement,
            horizontal_alignment = horizontal,
            content = content
        );
    } else {
        let arrangement: HorizontalArrangement = match spacing {
            Some(spacing) => Arrangement::spaced_by(dp(spacing)).into(),
            None if options.centered => Arrangement::CENTER.into(),
            None if options.space_between => Arrangement::SPACE_BETWEEN.into(),
            None if reverse && !options.from_start => Arrangement::END,
            None => Arrangement::START,
        };
        let vertical_alignment = if alignment == Alignment::CENTER {
            Alignment::CENTER_VERTICALLY
        } else if alignment == Alignment::BOTTOM_END {
            Alignment::BOTTOM
        } else {
            Alignment::TOP
        };
        LazyRow!(
            modifier = modifier,
            state = state,
            content_padding = content_padding,
            reverse_layout = reverse,
            horizontal_arrangement = arrangement,
            vertical_alignment = vertical_alignment,
            content = content
        );
    }
}

/// An item as Compose's placement test's `Item`: `tag`, `size` long and as much across, moved by
/// `spec` alone.
fn item(
    scope: &LazyItemScope,
    params: Params,
    tag: i32,
    size: i32,
    cross_size: i32,
    spec: Option<AnimationSpec>,
) {
    let base = match spec {
        Some(spec) => Modifier.animate_item_with(scope, None, Some(spec), None),
        None => Modifier,
    };
    let sized = if params.vertical {
        base.required_height(dp(size)).required_width(dp(cross_size))
    } else {
        base.required_width(dp(size)).required_height(dp(cross_size))
    };
    Box!(modifier = sized.test_tag(tag.to_string()));
}

/// Items of `list`, each `ITEM` long and with Compose's `AnimSpec`.
fn plain_items(scope: &LazyListScope, list: Vec<i32>, params: Params) {
    scope.items_list_keyed(
        list,
        |it: &i32| *it,
        move |item_scope, it: &i32| item(&item_scope, params, *it, ITEM, ITEM, Some(anim_spec())),
    );
}

struct Fixture {
    ui: ComposeTestRule,
    list: OwnedState<Vec<i32>>,
    state: LazyListState,
    params: Params,
}

impl Fixture {
    fn new(params: Params, options: ListOptions, list: Vec<i32>) -> Fixture {
        let list = mutable_state_of(list);
        let handle = list.handle();
        let state = LazyListState::new(options.start_index, 0);
        let state_handle = *state;
        let ui = paused(IntSize::new(1000, 1000), move || {
            let _ = &state;
            let items = handle.get();
            lazy_list(params, options_again(&options), state_handle, move |scope| {
                plain_items(&scope, items.clone(), params);
            });
        });
        Fixture { ui, list, state: state_handle, params }
    }

    fn set_list(&mut self, list: Vec<i32>) {
        let handle = self.list.handle();
        self.ui.run_on_ui_thread(move |_| handle.set(list));
    }

    fn assert_positions(&mut self, expected: &[(i32, f32)], fraction: Option<f32>) {
        assert_positions_of(&mut self.ui, self.params, expected, fraction, self.params.reverse);
    }
}

fn options_again(options: &ListOptions) -> ListOptions {
    ListOptions {
        centered: options.centered,
        space_between: options.space_between,
        from_start: options.from_start,
        arrangement_spacing: options.arrangement_spacing,
        min_size: options.min_size,
        max_size: options.max_size,
        start_index: options.start_index,
        cross_size: options.cross_size,
        alignment: options.alignment,
        start_padding: options.start_padding,
        end_padding: options.end_padding,
    }
}

/// Compose's `assertPositions`: along the axis, from the other end when reversed.
fn assert_positions_of(
    ui: &mut ComposeTestRule,
    params: Params,
    expected: &[(i32, f32)],
    fraction: Option<f32>,
    auto_reverse: bool,
) {
    let main = |pair: (i32, i32)| if params.vertical { pair.1 } else { pair.0 };
    let container = main(size_of(ui, LIST_TAG));
    let container_at = main(position_of(ui, LIST_TAG));
    let actual: Vec<(i32, i32)> = expected
        .iter()
        .map(|(tag, _)| (*tag, main(position_of(ui, &tag.to_string())) - container_at))
        .collect();
    let wanted: Vec<(i32, i32)> = expected
        .iter()
        .map(|(tag, at)| {
            let at = at.round() as i32;
            let size = main(size_of(ui, &tag.to_string()));
            (*tag, if auto_reverse { container - size - at } else { at })
        })
        .collect();
    assert_eq!(actual, wanted, "{params:?} at fraction {fraction:?}");
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, reorderTwoItems
#[test]
fn two_items_reordered_swap_over_the_animation() {
    for params in PARAMS {
        let mut fixture = Fixture::new(params, ListOptions::default(), vec![0, 1]);
        fixture.assert_positions(&[(0, 0.0), (1, f(ITEM))], None);
        fixture.set_list(vec![1, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(0, f(ITEM) * fraction), (1, f(ITEM) * (1.0 - fraction))],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, reorderTwoItems_layoutInfoHasFinalPositions
#[test]
fn the_layout_info_has_the_final_positions_at_once() {
    for params in PARAMS {
        let mut fixture = Fixture::new(params, ListOptions::default(), vec![0, 1]);
        fixture.set_list(vec![1, 0]);
        let state = fixture.state;
        on_animation_frame(&mut fixture.ui, |_, _| {
            let offsets: Vec<(usize, i32)> = state
                .layout_info()
                .visible_items_info
                .iter()
                .map(|item| (item.index, item.offset))
                .collect();
            assert_eq!(offsets, vec![(0, 0), (1, ITEM)], "{params:?}");
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveFirstItemToEndCausingAllItemsToAnimate
#[test]
fn the_first_item_moved_to_the_end_moves_all() {
    for params in PARAMS {
        let options = ListOptions { max_size: ITEM * 4, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3]);
        fixture.set_list(vec![1, 2, 3, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (0, f(ITEM * 3) * fraction),
                    (1, f(ITEM) - f(ITEM) * fraction),
                    (2, f(ITEM * 2) - f(ITEM) * fraction),
                    (3, f(ITEM * 3) - f(ITEM) * fraction),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveItemToTheBottomOutsideOfBounds
#[test]
fn an_item_moved_past_the_end_stays_composed_while_it_leaves() {
    for params in PARAMS {
        let options = ListOptions { max_size: ITEM * 3, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4, 5]);
        fixture.assert_positions(&[(0, 0.0), (1, f(ITEM)), (2, f(ITEM * 2))], None);
        fixture.set_list(vec![0, 4, 2, 3, 1, 5]);
        let list = ITEM * 3;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = f(ITEM) + f(list - ITEM) * fraction;
            let item4 = f(list) - f(list - ITEM) * fraction;
            let mut expected = vec![(0, 0.0), (2, f(ITEM * 2))];
            for (tag, at) in [(1, item1), (4, item4)] {
                if at < f(list) {
                    expected.push((tag, at));
                } else {
                    ui.on_node_with_tag(&tag.to_string()).assert_is_not_displayed();
                }
            }
            assert_positions_of(ui, params, &expected, Some(fraction), params.reverse);
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveItemToTheTopOutsideOfBounds
#[test]
fn an_item_moved_past_the_start_stays_composed_while_it_leaves() {
    for params in PARAMS {
        let options = ListOptions { start_index: 3, max_size: ITEM * 3, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4, 5]);
        fixture.assert_positions(&[(3, 0.0), (4, f(ITEM)), (5, f(ITEM * 2))], None);
        fixture.set_list(vec![2, 4, 0, 3, 1, 5]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = -f(ITEM) + f(ITEM * 2) * fraction;
            let item4 = f(ITEM) - f(ITEM * 2) * fraction;
            let mut expected = vec![(3, 0.0), (5, f(ITEM * 2))];
            for (tag, at) in [(1, item1), (4, item4)] {
                if at > -f(ITEM) {
                    expected.push((tag, at));
                } else {
                    ui.on_node_with_tag(&tag.to_string()).assert_is_not_displayed();
                }
            }
            assert_positions_of(ui, params, &expected, Some(fraction), params.reverse);
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenScrollForwardBySmallOffset
#[test]
fn a_small_scroll_animates_nothing() {
    for params in PARAMS {
        let options = ListOptions { max_size: ITEM * 3, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4]);
        let state = fixture.state;
        scroll_list(&mut fixture.ui, state, ITEM / 2);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let half = f(ITEM / 2);
            assert_positions_of(
                ui,
                params,
                &[(0, -half), (1, f(ITEM) - half), (2, f(ITEM * 2) - half)],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenScrolledToOtherPosition
#[test]
fn a_jump_animates_nothing() {
    for params in PARAMS {
        let options = ListOptions { max_size: ITEM * 3, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let state = fixture.state;
        run(&mut fixture.ui, async move {
            state.scroll_to_item(0, ITEM / 2).await.ok();
        });
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let half = f(ITEM / 2);
            assert_positions_of(
                ui,
                params,
                &[(0, -half), (1, half), (2, half * 3.0), (3, half * 5.0)],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, scrollIsAffectingItemsMovingWithinViewport
#[test]
fn a_scroll_moves_the_items_moving_in_view() {
    for params in PARAMS {
        let mut fixture = Fixture::new(params, ListOptions::default(), vec![0, 1, 2, 3]);
        let scroll = SPACING;
        fixture.set_list(vec![0, 2, 1, 3]);
        let state = fixture.state;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            if fraction == 0.0 {
                assert_positions_of(
                    ui,
                    params,
                    &[(0, 0.0), (1, f(ITEM)), (2, f(ITEM * 2))],
                    Some(fraction),
                    params.reverse,
                );
                scroll_list(ui, state, scroll);
            }
            let at = |start: f32, end: f32| start + (end - start) * fraction;
            assert_positions_of(
                ui,
                params,
                &[
                    (0, -f(scroll)),
                    (1, at(f(ITEM), f(ITEM * 2)) - f(scroll)),
                    (2, at(f(ITEM * 2), f(ITEM)) - f(scroll)),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

/// A colored item for the fade tests, with `AnimSpec`, or no spec.
fn colored_item(
    scope: &LazyItemScope,
    vertical: bool,
    color: Color,
    fade_in: Option<AnimationSpec>,
    fade_out: Option<AnimationSpec>,
) {
    let (main, cross) = (dp(ITEM), dp(ITEM));
    let modifier = Modifier.animate_item_with(scope, fade_in, None, fade_out);
    let modifier =
        if vertical { modifier.size((cross, main)) } else { modifier.size((main, cross)) };
    Box!(modifier = modifier.background(color));
}

struct FadeList {
    colors: Vec<Color>,
    vertical: bool,
    reverse: bool,
    wrap: bool,
    max_size: i32,
    padding: PaddingValues,
    direction: LayoutDirection,
    fade_in: fn(Color) -> Option<AnimationSpec>,
    fade_out: fn(Color) -> Option<AnimationSpec>,
}

fn with_anim_spec(_: Color) -> Option<AnimationSpec> {
    Some(anim_spec())
}

fn no_spec(_: Color) -> Option<AnimationSpec> {
    None
}

impl Default for FadeList {
    fn default() -> FadeList {
        FadeList {
            colors: Vec::new(),
            vertical: true,
            reverse: false,
            wrap: false,
            max_size: ITEM * 2,
            padding: PaddingValues::ZERO,
            direction: LayoutDirection::Ltr,
            fade_in: with_anim_spec,
            fade_out: with_anim_spec,
        }
    }
}

struct FadeFixture {
    ui: ComposeTestRule,
    list: OwnedState<Vec<Color>>,
    state: LazyListState,
}

fn fade_fixture(options: FadeList) -> FadeFixture {
    let FadeList {
        colors,
        vertical,
        reverse,
        wrap,
        max_size,
        padding,
        direction,
        fade_in,
        fade_out,
    } = options;
    let list = mutable_state_of(colors);
    let handle = list.handle();
    let state = LazyListState::new(0, 0);
    let state_handle = *state;
    let ui = paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let colors = handle.get();
        let content = move |scope: LazyListScope| {
            scope.items_list_keyed(
                colors.clone(),
                |color: &Color| color.argb(),
                move |item_scope, color: &Color| {
                    colored_item(&item_scope, vertical, *color, fade_in(*color), fade_out(*color));
                },
            );
        };
        let sized = |modifier: Modifier| {
            let main = if wrap {
                if vertical {
                    modifier.wrap_content_height()
                } else {
                    modifier.wrap_content_width()
                }
            } else if vertical {
                modifier.height(dp(max_size))
            } else {
                modifier.width(dp(max_size))
            };
            if vertical { main.width(dp(ITEM)) } else { main.height(dp(ITEM)) }
        };
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
            let content = content.clone();
            if vertical {
                LazyColumn!(
                    modifier = sized(Modifier).test_tag(LIST_TAG),
                    state = state_handle,
                    reverse_layout = reverse,
                    content_padding = padding,
                    content = content
                );
            } else {
                LazyRow!(
                    modifier = sized(Modifier).test_tag(LIST_TAG),
                    state = state_handle,
                    reverse_layout = reverse,
                    content_padding = padding,
                    content = content
                );
            }
        });
    });
    FadeFixture { ui, list, state: state_handle }
}

impl FadeFixture {
    fn set(&mut self, colors: Vec<Color>) {
        let handle = self.list.handle();
        self.ui.run_on_ui_thread(move |_| handle.set(colors));
    }
}

/// The color each `ITEM` along the axis shows, with the alpha it should have (`None`: nothing).
fn assert_slots(
    ui: &mut ComposeTestRule,
    vertical: bool,
    fraction: f32,
    expected: &[Option<(Color, f32)>],
) {
    for (slot, wanted) in expected.iter().enumerate() {
        let main = (slot as i32 * ITEM + ITEM / 2) as f32;
        let (x, y) = if vertical { (f(ITEM / 2), main) } else { (main, f(ITEM / 2)) };
        let color = color_at(ui, x, y);
        match wanted {
            Some((wanted, alpha)) => assert!(
                color.is(*wanted, *alpha),
                "slot {slot} at fraction {fraction}: {color:?}, wanted {wanted:?} at {alpha}"
            ),
            None => assert!(
                color.is_transparent(),
                "slot {slot} at fraction {fraction}: {color:?}, wanted nothing"
            ),
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, oneItemAdded
#[test]
fn an_item_added_fades_in() {
    let mut fixture = fade_fixture(FadeList { max_size: ITEM, ..FadeList::default() });
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_slots(ui, true, fraction, &[Some((Color::BLACK, fraction))]);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, noAnimationForInitialList
#[test]
fn the_first_items_show_at_once() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK],
        max_size: ITEM,
        ..FadeList::default()
    });
    assert_slots(&mut fixture.ui, true, 0.0, &[Some((Color::BLACK, 1.0))]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, oneExistTwoAdded
#[test]
fn only_those_added_fade_in() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK],
        max_size: ITEM * 3,
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK, Color::RED, Color::GREEN]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_slots(
            ui,
            true,
            fraction,
            &[
                Some((Color::BLACK, 1.0)),
                Some((Color::RED, fraction)),
                Some((Color::GREEN, fraction)),
            ],
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, onlyItemWithSpecsIsAnimating
#[test]
fn only_an_item_with_a_fade_in_fades_in() {
    fn red_only(color: Color) -> Option<AnimationSpec> {
        (color == Color::RED).then(anim_spec)
    }
    let mut fixture = fade_fixture(FadeList { fade_in: red_only, ..FadeList::default() });
    fixture.set(vec![Color::BLACK, Color::RED]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_slots(
            ui,
            true,
            fraction,
            &[Some((Color::BLACK, 1.0)), Some((Color::RED, fraction))],
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, oneItemRemoved
#[test]
fn an_item_removed_fades_out_after_it_left_the_composition() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK],
        max_size: ITEM,
        ..FadeList::default()
    });
    fixture.set(Vec::new());
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_slots(ui, true, fraction, &[Some((Color::BLACK, 1.0 - fraction))]);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, threeExistTwoRemoved
#[test]
fn only_those_removed_fade_out() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::RED, Color::GREEN],
        max_size: ITEM * 3,
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        let alpha = 1.0 - fraction;
        assert_slots(
            ui,
            true,
            fraction,
            &[Some((Color::BLACK, 1.0)), Some((Color::RED, alpha)), Some((Color::GREEN, alpha))],
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, threeExistTwoRemoved_reverseLayout
#[test]
fn those_removed_from_a_reversed_list_fade_out_where_they_were() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::RED, Color::GREEN],
        max_size: ITEM * 3,
        reverse: true,
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        let alpha = 1.0 - fraction;
        assert_slots(
            ui,
            true,
            fraction,
            &[Some((Color::GREEN, alpha)), Some((Color::RED, alpha)), Some((Color::BLACK, 1.0))],
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, oneRemoved_reverseLayout_contentPadding
#[test]
fn a_removed_item_fades_out_over_a_reversed_lists_padding() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::RED],
        max_size: ITEM * 3,
        reverse: true,
        padding: PaddingValues { bottom: dp(ITEM), ..PaddingValues::ZERO },
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_slots(
            ui,
            true,
            fraction,
            &[Some((Color::RED, 1.0 - fraction)), Some((Color::BLACK, 1.0)), None],
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, oneRemoved_rtl_contentPadding
#[test]
fn a_removed_item_fades_out_where_it_was_right_to_left() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::RED],
        max_size: ITEM * 3,
        vertical: false,
        padding: PaddingValues { start: dp(ITEM), ..PaddingValues::ZERO },
        direction: LayoutDirection::Rtl,
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_slots(
            ui,
            false,
            fraction,
            &[Some((Color::RED, 1.0 - fraction)), Some((Color::BLACK, 1.0)), None],
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, onlyItemWithSpecsIsAnimating
#[test]
fn only_an_item_with_a_fade_out_fades_out() {
    fn red_only(color: Color) -> Option<AnimationSpec> {
        (color == Color::RED).then(anim_spec)
    }
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::RED],
        fade_out: red_only,
        ..FadeList::default()
    });
    fixture.set(Vec::new());
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_slots(ui, true, fraction, &[None, Some((Color::RED, 1.0 - fraction))]);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, itemsBeingRemovedAreAffectingTheContainerSizeForTheDurationOfAnimation
#[test]
fn an_item_fading_out_keeps_the_room_it_took() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::RED],
        wrap: true,
        ..FadeList::default()
    });
    assert_eq!(size_of(&mut fixture.ui, LIST_TAG).1, ITEM * 2);
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        let height = size_of(ui, LIST_TAG).1;
        assert_eq!(height, if fraction < 1.0 { ITEM * 2 } else { ITEM }, "fraction {fraction}");
        if fraction < 1.0 {
            assert_slots(
                ui,
                true,
                fraction,
                &[Some((Color::BLACK, 1.0)), Some((Color::RED, 1.0 - fraction))],
            );
        } else {
            assert_slots(ui, true, fraction, &[Some((Color::BLACK, 1.0))]);
        }
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, itemsBeingRemovedAreAffectingTheContainerSizeForTheDurationOfAnimation_reverseLayout
#[test]
fn an_item_fading_out_of_a_reversed_list_keeps_the_room_it_took() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::RED],
        wrap: true,
        reverse: true,
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        let height = size_of(ui, LIST_TAG).1;
        assert_eq!(height, if fraction < 1.0 { ITEM * 2 } else { ITEM }, "fraction {fraction}");
        if fraction < 1.0 {
            assert_slots(
                ui,
                true,
                fraction,
                &[Some((Color::RED, 1.0 - fraction)), Some((Color::BLACK, 1.0))],
            );
        } else {
            assert_slots(ui, true, fraction, &[Some((Color::BLACK, 1.0))]);
        }
    });
}

/// `animate_item` costs no composition in a scroll.
#[test]
fn a_scroll_with_animate_item_recomposes_no_item() {
    let compositions = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = compositions.clone();
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let counter = counter.clone();
        LazyColumn!(
            modifier = Modifier.height(dp(ITEM * 2)),
            state = handle,
            move |scope: LazyListScope| {
                let item_scope = scope.item_scope();
                let counter = counter.clone();
                scope.items_keyed(
                    20,
                    |index| index,
                    move |index| {
                        // The next one comes once, by prefetch.
                        if index < 2 {
                            counter.set(counter.get() + 1);
                        }
                        item(&item_scope, PARAMS[0], index as i32, ITEM, ITEM, Some(anim_spec()));
                    },
                );
            }
        );
    });
    ui.wait_for_idle();
    let before = compositions.get();
    for _ in 0..4 {
        scroll_list(&mut ui, handle, 1);
    }
    assert_eq!(compositions.get(), before);
}

const ITEM2: i32 = 24;
const ITEM3: i32 = 16;
const SPACING: i32 = 8;

/// A list recomposing with `list`'s items, each given to `content`.
fn fixture_of(
    params: Params,
    options: ListOptions,
    list: Vec<i32>,
    content: impl Fn(&LazyListScope, Vec<i32>) + Clone + 'static,
) -> Fixture {
    let list = mutable_state_of(list);
    let handle = list.handle();
    let state = LazyListState::new(options.start_index, 0);
    let state_handle = *state;
    let ui = paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let items = handle.get();
        let content = content.clone();
        lazy_list(params, options_again(&options), state_handle, move |scope| {
            content(&scope, items.clone());
        });
    });
    Fixture { ui, list, state: state_handle, params }
}

/// Keyed items with `AnimSpec` of the sizes `size_of` gives by tag.
fn sized_items(
    params: Params,
    size_of: fn(i32) -> i32,
) -> impl Fn(&LazyListScope, Vec<i32>) + Clone {
    move |scope: &LazyListScope, list: Vec<i32>| {
        scope.items_list_keyed(
            list,
            |it: &i32| *it,
            move |item_scope, it: &i32| {
                item(&item_scope, params, *it, size_of(*it), ITEM, Some(anim_spec()));
            },
        );
    }
}

/// Keyed items with a move's spec only as `spec` says, by tag.
fn specced_items(
    params: Params,
    spec: fn(i32) -> Option<AnimationSpec>,
) -> impl Fn(&LazyListScope, Vec<i32>) + Clone {
    move |scope: &LazyListScope, list: Vec<i32>| {
        scope.items_list_keyed(
            list,
            |it: &i32| *it,
            move |item_scope, it: &i32| item(&item_scope, params, *it, ITEM, ITEM, spec(*it)),
        );
    }
}

/// For the `items` still in view, per `in_view`, Compose's `assertPositions`; those out of it aren't
/// displayed.
fn assert_in_view(
    ui: &mut ComposeTestRule,
    params: Params,
    fraction: f32,
    items: &[(i32, f32)],
    in_view: impl Fn(f32) -> bool,
) {
    let mut expected = Vec::new();
    for &(tag, at) in items {
        if in_view(at) {
            expected.push((tag, at));
        } else {
            ui.on_node_with_tag(&tag.to_string()).assert_is_not_displayed();
        }
    }
    assert_positions_of(ui, params, &expected, Some(fraction), params.reverse);
}

fn five_items_options() -> ListOptions {
    ListOptions { max_size: ITEM * 5, ..ListOptions::default() }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, reorderFirstAndLastItems
#[test]
fn the_first_and_the_last_items_swap() {
    for params in PARAMS {
        let mut fixture = Fixture::new(params, five_items_options(), vec![0, 1, 2, 3, 4]);
        fixture.set_list(vec![4, 1, 2, 3, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (0, f(ITEM * 4) * fraction),
                    (1, f(ITEM)),
                    (2, f(ITEM * 2)),
                    (3, f(ITEM * 3)),
                    (4, f(ITEM * 4) - f(ITEM * 4) * fraction),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, itemSizeChangeAnimatesNextItems
#[test]
fn an_item_growing_moves_the_next_ones() {
    for params in PARAMS {
        let size = mutable_state_of(ITEM);
        let size_handle = size.handle();
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = paused(IntSize::new(1000, 1000), move || {
            let _ = &state;
            let grown = size_handle.get();
            let options = ListOptions { min_size: ITEM * 5, ..five_items_options() };
            lazy_list(params, options, handle, move |scope| {
                scope.items_list_keyed(
                    vec![0, 1, 2, 3],
                    |it: &i32| *it,
                    move |item_scope, it: &i32| {
                        let main = if *it == 1 { grown } else { ITEM };
                        item(&item_scope, params, *it, main, ITEM, Some(anim_spec()));
                    },
                );
            });
        });
        ui.run_on_ui_thread(|_| size.set(ITEM * 2));
        on_animation_frame(&mut ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (0, 0.0),
                    (1, f(ITEM)),
                    (2, f(ITEM * 2) + f(ITEM) * fraction),
                    (3, f(ITEM * 3) + f(ITEM) * fraction),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, onlyItemsWithModifierAnimates
#[test]
fn only_the_items_with_the_modifier_animate() {
    for params in PARAMS {
        fn some(tag: i32) -> Option<AnimationSpec> {
            (tag == 1 || tag == 3).then(anim_spec)
        }
        let content = specced_items(params, some);
        let mut fixture = fixture_of(params, five_items_options(), vec![0, 1, 2, 3, 4], content);
        fixture.set_list(vec![1, 2, 3, 4, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (0, f(ITEM * 4)),
                    (1, f(ITEM) - f(ITEM) * fraction),
                    (2, f(ITEM)),
                    (3, f(ITEM * 3) - f(ITEM) * fraction),
                    (4, f(ITEM * 3)),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, animationsWithDifferentDurations
#[test]
fn each_item_takes_the_time_of_its_spec() {
    for params in PARAMS {
        let content = move |scope: &LazyListScope, list: Vec<i32>| {
            scope.items_list_keyed(
                list,
                |it: &i32| *it,
                move |item_scope, it: &i32| {
                    let duration = if *it == 1 || *it == 3 { DURATION * 2 } else { DURATION };
                    let spec = tween(duration as u32).easing(LINEAR_EASING).into();
                    item(&item_scope, params, *it, ITEM, ITEM, Some(spec));
                },
            );
        };
        let mut fixture = fixture_of(params, five_items_options(), vec![0, 1, 2, 3, 4], content);
        fixture.set_list(vec![1, 2, 3, 4, 0]);
        on_animation_frame_of(&mut fixture.ui, DURATION * 2, |ui, fraction| {
            let shorter = (fraction * 2.0).min(1.0);
            assert_positions_of(
                ui,
                params,
                &[
                    (0, f(ITEM * 4) * shorter),
                    (1, f(ITEM) - f(ITEM) * fraction),
                    (2, f(ITEM * 2) - f(ITEM) * shorter),
                    (3, f(ITEM * 3) - f(ITEM) * fraction),
                    (4, f(ITEM * 4) - f(ITEM) * shorter),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

fn two_child_items(
    params: Params,
    second: Option<AnimationSpec>,
) -> impl Fn(&LazyListScope, Vec<i32>) + Clone {
    move |scope: &LazyListScope, list: Vec<i32>| {
        let second = second.clone();
        scope.items_list_keyed(
            list,
            |it: &i32| *it,
            move |item_scope, it: &i32| {
                item(&item_scope, params, *it, ITEM, ITEM, Some(anim_spec()));
                item(&item_scope, params, *it + 1, ITEM, ITEM, second.clone());
            },
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, multipleChildrenPerItem
#[test]
fn each_child_of_an_item_moves() {
    for params in PARAMS {
        let content = two_child_items(params, Some(anim_spec()));
        let mut fixture = fixture_of(params, five_items_options(), vec![0, 2], content);
        fixture
            .assert_positions(&[(0, 0.0), (1, f(ITEM)), (2, f(ITEM * 2)), (3, f(ITEM * 3))], None);
        fixture.set_list(vec![2, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let moved = f(ITEM * 2) * fraction;
            assert_positions_of(
                ui,
                params,
                &[
                    (0, moved),
                    (1, f(ITEM) + moved),
                    (2, f(ITEM * 2) - moved),
                    (3, f(ITEM * 3) - moved),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, multipleChildrenPerItemSomeDoNotAnimate
#[test]
fn a_child_without_the_modifier_jumps_while_the_other_moves() {
    for params in PARAMS {
        let content = two_child_items(params, None);
        let mut fixture = fixture_of(params, five_items_options(), vec![0, 2], content);
        fixture.set_list(vec![2, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let moved = f(ITEM * 2) * fraction;
            assert_positions_of(
                ui,
                params,
                &[(0, moved), (1, f(ITEM * 3)), (2, f(ITEM * 2) - moved), (3, f(ITEM))],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveFirstItemToEndCausingAllItemsToAnimate_withSpacing
#[test]
fn the_first_item_moved_to_the_end_with_spacing_moves_all() {
    for params in PARAMS {
        let options = ListOptions {
            arrangement_spacing: Some(SPACING),
            max_size: (ITEM + SPACING) * 4,
            ..ListOptions::default()
        };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3]);
        fixture.set_list(vec![1, 2, 3, 0]);
        let step = f(ITEM + SPACING);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (0, step * 3.0 * fraction),
                    (1, step - step * fraction),
                    (2, step * 2.0 - step * fraction),
                    (3, step * 3.0 - step * fraction),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveItemToTheTopOutsideOfBounds_withStickyHeader
#[test]
fn a_header_changes_nothing_of_a_move_past_the_start() {
    for params in PARAMS {
        let options = ListOptions { start_index: 4, ..ListOptions::default() };
        let content = move |scope: &LazyListScope, list: Vec<i32>| {
            scope.sticky_header(|_| {});
            plain_items(scope, list, params);
        };
        let mut fixture = fixture_of(params, options, vec![0, 1, 2, 3, 4], content);
        fixture.assert_positions(&[(3, 0.0), (4, f(ITEM))], None);
        fixture.set_list(vec![2, 4, 0, 3, 1]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = -f(ITEM) + f(ITEM * 2) * fraction;
            let item4 = f(ITEM) - f(ITEM * 2) * fraction;
            assert_in_view(ui, params, fraction, &[(4, item4), (3, 0.0), (1, item1)], |at| {
                at > -f(ITEM)
            });
        });
    }
}

/// Item with a child after it for all but the last, as Compose's `Box(modifier = Modifier)`.
fn items_with_more_children(params: Params) -> impl Fn(&LazyListScope, Vec<i32>) + Clone {
    move |scope: &LazyListScope, list: Vec<i32>| {
        let last = *list.last().unwrap();
        scope.items_list_keyed(
            list,
            |it: &i32| *it,
            move |item_scope, it: &i32| {
                item(&item_scope, params, *it, ITEM, ITEM, Some(anim_spec()));
                if *it != last {
                    Box!(modifier = Modifier);
                }
            },
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveItemToTheTop_itemWithMoreChildren_outsideBounds_shouldNotCrash
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, movingAwayItem_itemWithMoreChildren_crossAxisAlignmentDefined_shouldNotCrash
#[test]
fn an_item_moving_out_with_more_children_than_it_had_is_fine() {
    for params in PARAMS {
        for alignment in [Alignment::TOP_START, Alignment::CENTER] {
            let options = ListOptions {
                start_index: 3,
                max_size: ITEM * 3,
                alignment,
                ..ListOptions::default()
            };
            let content = items_with_more_children(params);
            let mut fixture = fixture_of(params, options, vec![0, 1, 2, 3, 4, 5], content);
            fixture.assert_positions(&[(3, 0.0), (4, f(ITEM)), (5, f(ITEM * 2))], None);
            fixture.set_list(vec![5, 0, 1, 2, 3, 4]);
            on_animation_frame(&mut fixture.ui, |ui, fraction| {
                if fraction == 1.0 {
                    assert_positions_of(
                        ui,
                        params,
                        &[(2, 0.0), (3, f(ITEM)), (4, f(ITEM * 2))],
                        Some(fraction),
                        params.reverse,
                    );
                }
            });
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveItemToTheBottomOutsideOfBounds_withSpacing
#[test]
fn an_item_moved_past_the_end_goes_past_the_spacing() {
    for params in PARAMS {
        let options = ListOptions {
            arrangement_spacing: Some(SPACING),
            max_size: ITEM * 3 + SPACING * 2,
            ..ListOptions::default()
        };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4, 5]);
        let step = f(ITEM + SPACING);
        fixture.assert_positions(&[(0, 0.0), (1, step), (2, step * 2.0)], None);
        fixture.set_list(vec![0, 4, 2, 3, 1, 5]);
        let after_last = f(ITEM * 3 + SPACING * 3);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = step + (after_last - step) * fraction;
            let item4 = after_last - (after_last - step) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[(0, 0.0), (1, item1), (2, step * 2.0), (4, item4)],
                |at| at < after_last,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveItemToTheTopOutsideOfBounds_withSpacing
#[test]
fn an_item_moved_past_the_start_goes_past_the_spacing() {
    for params in PARAMS {
        let options = ListOptions {
            arrangement_spacing: Some(SPACING),
            max_size: ITEM * 3 + SPACING * 2,
            start_index: 3,
            ..ListOptions::default()
        };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let step = f(ITEM + SPACING);
        fixture.assert_positions(&[(3, 0.0), (4, step), (5, step * 2.0)], None);
        fixture.set_list(vec![2, 4, 0, 3, 1, 5, 6, 7]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = -step + (2.0 * step) * fraction;
            let item4 = step - (2.0 * step) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[(4, item4), (3, 0.0), (1, item1), (5, step * 2.0)],
                |at| at > -f(ITEM),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveItemToTheTopOutsideOfBounds_differentSizes
#[test]
fn items_of_different_sizes_moved_past_the_start() {
    for params in PARAMS {
        fn sizes(tag: i32) -> i32 {
            match tag {
                3 => ITEM2,
                1 => ITEM3,
                _ => ITEM,
            }
        }
        let options = ListOptions {
            max_size: ITEM2 + ITEM3 + ITEM,
            start_index: 3,
            ..ListOptions::default()
        };
        let content = sized_items(params, sizes);
        let mut fixture = fixture_of(params, options, vec![0, 1, 2, 3, 4, 5], content);
        fixture.assert_positions(&[(3, 0.0), (4, f(ITEM2)), (5, f(ITEM2 + ITEM))], None);
        fixture.set_list(vec![0, 4, 2, 3, 1, 5]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            // Item 2 was between them and isn't composed.
            ui.on_node_with_tag("2").assert_does_not_exist();
            let item1 = -f(ITEM3) + f(ITEM2 + ITEM3) * fraction;
            let item4 = f(ITEM2) - f(ITEM2 + ITEM) * fraction;
            let item5 = f(ITEM2 + ITEM) - f(ITEM - ITEM3) * fraction;
            let mut expected = vec![(3, 0.0), (5, item5)];
            if item4 > -f(ITEM) {
                expected.push((4, item4));
            } else {
                ui.on_node_with_tag("4").assert_is_not_displayed();
            }
            if item1 > -f(ITEM3) {
                expected.push((1, item1));
            } else {
                ui.on_node_with_tag("1").assert_is_not_displayed();
            }
            assert_positions_of(ui, params, &expected, Some(fraction), params.reverse);
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveItemToTheBottomOutsideOfBounds_differentSizes
#[test]
fn items_of_different_sizes_moved_past_the_end() {
    for params in PARAMS {
        fn sizes(tag: i32) -> i32 {
            match tag {
                0 => ITEM2,
                4 => ITEM3,
                _ => ITEM,
            }
        }
        let list = ITEM2 + ITEM3 + ITEM;
        let options = ListOptions { max_size: list, ..ListOptions::default() };
        let content = sized_items(params, sizes);
        let mut fixture = fixture_of(params, options, vec![0, 1, 2, 3, 4, 5], content);
        fixture.assert_positions(&[(0, 0.0), (1, f(ITEM2)), (2, f(ITEM2 + ITEM))], None);
        fixture.set_list(vec![0, 4, 2, 3, 1, 5]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = f(ITEM2) + f(list - ITEM2) * fraction;
            let item4 = f(list) - f(list - ITEM2) * fraction;
            let item2 = f(ITEM2 + ITEM) - f(ITEM - ITEM3) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[(0, 0.0), (1, item1), (2, item2), (4, item4)],
                |at| at < f(list),
            );
        });
    }
}

/// Across the axis, as Compose's `crossAxis` assertion.
fn assert_cross(ui: &mut ComposeTestRule, params: Params, expected: &[(i32, f32)]) {
    for (tag, at) in expected {
        let (x, y) = position_of(ui, &tag.to_string());
        let (list_x, list_y) = position_of(ui, LIST_TAG);
        let actual = if params.vertical { x - list_x } else { y - list_y };
        assert_eq!(actual, at.round() as i32, "item {tag} across, {params:?}");
    }
}

/// A list of `ITEM` across with three items, narrower by tag, aligned as `alignment` says.
fn aligned_list(
    params: Params,
    alignment: &OwnedState<Alignment>,
    cross_size: i32,
    direction: LayoutDirection,
) -> ComposeTestRule {
    let handle = alignment.handle();
    let state = LazyListState::new(0, 0);
    let state_handle = *state;
    paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let alignment = handle.get();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
            let options = ListOptions {
                alignment,
                cross_size: Some(cross_size),
                max_size: ITEM * 3,
                ..ListOptions::default()
            };
            lazy_list(params, options, state_handle, move |scope| {
                scope.items_list_keyed(
                    vec![1, 2, 3],
                    |it: &i32| *it,
                    move |item_scope, it: &i32| {
                        let narrower = match *it {
                            1 => ITEM,
                            2 => ITEM2,
                            _ => ITEM3,
                        };
                        item(&item_scope, params, *it, ITEM, narrower, Some(anim_spec()));
                    },
                );
            });
        });
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, animateAlignmentChange
#[test]
fn a_change_of_alignment_moves_items_across_the_list() {
    for params in PARAMS {
        let alignment = mutable_state_of(Alignment::BOTTOM_END);
        let mut ui = aligned_list(params, &alignment, ITEM, LayoutDirection::Ltr);
        let (start2, start3) = (f(ITEM - ITEM2), f(ITEM - ITEM3));
        assert_cross(&mut ui, params, &[(1, 0.0), (2, start2), (3, start3)]);
        ui.run_on_ui_thread(|_| alignment.set(Alignment::CENTER));
        let (end2, end3) = (f(ITEM / 2 - ITEM2 / 2), f(ITEM / 2 - ITEM3 / 2));
        on_animation_frame(&mut ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(1, 0.0), (2, f(ITEM)), (3, f(ITEM * 2))],
                Some(fraction),
                params.reverse,
            );
            assert_cross(
                ui,
                params,
                &[
                    (1, 0.0),
                    (2, start2 + (end2 - start2) * fraction),
                    (3, start3 + (end3 - start3) * fraction),
                ],
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, animateAlignmentChange_rtl
#[test]
fn a_change_of_alignment_right_to_left_moves_items_across_the_list() {
    let params = PARAMS[0];
    let alignment = mutable_state_of(Alignment::BOTTOM_END);
    let mut ui = aligned_list(params, &alignment, ITEM, LayoutDirection::Rtl);
    assert_cross(&mut ui, params, &[(1, 0.0), (2, 0.0), (3, 0.0)]);
    ui.run_on_ui_thread(|_| alignment.set(Alignment::CENTER));
    on_animation_frame(&mut ui, |ui, fraction| {
        assert_cross(
            ui,
            params,
            &[
                (1, 0.0),
                (2, f(ITEM / 2 - ITEM2 / 2) * fraction),
                (3, f(ITEM / 2 - ITEM3 / 2) * fraction),
            ],
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveItemToEndCausingNextItemsToAnimate_withContentPadding
#[test]
fn a_move_within_content_padding() {
    for params in PARAMS {
        let options = ListOptions {
            start_padding: 8,
            end_padding: 12,
            max_size: ITEM * 5 + 20,
            ..ListOptions::default()
        };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4]);
        let start = if params.reverse { 12.0 } else { 8.0 };
        fixture.set_list(vec![0, 2, 3, 4, 1]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (0, start),
                    (1, start + f(ITEM) + f(ITEM * 3) * fraction),
                    (2, start + f(ITEM * 2) - f(ITEM) * fraction),
                    (3, start + f(ITEM * 3) - f(ITEM) * fraction),
                    (4, start + f(ITEM * 4) - f(ITEM) * fraction),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, removingItemsCauseOutOfBoundsItemToPopUp_withContentPadding
#[test]
fn an_item_removed_brings_one_from_out_of_view() {
    for params in PARAMS {
        let options = ListOptions {
            start_padding: 8,
            end_padding: 12,
            max_size: ITEM * 4,
            ..ListOptions::default()
        };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4]);
        let start = if params.reverse { 12.0 } else { 8.0 };
        fixture.set_list(vec![1, 2, 3, 4]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let at = |slot: i32| start + f(ITEM * slot) - f(ITEM) * fraction;
            assert_positions_of(
                ui,
                params,
                &[(1, at(1)), (2, at(2)), (3, at(3)), (4, at(4))],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

/// Scrolling items that don't otherwise change, at `start_index`, and where they are after: the
/// `no animation` tests of Compose.
fn assert_scroll_animates_nothing(
    params: Params,
    start_index: usize,
    size_of_tag: fn(i32) -> i32,
    max_size: i32,
    scrolls: &[i32],
    expected: &[(i32, f32)],
) {
    let options = ListOptions { max_size, start_index, ..ListOptions::default() };
    let content = sized_items(params, size_of_tag);
    let mut fixture = fixture_of(params, options, vec![0, 1, 2, 3, 4, 5, 6, 7], content);
    let state = fixture.state;
    for scroll in scrolls {
        scroll_list(&mut fixture.ui, state, *scroll);
    }
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_positions_of(ui, params, expected, Some(fraction), params.reverse);
    });
}

fn all_same(_: i32) -> i32 {
    ITEM
}

fn alternating(tag: i32) -> i32 {
    if tag % 2 == 0 { ITEM } else { ITEM2 }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenScrollBackwardBySmallOffset
#[test]
fn a_small_scroll_back_animates_nothing() {
    let half = f(ITEM / 2);
    for params in PARAMS {
        assert_scroll_animates_nothing(
            params,
            2,
            all_same,
            ITEM * 3,
            &[-ITEM / 2],
            &[(1, -half), (2, half), (3, half * 3.0), (4, half * 5.0)],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenScrollForwardByLargeOffset
#[test]
fn a_large_scroll_animates_nothing() {
    let half = f(ITEM / 2);
    for params in PARAMS {
        assert_scroll_animates_nothing(
            params,
            0,
            all_same,
            ITEM * 3,
            &[ITEM * 5 / 2],
            &[(2, -half), (3, half), (4, half * 3.0), (5, half * 5.0)],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenScrollBackwardByLargeOffset
#[test]
fn a_large_scroll_back_animates_nothing() {
    let half = f(ITEM / 2);
    for params in PARAMS {
        assert_scroll_animates_nothing(
            params,
            3,
            all_same,
            ITEM * 3,
            &[-ITEM * 5 / 2],
            &[(0, -half), (1, half), (2, half * 3.0), (3, half * 5.0)],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenScrollForwardByLargeOffset_differentSizes
#[test]
fn a_large_scroll_over_different_sizes_animates_nothing() {
    let half = f(ITEM / 2);
    for params in PARAMS {
        assert_scroll_animates_nothing(
            params,
            0,
            alternating,
            ITEM * 3,
            &[ITEM + ITEM2 + ITEM / 2],
            &[(2, -half), (3, half), (4, f(ITEM2) + half), (5, f(ITEM2) + half * 3.0)],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenScrollBackwardByLargeOffset_differentSizes
#[test]
fn a_large_scroll_back_over_different_sizes_animates_nothing() {
    let half = f(ITEM / 2);
    for params in PARAMS {
        assert_scroll_animates_nothing(
            params,
            3,
            alternating,
            ITEM * 3,
            &[-(ITEM + ITEM2 + ITEM / 2)],
            &[(0, -half), (1, half), (2, f(ITEM2) + half), (3, f(ITEM2) + half * 3.0)],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenScrollForwardBySmallOffsetAndThenLargeOffset
#[test]
fn a_small_scroll_and_a_large_one_animate_nothing() {
    let half = f(ITEM / 2);
    for params in PARAMS {
        assert_scroll_animates_nothing(
            params,
            0,
            all_same,
            ITEM * 22 / 10,
            &[ITEM / 2, ITEM],
            &[(1, -half), (2, half), (3, half * 3.0)],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, itemWithSpecsIsMovingOut
#[test]
fn only_the_item_with_a_spec_moves_out() {
    for params in PARAMS {
        fn only_one(tag: i32) -> Option<AnimationSpec> {
            (tag == 1).then(anim_spec)
        }
        let content = specced_items(params, only_one);
        let mut fixture = fixture_of(params, ListOptions::default(), vec![0, 1, 2, 3], content);
        let list = ITEM * 2;
        fixture.set_list(vec![0, 2, 3, 1]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = f(ITEM) + f(list - ITEM) * fraction;
            assert_in_view(ui, params, fraction, &[(0, 0.0), (1, item1)], |at| at < f(list));
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveTwoItemsToTheTopOutsideOfBounds
#[test]
fn two_items_moved_past_the_start_line_up() {
    for params in PARAMS {
        let options = ListOptions { max_size: ITEM * 3, start_index: 3, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4, 5]);
        fixture.set_list(vec![0, 4, 5, 3, 1, 2]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let moved = f(ITEM * 3) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (1, -f(ITEM * 2) + moved),
                    (2, -f(ITEM) + moved),
                    (3, 0.0),
                    (4, f(ITEM) - moved),
                    (5, f(ITEM * 2) - moved),
                ],
                |at| at > -f(ITEM),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveTwoItemsToTheTopOutsideOfBounds_withReordering
#[test]
fn two_items_moved_past_the_start_in_another_order_line_up() {
    for params in PARAMS {
        let options = ListOptions { max_size: ITEM * 3, start_index: 3, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4, 5]);
        fixture.set_list(vec![0, 5, 4, 3, 2, 1]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (1, -f(ITEM * 2) + f(ITEM * 4) * fraction),
                    (2, -f(ITEM) + f(ITEM * 2) * fraction),
                    (3, 0.0),
                    (4, f(ITEM) - f(ITEM * 2) * fraction),
                    (5, f(ITEM * 2) - f(ITEM * 4) * fraction),
                ],
                |at| at > -f(ITEM),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveTwoItemsToTheBottomOutsideOfBounds
#[test]
fn two_items_moved_past_the_end_line_up() {
    for params in PARAMS {
        let options = ListOptions { max_size: ITEM * 3, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4]);
        fixture.set_list(vec![0, 3, 4, 1, 2]);
        let list = ITEM * 3;
        let by = f(list - ITEM);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (0, 0.0),
                    (1, f(ITEM) + by * fraction),
                    (2, f(ITEM * 2) + by * fraction),
                    (3, f(list) - by * fraction),
                    (4, f(list + ITEM) - by * fraction),
                ],
                |at| at < f(list),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, moveTwoItemsToTheBottomOutsideOfBounds_withReordering
#[test]
fn two_items_moved_past_the_end_in_another_order_line_up() {
    for params in PARAMS {
        let options = ListOptions { max_size: ITEM * 3, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4]);
        fixture.set_list(vec![0, 4, 3, 2, 1]);
        let list = ITEM * 3;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (0, 0.0),
                    (1, f(ITEM) + f(list) * fraction),
                    (2, f(ITEM * 2) + f(list - ITEM * 2) * fraction),
                    (3, f(list) - f(list - ITEM * 2) * fraction),
                    (4, f(list + ITEM) - f(list) * fraction),
                ],
                |at| at < f(list),
            );
        });
    }
}

/// A list of `ITEM`s in `size` that a state changes.
fn resized_list(params: Params, size: &OwnedState<i32>) -> ComposeTestRule {
    let handle = size.handle();
    let state = LazyListState::new(0, 0);
    let state_handle = *state;
    paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let options = ListOptions { max_size: handle.get(), ..ListOptions::default() };
        lazy_list(params, options, state_handle, move |scope| {
            plain_items(&scope, vec![0, 1, 2], params);
        });
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenParentSizeShrinks
#[test]
fn a_list_shrinking_animates_nothing() {
    for params in PARAMS {
        let size = mutable_state_of(ITEM * 3);
        let mut ui = resized_list(params, &size);
        ui.run_on_ui_thread(|_| size.set(ITEM * 2));
        on_animation_frame(&mut ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(0, 0.0), (1, f(ITEM))],
                Some(fraction),
                params.reverse,
            );
            ui.on_node_with_tag("2").assert_is_not_displayed();
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenParentSizeExpands
#[test]
fn a_list_growing_animates_nothing() {
    for params in PARAMS {
        let size = mutable_state_of(ITEM * 2);
        let mut ui = resized_list(params, &size);
        ui.run_on_ui_thread(|_| size.set(ITEM * 3));
        on_animation_frame(&mut ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(0, 0.0), (1, f(ITEM)), (2, f(ITEM * 2))],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, scrollIsNotAffectingItemMovingToTheBottomOutsideOfBounds
#[test]
fn a_scroll_leaves_an_item_moving_past_the_end_as_it_goes() {
    for params in PARAMS {
        let mut fixture = Fixture::new(params, ListOptions::default(), vec![0, 1, 2, 3, 4]);
        let state = fixture.state;
        fixture.set_list(vec![0, 4, 2, 3, 1]);
        let list = ITEM * 2;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            if fraction == 0.0 {
                assert_positions_of(
                    ui,
                    params,
                    &[(0, 0.0), (1, f(ITEM))],
                    Some(fraction),
                    params.reverse,
                );
                scroll_list(ui, state, SPACING);
            }
            // At the bound it has left: an unplaced node has no semantics here (D1472).
            assert_in_view(
                ui,
                params,
                fraction,
                &[(0, -f(SPACING)), (1, f(ITEM) + f(list - ITEM) * fraction)],
                |at| at < f(list),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, scrollIsNotAffectingItemMovingToTheTopOutsideOfBounds
#[test]
fn a_scroll_leaves_an_item_moving_past_the_start_as_it_goes() {
    for params in PARAMS {
        let options = ListOptions { start_index: 2, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![0, 1, 2, 3, 4]);
        let state = fixture.state;
        fixture.set_list(vec![3, 0, 1, 2, 4]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            if fraction == 0.0 {
                assert_positions_of(
                    ui,
                    params,
                    &[(2, 0.0), (3, f(ITEM))],
                    Some(fraction),
                    params.reverse,
                );
                scroll_list(ui, state, -SPACING);
            }
            // At the bound it has left: an unplaced node has no semantics here (D1472).
            assert_in_view(
                ui,
                params,
                fraction,
                &[(2, f(SPACING)), (3, f(ITEM) - f(ITEM * 2) * fraction)],
                |at| at > -f(ITEM),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, afterScrollingEnoughToReachNewPositionScrollDeltasStartAffectingPosition
#[test]
fn an_item_reached_by_a_scroll_follows_later_scrolls() {
    for params in PARAMS {
        let mut fixture = Fixture::new(params, ListOptions::default(), vec![0, 1, 2, 3, 4]);
        let state = fixture.state;
        fixture.set_list(vec![0, 4, 2, 3, 1]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            if fraction == 0.0 {
                assert_positions_of(
                    ui,
                    params,
                    &[(0, 0.0), (1, f(ITEM))],
                    Some(fraction),
                    params.reverse,
                );
                scroll_list(ui, state, ITEM * 2);
                assert_positions_of(
                    ui,
                    params,
                    // Its new position not reached, it still goes to right after the bounds.
                    &[(2, 0.0), (3, f(ITEM)), (1, f(ITEM))],
                    Some(fraction),
                    params.reverse,
                );
                scroll_list(ui, state, SPACING);
            }
            let scrolled = f(SPACING);
            assert_positions_of(
                ui,
                params,
                &[
                    (2, -scrolled),
                    (3, f(ITEM) - scrolled),
                    (1, f(ITEM) - scrolled + f(ITEM) * fraction),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, animationWhenTryingToStayInTheStart
#[test]
fn staying_at_the_start_over_a_new_first_item_animates() {
    for params in PARAMS {
        let options = ListOptions { max_size: ITEM * 3, ..ListOptions::default() };
        let mut fixture = Fixture::new(params, options, vec![1, 2, 3, 4]);
        let state = fixture.state;
        let handle = fixture.list.handle();
        fixture.ui.run_on_ui_thread(move |_| {
            handle.set(vec![0, 1, 2, 3]);
            state.request_scroll_to_item(0, 0);
        });
        let list = f(ITEM) * 2.5;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let mut expected =
                vec![(0, 0.0), (1, f(ITEM) * fraction), (2, f(ITEM) + f(ITEM) * fraction)];
            let item3 = f(ITEM * 2) + f(ITEM) * fraction;
            if item3 < list {
                expected.push((3, item3));
            }
            ui.on_node_with_tag("4").assert_is_not_displayed();
            assert_positions_of(ui, params, &expected, Some(fraction), params.reverse);
        });
    }
}

/// A state read by a list of `sizes`, at each pass, through `build`.
fn list_of_state<T: Clone + 'static>(
    params: Params,
    value: &OwnedState<T>,
    options: fn(&T) -> ListOptions,
    content: impl Fn(&LazyListScope, T) + Clone + 'static,
) -> ComposeTestRule {
    let handle = value.handle();
    let state = LazyListState::new(0, 0);
    let state_handle = *state;
    paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let value = handle.get();
        let content = content.clone();
        let options = options(&value);
        lazy_list(params, options, state_handle, move |scope| content(&scope, value.clone()));
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, animateArrangementChange
#[test]
fn a_change_of_arrangement_moves_the_items() {
    for params in PARAMS {
        let arranged = mutable_state_of(false);
        fn options(arranged: &bool) -> ListOptions {
            let mut options =
                ListOptions { min_size: ITEM * 5, max_size: ITEM * 5, ..ListOptions::default() };
            options.space_between = *arranged;
            options.centered = !*arranged;
            options
        }
        let mut ui = list_of_state(params, &arranged, options, move |scope, _| {
            plain_items(scope, vec![1, 2, 3], params);
        });
        assert_positions_of(
            &mut ui,
            params,
            &[(1, f(ITEM)), (2, f(ITEM * 2)), (3, f(ITEM * 3))],
            None,
            params.reverse,
        );
        ui.run_on_ui_thread(|_| arranged.set(true));
        on_animation_frame(&mut ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (1, f(ITEM) - f(ITEM) * fraction),
                    (2, f(ITEM * 2)),
                    (3, f(ITEM * 3) + f(ITEM) * fraction),
                ],
                Some(fraction),
                params.reverse,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenParentSizeChanges_reverseLayout_itemsStayInPlace
#[test]
fn a_reversed_list_resized_moves_nothing_that_stays_in_place() {
    for vertical in [true, false] {
        let params = Params { vertical, reverse: true };
        let size = mutable_state_of(ITEM * 5);
        fn options(size: &i32) -> ListOptions {
            ListOptions {
                min_size: *size,
                max_size: *size,
                from_start: true,
                ..ListOptions::default()
            }
        }
        let mut ui = list_of_state(params, &size, options, move |scope, _| {
            plain_items(scope, vec![0, 1], params);
        });
        assert_positions_of(&mut ui, params, &[(1, 0.0), (0, f(ITEM))], None, false);
        ui.run_on_ui_thread(|_| size.set(ITEM * 3));
        on_animation_frame(&mut ui, |ui, fraction| {
            assert_positions_of(ui, params, &[(1, 0.0), (0, f(ITEM))], Some(fraction), false);
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, noAnimationWhenLayoutShrinksWithItem_reverseLayout
#[test]
fn a_reversed_list_shrinking_with_its_item_moves_nothing() {
    for vertical in [true, false] {
        let params = Params { vertical, reverse: true };
        let first = mutable_state_of(ITEM);
        fn options(_: &i32) -> ListOptions {
            ListOptions::default()
        }
        let mut ui = list_of_state(params, &first, options, move |scope, size| {
            scope.items_list_keyed(
                vec![0, 1],
                |it: &i32| *it,
                move |item_scope, it: &i32| {
                    let main = if *it == 0 { size } else { ITEM };
                    item(&item_scope, params, *it, main, ITEM, Some(anim_spec()));
                },
            );
        });
        assert_positions_of(&mut ui, params, &[(1, 0.0), (0, f(ITEM))], None, false);
        ui.run_on_ui_thread(|_| first.set(ITEM2));
        on_animation_frame(&mut ui, |ui, fraction| {
            assert_positions_of(ui, params, &[(1, 0.0), (0, f(ITEM))], Some(fraction), false);
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt, reorderFirstAndLastItems_noNewLayoutInfoProduced
#[test]
fn an_animation_measures_the_list_no_more_than_twice() {
    let params = PARAMS[0];
    thread_local! {
        static READS: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
    }
    #[composable]
    fn LayoutInfoReader(state: LazyListState) {
        let _ = state.layout_info();
        READS.with(|reads| reads.set(reads.get() + 1));
    }
    let list = mutable_state_of(vec![0, 1, 2, 3, 4]);
    let handle = list.handle();
    let state = LazyListState::new(0, 0);
    let state_handle = *state;
    let mut ui = paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let items = handle.get();
        lazy_list(params, ListOptions::default(), state_handle, move |scope| {
            plain_items(&scope, items.clone(), params);
        });
        LayoutInfoReader(state_handle);
    });
    ui.run_on_ui_thread(|_| list.set(vec![4, 1, 2, 3, 0]));
    let mut start = i32::MIN;
    on_animation_frame(&mut ui, |_, fraction| {
        if fraction == 0.0 {
            start = READS.with(std::cell::Cell::get);
        }
    });
    ui.advance_time_by_frame();
    let reads = READS.with(std::cell::Cell::get);
    assert!(reads <= start + 2, "{reads} after {start}");
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, itemAddedOutsideOfViewportIsNotAnimated
#[test]
fn an_item_added_out_of_view_shows_at_once_when_scrolled_to() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::RED, Color::GREEN],
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK, Color::RED, Color::BLUE, Color::GREEN]);
    let state = fixture.state;
    fixture.ui.wait_for_idle();
    fixture.ui.advance_time_by_frame();
    scroll_list(&mut fixture.ui, state, ITEM * 3 / 2);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        let main = |at: i32| f(at);
        for (at, wanted) in
            [(ITEM / 4, Color::RED), (ITEM, Color::BLUE), (ITEM * 7 / 4, Color::GREEN)]
        {
            let color = color_at(ui, f(ITEM / 2), main(at));
            assert!(color.is(wanted, 1.0), "at {at} on fraction {fraction}: {color:?}");
        }
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, animatedItemChangesTheContainerSize
#[test]
fn an_item_fading_in_sizes_the_list_at_once() {
    let mut fixture =
        fade_fixture(FadeList { colors: vec![Color::BLACK], wrap: true, ..FadeList::default() });
    assert_eq!(size_of(&mut fixture.ui, LIST_TAG).1, ITEM);
    fixture.set(vec![Color::BLACK, Color::RED]);
    on_animation_frame(&mut fixture.ui, |ui, _| {
        assert_eq!(size_of(ui, LIST_TAG).1, ITEM * 2);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, removeItemBeingAnimated
#[test]
fn an_item_fading_in_goes_at_once_with_no_fade_out() {
    let mut fixture =
        fade_fixture(FadeList { max_size: ITEM, fade_out: no_spec, ..FadeList::default() });
    fixture.set(vec![Color::BLACK]);
    let list = fixture.list.handle();
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        if fraction < 0.5 {
            assert_slots(ui, true, fraction, &[Some((Color::BLACK, fraction))]);
        } else {
            if fraction == 0.5 {
                ui.run_on_ui_thread(|_| list.set(Vec::new()));
                ui.host().frame();
            }
            assert_slots(ui, true, fraction, &[None]);
        }
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, scrollAwayFromAnimatedItem
#[test]
fn an_item_scrolled_away_while_fading_in_comes_back_opaque() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::GREEN, Color::BLUE, Color::YELLOW],
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK, Color::RED, Color::GREEN, Color::BLUE, Color::YELLOW]);
    let state = fixture.state;
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        if fraction < 0.35 {
            assert_slots(
                ui,
                true,
                fraction,
                &[Some((Color::BLACK, 1.0)), Some((Color::RED, fraction))],
            );
        } else if fraction == 0.5 {
            scroll_list(ui, state, ITEM * 2);
            scroll_list(ui, state, ITEM / 2);
            for (at, wanted) in
                [(ITEM / 4, Color::GREEN), (ITEM, Color::BLUE), (ITEM * 7 / 4, Color::YELLOW)]
            {
                let color = color_at(ui, f(ITEM / 2), f(at));
                assert!(color.is(wanted, 1.0), "at {at} on fraction {fraction}: {color:?}");
            }
        } else {
            if fraction == 0.75 {
                scroll_list(ui, state, -ITEM * 3 / 2);
            }
            assert_slots(ui, true, fraction, &[Some((Color::RED, 1.0)), Some((Color::GREEN, 1.0))]);
        }
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, itemOutsideOfViewPortBeingAnimatedIn_shouldBePlacedAtTheEndOfList
#[test]
fn an_item_coming_into_view_with_a_removal_shows_at_once() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::GREEN, Color::BLUE, Color::YELLOW, Color::GRAY],
        max_size: ITEM * 5 / 2,
        fade_in: with_anim_spec,
        fade_out: no_spec,
        ..FadeList::default()
    });
    fixture.set(vec![Color::GREEN, Color::BLUE, Color::YELLOW, Color::GRAY]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        if fraction == 0.5 {
            assert_slots(
                ui,
                true,
                fraction,
                &[Some((Color::GREEN, 1.0)), Some((Color::BLUE, 1.0))],
            );
            let color = color_at(ui, f(ITEM / 2), f(ITEM * 9 / 4));
            assert!(color.is(Color::YELLOW, 1.0), "{color:?}");
        }
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, itemRemovedOutsideOfViewportIsNotAnimated
#[test]
fn an_item_removed_out_of_view_doesnt_show() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK, Color::RED, Color::BLUE, Color::GREEN],
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK, Color::RED, Color::GREEN]);
    let state = fixture.state;
    fixture.ui.wait_for_idle();
    fixture.ui.advance_time_by_frame();
    scroll_list(&mut fixture.ui, state, ITEM / 2);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        for (at, wanted) in
            [(ITEM / 4, Color::BLACK), (ITEM, Color::RED), (ITEM * 7 / 4, Color::GREEN)]
        {
            let color = color_at(ui, f(ITEM / 2), f(at));
            assert!(color.is(wanted, 1.0), "at {at} on fraction {fraction}: {color:?}");
        }
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, reAddItemBeingAnimated_withoutAppearanceAnimation
#[test]
fn an_item_added_back_with_no_fade_in_shows_at_once() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK],
        max_size: ITEM,
        fade_in: no_spec,
        ..FadeList::default()
    });
    fixture.set(Vec::new());
    let list = fixture.list.handle();
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        if fraction < 0.5 {
            assert_slots(ui, true, fraction, &[Some((Color::BLACK, 1.0 - fraction))]);
        } else {
            if fraction == 0.5 {
                ui.run_on_ui_thread(|_| list.set(vec![Color::BLACK]));
                ui.host().frame();
            }
            assert_slots(ui, true, fraction, &[Some((Color::BLACK, 1.0))]);
        }
    });
}

fn half_duration(_: Color) -> Option<AnimationSpec> {
    Some(tween((DURATION / 2) as u32).easing(LINEAR_EASING).into())
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, reAddItemBeingAnimated_withAppearanceAnimation
#[test]
fn an_item_added_back_fades_in_from_where_it_was() {
    let mut fixture = fade_fixture(FadeList {
        colors: vec![Color::BLACK],
        max_size: ITEM,
        fade_in: half_duration,
        ..FadeList::default()
    });
    fixture.set(Vec::new());
    let list = fixture.list.handle();
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        if fraction < 0.5 {
            assert_slots(ui, true, fraction, &[Some((Color::BLACK, 1.0 - fraction))]);
        } else {
            if fraction == 0.5 {
                ui.run_on_ui_thread(|_| list.set(vec![Color::BLACK]));
                ui.host().frame();
            }
            assert_slots(ui, true, fraction, &[Some((Color::BLACK, fraction))]);
        }
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt, removeItemBeingAnimatedForAppearance
#[test]
fn an_item_removed_while_fading_in_fades_out_from_where_it_was() {
    let mut fixture = fade_fixture(FadeList {
        max_size: ITEM,
        fade_in: with_anim_spec,
        fade_out: half_duration,
        ..FadeList::default()
    });
    fixture.set(vec![Color::BLACK]);
    let list = fixture.list.handle();
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        if fraction < 0.5 {
            assert_slots(ui, true, fraction, &[Some((Color::BLACK, fraction))]);
        } else {
            if fraction == 0.5 {
                ui.run_on_ui_thread(|_| list.set(Vec::new()));
                ui.host().frame();
            }
            assert_slots(ui, true, fraction, &[Some((Color::BLACK, 1.0 - fraction))]);
        }
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt, snapToPosition_noAnimation
#[test]
fn an_item_jumped_to_far_away_shows_at_once() {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        LazyColumn!(
            modifier = Modifier.size((dp(ITEM), dp(ITEM))),
            state = handle,
            move |scope: LazyListScope| {
                let item_scope = scope.item_scope();
                scope.items_keyed(
                    200,
                    |index| index,
                    move |_| {
                        colored_item(&item_scope, true, Color::BLACK, Some(anim_spec()), None);
                    },
                );
            }
        );
    });
    ui.run_on_ui_thread(move |_| handle.request_scroll_to_item(200, 0));
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    assert_eq!(handle.first_visible_item_index(), 199);
    assert_slots(&mut ui, true, 0.0, &[Some((Color::BLACK, 1.0))]);
}

/// Past the fade's end: a removal of an item before the last, seen again by later measurements
/// that find another item at its index.
fn run_removal_past_its_fade(
    colors: Vec<Color>,
    max_size: i32,
    left: Vec<Color>,
    wanted: &[Color],
) {
    let mut fixture = fade_fixture(FadeList { colors, max_size, ..FadeList::default() });
    fixture.set(left);
    on_animation_frame(&mut fixture.ui, |_, _| {});
    for _ in 0..4 {
        fixture.ui.advance_time_by_frame();
    }
    let expected: Vec<_> = wanted.iter().map(|color| Some((*color, 1.0))).collect();
    assert_slots(&mut fixture.ui, true, 1.0, &expected);
    assert_eq!(size_of(&mut fixture.ui, LIST_TAG).1, max_size);
}

#[test]
fn a_removal_before_the_last_item_of_a_list_longer_than_its_viewport_ends() {
    let colors =
        vec![Color::BLACK, Color::RED, Color::GREEN, Color::BLUE, Color::YELLOW, Color::GRAY];
    for removed in [0, 1] {
        let mut left = colors.clone();
        left.remove(removed);
        let wanted = [left[0], left[1]];
        run_removal_past_its_fade(colors.clone(), ITEM * 2, left, &wanted);
    }
}

#[test]
fn a_removal_before_the_last_item_of_a_list_that_fits_ends() {
    let colors = vec![Color::BLACK, Color::RED, Color::GREEN];
    run_removal_past_its_fade(
        colors,
        ITEM * 3,
        vec![Color::RED, Color::GREEN],
        &[Color::RED, Color::GREEN],
    );
}

/// Compose's `LazyListMeasure`: a list scrolling back to fill a gap at its end moves no less for
/// it, which `animate_item` animates.
#[test]
fn a_move_that_makes_a_list_scroll_back_starts_where_the_item_was() {
    for params in PARAMS {
        let mut fixture = Fixture::new(params, ListOptions::default(), vec![0, 1, 2, 3]);
        fixture.set_list(vec![1, 2, 3, 0]);
        fixture.ui.wait_for_idle();
        // The frame of the change draws the old place.
        fixture.ui.advance_time_by_frame();
        fixture.assert_positions(&[(0, 0.0)], Some(0.0));
        fixture.ui.advance_time_by_frame();
        let mut passed = 0.0;
        for _ in 0..4 {
            let fraction = passed / DURATION as f32;
            fixture.assert_positions(&[(0, f(ITEM) * fraction)], Some(fraction));
            fixture.ui.advance_time_by(std::time::Duration::from_millis(FRAME));
            passed += FRAME as f32;
        }
    }
}

/// Parent data that isn't `animate_item`'s asks nothing of the animator.
#[test]
fn a_layout_id_on_an_item_leaves_the_list_as_it_was() {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        LazyColumn!(
            modifier = Modifier.height(dp(ITEM * 2)),
            state = handle,
            |scope: LazyListScope| {
                scope.items(4, |_| {
                    Box!(modifier = Modifier.layout_id("row").size((dp(ITEM), dp(ITEM))));
                });
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(handle.item_animator_passes(), None);
}

/// Many items moved out of view at once settle in a round or two a frame.
#[test]
fn several_items_moving_out_of_view_take_few_rounds_a_frame() {
    let params = PARAMS[0];
    let options = ListOptions { max_size: ITEM * 12, ..ListOptions::default() };
    let mut fixture = Fixture::new(params, options, (0..60).collect());
    // Nine in view go below it.
    let mut moved: Vec<i32> = vec![0];
    moved.extend(10..30);
    moved.extend(1..10);
    moved.extend(30..60);
    fixture.set_list(moved);
    let state = fixture.state;
    fixture.ui.wait_for_idle();
    fixture.ui.advance_time_by_frame();
    fixture.ui.advance_time_by_frame();
    for _ in 0..3 {
        let before = state.item_animator_passes().unwrap();
        fixture.ui.advance_time_by(std::time::Duration::from_millis(FRAME));
        let passes = state.item_animator_passes().unwrap() - before;
        // One for the frame, and one more when an item's left and goes from the composition.
        assert!(passes <= 3, "{passes} measurements in a frame");
    }
}
