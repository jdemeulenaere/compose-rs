//! Window insets (D1355–D1358): the values and their operations, the system's insets the host
//! reports, the paddings and what they consume, the size modifiers, visibility. Adapted from
//! Compose's `WindowInsetsTest`, `WindowInsetsPaddingTest`, `WindowInsetsSizeTest` and
//! `WindowInsetsIgnoringVisibilityTest`: their device's insets are the host's
//! (`UiHost::set_window_insets`), at density 1 in a window of 400 × 800 px. What they check of
//! the view's consumption (`assertIsConsumed`, `consumeWindowInsets` on a `ComposeView`) and of
//! the keyboard's animation is the Android host's, not ported here.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{Insets, InsetsType, PlatformWindowInsets};
use compose_ui::{
    Alignment, Density, IntSize, LOCAL_DENSITY, LOCAL_LAYOUT_DIRECTION, LayoutCoordinates,
    LayoutDirection, Rect, UnitExt,
};

const WIDTH: i32 = 400;
const HEIGHT: i32 = 800;

fn density() -> Density {
    Density::new(1.0)
}

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(WIDTH, HEIGHT), content);
    ui.wait_for_idle();
    ui
}

/// The host reports `insets`, and the content lays out again.
fn send(ui: &mut ComposeTestRule, insets: PlatformWindowInsets) {
    ui.host().set_window_insets(insets);
    ui.wait_for_idle();
}

/// The insets of one type, as the tests' `sendInsets(type, insets)`.
fn of(kind: InsetsType, insets: Insets) -> PlatformWindowInsets {
    with(PlatformWindowInsets::default(), kind, insets)
}

/// `platform` with `kind`'s insets set, as `WindowInsetsCompat.Builder.setInsets`: a system
/// bar's are the system bars' too (Android's `getInsets(systemBars())` is their union).
fn with(platform: PlatformWindowInsets, kind: InsetsType, insets: Insets) -> PlatformWindowInsets {
    let mut platform = platform.with(kind, insets);
    if matches!(kind, InsetsType::StatusBars | InsetsType::NavigationBars | InsetsType::CaptionBar)
    {
        platform.system_bars = platform.system_bars.union(insets);
    }
    platform
}

type Bounds = Rc<Cell<Option<Rect>>>;

fn bounds_cell() -> Bounds {
    Rc::new(Cell::new(None))
}

fn bounds_into(bounds: &Bounds) -> Modifier {
    let bounds = bounds.clone();
    Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
        bounds.set(Some(coordinates.bounds_in_root()))
    })
}

fn rect(bounds: &Bounds) -> [f32; 4] {
    let bounds = bounds.get().expect("laid out");
    [bounds.left, bounds.top, bounds.right, bounds.bottom]
}

fn height(bounds: &Bounds) -> f32 {
    bounds.get().expect("laid out").height()
}

/// The ten pixels' insets the tests send: 10, 11, 12 and 13.
const SENT: Insets = Insets::new(10, 11, 12, 13);

fn inside(insets: Insets) -> [f32; 4] {
    [
        insets.left as f32,
        insets.top as f32,
        (WIDTH - insets.right) as f32,
        (HEIGHT - insets.bottom) as f32,
    ]
}

fn ltr(direction: LayoutDirection, content: impl Fn() + 'static) -> impl Fn() + 'static {
    let content = Rc::new(content);
    move || {
        let content = content.clone();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || content());
    }
}

// ---- Values -------------------------------------------------------------------------------------

// Compose: compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt, fixedIntInsets
// and fixedIntInsets_toString
#[test]
fn pixel_insets_are_their_sizes_at_any_density_and_direction() {
    let insets = WindowInsets::new(10, 11, 12, 13);
    let double = Density::new(2.0);
    assert_eq!(insets.get_left(&density(), LayoutDirection::Ltr), 10);
    assert_eq!(insets.get_top(&density()), 11);
    assert_eq!(insets.get_right(&density(), LayoutDirection::Ltr), 12);
    assert_eq!(insets.get_bottom(&density()), 13);
    assert_eq!(insets.get_left(&double, LayoutDirection::Ltr), 10);
    assert_eq!(insets.get_left(&density(), LayoutDirection::Rtl), 10);
    assert_eq!(insets.to_string(), "Insets(left=10, top=11, right=12, bottom=13)");
}

// Compose: compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt, fixedDpInsets
// and fixedDpInsets_toString
#[test]
fn dp_insets_are_their_sizes_in_pixels_at_the_density() {
    let insets = WindowInsets::new(10.dp(), 11.dp(), 12.dp(), 13.dp());
    let double = Density::new(2.0);
    assert_eq!(insets.get_left(&density(), LayoutDirection::Ltr), 10);
    assert_eq!(insets.get_top(&density()), 11);
    assert_eq!(insets.get_right(&density(), LayoutDirection::Ltr), 12);
    assert_eq!(insets.get_bottom(&density()), 13);
    assert_eq!(insets.get_left(&double, LayoutDirection::Ltr), 20);
    assert_eq!(insets.get_left(&density(), LayoutDirection::Rtl), 10);
    assert_eq!(
        insets.to_string(),
        "Insets(left=10.0.dp, top=11.0.dp, right=12.0.dp, bottom=13.0.dp)"
    );
}

fn sides(insets: &WindowInsets, direction: LayoutDirection) -> [i32; 4] {
    [
        insets.get_left(&density(), direction),
        insets.get_top(&density()),
        insets.get_right(&density(), direction),
        insets.get_bottom(&density()),
    ]
}

// Compose: compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt, union, union_toString,
// exclude, exclude_toString, add and add_toString
#[test]
fn union_exclude_and_add_combine_insets_side_by_side() {
    let first = WindowInsets::new(10, 11, 12, 13);
    let second = WindowInsets::new(5, 20, 14, 2);
    let union = first.clone().union(second.clone());
    assert_eq!(sides(&union, LayoutDirection::Ltr), [10, 20, 14, 13]);
    assert_eq!(
        union.to_string(),
        "(Insets(left=10, top=11, right=12, bottom=13) ∪ Insets(left=5, top=20, right=14, bottom=2))"
    );
    let exclude = first.clone().exclude(second.clone());
    assert_eq!(sides(&exclude, LayoutDirection::Ltr), [5, 0, 0, 11]);
    assert_eq!(
        exclude.to_string(),
        "(Insets(left=10, top=11, right=12, bottom=13) - Insets(left=5, top=20, right=14, bottom=2))"
    );
    let add = first.add(second);
    assert_eq!(sides(&add, LayoutDirection::Ltr), [15, 31, 26, 15]);
    assert_eq!(
        add.to_string(),
        "(Insets(left=10, top=11, right=12, bottom=13) + Insets(left=5, top=20, right=14, bottom=2))"
    );
}

// Compose: compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt, onlyStart, onlyTop,
// onlyEnd, onlyBottom, onlyLeft, onlyRight, onlyHorizontal, onlyVertical, plus and limitInsets_toString
#[test]
fn only_keeps_the_sides_asked_start_and_end_by_the_direction() {
    use LayoutDirection::{Ltr, Rtl};
    let insets = || WindowInsets::new(10, 11, 12, 13);
    let cases = [
        (WindowInsetsSides::START, [10, 0, 0, 0], [0, 0, 12, 0]),
        (WindowInsetsSides::TOP, [0, 11, 0, 0], [0, 11, 0, 0]),
        (WindowInsetsSides::END, [0, 0, 12, 0], [10, 0, 0, 0]),
        (WindowInsetsSides::BOTTOM, [0, 0, 0, 13], [0, 0, 0, 13]),
        (WindowInsetsSides::LEFT, [10, 0, 0, 0], [10, 0, 0, 0]),
        (WindowInsetsSides::RIGHT, [0, 0, 12, 0], [0, 0, 12, 0]),
        (WindowInsetsSides::HORIZONTAL, [10, 0, 12, 0], [10, 0, 12, 0]),
        (WindowInsetsSides::VERTICAL, [0, 11, 0, 13], [0, 11, 0, 13]),
        (
            WindowInsetsSides::VERTICAL + WindowInsetsSides::HORIZONTAL,
            [10, 11, 12, 13],
            [10, 11, 12, 13],
        ),
    ];
    for (only, left_to_right, right_to_left) in cases {
        let limited = insets().only(only);
        assert_eq!(sides(&limited, Ltr), left_to_right, "{only}");
        assert_eq!(sides(&limited, Rtl), right_to_left, "{only}, right to left");
    }
    assert_eq!(
        insets().only(WindowInsetsSides::START + WindowInsetsSides::VERTICAL).to_string(),
        "(Insets(left=10, top=11, right=12, bottom=13) only WindowInsetsSides(Start+Top+Bottom))"
    );
}

// Compose: compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt, paddingValuesWithDensity
#[test]
fn insets_as_paddings_are_in_dp_at_the_density() {
    let padding = WindowInsets::new(20, 22, 24, 26)
        .as_padding_values_with(&Density::new(2.0), LayoutDirection::Ltr);
    assert_eq!(padding, PaddingValues::new(10.dp(), 11.dp(), 12.dp(), 13.dp()));
}

// Compose: compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt, mutableWindowInsetsDefaultConstructor,
// mutableWindowInsetsConstructor and mutableWindowInsetsChanging (what reads them reads their
// state: a padding of them measures again when they change)
#[test]
fn mutable_insets_hold_insets_as_state() {
    assert_eq!(
        sides(&MutableWindowInsets::default().as_window_insets(), LayoutDirection::Ltr),
        [0; 4]
    );
    let held = MutableWindowInsets::new(WindowInsets::new(1, 2, 3, 4));
    assert_eq!(held.get_left(&Density::new(2.0), LayoutDirection::Ltr), 1);
    assert_eq!(held.get_top(&Density::new(2.0)), 2);
    assert_eq!(held.get_right(&Density::new(2.0), LayoutDirection::Ltr), 3);
    assert_eq!(held.get_bottom(&Density::new(2.0)), 4);
    let mutable = MutableWindowInsets::default();
    let bounds = bounds_cell();
    let (insets, recorded) = (mutable.clone(), bounds.clone());
    let mut ui = rule(move || {
        let recorded = recorded.clone();
        Box!(
            modifier = Modifier.fill_max_size().window_insets_padding(insets.as_window_insets()),
            {
                Box!(modifier = Modifier.fill_max_size().then(bounds_into(&recorded)));
            }
        );
    });
    assert_eq!(rect(&bounds), inside(Insets::ZERO));
    mutable.set_insets(WindowInsets::new(1, 2, 3, 4));
    ui.wait_for_idle();
    assert_eq!(rect(&bounds), inside(Insets::new(1, 2, 3, 4)));
    assert_eq!(sides(&mutable.as_window_insets(), LayoutDirection::Ltr), [1, 2, 3, 4]);
}

// Compose: compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt, valueInsets,
// valueInsets_toString and valueInsets_toString_afterUpdate (Compose's value insets are internal:
// here the system's, read when they change)
#[test]
fn the_systems_insets_are_read_where_used_and_show_their_values() {
    let kept = Rc::new(RefCell::new(None::<WindowInsets>));
    let into = kept.clone();
    let mut ui = rule(move || {
        *into.borrow_mut() = Some(WindowInsets::waterfall());
    });
    let waterfall = kept.borrow().clone().expect("composed");
    send(&mut ui, of(InsetsType::Waterfall, SENT));
    assert_eq!(sides(&waterfall, LayoutDirection::Ltr), [10, 11, 12, 13]);
    assert_eq!(sides(&waterfall, LayoutDirection::Rtl), [10, 11, 12, 13]);
    assert_eq!(waterfall.get_left(&Density::new(2.0), LayoutDirection::Ltr), 10);
    assert_eq!(waterfall.to_string(), "waterfall(left=10, top=11, right=12, bottom=13)");
    send(&mut ui, of(InsetsType::Waterfall, Insets::new(20, 21, 22, 23)));
    assert_eq!(waterfall.to_string(), "waterfall(left=20, top=21, right=22, bottom=23)");
}

// ---- Paddings -----------------------------------------------------------------------------------

/// The content's bounds inside a full-size box with `modifier`, once the host sent `sent`.
fn padded_bounds(modifier: fn() -> Modifier, sent: PlatformWindowInsets) -> [f32; 4] {
    let bounds = bounds_cell();
    let recorded = bounds.clone();
    let mut ui = rule(move || {
        let recorded = recorded.clone();
        Box!(modifier = Modifier.fill_max_size().then(modifier()), {
            Box!(modifier = Modifier.fill_max_size().then(bounds_into(&recorded)));
        });
    });
    send(&mut ui, sent);
    rect(&bounds)
}

/// A padding's name, the type of insets sent, the insets, and the modifier.
type PaddingCase = (&'static str, InsetsType, Insets, fn() -> Modifier);

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, systemBarsPadding,
// statusBarsPaddingApi21, statusBarsPaddingApi30, captionBarPadding, navigationBarsPaddingLeft, navigationBarsPaddingRight,
// navigationBarsPaddingBottom, navigationBarsPaddingApi30, displayCutoutPadding, insetsPaddingIme, insetsPaddingDisplayCutout,
// insetsPaddingStatusBarsTop, insetsPaddingStatusBarsApi30, insetsPaddingSystemBars, insetsPaddingTappableElement,
// insetsPaddingCaptionBar, insetsPaddingMandatorySystemGestures, insetsPaddingNavigationBarsLeft, insetsPaddingNavigationBarsRight,
// insetsPaddingNavigationBarsBottom, insetsPaddingNavigationBarsApi30, insetsPaddingWaterfall and insetsPaddingSystemGestures
#[test]
fn each_padding_pads_by_its_insets() {
    use InsetsType::*;
    let cases: [PaddingCase; 24] = [
        ("systemBarsPadding", SystemBars, SENT, || Modifier.system_bars_padding()),
        ("statusBarsPadding top", StatusBars, Insets::new(0, 11, 0, 0), || {
            Modifier.status_bars_padding()
        }),
        ("statusBarsPadding", StatusBars, SENT, || Modifier.status_bars_padding()),
        ("captionBarPadding", CaptionBar, SENT, || Modifier.caption_bar_padding()),
        ("navigationBarsPadding left", NavigationBars, Insets::new(10, 0, 0, 0), || {
            Modifier.navigation_bars_padding()
        }),
        ("navigationBarsPadding right", NavigationBars, Insets::new(0, 0, 12, 0), || {
            Modifier.navigation_bars_padding()
        }),
        ("navigationBarsPadding bottom", NavigationBars, Insets::new(0, 0, 0, 13), || {
            Modifier.navigation_bars_padding()
        }),
        ("navigationBarsPadding", NavigationBars, SENT, || Modifier.navigation_bars_padding()),
        ("displayCutoutPadding", DisplayCutout, SENT, || Modifier.display_cutout_padding()),
        ("imePadding", Ime, SENT, || Modifier.ime_padding()),
        ("waterfallPadding", Waterfall, Insets::new(1, 2, 3, 4), || Modifier.waterfall_padding()),
        ("systemGesturesPadding", SystemGestures, SENT, || Modifier.system_gestures_padding()),
        ("mandatorySystemGesturesPadding", MandatorySystemGestures, SENT, || {
            Modifier.mandatory_system_gestures_padding()
        }),
        ("ime", Ime, SENT, || Modifier.window_insets_padding(WindowInsets::ime())),
        ("displayCutout", DisplayCutout, SENT, || {
            Modifier.window_insets_padding(WindowInsets::display_cutout())
        }),
        ("statusBars top", StatusBars, Insets::new(0, 10, 0, 0), || {
            Modifier.window_insets_padding(WindowInsets::status_bars())
        }),
        ("statusBars", StatusBars, SENT, || {
            Modifier.window_insets_padding(WindowInsets::status_bars())
        }),
        ("systemBars", SystemBars, SENT, || {
            Modifier.window_insets_padding(WindowInsets::system_bars())
        }),
        ("tappableElement", TappableElement, SENT, || {
            Modifier.window_insets_padding(WindowInsets::tappable_element())
        }),
        ("captionBar", CaptionBar, SENT, || {
            Modifier.window_insets_padding(WindowInsets::caption_bar())
        }),
        ("mandatorySystemGestures", MandatorySystemGestures, SENT, || {
            Modifier.window_insets_padding(WindowInsets::mandatory_system_gestures())
        }),
        ("navigationBars left", NavigationBars, Insets::new(10, 0, 0, 0), || {
            Modifier.window_insets_padding(WindowInsets::navigation_bars())
        }),
        ("waterfall", Waterfall, Insets::new(1, 2, 3, 4), || {
            Modifier.window_insets_padding(WindowInsets::waterfall())
        }),
        ("systemGestures", SystemGestures, SENT, || {
            Modifier.window_insets_padding(WindowInsets::system_gestures())
        }),
    ];
    for (name, kind, sent, modifier) in cases {
        assert_eq!(padded_bounds(modifier, of(kind, sent)), inside(sent), "{name}");
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, cutoutPath
#[test]
fn the_cutout_path_is_what_the_host_reports_read_in_composition() {
    let read = Rc::new(RefCell::new(None::<compose_ui::Path>));
    let recompositions = Rc::new(Cell::new(0));
    let (into, count) = (read.clone(), recompositions.clone());
    let mut ui = rule(move || {
        *into.borrow_mut() = WindowInsets::cutout_path();
        count.set(count.get() + 1);
    });
    assert!(read.borrow().is_none());
    let circle = compose_ui::Path::new();
    circle.add_oval(Rect::new(199.0, 4.0, 201.0, 6.0));
    ui.host().set_window_cutout_path(Some(circle.clone()));
    ui.wait_for_idle();
    assert!(read.borrow().as_ref().is_some_and(|path| path.ptr_eq(&circle)));
    // The same path: no change.
    let composed = recompositions.get();
    ui.host().set_window_cutout_path(Some(circle));
    ui.wait_for_idle();
    assert_eq!(recompositions.get(), composed);
    ui.host().set_window_cutout_path(None);
    ui.wait_for_idle();
    assert!(read.borrow().is_none());
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, mixedInsetsPadding
#[test]
fn a_padding_of_a_union_pads_by_the_largest_at_each_side() {
    let sent = PlatformWindowInsets::default()
        .with(InsetsType::NavigationBars, Insets::new(0, 0, 0, 15))
        .with(InsetsType::StatusBars, Insets::new(0, 10, 0, 0))
        .with(InsetsType::Ime, Insets::new(0, 0, 0, 5));
    let modifier = || {
        Modifier.window_insets_padding(
            WindowInsets::navigation_bars()
                .union(WindowInsets::status_bars())
                .union(WindowInsets::ime()),
        )
    };
    assert_eq!(padded_bounds(modifier, sent), inside(Insets::new(0, 10, 0, 15)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, paddingValues
#[test]
fn insets_as_padding_values_pad_by_them() {
    let modifier = || Modifier.padding(WindowInsets::system_bars().as_padding_values());
    assert_eq!(padded_bounds(modifier, of(InsetsType::SystemBars, SENT)), inside(SENT));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, consumedInsets
#[test]
fn a_padding_doesnt_pad_by_what_its_ancestors_consumed() {
    let bounds = bounds_cell();
    let recorded = bounds.clone();
    let mut ui = rule(ltr(LayoutDirection::Ltr, move || {
        let recorded = recorded.clone();
        Box!(
            modifier = Modifier
                .fill_max_size()
                .padding((5.dp(), 4.dp(), 3.dp(), 2.dp()))
                .consume_window_insets(WindowInsets::new(5, 4, 3, 2)),
            {
                let recorded = recorded.clone();
                Box!(modifier = Modifier.fill_max_size().system_bars_padding(), {
                    Box!(modifier = Modifier.fill_max_size().then(bounds_into(&recorded)));
                });
            }
        );
    }));
    send(&mut ui, of(InsetsType::SystemBars, SENT));
    assert_eq!(rect(&bounds), inside(SENT));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, consumedPadding
#[test]
fn nested_paddings_pad_by_each_others_remainder() {
    let bounds = bounds_cell();
    let recorded = bounds.clone();
    let mut ui = rule(move || {
        let recorded = recorded.clone();
        Box!(modifier = Modifier.status_bars_padding(), {
            let recorded = recorded.clone();
            Box!(modifier = Modifier.system_bars_padding(), {
                Box!(modifier = Modifier.fill_max_size().then(bounds_into(&recorded)));
            });
        });
    });
    send(
        &mut ui,
        PlatformWindowInsets::default()
            .with(InsetsType::StatusBars, Insets::new(0, 5, 0, 0))
            .with(InsetsType::SystemBars, SENT),
    );
    assert_eq!(rect(&bounds), inside(SENT));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, withConsumedWindowInsets
#[test]
fn the_consumed_insets_are_heard_when_an_ancestor_consumes() {
    let top = Rc::new(Cell::new(-1));
    let consuming = mutable_state_of(false);
    let handle = consuming.handle();
    let heard = top.clone();
    let mut ui = rule(move || {
        let heard = heard.clone();
        let modifier = if handle.get() {
            Modifier.consume_window_insets(WindowInsets::new(0, 5, 0, 0))
        } else {
            Modifier
        };
        Box!(modifier = modifier, {
            let heard = heard.clone();
            let density = LOCAL_DENSITY.current();
            Box!(
                modifier = Modifier.fill_max_size().on_consumed_window_insets_changed(
                    move |consumed: WindowInsets| heard.set(consumed.get_top(&density))
                )
            );
        });
    });
    assert_eq!(top.get(), 0);
    send(&mut ui, of(InsetsType::StatusBars, Insets::new(0, 5, 0, 0)));
    assert_eq!(top.get(), 0);
    consuming.set(true);
    ui.wait_for_idle();
    assert_eq!(top.get(), 5);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, removeLastInsetsPadding
// (the content's bounds; what the view consumes is the Android host's)
#[test]
fn removing_the_paddings_pads_no_more() {
    let status = mutable_state_of(true);
    let navigation = mutable_state_of(true);
    let (status_handle, navigation_handle) = (status.handle(), navigation.handle());
    let bounds = bounds_cell();
    let recorded = bounds.clone();
    let mut ui = rule(move || {
        let recorded = recorded.clone();
        let status = if status_handle.get() { Modifier.status_bars_padding() } else { Modifier };
        let navigation =
            if navigation_handle.get() { Modifier.navigation_bars_padding() } else { Modifier };
        Box!(modifier = Modifier.fill_max_size().then(status).then(navigation), {
            Box!(modifier = Modifier.fill_max_size().then(bounds_into(&recorded)));
        });
    });
    status.set(false);
    send(&mut ui, of(InsetsType::NavigationBars, SENT));
    assert_eq!(rect(&bounds), inside(SENT));
    navigation.set(false);
    ui.wait_for_idle();
    assert_eq!(rect(&bounds), inside(Insets::ZERO));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, consumeAtEachDepth
#[test]
fn each_depth_consumes_its_part_of_the_insets() {
    let (status, navigation, ime) = (bounds_cell(), bounds_cell(), bounds_cell());
    let recorded = (status.clone(), navigation.clone(), ime.clone());
    let mut ui = rule(move || {
        let (status, navigation, ime) = recorded.clone();
        Box!(
            modifier = Modifier.fill_max_size().status_bars_padding().then(bounds_into(&status)),
            {
                let (navigation, ime) = (navigation.clone(), ime.clone());
                Box!(
                    modifier = Modifier.navigation_bars_padding().then(bounds_into(&navigation)),
                    {
                        Box!(
                            modifier =
                                Modifier.ime_padding().fill_max_size().then(bounds_into(&ime))
                        );
                    }
                );
            }
        );
    });
    send(
        &mut ui,
        PlatformWindowInsets::default()
            .with(InsetsType::StatusBars, Insets::new(0, 10, 0, 0))
            .with(InsetsType::NavigationBars, Insets::new(0, 0, 0, 11))
            .with(InsetsType::Ime, Insets::new(0, 10, 0, 20)),
    );
    let height_of = HEIGHT as f32;
    assert_eq!(height(&status), height_of - 10.0);
    assert_eq!(height(&navigation), height_of - 21.0);
    assert_eq!(height(&ime), height_of - 30.0);
}

/// Three nested boxes, each consuming `consumed(depth)` then padding by `top` (10, 20, 30):
/// their heights once the status bar is 35 px tall.
fn consumed_heights(consumed: fn(i32) -> Modifier) -> [f32; 3] {
    let (outer, middle, inner) = (bounds_cell(), bounds_cell(), bounds_cell());
    let recorded = (outer.clone(), middle.clone(), inner.clone());
    let mut ui = rule(move || {
        let (outer, middle, inner) = recorded.clone();
        Box!(
            modifier = Modifier
                .fill_max_size()
                .then(consumed(0))
                .window_insets_padding(WindowInsets::new(0, 10, 0, 0))
                .then(bounds_into(&outer)),
            {
                let (middle, inner) = (middle.clone(), inner.clone());
                Box!(
                    modifier = consumed(1)
                        .window_insets_padding(WindowInsets::new(0, 20, 0, 0))
                        .then(bounds_into(&middle)),
                    {
                        Box!(
                            modifier = consumed(2)
                                .window_insets_padding(WindowInsets::new(0, 30, 0, 0))
                                .fill_max_size()
                                .then(bounds_into(&inner))
                        );
                    }
                );
            }
        );
    });
    send(&mut ui, of(InsetsType::StatusBars, Insets::new(0, 35, 0, 0)));
    let full = HEIGHT as f32;
    [full - height(&outer), full - height(&middle), full - height(&inner)]
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, consumedInsetsPadding
// and consumedInsetsLimitedConsumption
#[test]
fn consumed_paddings_add_up_and_consumed_insets_unite() {
    assert_eq!(
        consumed_heights(|_| Modifier.consume_window_insets(PaddingValues::new(
            0.dp(),
            1.dp(),
            0.dp(),
            0.dp()
        ))),
        [9.0, 18.0, 27.0]
    );
    assert_eq!(
        consumed_heights(|depth| {
            let top = [1, 10, 20][depth as usize];
            Modifier.consume_window_insets(WindowInsets::new(0, top, 0, 0))
        }),
        [9.0, 19.0, 29.0]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, newInsetsCausesLayout
#[test]
fn consuming_other_insets_lays_out_the_paddings_below_again() {
    let first = mutable_state_of(true);
    let handle = first.handle();
    let size = Rc::new(Cell::new(IntSize::ZERO));
    let recorded = size.clone();
    let mut ui = rule(move || {
        let recorded = recorded.clone();
        Box!(modifier = Modifier.fill_max_size(), {
            let recorded = recorded.clone();
            let top = if handle.get() { 1 } else { 2 };
            let modifier = Modifier.consume_window_insets(WindowInsets::new(0, top, 0, 0));
            Box!(modifier = modifier.size(50.dp()), {
                let recorded = recorded.clone();
                Box!(
                    modifier = Modifier
                        .window_insets_padding(WindowInsets::new(0, 10, 0, 0))
                        .fill_max_size()
                        .on_size_changed(move |size: IntSize| recorded.set(size))
                );
            });
        });
    });
    send(&mut ui, of(InsetsType::StatusBars, Insets::new(0, 20, 0, 0)));
    assert_eq!(size.get().height, 41);
    first.set(false);
    ui.wait_for_idle();
    assert_eq!(size.get().height, 42);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, noInsets
#[test]
fn without_insets_the_safe_content_is_zero() {
    let read = Rc::new(Cell::new([-1; 4]));
    let into = read.clone();
    let mut ui = rule(move || {
        let insets = WindowInsets::safe_content();
        let (density, direction) = (LOCAL_DENSITY.current(), LOCAL_LAYOUT_DIRECTION.current());
        into.set([
            insets.get_left(&density, direction),
            insets.get_top(&density),
            insets.get_right(&density, direction),
            insets.get_bottom(&density),
        ]);
    });
    send(&mut ui, PlatformWindowInsets::default());
    assert_eq!(read.get(), [0; 4]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, reuseModifier
#[test]
fn a_modifier_used_twice_consumes_twice() {
    let consumed = Rc::new(RefCell::new((WindowInsets::default(), WindowInsets::default())));
    let into = consumed.clone();
    let mut ui = rule(move || {
        let modifier = Modifier.consume_window_insets(PaddingValues::from(10.dp()));
        let (first, second) = (into.clone(), into.clone());
        Box!(
            modifier = modifier.clone().fill_max_size().on_consumed_window_insets_changed(
                move |insets: WindowInsets| first.borrow_mut().0 = insets
            ),
            {
                let second = second.clone();
                Box!(
                    modifier = modifier.clone().fill_max_size().on_consumed_window_insets_changed(
                        move |insets: WindowInsets| second.borrow_mut().1 = insets
                    )
                );
            }
        );
    });
    send(&mut ui, of(InsetsType::StatusBars, Insets::new(0, 30, 0, 0)));
    let (first, second) = consumed.borrow().clone();
    assert_eq!(first.get_top(&ui.density()), 10);
    assert_eq!(second.get_top(&ui.density()), 20);
}

/// Compose's `removedPaddingModifierUpdatesChildren`, with two consuming nodes leaving one chain
/// at once: what's below them consumes as if neither had been there (D1355).
#[test]
fn removing_two_paddings_at_once_leaves_their_children_what_their_ancestors_consumed() {
    let padded = mutable_state_of(true);
    let handle = padded.handle();
    let consumed = Rc::new(Cell::new(-1));
    let heard = consumed.clone();
    let mut ui = rule(move || {
        let heard = heard.clone();
        let padding = if handle.get() {
            Modifier
                .window_insets_padding(WindowInsets::new(0, 10, 0, 0))
                .window_insets_padding(WindowInsets::new(0, 20, 0, 0))
        } else {
            Modifier
        };
        let outer = Modifier.consume_window_insets(WindowInsets::new(0, 5, 0, 0));
        Box!(
            modifier = outer.then(padding).on_consumed_window_insets_changed(
                move |insets: WindowInsets| heard.set(insets.get_top(&density()))
            )
        );
    });
    assert_eq!(consumed.get(), 20);
    padded.set(false);
    ui.wait_for_idle();
    assert_eq!(consumed.get(), 5);
}

/// Two layout nodes, one inside the other, each losing its padding in one pass. The outer's leaves
/// first with the inner's already out of its chain, so the inner's, leaving next, still holds what
/// the outer consumed: what's below must look for its nearest ancestor, not take that (D1355).
#[test]
fn nested_paddings_removed_together_leave_their_children_what_their_ancestors_consumed() {
    let padded = mutable_state_of(true);
    let handle = padded.handle();
    let consumed = Rc::new(Cell::new(-1));
    let heard = consumed.clone();
    let mut ui = rule(move || {
        let heard = heard.clone();
        let (outer, inner) = if handle.get() {
            (
                Modifier.window_insets_padding(WindowInsets::new(0, 10, 0, 0)),
                Modifier.window_insets_padding(WindowInsets::new(0, 20, 0, 0)),
            )
        } else {
            (Modifier, Modifier)
        };
        Box!(modifier = outer, {
            let heard = heard.clone();
            Box!(
                modifier =
                    inner.clone().on_consumed_window_insets_changed(move |insets: WindowInsets| {
                        heard.set(insets.get_top(&density()))
                    })
            );
        });
    });
    assert_eq!(consumed.get(), 20);
    padded.set(false);
    ui.wait_for_idle();
    assert_eq!(consumed.get(), 0);
}

/// What D1355 says of a node consuming nothing new: it tells nothing below, so a scaffold's doesn't
/// visit the screen; one that consumes does, once.
#[test]
fn a_node_consuming_nothing_new_doesnt_walk_what_is_below_it() {
    let many = |modifier: Modifier| {
        Box!(modifier = modifier, {
            for index in 0..50 {
                key(index, || Box!(modifier = Modifier.size(1.dp())));
            }
        });
    };
    let walks = compose_foundation::layout::consumed_insets_walks();
    let _ui = rule(move || {
        many(Modifier.on_consumed_window_insets_changed(|_: WindowInsets| {}));
    });
    assert_eq!(compose_foundation::layout::consumed_insets_walks(), walks);
    let _ui = rule(move || {
        many(Modifier.consume_window_insets(WindowInsets::new(0, 1, 0, 0)));
    });
    assert_eq!(compose_foundation::layout::consumed_insets_walks(), walks + 1);
}

/// A keyboard's frame measures again what read the keyboard's insets, not what read the system
/// bars', as Compose's states per type (D1363): with Material's defaults, every scaffold and bar.
#[test]
fn a_change_of_one_type_measures_only_what_read_it() {
    struct Reads(Rc<Cell<u32>>);
    impl WindowInsetsProvider for Reads {
        fn get_left(&self, _: &Density, _: LayoutDirection) -> i32 {
            self.0.set(self.0.get() + 1);
            0
        }
        fn get_top(&self, _: &Density) -> i32 {
            0
        }
        fn get_right(&self, _: &Density, _: LayoutDirection) -> i32 {
            0
        }
        fn get_bottom(&self, _: &Density) -> i32 {
            0
        }
    }
    impl std::fmt::Display for Reads {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("Reads")
        }
    }
    let (bars, keyboard) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (into_bars, into_keyboard) = (bars.clone(), keyboard.clone());
    let mut ui = rule(move || {
        let bars = WindowInsets::system_bars()
            .union(WindowInsets::from_provider(Reads(into_bars.clone())));
        let keyboard =
            WindowInsets::ime().union(WindowInsets::from_provider(Reads(into_keyboard.clone())));
        Box!(modifier = Modifier.window_insets_padding(bars));
        Box!(modifier = Modifier.window_insets_padding(keyboard));
    });
    let (bars_before, keyboard_before) = (bars.get(), keyboard.get());
    send(&mut ui, of(InsetsType::Ime, Insets::new(0, 0, 0, 100)));
    assert!(keyboard.get() > keyboard_before);
    assert_eq!(bars.get(), bars_before);
}

/// A host's insets are reported before anything reads them: what's read first is what was reported
/// last, and a host whose content reads none makes no state.
#[test]
fn insets_reported_before_they_are_read_are_what_is_read() {
    let shown = mutable_state_of(false);
    let handle = shown.handle();
    let bounds = bounds_cell();
    let recorded = bounds.clone();
    let mut ui = rule(move || {
        if handle.get() {
            let recorded = recorded.clone();
            Box!(modifier = Modifier.fill_max_size().status_bars_padding(), {
                Box!(modifier = Modifier.fill_max_size().then(bounds_into(&recorded)));
            });
        }
    });
    send(&mut ui, of(InsetsType::StatusBars, Insets::new(0, 30, 0, 0)));
    shown.set(true);
    ui.wait_for_idle();
    assert_eq!(rect(&bounds), inside(Insets::new(0, 30, 0, 0)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, removedPaddingModifierUpdatesChildren
#[test]
fn removing_an_ancestors_padding_updates_what_its_children_consume() {
    let padded = mutable_state_of(true);
    let handle = padded.handle();
    let (outside, padding, content) = (bounds_cell(), bounds_cell(), bounds_cell());
    let recorded = (outside.clone(), padding.clone(), content.clone());
    let mut ui = rule(move || {
        let (outside, padding, content) = recorded.clone();
        let modifier =
            if handle.get() { Modifier.status_bars_padding() } else { Modifier.fill_max_size() };
        Box!(
            modifier = modifier
                .then(bounds_into(&outside))
                .fill_max_size()
                .system_bars_padding()
                .then(bounds_into(&padding)),
            {
                Box!(modifier = Modifier.fill_max_size().then(bounds_into(&content)));
            }
        );
    });
    send(
        &mut ui,
        PlatformWindowInsets::default()
            .with(InsetsType::StatusBars, Insets::new(0, 24, 0, 0))
            .with(InsetsType::NavigationBars, Insets::new(0, 0, 0, 48))
            .with(InsetsType::SystemBars, Insets::new(0, 24, 0, 48)),
    );
    assert_eq!(rect(&content), rect(&padding));
    assert_ne!(height(&outside), height(&padding));
    let (padding_before, outside_before) = (rect(&padding), height(&outside));
    padded.set(false);
    ui.wait_for_idle();
    // The system bars' padding pads by the status bar too now: where it was.
    assert_eq!(rect(&padding), padding_before);
    assert!(rect(&padding)[1] > 0.0);
    assert!(height(&outside) > outside_before);
    assert_eq!(rect(&outside)[1], 0.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, contentNotRecomposedWhenContainerRecomposed
#[test]
fn a_padding_doesnt_recompose_its_content() {
    let force = mutable_state_of(0);
    let handle = force.handle();
    let recomposed = Rc::new(Cell::new(false));
    let marked = recomposed.clone();
    let mut ui = rule(move || {
        let marked = marked.clone();
        Box!(modifier = Modifier.fill_max_size(), {
            let _ = handle.get();
            let marked = marked.clone();
            InnerContent(Modifier.system_gestures_padding(), marked);
        });
    });
    recomposed.set(false);
    force.set(1);
    ui.wait_for_idle();
    assert!(!recomposed.get());
}

#[composable]
fn InnerContent(modifier: Modifier, recomposed: Rc<Cell<bool>>) {
    Box!(modifier = modifier, { recomposed.set(true) });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, onConsumedWindowInsetsChanged_updateBlockRecalculates
#[test]
fn a_new_block_hears_the_consumed_insets() {
    let (first, second) = (MutableWindowInsets::default(), MutableWindowInsets::default());
    let use_first = mutable_state_of(true);
    let handle = use_first.handle();
    let targets = (first.clone(), second.clone());
    let mut ui = rule(move || {
        let target = if handle.get() { targets.0.clone() } else { targets.1.clone() };
        Box!(
            modifier = Modifier
                .consume_window_insets(PaddingValues::new(0.dp(), 10.dp(), 0.dp(), 0.dp()))
                .on_consumed_window_insets_changed(move |consumed: WindowInsets| {
                    target.set_insets(consumed)
                })
        );
    });
    send(&mut ui, of(InsetsType::StatusBars, Insets::new(0, 30, 0, 0)));
    let density = ui.density();
    assert_eq!(first.get_top(&density), 10);
    assert_eq!(second.get_top(&density), 0);
    use_first.set(false);
    ui.wait_for_idle();
    assert_eq!(second.get_top(&density), 10);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, recalculateWindowInsets
#[test]
fn recalculated_insets_are_the_space_around_the_content() {
    let padding = mutable_state_of(10);
    let handle = padding.handle();
    let bounds = bounds_cell();
    let recorded = bounds.clone();
    let mut ui = rule(move || {
        let recorded = recorded.clone();
        Box!(modifier = Modifier.padding((handle.get() as f32).dp()), {
            let recorded = recorded.clone();
            Box!(
                modifier =
                    Modifier.fill_max_size().recalculate_window_insets().safe_drawing_padding(),
                {
                    Box!(modifier = Modifier.fill_max_size().then(bounds_into(&recorded)));
                }
            );
        });
    });
    send(&mut ui, of(InsetsType::SystemBars, Insets::new(11, 17, 23, 29)));
    assert_eq!(rect(&bounds), inside(Insets::new(11, 17, 23, 29)));
    padding.set(5);
    ui.wait_for_idle();
    assert_eq!(rect(&bounds), inside(Insets::new(11, 17, 23, 29)));
    padding.set(20);
    ui.wait_for_idle();
    assert_eq!(rect(&bounds), inside(Insets::new(20, 20, 23, 29)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt, recalculateWindowInsetsWithMovement
// and recalculateWindowInsetsWithNestedMovement
#[test]
fn recalculated_insets_follow_the_content_where_it_moves() {
    for nested in [false, true] {
        let alignment = mutable_state_of(Alignment::TOP_START);
        let handle = alignment.handle();
        let bounds = bounds_cell();
        let recorded = bounds.clone();
        let mut ui = rule(move || {
            let recorded = recorded.clone();
            Box!(modifier = Modifier.fill_max_size(), |scope| {
                let recorded = recorded.clone();
                let padded = move || {
                    Modifier
                        .size(100.dp())
                        .recalculate_window_insets()
                        .safe_drawing_padding()
                        .then(bounds_into(&recorded))
                };
                if nested {
                    Box!(modifier = Modifier.size(100.dp()).align(scope, handle.get()), {
                        let padded = padded.clone();
                        Box!(modifier = Modifier.required_size(100.dp()), {
                            Box!(modifier = padded());
                        });
                    });
                } else {
                    Box!(modifier = padded().align(scope, handle.get()));
                }
            });
        });
        send(&mut ui, of(InsetsType::StatusBars, Insets::new(11, 17, 23, 29)));
        assert_eq!(rect(&bounds), [11.0, 17.0, 100.0, 100.0], "nested {nested}");
        alignment.set(Alignment::BOTTOM_END);
        ui.wait_for_idle();
        let (width, height) = (WIDTH as f32, HEIGHT as f32);
        assert_eq!(
            rect(&bounds),
            [width - 100.0, height - 100.0, width - 23.0, height - 29.0],
            "nested {nested}"
        );
    }
}

/// How many times `modifier`'s node is measured over five more frames, once settled, inside what
/// `wrap` gives it.
fn frames_measured_after_settling(content: impl Fn(Modifier) + 'static) -> u32 {
    let measures = Rc::new(Cell::new(0));
    let counted = measures.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let counted = counted.clone();
        content(Modifier.layout(move |scope, measurable, constraints: compose_ui::Constraints| {
            counted.set(counted.get() + 1);
            let placeable = scope.measure(measurable, constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        }));
    });
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    let settled = measures.get();
    for _ in 0..5 {
        ui.advance_time_by_frame();
    }
    measures.get() - settled
}

/// Without fixed constraints it can't say what's around it and passes its ancestors' consumption
/// on: its position isn't written, or the next measure would undo it, a frame every vsync (D1362).
#[test]
fn recalculated_insets_without_fixed_constraints_settle() {
    let measured = frames_measured_after_settling(|counted| {
        Box!(modifier = Modifier.padding(10.dp()), {
            let counted = counted.clone();
            Box!(modifier = Modifier.recalculate_window_insets().safe_drawing_padding(), {
                Box!(modifier = counted.clone().size(20.dp()));
            });
        });
    });
    assert_eq!(measured, 0);
}

/// As above, from a fixed size to a wrapping one: it had written where it was, and forgets.
#[test]
fn recalculated_insets_losing_their_fixed_constraints_settle() {
    let wrapping = mutable_state_of(false);
    let handle = wrapping.handle();
    let measures = Rc::new(Cell::new(0));
    let counted = measures.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let counted = counted.clone();
        let size =
            if handle.get() { Modifier.wrap_content_size() } else { Modifier.fill_max_size() };
        Box!(modifier = Modifier.padding(10.dp()).then(size).recalculate_window_insets(), {
            let counted = counted.clone();
            Box!(
                modifier = Modifier.safe_drawing_padding().layout(
                    move |scope, measurable, constraints: compose_ui::Constraints| {
                        counted.set(counted.get() + 1);
                        let placeable = scope.measure(measurable, constraints);
                        scope.place(&placeable, 0, 0);
                        scope.layout(placeable.width, placeable.height)
                    }
                ),
                {
                    Box!(modifier = Modifier.size(20.dp()));
                }
            );
        });
    });
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    wrapping.set(true);
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    let settled = measures.get();
    for _ in 0..5 {
        ui.advance_time_by_frame();
    }
    assert_eq!(measures.get() - settled, 0);
}

// ---- Sizes --------------------------------------------------------------------------------------

/// The size of a wrapping box holding a box with `modifier`, right to left or not, once the
/// host sent `sent`.
fn sized(
    modifier: fn() -> Modifier,
    direction: LayoutDirection,
    sent: PlatformWindowInsets,
) -> IntSize {
    let size = Rc::new(Cell::new(IntSize::ZERO));
    let recorded = size.clone();
    let mut ui = rule(ltr(direction, move || {
        let recorded = recorded.clone();
        Box!(
            modifier = Modifier
                .wrap_content_size()
                .on_size_changed(move |size: IntSize| recorded.set(size)),
            {
                Box!(modifier = modifier());
            }
        );
    }));
    send(&mut ui, sent);
    size.get()
}

/// A size modifier's name, the type of insets sent, the insets, the direction, the size expected
/// and the modifier.
type SizeCase = (&'static str, InsetsType, Insets, LayoutDirection, IntSize, fn() -> Modifier);

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsSizeTest.kt, insetsStartWidthIme,
// insetsStartWidthImeRtl, insetsEndWidthIme, insetsTopHeightIme, insetsBottomHeightIme, insetsStartWidthNavigationBars,
// insetsStartWidthNavigationBarsRtl, insetsEndWidthNavigationBars and insetsTopHeightStatusBars
#[test]
fn the_size_modifiers_take_their_insets_side() {
    use InsetsType::{Ime, NavigationBars, StatusBars};
    use LayoutDirection::{Ltr, Rtl};
    let wide = IntSize::new(10, HEIGHT);
    let tall = IntSize::new(WIDTH, 10);
    let cases: [SizeCase; 9] = [
        ("start ime", Ime, Insets::new(10, 0, 0, 0), Ltr, wide, || {
            Modifier.window_insets_start_width(WindowInsets::ime()).fill_max_height()
        }),
        ("start ime rtl", Ime, Insets::new(0, 0, 10, 0), Rtl, wide, || {
            Modifier.window_insets_start_width(WindowInsets::ime()).fill_max_height()
        }),
        ("end ime", Ime, Insets::new(0, 0, 10, 0), Ltr, wide, || {
            Modifier.window_insets_end_width(WindowInsets::ime()).fill_max_height()
        }),
        ("top ime", Ime, Insets::new(0, 10, 0, 0), Ltr, tall, || {
            Modifier.window_insets_top_height(WindowInsets::ime()).fill_max_width()
        }),
        ("bottom ime", Ime, Insets::new(0, 0, 0, 10), Ltr, tall, || {
            Modifier.window_insets_bottom_height(WindowInsets::ime()).fill_max_width()
        }),
        ("start navigation", NavigationBars, Insets::new(10, 0, 0, 0), Ltr, wide, || {
            Modifier.window_insets_start_width(WindowInsets::navigation_bars()).fill_max_height()
        }),
        ("start navigation rtl", NavigationBars, Insets::new(0, 0, 10, 0), Rtl, wide, || {
            Modifier.window_insets_start_width(WindowInsets::navigation_bars()).fill_max_height()
        }),
        ("end navigation", NavigationBars, Insets::new(0, 0, 10, 0), Ltr, wide, || {
            Modifier.window_insets_end_width(WindowInsets::navigation_bars()).fill_max_height()
        }),
        ("top status", StatusBars, Insets::new(0, 10, 0, 0), Ltr, tall, || {
            Modifier.window_insets_top_height(WindowInsets::status_bars()).fill_max_width()
        }),
    ];
    for (name, kind, sent, direction, expected, modifier) in cases {
        assert_eq!(sized(modifier, direction, of(kind, sent)), expected, "{name}");
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsSizeTest.kt, insetsTopHeightMixed
#[test]
fn a_size_modifier_of_a_union_takes_the_largest() {
    let sent = PlatformWindowInsets::default()
        .with(InsetsType::NavigationBars, Insets::new(0, 3, 0, 0))
        .with(InsetsType::SystemBars, Insets::new(0, 10, 0, 0));
    let modifier = || {
        Modifier
            .window_insets_top_height(
                WindowInsets::navigation_bars().union(WindowInsets::system_bars()),
            )
            .fill_max_width()
    };
    assert_eq!(sized(modifier, LayoutDirection::Ltr, sent), IntSize::new(WIDTH, 10));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsSizeTest.kt, insetsSideWidthConsumption
// and insetsSideHeightConsumption
#[test]
fn the_size_modifiers_take_what_isnt_consumed() {
    let width = || {
        Modifier
            .consume_window_insets(WindowInsets::new(10, 0, 0, 0))
            .fill_max_height()
            .window_insets_start_width(WindowInsets::navigation_bars())
    };
    let sent = of(InsetsType::NavigationBars, Insets::new(25, 0, 0, 0));
    assert_eq!(sized(width, LayoutDirection::Ltr, sent), IntSize::new(15, HEIGHT));
    let height = || {
        Modifier
            .consume_window_insets(WindowInsets::new(0, 0, 0, 10))
            .fill_max_width()
            .window_insets_bottom_height(WindowInsets::navigation_bars())
    };
    let sent = of(InsetsType::NavigationBars, Insets::new(0, 0, 0, 25));
    assert_eq!(sized(height, LayoutDirection::Ltr, sent), IntSize::new(WIDTH, 15));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsSizeTest.kt, topHeightModifiersAreEqual,
// bottomHeightModifiersAreEqual, startWidthModifiersAreEqual and endWidthModifiersAreEqual
#[test]
fn equal_size_modifiers_are_equal() {
    let checked = Rc::new(Cell::new(0));
    let count = checked.clone();
    let _ui = rule(move || {
        let pairs = [
            (
                Modifier.window_insets_top_height(WindowInsets::status_bars()),
                Modifier.window_insets_top_height(WindowInsets::status_bars()),
            ),
            (
                Modifier.window_insets_bottom_height(WindowInsets::navigation_bars()),
                Modifier.window_insets_bottom_height(WindowInsets::navigation_bars()),
            ),
            (
                Modifier.window_insets_start_width(WindowInsets::navigation_bars()),
                Modifier.window_insets_start_width(WindowInsets::navigation_bars()),
            ),
            (
                Modifier.window_insets_end_width(WindowInsets::navigation_bars()),
                Modifier.window_insets_end_width(WindowInsets::navigation_bars()),
            ),
        ];
        for (first, second) in pairs {
            assert!(ComposeArg::same(&first, &second), "{first:?} and {second:?}");
            count.set(count.get() + 1);
        }
        assert!(!ComposeArg::same(
            &Modifier.window_insets_top_height(WindowInsets::status_bars()),
            &Modifier.window_insets_bottom_height(WindowInsets::status_bars()),
        ));
    });
    assert_eq!(checked.get(), 4);
}

// ---- Visibility ---------------------------------------------------------------------------------

/// The insets of `kind` at `insets` were it shown, shown or hidden, as the test's `createInsets`.
fn shown(kind: InsetsType, insets: Insets, visible: bool) -> PlatformWindowInsets {
    let mut platform = of(kind, if visible { insets } else { Insets::ZERO });
    platform.set_visible(kind, visible);
    match kind {
        InsetsType::CaptionBar => platform.caption_bar_ignoring_visibility = insets,
        InsetsType::NavigationBars => platform.navigation_bars_ignoring_visibility = insets,
        InsetsType::StatusBars => platform.status_bars_ignoring_visibility = insets,
        InsetsType::SystemBars => platform.system_bars_ignoring_visibility = insets,
        InsetsType::TappableElement => platform.tappable_element_ignoring_visibility = insets,
        _ => {}
    }
    platform
}

/// A type of insets, the insets sent, and how the content reads whether it's shown.
type VisibilityCase = (InsetsType, Insets, fn() -> bool);

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsIgnoringVisibilityTest.kt, isCaptionBarVisible,
// isImeVisible, areStatusBarsVisible, areNavigationBarsVisible, areSystemBarsVisible and isTappableElementVisible
#[test]
fn visibility_is_read_in_composition() {
    use InsetsType::*;
    let cases: [VisibilityCase; 6] = [
        (CaptionBar, Insets::new(0, 10, 0, 0), WindowInsets::is_caption_bar_visible),
        (Ime, Insets::new(0, 0, 0, 10), WindowInsets::is_ime_visible),
        (StatusBars, Insets::new(0, 10, 0, 0), WindowInsets::are_status_bars_visible),
        (NavigationBars, Insets::new(0, 0, 0, 10), WindowInsets::are_navigation_bars_visible),
        (SystemBars, Insets::new(0, 10, 0, 0), WindowInsets::are_system_bars_visible),
        (TappableElement, Insets::new(0, 10, 0, 0), WindowInsets::is_tappable_element_visible),
    ];
    for (kind, insets, visible) in cases {
        let read = Rc::new(Cell::new(false));
        let into = read.clone();
        let mut ui = rule(move || into.set(visible()));
        send(&mut ui, shown(kind, insets, true));
        assert!(read.get(), "{kind:?}");
        send(&mut ui, shown(kind, insets, false));
        assert!(!read.get(), "{kind:?} hidden");
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsIgnoringVisibilityTest.kt, captionBarIgnoringVisibility,
// navigationBarsIgnoringVisibility, statusBarsIgnoringVisibility, systemBarsIgnoringVisibility and tappableElementIgnoringVisibility
#[test]
fn the_insets_ignoring_visibility_stay_when_hidden() {
    use InsetsType::*;
    let cases: [(InsetsType, fn() -> WindowInsets); 5] = [
        (CaptionBar, WindowInsets::caption_bar_ignoring_visibility),
        (NavigationBars, WindowInsets::navigation_bars_ignoring_visibility),
        (StatusBars, WindowInsets::status_bars_ignoring_visibility),
        (SystemBars, WindowInsets::system_bars_ignoring_visibility),
        (TappableElement, WindowInsets::tappable_element_ignoring_visibility),
    ];
    for (kind, ignoring) in cases {
        let kept = Rc::new(RefCell::new(WindowInsets::default()));
        let into = kept.clone();
        let mut ui = rule(move || *into.borrow_mut() = ignoring());
        send(&mut ui, shown(kind, SENT, true));
        assert_eq!(sides(&kept.borrow(), LayoutDirection::Ltr), [10, 11, 12, 13], "{kind:?}");
        send(&mut ui, shown(kind, SENT, false));
        assert_eq!(
            sides(&kept.borrow(), LayoutDirection::Ltr),
            [10, 11, 12, 13],
            "{kind:?} hidden"
        );
    }
}
