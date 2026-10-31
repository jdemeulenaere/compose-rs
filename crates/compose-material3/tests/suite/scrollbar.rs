//! Compose's `ScrollbarTest.kt`: what its pixels show, read from the display list, in each of its
//! four orientations and directions. Its `inspectableProperties` have no counterpart here.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::gestures::{Orientation, ScrollableState as _};
use compose_foundation::layout::*;
use compose_foundation::{ScrollIndicatorState, ScrollModifiers, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, IntSize, LayoutDirection, Modifier, Rect, UnitExt};

const SIZE: f32 = 100.0;
const THUMB: Color = Color::RED;
const TRACK: Color = Color::BLUE;

/// A state of fixed lengths, its offset settable.
#[derive(Clone)]
struct Fixed {
    offset: MutableState<i32>,
    content: i32,
    viewport: i32,
}

impl ScrollIndicatorState for Fixed {
    fn scroll_offset(&self) -> i32 {
        self.offset.get()
    }
    fn content_size(&self) -> i32 {
        self.content
    }
    fn viewport_size(&self) -> i32 {
        self.viewport
    }
}

const CASES: [(Orientation, LayoutDirection); 4] = [
    (Orientation::Vertical, LayoutDirection::Ltr),
    (Orientation::Vertical, LayoutDirection::Rtl),
    (Orientation::Horizontal, LayoutDirection::Ltr),
    (Orientation::Horizontal, LayoutDirection::Rtl),
];

fn test_style() -> NonInteractiveScrollbarStyle {
    NonInteractiveScrollbarStyle {
        thumb_color: THUMB,
        track_color: TRACK,
        thickness: 10.dp(),
        thumb_min_length: 24.dp(),
        thumb_max_length_fraction: NonInteractiveScrollbarDefaults::THUMB_MAX_LENGTH_FRACTION,
        is_fade_enabled: true,
        fade_duration_millis: 250,
        fade_delay_millis: 400,
        main_axis_track_inset: 0.dp(),
        cross_axis_track_inset: 0.dp(),
    }
}

fn no_fade() -> NonInteractiveScrollbarStyle {
    NonInteractiveScrollbarStyle { is_fade_enabled: false, ..test_style() }
}

/// The box with the scrollbar: its size, `state`, `style`.
fn rule(
    state: Rc<dyn ScrollIndicatorState>,
    case: (Orientation, LayoutDirection),
    style: NonInteractiveScrollbarStyle,
    size: f32,
    shown: Option<MutableState<bool>>,
) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let state = state.clone();
        CompositionLocalProvider(compose_ui::LOCAL_LAYOUT_DIRECTION.provides(case.1), move || {
            if shown.is_none_or(|shown| shown.get()) {
                Box!(
                    modifier = Modifier.size(size.dp()).non_interactive_scrollbar(
                        state.clone(),
                        case.0,
                        style,
                    )
                );
            }
        });
    })
}

fn fixed(content: i32, viewport: i32) -> (OwnedState<i32>, Rc<dyn ScrollIndicatorState>) {
    let offset = mutable_state_of(0);
    let state = Fixed { offset: offset.handle(), content, viewport };
    (offset, Rc::new(state))
}

/// The thumb (a round rectangle), its bounds and color.
fn thumb(ui: &mut ComposeTestRule) -> Option<(Rect, Color)> {
    ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::DrawRoundRect(rect, _, paint) => Some((*rect, paint.color)),
        _ => None,
    })
}

/// The track (a plain rectangle).
fn track(ui: &mut ComposeTestRule) -> Option<(Rect, Color)> {
    ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::DrawRect(rect, paint) => Some((*rect, paint.color)),
        _ => None,
    })
}

fn length(rect: Rect, orientation: Orientation) -> f32 {
    match orientation {
        Orientation::Vertical => rect.height(),
        Orientation::Horizontal => rect.width(),
    }
}

/// Along the axis, from the start of the scroll: from the right, a right-to-left row's.
fn start(rect: Rect, case: (Orientation, LayoutDirection)) -> f32 {
    match case {
        (Orientation::Vertical, _) => rect.top,
        (Orientation::Horizontal, LayoutDirection::Ltr) => rect.left,
        (Orientation::Horizontal, LayoutDirection::Rtl) => SIZE - rect.right,
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrollbarTest.kt, nonInteractiveScrollbar_fadeAnimation
#[test]
fn it_shows_on_a_scroll_then_fades() {
    for case in CASES {
        let (offset, state) = fixed(200, 100);
        let style = NonInteractiveScrollbarStyle {
            fade_delay_millis: 100,
            fade_duration_millis: 100,
            ..test_style()
        };
        let mut ui = rule(state, case, style, SIZE, None);
        ui.wait_for_idle();
        ui.set_auto_advance(false);
        assert_eq!(thumb(&mut ui), None, "not shown until it scrolls");
        offset.set(10);
        ui.advance_time_by(Duration::from_millis(32));
        assert_eq!(thumb(&mut ui).map(|thumb| thumb.1), Some(THUMB), "at once");
        ui.advance_time_by(Duration::from_millis(50));
        assert_eq!(thumb(&mut ui).map(|thumb| thumb.1), Some(THUMB), "during the delay");
        ui.advance_time_by(Duration::from_millis(100));
        let fading = thumb(&mut ui).expect("fading").1;
        assert!(fading.alpha() < 255 && fading.alpha() > 0, "{fading:?}");
        ui.advance_time_by(Duration::from_millis(100));
        assert_eq!(thumb(&mut ui), None, "gone");
    }
}

/// A scroll during the delay starts it over; a scroll during the fade shows it again.
#[test]
fn a_scroll_starts_the_delay_over() {
    let (offset, state) = fixed(200, 100);
    let style = NonInteractiveScrollbarStyle {
        fade_delay_millis: 100,
        fade_duration_millis: 100,
        ..test_style()
    };
    let mut ui = rule(state, CASES[0], style, SIZE, None);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    offset.set(10);
    ui.advance_time_by(Duration::from_millis(80));
    offset.set(20);
    ui.advance_time_by(Duration::from_millis(80));
    assert_eq!(thumb(&mut ui).map(|thumb| thumb.1), Some(THUMB), "160 ms on, 80 from the last");
    ui.advance_time_by(Duration::from_millis(80));
    let fading = thumb(&mut ui).expect("fading").1;
    assert!(fading.alpha() < 255);
    offset.set(30);
    ui.advance_time_by(Duration::from_millis(32));
    assert_eq!(thumb(&mut ui).map(|thumb| thumb.1), Some(THUMB), "shown again");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrollbarTest.kt, nonInteractiveScrollbar_fadeAnimation_onNodeReattach
#[test]
fn attached_again_it_isnt_shown_for_where_it_is() {
    for case in CASES {
        let (offset, state) = fixed(200, 100);
        let shown = mutable_state_of(true);
        let mut ui = rule(state, case, test_style(), SIZE, Some(shown.handle()));
        ui.wait_for_idle();
        shown.set(false);
        ui.wait_for_idle();
        offset.set(30);
        ui.wait_for_idle();
        shown.set(true);
        ui.wait_for_idle();
        assert_eq!(thumb(&mut ui), None);
        offset.set(40);
        ui.set_auto_advance(false);
        ui.advance_time_by(Duration::from_millis(32));
        assert!(thumb(&mut ui).is_some(), "a scroll shows it");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrollbarTest.kt, nonInteractiveScrollbar_fadeAnimation_whenSystemDisablesAnimations
#[test]
fn with_animations_off_it_goes_at_the_delays_end() {
    let (offset, state) = fixed(200, 100);
    let style = NonInteractiveScrollbarStyle {
        fade_delay_millis: 100,
        fade_duration_millis: 100,
        ..test_style()
    };
    let mut ui = rule(state, CASES[0], style, SIZE, None);
    ui.set_motion_duration_scale(0.0);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    offset.set(10);
    ui.advance_time_by(Duration::from_millis(32));
    assert!(thumb(&mut ui).is_some());
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(thumb(&mut ui), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrollbarTest.kt, nonInteractiveScrollbar_updatesOnScroll
#[test]
fn the_thumb_goes_with_the_offset() {
    for case in CASES {
        let (offset, state) = fixed(200, 100);
        let mut ui = rule(state, case, no_fade(), SIZE, None);
        ui.wait_for_idle();
        let at_start = thumb(&mut ui).expect("a thumb").0;
        assert_eq!(start(at_start, case), 0.0);
        assert_eq!(length(at_start, case.0), 50.0);
        offset.set(100);
        ui.wait_for_idle();
        assert_eq!(start(thumb(&mut ui).expect("a thumb").0, case), 50.0, "at the end");
        offset.set(50);
        ui.wait_for_idle();
        assert_eq!(start(thumb(&mut ui).expect("a thumb").0, case), 25.0, "half way");
    }
}

/// Each way a content can fit, from the start.
#[test]
fn nothing_shows_while_the_content_fits() {
    for case in CASES {
        for (content, viewport) in [(100, 100), (50, 100), (0, 0), (100, 0)] {
            let (_offset, state) = fixed(content, viewport);
            let mut ui = rule(state, case, no_fade(), SIZE, None);
            ui.wait_for_idle();
            assert_eq!(thumb(&mut ui), None);
            assert_eq!(track(&mut ui), None);
        }
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrollbarTest.kt, nonInteractiveScrollbar_coercesToMinThumbLength
#[test]
fn the_thumb_is_no_shorter_than_its_minimum() {
    for case in CASES {
        let (_offset, state) = fixed(1000, 100);
        let style = NonInteractiveScrollbarStyle { thumb_min_length: 30.dp(), ..no_fade() };
        let mut ui = rule(state, case, style, SIZE, None);
        ui.wait_for_idle();
        assert_eq!(length(thumb(&mut ui).expect("a thumb").0, case.0), 30.0);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrollbarTest.kt, nonInteractiveScrollbar_coercesToMaxThumbLength
#[test]
fn the_thumb_is_no_longer_than_its_fraction() {
    for case in CASES {
        let (_offset, state) = fixed(110, 100);
        let style = NonInteractiveScrollbarStyle { thumb_max_length_fraction: 0.5, ..no_fade() };
        let mut ui = rule(state, case, style, SIZE, None);
        ui.wait_for_idle();
        assert_eq!(length(thumb(&mut ui).expect("a thumb").0, case.0), 50.0);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrollbarTest.kt, nonInteractiveScrollbar_hidesWhenTrackTooSmallForMinThumbLength
#[test]
fn nothing_shows_on_a_track_shorter_than_the_least_thumb() {
    for case in CASES {
        let (_offset, state) = fixed(200, 100);
        let mut ui = rule(state, case, no_fade(), 20.0, None);
        ui.wait_for_idle();
        assert_eq!(thumb(&mut ui), None);
        assert_eq!(track(&mut ui), None);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrollbarTest.kt, nonInteractiveScrollbar_respectsTrackInsets
#[test]
fn the_track_is_inside_its_insets() {
    for case in CASES {
        let (_offset, state) = fixed(200, 100);
        let style = NonInteractiveScrollbarStyle {
            main_axis_track_inset: 10.dp(),
            cross_axis_track_inset: 5.dp(),
            ..no_fade()
        };
        let mut ui = rule(state, case, style, SIZE, None);
        ui.wait_for_idle();
        let (bounds, color) = track(&mut ui).expect("a track");
        assert_eq!(color, TRACK);
        let expected = match case {
            (Orientation::Vertical, LayoutDirection::Ltr) => Rect::new(85.0, 10.0, 95.0, 90.0),
            (Orientation::Vertical, LayoutDirection::Rtl) => Rect::new(5.0, 10.0, 15.0, 90.0),
            (Orientation::Horizontal, _) => Rect::new(10.0, 85.0, 90.0, 95.0),
        };
        assert_eq!(bounds, expected, "{case:?}");
    }
}

#[test]
#[should_panic(expected = "thumbMaxLengthFraction must be between 0f and 1f")]
fn a_fraction_over_one_is_refused() {
    let (_offset, state) = fixed(200, 100);
    let style = NonInteractiveScrollbarStyle { thumb_max_length_fraction: 1.5, ..no_fade() };
    let _ = Modifier.non_interactive_scrollbar(state, Orientation::Vertical, style);
}

#[test]
fn the_defaults_are_composes() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        MaterialTheme!({
            let style = NonInteractiveScrollbarDefaults::style();
            let outline = MaterialTheme::color_scheme().outline;
            assert_eq!(style.thumb_color, outline.with_alpha(0.7));
            assert_eq!(style.track_color, Color::TRANSPARENT);
            assert_eq!(style.thickness, 4.dp());
            assert_eq!(style.thumb_min_length, 24.dp());
            assert_eq!(style.thumb_max_length_fraction, 0.9);
            assert!(style.is_fade_enabled);
            assert_eq!(style.fade_duration_millis, 250);
            assert_eq!(style.fade_delay_millis, 400);
            assert_eq!(style.main_axis_track_inset, 2.dp());
            assert_eq!(style.cross_axis_track_inset, 0.dp());
        });
    });
    ui.wait_for_idle();
}

/// On a real scroll, as `ScrollState`'s indicator state.
#[test]
fn it_follows_a_scroll_state() {
    let mut scroll_state = None;
    let captured = std::rc::Rc::new(std::cell::Cell::new(None));
    let keep = captured.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let state = remember_scroll_state!();
        keep.set(Some(state));
        Box!(
            modifier = Modifier
                .size(100.dp())
                .non_interactive_scrollbar(
                    compose_runtime::remember(|| {
                        Rc::new(state) as Rc<dyn ScrollIndicatorState>
                    }),
                    Orientation::Vertical,
                    no_fade(),
                )
                .vertical_scroll(state),
            content = |_| Spacer(Modifier.width(100.dp()).height(400.dp()))
        );
    });
    ui.wait_for_idle();
    scroll_state.replace(captured.get().expect("composed"));
    let state = scroll_state.unwrap();
    assert_eq!(ScrollIndicatorState::scroll_offset(&state), 0);
    assert_eq!(ScrollIndicatorState::content_size(&state), 400);
    assert_eq!(ScrollIndicatorState::viewport_size(&state), 100);
    let (bounds, _) = thumb(&mut ui).expect("a thumb");
    assert_eq!((bounds.top, bounds.height()), (0.0, 25.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrollbarTest.kt, nonInteractiveScrollbar_hidesWhenContentFitsViewport
#[test]
fn the_thumb_comes_and_goes_with_the_viewport() {
    struct Live(MutableState<i32>);
    impl ScrollIndicatorState for Live {
        fn scroll_offset(&self) -> i32 {
            0
        }
        fn content_size(&self) -> i32 {
            100
        }
        fn viewport_size(&self) -> i32 {
            self.0.get()
        }
    }
    for case in CASES {
        let viewport = mutable_state_of(100);
        let state: Rc<dyn ScrollIndicatorState> = Rc::new(Live(viewport.handle()));
        let mut ui = rule(state, case, no_fade(), SIZE, None);
        ui.wait_for_idle();
        assert_eq!(thumb(&mut ui), None);
        viewport.set(50);
        ui.wait_for_idle();
        assert!(thumb(&mut ui).is_some());
        viewport.set(150);
        ui.wait_for_idle();
        assert_eq!(thumb(&mut ui), None);
    }
}

/// `scroll_indicator_state()` in composition: the same object, so a recomposition's element is the
/// same and nothing's drawn again.
#[test]
fn asking_for_the_indicator_each_composition_draws_nothing_again() {
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let draws = Rc::new(Cell::new(0));
    let count = draws.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let _ = handle.get();
        let count = count.clone();
        MaterialTheme!({
            let count = count.clone();
            let state = remember_scroll_state!();
            let counting = compose_runtime::remember(move || {
                Modifier.draw_behind(move |_| count.set(count.get() + 1))
            });
            let style = compose_runtime::remember(no_fade);
            Box!(
                modifier = Modifier
                    .size(100.dp())
                    .then(counting)
                    .non_interactive_scrollbar(
                        state.scroll_indicator_state().unwrap(),
                        Orientation::Vertical,
                        style,
                    )
                    .vertical_scroll(state),
                content = |_| Spacer(Modifier.width(100.dp()).height(400.dp()))
            );
        });
    });
    ui.wait_for_idle();
    let before = draws.get();
    tick.set(1);
    ui.wait_for_idle();
    assert_eq!(draws.get(), before);
}

/// Another state's offset isn't a scroll: nothing shows for it, as Compose's node made again.
#[test]
fn a_new_state_doesnt_show_the_thumb() {
    let which = mutable_state_of(false);
    let handle = which.handle();
    let (_first_offset, first) = fixed(1000, 100);
    let (second_offset, second) = fixed(1000, 100);
    second_offset.set(300);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let state = if handle.get() { second.clone() } else { first.clone() };
        Box!(
            modifier = Modifier.size(SIZE.dp()).non_interactive_scrollbar(
                state,
                Orientation::Vertical,
                test_style(),
            )
        );
    });
    ui.wait_for_idle();
    assert_eq!(thumb(&mut ui), None, "it hasn't scrolled");
    which.set(true);
    ui.wait_for_idle();
    assert_eq!(thumb(&mut ui), None, "nor has the other");
}

/// `ScrollState.reverseScrolling` read in the offset: a scroll turned round moves the thumb.
#[test]
fn a_scroll_turned_round_moves_the_thumb() {
    let reversed = mutable_state_of(false);
    let handle = reversed.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let on = handle.get();
        MaterialTheme!({
            let state = remember_scroll_state!();
            let indicator = state.scroll_indicator_state().unwrap();
            Box!(
                modifier = Modifier.size(100.dp()).non_interactive_scrollbar(
                    indicator,
                    Orientation::Vertical,
                    compose_runtime::remember(no_fade),
                ),
                content = move |_| {
                    Box!(
                        modifier =
                            Modifier.size(100.dp()).vertical_scroll_with(state, true, None, on),
                        content = |_| Spacer(Modifier.width(100.dp()).height(400.dp()))
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    let before = thumb(&mut ui).expect("a thumb").0.top;
    reversed.set(true);
    ui.wait_for_idle();
    assert_ne!(thumb(&mut ui).expect("a thumb").0.top, before);
}

/// The track fades with the thumb, and a new style is drawn.
#[test]
fn the_track_fades_and_a_new_style_is_drawn() {
    let which = mutable_state_of(false);
    let handle = which.handle();
    let (offset, state) = fixed(1000, 100);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let style = NonInteractiveScrollbarStyle {
            is_fade_enabled: true,
            fade_delay_millis: 0,
            fade_duration_millis: 1000,
            thumb_color: if handle.get() { Color::BLUE } else { THUMB },
            ..test_style()
        };
        Box!(
            modifier = Modifier.size(SIZE.dp()).non_interactive_scrollbar(
                state.clone(),
                Orientation::Vertical,
                style,
            )
        );
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    offset.set(100);
    ui.advance_time_by(Duration::from_millis(500));
    let alpha = track(&mut ui).expect("a track").1.alpha();
    assert!(alpha > 0 && alpha < TRACK.alpha(), "part way: {alpha}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    // A new style, and a scroll to show it: it's what's drawn.
    which.set(true);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    offset.set(200);
    ui.advance_time_by(Duration::from_millis(50));
    let color = thumb(&mut ui).expect("a thumb").1;
    assert_eq!(color.argb() & 0xFF_FFFF, Color::BLUE.argb() & 0xFF_FFFF);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
}
