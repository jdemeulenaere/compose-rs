//! Adversarial review of `magnifier` (D2072–D2074):
//! - Global position & graphics layer tracking: removing an `on_globally_positioned` node elsewhere
//!   in the tree while a `SelectionContainer` has an active selection (D2073 finding 3), and moving
//!   a `SelectionContainer` or `BasicTextFieldOfState` via layout offset or `graphics_layer` while
//!   a selection handle is held.
//! - `Modifier.magnifier` lifecycle & state: `LOCAL_DENSITY` changes while hidden, `ReusableContent`
//!   reuse and removal, partial-NaN centers, content copies at frames, NaN-to-NaN zoom stability,
//!   style/factory recreation, and text defaults.
//! - Selection magnifier edge cases: handle detachment before touch slop (selection collapse, blur
//!   and refocus), value-based `BasicTextField` with `IncreasedVisualTransformation` and
//!   `ReducedVisualTransformation` (from Compose's `TextFieldVisualTransformationMagnifierTest`)
//!   plus unclamped/multibyte `OffsetMapping`, `OutputTransformation` prefix offsets, scrolled
//!   value-based fields, half-magnifier-width horizontal hide/re-show threshold, vertical spring vs
//!   horizontal snap on line changes, and mid-drag detachment.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{IntoCallback, ReusableContent, mutable_state_of, remember};
use compose_test::*;
use compose_ui::platform::{FixedFontMetrics, FixedTextMeasurer};
use compose_ui::{
    Color, Density, Dp, DpSize, InputMode, IntOffset, IntSize, LayoutDirection, Offset, Paint,
    PlatformMagnifier, PlatformMagnifierConfig, PlatformMagnifierFactory, TextLayoutResult,
    TextRange, UnitExt,
};

use crate::text_field_touch_support::*;

const TAG: &str = "tag";
const OTHER_TAG: &str = "other";
const FONT: f32 = 20.0;
const RTL_CHAR: char = '\u{05D1}';

struct OneMagnifier {
    magnifier: Rc<TestMagnifier>,
    can_update_zoom: bool,
}

impl PlatformMagnifierFactory for OneMagnifier {
    fn can_update_zoom(&self) -> bool {
        self.can_update_zoom
    }
    fn create(&self, _config: &PlatformMagnifierConfig) -> Rc<dyn PlatformMagnifier> {
        self.magnifier.clone()
    }
}

fn counting_magnifier(size: IntSize) -> Rc<TestMagnifier> {
    let config = PlatformMagnifierConfig {
        use_text_default: false,
        size: DpSize::UNSPECIFIED,
        corner_radius: Dp::UNSPECIFIED,
        elevation: Dp::UNSPECIFIED,
        clipping_enabled: true,
        density: Density::new(1.0),
        initial_zoom: f32::NAN,
    };
    Rc::new(TestMagnifier::new(config, size))
}

fn factory_of(
    magnifier: &Rc<TestMagnifier>,
    can_update_zoom: bool,
) -> Rc<dyn PlatformMagnifierFactory> {
    Rc::new(OneMagnifier { magnifier: magnifier.clone(), can_update_zoom })
}

fn counting_factory(can_update_zoom: bool) -> Rc<TestMagnifierFactory> {
    let factory = Rc::new(TestMagnifierFactory::new());
    factory.set_can_update_zoom(can_update_zoom);
    factory
}

fn style() -> TextStyle {
    TextStyle { font_size: FONT.sp(), ..TextStyle::DEFAULT }
}

fn touch_rule(direction: LayoutDirection, content: impl Fn() + 'static) -> ComposeTestRule {
    let measurer = FixedTextMeasurer::with_metrics(FixedFontMetrics::SAMPLE_FONT);
    let content = Rc::new(content);
    let mut ui = compose_test_with(
        IntSize::new(600, 600),
        Density::new(1.0),
        Box::new(measurer),
        move || {
            let content = content.clone();
            compose_runtime::CompositionLocalProvider(
                compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction),
                move || content(),
            );
        },
    );
    ui.host().set_input_mode(InputMode::Touch);
    ui.wait_for_idle();
    ui
}

fn magnifier_positions(ui: &mut ComposeTestRule) -> Vec<Offset> {
    let nodes =
        ui.on_all_nodes(key_is_defined(&MAGNIFIER_POSITION_IN_ROOT)).fetch_semantics_nodes();
    let positions: Vec<MagnifierPositionInRoot> = nodes
        .iter()
        .filter_map(|node| node.config().get(&MAGNIFIER_POSITION_IN_ROOT).cloned())
        .collect();
    ui.wait_for_idle();
    positions.iter().map(MagnifierPositionInRoot::get).collect()
}

#[track_caller]
fn magnifier_center(ui: &mut ComposeTestRule) -> Offset {
    let positions = magnifier_positions(ui);
    let specified: Vec<Offset> =
        positions.iter().copied().filter(|position| position.is_specified()).collect();
    assert_eq!(specified.len(), 1, "expected one specified magnifier position: {positions:?}");
    specified[0]
}

#[track_caller]
fn assert_no_magnifier_exists(ui: &mut ComposeTestRule) {
    let positions = magnifier_positions(ui);
    assert!(
        positions.iter().all(|position| position.is_unspecified()),
        "expected no specified magnifier position: {positions:?}"
    );
    assert!(ui.magnifiers().showing().is_empty(), "expected no showing platform magnifier");
}

#[track_caller]
fn assert_offset_near(actual: Offset, expected: Offset, tolerance: f32) {
    assert!(
        (actual.x - expected.x).abs() <= tolerance && (actual.y - expected.y).abs() <= tolerance,
        "expected {expected:?} within {tolerance}, found {actual:?}"
    );
}

fn move_past_slop_by(touch: &mut TouchInjectionScope<'_>, delta: Offset) {
    let slop = touch.view_configuration().touch_slop;
    let sign = |v: f32| {
        if v > 0.0 {
            1.0
        } else if v < 0.0 {
            -1.0
        } else {
            0.0
        }
    };
    touch.move_by(delta + Offset::new(slop * sign(delta.x), slop * sign(delta.y)));
}

fn long_press(touch: &mut TouchInjectionScope<'_>, at: Offset) {
    let timeout = touch.view_configuration().long_press_timeout_millis;
    touch.down(at);
    touch.advance_event_time(timeout + 20);
    touch.move_to(at);
}

// ---- A position-aware node leaving vs a selection's handles ---------------------------------

/// A position-aware node leaving (here an `on_globally_positioned` box, and the gesture's
/// magnifier delegate) while a `SelectionContainer` has a selection keeps the graphics layers
/// above the container telling their moves: its handles follow a layer's move with no layout
/// (D2073's finding 3, fixed by D2105).
#[test]
fn removing_a_positioned_node_keeps_selection_handles_following_a_graphics_layer() {
    let show_probe = mutable_state_of(true);
    let move_layer = mutable_state_of(false);
    let (probe_read, layer_read) = (show_probe.handle(), move_layer.handle());
    let mut ui = touch_rule(LayoutDirection::Ltr, move || {
        Column!({
            // The lines' bottoms at 560 and 580, the window 600 px high.
            Box!(
                modifier = Modifier
                    .offset(0.dp(), 540.dp())
                    .graphics_layer(move |layer| {
                        layer.translation_y = if layer_read.get() { 30.0 } else { 0.0 };
                    })
                    .test_tag(TAG),
                {
                    SelectionContainer!({
                        BasicText!("aaaa\nbbbb", style = style());
                    });
                }
            );
            // After the container: its leaving doesn't move it, so nothing refreshes its handles.
            if probe_read.get() {
                Box!(modifier = Modifier.size(10.dp()).on_globally_positioned(|_| {}));
            }
        });
    });

    // A selection over both lines.
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        long_press(touch, Offset::new(10.0, 10.0));
        touch.move_to(Offset::new(30.0, 30.0));
        touch.up();
    });
    ui.wait_for_idle();
    let handle_shown = |ui: &mut ComposeTestRule, handle: Handle| {
        let nodes = ui.on_all_nodes(is_selection_handle(Some(handle))).fetch_semantics_nodes();
        nodes.iter().any(|node| {
            node.config().get(&SELECTION_HANDLE_INFO_KEY).is_some_and(|info| info.visible)
        })
    };
    assert!(handle_shown(&mut ui, Handle::SelectionStart));
    assert!(handle_shown(&mut ui, Handle::SelectionEnd));

    // The position-aware node leaves; the container stays where it is.
    show_probe.set(false);
    ui.wait_for_idle();
    assert!(handle_shown(&mut ui, Handle::SelectionStart));
    assert!(handle_shown(&mut ui, Handle::SelectionEnd));

    // 30 px down by the layer, with no layout: the end handle 10 px past the window, the start
    // one 10 px in it.
    move_layer.set(true);
    ui.wait_for_idle();
    assert!(handle_shown(&mut ui, Handle::SelectionStart), "start handle still in the window");
    assert!(!handle_shown(&mut ui, Handle::SelectionEnd), "end handle moved past the window");

    move_layer.set(false);
    ui.wait_for_idle();
    assert!(handle_shown(&mut ui, Handle::SelectionStart));
    assert!(handle_shown(&mut ui, Handle::SelectionEnd));
}

// ---- The selection magnifier following its container's moves -------------------------------

/// Moving a `SelectionContainer` or `BasicTextFieldOfState` by a layout offset or a graphics
/// layer while a selection handle is held moves both `MagnifierPositionInRoot` and the platform
/// magnifier's `source_center`, as Compose's `MagnifierNode.onGloballyPositioned` (D2105).
#[test]
fn selection_magnifier_follows_layout_offset_and_graphics_layer_moves_during_hold() {
    for is_field in [false, true] {
        let layout_shift = mutable_state_of(IntOffset::new(80, 90));
        let layer_shift = mutable_state_of(Offset::ZERO);
        let (layout_read, layer_read) = (layout_shift.handle(), layer_shift.handle());
        let state = **Box::leak(Box::new(TextFieldState::new("aaaa bbbb cccc")));
        let mut ui = touch_rule(LayoutDirection::Ltr, move || {
            let modifier = Modifier
                .offset_with(move |_| layout_read.get())
                .graphics_layer(move |layer| {
                    let delta = layer_read.get();
                    layer.translation_x = delta.x;
                    layer.translation_y = delta.y;
                })
                .test_tag(TAG);
            if is_field {
                BasicTextFieldOfState!(state, modifier = modifier, text_style = style());
            } else {
                SelectionContainer!(modifier = modifier, {
                    BasicText!("aaaa bbbb cccc", style = style());
                });
            }
        });

        ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.long_click());
        ui.wait_for_idle();
        ui.on_node(is_selection_handle(Some(Handle::SelectionStart))).perform_touch_input(
            |touch| {
                let center = touch.center();
                touch.down(center);
            },
        );
        ui.wait_for_idle();

        let initial_sem = magnifier_center(&mut ui);
        let initial_plat =
            ui.magnifiers().last().expect("created").shown().expect("showing").source_center;
        assert_offset_near(initial_plat, initial_sem, 0.01);

        // Move via layout offset while the handle remains pressed.
        layout_shift.set(IntOffset::new(110, 130));
        ui.wait_for_idle();
        let after_layout_sem = magnifier_center(&mut ui);
        let after_layout_plat =
            ui.magnifiers().last().expect("created").shown().expect("showing").source_center;
        assert_offset_near(after_layout_sem, initial_sem + Offset::new(30.0, 40.0), 0.01);
        assert_offset_near(after_layout_plat, after_layout_sem, 0.01);

        // Move via graphics layer translation while the handle remains pressed.
        layer_shift.set(Offset::new(25.0, -15.0));
        ui.wait_for_idle();
        let after_layer_sem = magnifier_center(&mut ui);
        let after_layer_plat =
            ui.magnifiers().last().expect("created").shown().expect("showing").source_center;
        assert_offset_near(after_layer_sem, after_layout_sem + Offset::new(25.0, -15.0), 0.01);
        assert_offset_near(after_layer_plat, after_layer_sem, 0.01);

        ui.on_node(is_selection_handle(Some(Handle::SelectionStart)))
            .perform_touch_input(|touch| touch.up());
        assert_no_magnifier_exists(&mut ui);
    }
}

// ---- `LOCAL_DENSITY` changing while `Modifier.magnifier` is hidden --------------------------

/// If `LOCAL_DENSITY` changes while `source_center` is `Offset::UNSPECIFIED` (no magnifier made
/// yet), showing the magnifier afterwards asks `source_center` and `magnifier_center` at the new
/// density, the density it's made at (`PlatformMagnifierConfig::density`, `on_size_changed`);
/// and a `source_center` specified at the new density only shows on the density change (D2107;
/// Compose keeps its first density for good).
#[test]
fn magnifier_reads_new_density_when_density_changed_while_hidden() {
    let factory = counting_factory(false);
    factory.set_size(Some(IntSize::new(20, 30)));
    let density = mutable_state_of(1.0f32);
    let visible = mutable_state_of(false);
    let sizes: Rc<RefCell<Vec<DpSize>>> = Rc::default();
    let (d_read, v_read, given, recorded) =
        (density.handle(), visible.handle(), factory.clone(), sizes.clone());

    let mut ui = compose_test(move || {
        let (given, recorded) = (given.clone(), recorded.clone());
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_DENSITY.provides(Density::new(d_read.get())),
            move || {
                let recorded = recorded.clone();
                let args = MagnifierArgs {
                    source_center: (move |d: Density| {
                        if v_read.get() {
                            Offset::new(d.to_px(10.dp()), d.to_px(20.dp()))
                        } else {
                            Offset::UNSPECIFIED
                        }
                    })
                    .into_callback(),
                    magnifier_center: Some(
                        (|d: Density| Offset::new(d.to_px(5.dp()), d.to_px(6.dp())))
                            .into_callback(),
                    ),
                    on_size_changed: Some(
                        (move |size: DpSize| recorded.borrow_mut().push(size)).into_callback(),
                    ),
                    platform_magnifier_factory: Some(given.clone()),
                    ..MagnifierArgs::new(|_: Density| Offset::UNSPECIFIED)
                };
                Box!(modifier = Modifier.size(100.dp()).magnifier_of(args));
            },
        );
    });

    ui.run_on_idle(|_| assert_eq!(factory.creation_count(), 0));
    density.set(2.0);
    ui.run_on_idle(|_| assert_eq!(factory.creation_count(), 0));

    visible.set(true);
    ui.run_on_idle(|_| {
        assert_eq!(factory.creation_count(), 1);
        let created = factory.last().expect("created");
        assert_eq!(created.config().density, Density::new(2.0));
        let shown = created.shown().expect("showing");
        assert_eq!(shown.source_center, Offset::new(20.0, 40.0));
        assert_eq!(shown.magnifier_center, Offset::new(10.0, 12.0));
        assert_eq!(*sizes.borrow(), vec![DpSize::new(10.dp(), 15.dp())]);
    });

    // Also check a `source_center` whose specification depends directly on `Density` without any
    // other state read changing.
    let density2 = mutable_state_of(1.0f32);
    let d2 = density2.handle();
    let factory2 = counting_factory(false);
    let given2 = factory2.clone();
    let mut ui2 = compose_test(move || {
        let given2 = given2.clone();
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_DENSITY.provides(Density::new(d2.get())),
            move || {
                let args = MagnifierArgs {
                    source_center: (|d: Density| {
                        if d.density >= 2.0 { Offset::new(12.0, 14.0) } else { Offset::UNSPECIFIED }
                    })
                    .into_callback(),
                    platform_magnifier_factory: Some(given2.clone()),
                    ..MagnifierArgs::new(|_: Density| Offset::UNSPECIFIED)
                };
                Box!(modifier = Modifier.size(50.dp()).magnifier_of(args));
            },
        );
    });
    ui2.run_on_idle(|_| assert_eq!(factory2.creation_count(), 0));
    density2.set(2.0);
    ui2.run_on_idle(|_| {
        assert_eq!(factory2.creation_count(), 1);
        assert_eq!(
            factory2.last().expect("created").shown().expect("showing").source_center,
            Offset::new(12.0, 14.0)
        );
    });
}

// ---- `ReusableContent` reuse and removal ----------------------------------------------------

/// A `MagnifierNode` reused for another `ReusableContent` key shows its magnifier at the new
/// slot's position (Compose's `onGloballyPositioned` sets the coordinates again), and it's
/// dismissed when its content is removed. Compose's node keeps its `previousSize` through a reuse
/// (no `onReset`), so the same size isn't told the new callback, as here.
#[test]
fn reused_or_removed_magnifier_node_follows_its_new_slot_and_dismisses() {
    let key = mutable_state_of(0i32);
    let show = mutable_state_of(true);
    let events: Rc<RefCell<Vec<(i32, DpSize)>>> = Rc::default();
    let magnifier = counting_magnifier(IntSize::new(10, 12));
    let (k_read, s_read, given, log) =
        (key.handle(), show.handle(), magnifier.clone(), events.clone());

    let mut ui = compose_test(move || {
        let (given, log) = (given.clone(), log.clone());
        if s_read.get() {
            let current_key = k_read.get();
            ReusableContent(current_key, move || {
                let log = log.clone();
                let (x, y) = if current_key == 0 { (50, 60) } else { (120, 140) };
                let args = MagnifierArgs {
                    source_center: (|_: Density| Offset::new(3.0, 4.0)).into_callback(),
                    on_size_changed: Some(
                        (move |size: DpSize| log.borrow_mut().push((current_key, size)))
                            .into_callback(),
                    ),
                    platform_magnifier_factory: Some(factory_of(&given, false)),
                    ..MagnifierArgs::new(|_: Density| Offset::ZERO)
                };
                Box!(modifier = Modifier.offset(x.dp(), y.dp()).size(20.dp()).magnifier_of(args));
            });
        }
    });

    ui.run_on_idle(|_| {
        assert_eq!(*events.borrow(), vec![(0, DpSize::new(10.dp(), 12.dp()))]);
        assert_eq!(magnifier.shown().expect("showing").source_center, Offset::new(53.0, 64.0));
    });

    key.set(1);
    ui.run_on_idle(|_| {
        assert_eq!(*events.borrow(), vec![(0, DpSize::new(10.dp(), 12.dp()))]);
        assert_eq!(magnifier.shown().expect("showing").source_center, Offset::new(123.0, 144.0));
    });
    assert_eq!(magnifier_center(&mut ui), Offset::new(123.0, 144.0));

    show.set(false);
    ui.run_on_idle(|_| assert!(!magnifier.is_showing()));
}

// ---- A pressed selection handle leaving before the touch slop -------------------------------

/// Pressing a selection handle shows the magnifier before the touch slop. If the handle leaves
/// before the slop (the selection collapsed in code, or the field unfocused), the magnifier
/// hides and stays hidden (Compose's press gesture's `invokeOnCompletion { clearHandleDragging() }`
/// in `TextFieldSelectionState.selectionHandleGestures`). A container's handle leaving with its
/// selection (released on the focus's loss) leaves no magnifier behind for the next long press.
#[test]
fn detaching_pressed_selection_handle_before_slop_hides_magnifier_and_clears_press() {
    // 1. The selection collapsed in code while `SelectionStart` is held.
    let state = **Box::leak(Box::new(TextFieldState::new("aaaa bbbb cccc")));
    let mut ui = touch_rule(LayoutDirection::Ltr, move || {
        Column!({
            BasicTextFieldOfState!(state, modifier = Modifier.test_tag(TAG), text_style = style(),);
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag(OTHER_TAG));
        });
    });

    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.long_click());
    ui.wait_for_idle();
    ui.on_node(is_selection_handle(Some(Handle::SelectionStart))).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    assert_eq!(ui.magnifiers().showing().len(), 1);

    state.edit(|buffer| buffer.place_cursor_at_end());
    ui.wait_for_idle();
    assert_no_magnifier_exists(&mut ui);
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    assert_no_magnifier_exists(&mut ui);

    // 2. The field unfocused while `SelectionEnd` is held, then focused again: no magnifier
    //    without a new gesture.
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        touch.advance_event_time(1000);
        touch.long_click();
    });
    ui.wait_for_idle();
    ui.on_node(is_selection_handle(Some(Handle::SelectionEnd))).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    assert_eq!(ui.magnifiers().showing().len(), 1);

    ui.on_node_with_tag(OTHER_TAG).request_focus();
    ui.wait_for_idle();
    assert_no_magnifier_exists(&mut ui);

    ui.on_node_with_tag(TAG).request_focus();
    ui.wait_for_idle();
    assert_no_magnifier_exists(&mut ui);
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    assert_no_magnifier_exists(&mut ui);

    // 3. A container's selection released (its focus lost) while `SelectionStart` is held: no
    //    magnifier then, nor after the next long press ends.
    let mut ui_container = touch_rule(LayoutDirection::Ltr, || {
        Column!({
            SelectionContainer!(modifier = Modifier.test_tag(TAG), {
                BasicText!("aaaa bbbb cccc", style = style());
            });
            Box!(modifier = Modifier.size(40.dp()).focusable().test_tag(OTHER_TAG));
        });
    });
    ui_container.on_node_with_tag(TAG).perform_touch_input(|touch| touch.long_click());
    ui_container.wait_for_idle();
    ui_container.on_node(is_selection_handle(Some(Handle::SelectionStart))).perform_touch_input(
        |touch| {
            let center = touch.center();
            touch.down(center);
        },
    );
    assert_eq!(ui_container.magnifiers().showing().len(), 1);

    ui_container.on_node_with_tag(OTHER_TAG).request_focus();
    ui_container.wait_for_idle();
    assert_no_magnifier_exists(&mut ui_container);
    ui_container.on_node_with_tag(TAG).perform_touch_input(|touch| touch.up());
    ui_container.wait_for_idle();
    assert_no_magnifier_exists(&mut ui_container);

    ui_container.on_node_with_tag(TAG).perform_touch_input(|touch| {
        touch.advance_event_time(1000);
        touch.long_click();
    });
    ui_container.wait_for_idle();
    assert_no_magnifier_exists(&mut ui_container);
}

// ---- `VisualTransformation` offsets past the text or inside a character ---------------------

struct OvershootingMultibyteTransform;

struct OvershootingMapping;

impl OffsetMapping for OvershootingMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        if offset == 0 {
            0
        } else {
            // Returns an odd byte index inside 2-byte Hebrew characters or past `shown.len()`.
            offset + 1
        }
    }
    fn transformed_to_original(&self, offset: usize) -> usize {
        offset.saturating_sub(1)
    }
}

impl VisualTransform for OvershootingMultibyteTransform {
    fn filter(&self, text: &str) -> TransformedText {
        TransformedText { text: text.to_string(), offset_mapping: Rc::new(OvershootingMapping) }
    }
}

/// As Compose's `calculateSelectionMagnifierCenterAndroid` coerces `originalToTransformed` into
/// `0..transformedText.length`, an `OffsetMapping` returning an offset past the transformed text or
/// mid-UTF-8 codepoint shows the magnifier without panicking (the paragraph's queries take the
/// offset to a character boundary).
#[test]
fn visual_transformation_with_unclamped_or_multibyte_offset_mapping_shows_magnifier() {
    let word: String = std::iter::repeat_n(RTL_CHAR, 4).collect();
    let initial = format!("{word} {word}");
    let transform = VisualTransformation::new(OvershootingMultibyteTransform);
    let mut ui = touch_rule(LayoutDirection::Rtl, move || {
        let value = remember(|| mutable_state_of(initial.clone()));
        let v = value;
        BasicTextField!(
            value.get(),
            move |next: String| v.set(next),
            modifier = Modifier.fill_max_size().wrap_content_size().test_tag(TAG),
            text_style = style(),
            visual_transformation = transform.clone(),
        );
    });

    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.long_click());
    ui.wait_for_idle();
    ui.on_node(is_selection_handle(Some(Handle::SelectionEnd))).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
        move_past_slop_by(touch, Offset::new(-2.0, 0.0));
    });
    ui.wait_for_idle();
    assert!(magnifier_center(&mut ui).is_specified());
}

// ---- `Modifier.magnifier` hand mutants & edge cases -----------------------------------------

/// Partial-NaN offsets (`Offset(x, NaN)` / `Offset(NaN, y)`) in `source_center` or
/// `magnifier_center` must be treated as unspecified: `source_center` dismisses the magnifier and
/// hides semantics; `magnifier_center` forwards `Offset::UNSPECIFIED` without adding the anchor
/// offset, while specified `magnifier_center` adds the node's root position.
#[test]
fn partial_nan_source_or_magnifier_center_is_treated_as_unspecified() {
    let source = mutable_state_of(Offset::new(5.0, 6.0));
    let lens: compose_runtime::OwnedState<Option<Offset>> =
        mutable_state_of(Some(Offset::new(7.0, 8.0)));
    let magnifier = counting_magnifier(IntSize::new(20, 20));
    let (s_read, l_read, given) = (source.handle(), lens.handle(), magnifier.clone());

    let mut ui = compose_test(move || {
        let mag_cb = l_read.get().map(|offset| (move |_: Density| offset).into_callback());
        let args = MagnifierArgs {
            source_center: (move |_: Density| s_read.get()).into_callback(),
            magnifier_center: mag_cb,
            platform_magnifier_factory: Some(factory_of(&given, false)),
            ..MagnifierArgs::new(|_: Density| Offset::ZERO)
        };
        Box!(modifier = Modifier.offset(30.dp(), 40.dp()).size(50.dp()).magnifier_of(args));
    });

    ui.run_on_idle(|_| {
        let shown = magnifier.shown().expect("showing");
        assert_eq!(shown.source_center, Offset::new(35.0, 46.0));
        assert_eq!(shown.magnifier_center, Offset::new(37.0, 48.0));
    });

    // Partial NaN in `magnifier_center` -> `Offset::UNSPECIFIED` passed to platform magnifier.
    lens.set(Some(Offset::new(7.0, f32::NAN)));
    ui.run_on_idle(|_| {
        let shown = magnifier.shown().expect("still showing");
        assert_eq!(shown.source_center, Offset::new(35.0, 46.0));
        assert!(shown.magnifier_center.is_unspecified());
    });

    // Switching `magnifier_center` callback from `Some` to `None` -> `Offset::UNSPECIFIED`.
    lens.set(Some(Offset::new(2.0, 3.0)));
    ui.run_on_idle(|_| {
        assert_eq!(magnifier.shown().expect("showing").magnifier_center, Offset::new(32.0, 43.0));
    });
    lens.set(None);
    ui.run_on_idle(|_| {
        assert!(magnifier.shown().expect("showing").magnifier_center.is_unspecified());
    });

    // Partial NaN in `source_center` (either axis) -> dismisses magnifier and removes semantics.
    for partial_nan in [Offset::new(f32::NAN, 6.0), Offset::new(5.0, f32::NAN)] {
        source.set(Offset::new(5.0, 6.0));
        ui.run_on_idle(|_| assert!(magnifier.is_showing()));
        source.set(partial_nan);
        ui.run_on_idle(|_| assert!(!magnifier.is_showing()));
        ui.on_node(key_is_defined(&MAGNIFIER_POSITION_IN_ROOT)).assert_does_not_exist();
    }
}

/// A drawing while shown copies the content once (`update_content`); drawing while hidden
/// (`source_center = Offset::UNSPECIFIED`) schedules none; and detaching while a drawing's copy
/// waits for its frame cancels it. (Several drawings before one copy, Compose's draw signal
/// coalescing, isn't observable here: the test rule's frame resumes the copy it drew for.)
#[test]
fn draw_updates_copy_once_and_skip_hidden_or_detached_magnifiers() {
    let radius = mutable_state_of(4.0f32);
    let source = mutable_state_of(Offset::UNSPECIFIED);
    let show_node = mutable_state_of(true);
    let magnifier = counting_magnifier(IntSize::new(20, 20));
    let (r_read, s_read, n_read, given) =
        (radius.handle(), source.handle(), show_node.handle(), magnifier.clone());

    let mut ui = compose_test(move || {
        if n_read.get() {
            let args = MagnifierArgs {
                source_center: (move |_: Density| s_read.get()).into_callback(),
                platform_magnifier_factory: Some(factory_of(&given, false)),
                ..MagnifierArgs::new(|_: Density| Offset::ZERO)
            };
            Box!(
                modifier = Modifier
                    .fill_max_size()
                    .draw_behind(move |scope| {
                        scope.draw_circle(scope.center(), r_read.get(), Paint::fill(Color::RED));
                    })
                    .magnifier_of(args)
            );
        }
    });

    // Hidden while drawing: zero content updates.
    ui.wait_for_idle();
    assert_eq!(magnifier.content_update_count(), 0);
    radius.set(8.0);
    ui.wait_for_idle();
    assert_eq!(magnifier.content_update_count(), 0);

    // Show: `update` updates properties without copying content until a draw happens; mutating
    // `radius` draws and schedules one `update_content` on the next frame.
    source.set(Offset::new(10.0, 10.0));
    ui.wait_for_idle();
    assert_eq!(magnifier.property_update_count(), 1);
    assert_eq!(magnifier.content_update_count(), 0);

    radius.set(10.0);
    ui.wait_for_idle();
    let baseline = magnifier.content_update_count();
    assert_eq!(baseline, 1);

    // Manual clock: trigger a draw, then detach BEFORE the next frame runs `with_frame_nanos`.
    ui.set_auto_advance(false);
    radius.set(12.0);
    show_node.set(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert!(!magnifier.is_showing());
    assert_eq!(
        magnifier.content_update_count(),
        baseline,
        "detached magnifier must not receive pending frame update_content"
    );
}

/// Keeping `zoom = f32::NAN` across updates when `can_update_zoom == false` must NOT recreate the
/// magnifier (probing `NaN != NaN`); changing `size`, `corner_radius`, `clipping_enabled`, or
/// `platform_magnifier_factory` must each recreate; and `use_text_default = true` uses the text
/// default dimensions (`100 x 48 dp`) in `TestMagnifierFactory`.
#[test]
fn nan_zoom_does_not_recreate_while_style_and_factory_changes_do() {
    let factory_a = counting_factory(false);
    let factory_b = counting_factory(false);
    let size = mutable_state_of(DpSize::new(60.dp(), 30.dp()));
    let corner = mutable_state_of(4.dp());
    let clip = mutable_state_of(true);
    let use_b = mutable_state_of(false);
    let use_text = mutable_state_of(false);
    let center = mutable_state_of(Offset::new(1.0, 1.0));
    let (sz, cr, cl, ub, ut, ct) = (
        size.handle(),
        corner.handle(),
        clip.handle(),
        use_b.handle(),
        use_text.handle(),
        center.handle(),
    );
    let (fa, fb) = (factory_a.clone(), factory_b.clone());

    let mut ui = compose_test(move || {
        let chosen: Rc<dyn PlatformMagnifierFactory> =
            if ub.get() { fb.clone() } else { fa.clone() };
        // Read here: a new center is a new element, so the node's update compares the zooms.
        let source_center = ct.get();
        let args = MagnifierArgs {
            source_center: (move |_: Density| source_center).into_callback(),
            zoom: f32::NAN,
            size: sz.get(),
            corner_radius: cr.get(),
            clipping_enabled: cl.get(),
            use_text_default: ut.get(),
            platform_magnifier_factory: Some(chosen),
            ..MagnifierArgs::new(|_: Density| Offset::ZERO)
        };
        Box!(modifier = Modifier.size(100.dp()).magnifier_of(args));
    });

    ui.run_on_idle(|_| assert_eq!(factory_a.creation_count(), 1));

    // Recompose with a new closure while `zoom` stays `f32::NAN` and `can_update_zoom == false`.
    center.set(Offset::new(2.0, 3.0));
    ui.run_on_idle(|_| assert_eq!(factory_a.creation_count(), 1));

    size.set(DpSize::new(70.dp(), 35.dp()));
    ui.run_on_idle(|_| {
        assert_eq!(factory_a.creation_count(), 2);
        assert_eq!(factory_a.last().unwrap().size(), IntSize::new(70, 35));
    });

    corner.set(8.dp());
    ui.run_on_idle(|_| assert_eq!(factory_a.creation_count(), 3));

    clip.set(false);
    ui.run_on_idle(|_| assert_eq!(factory_a.creation_count(), 4));

    use_text.set(true);
    ui.run_on_idle(|_| {
        assert_eq!(factory_a.creation_count(), 5);
        assert_eq!(factory_a.last().unwrap().size(), IntSize::new(100, 48));
    });

    let previous_from_a = factory_a.last().unwrap();
    assert!(previous_from_a.is_showing());
    use_b.set(true);
    ui.run_on_idle(|_| {
        assert!(!previous_from_a.is_showing(), "old factory's magnifier dismissed on swap");
        assert_eq!(factory_b.creation_count(), 1);
        assert!(factory_b.last().unwrap().is_showing());
    });
}

// ---- `TextFieldVisualTransformationMagnifierTest` (value-based `BasicTextField`) ------------

/// Mirrors Compose's `IncreasedVisualTransformation`: inserts `'a'` after each character
/// (UTF-8 char-aware so both ASCII and 2-byte `RTL_CHAR` map cleanly).
struct IncreasedVisualTransformation;

struct IncreasedMapping {
    char_byte_starts: Vec<usize>,
    transformed_starts: Vec<usize>,
    orig_len: usize,
    trans_len: usize,
}

impl OffsetMapping for IncreasedMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        let char_idx = self
            .char_byte_starts
            .partition_point(|&start| start < offset)
            .min(self.transformed_starts.len());
        self.transformed_starts.get(char_idx).copied().unwrap_or(self.trans_len)
    }
    fn transformed_to_original(&self, offset: usize) -> usize {
        let trans_char_idx = self
            .transformed_starts
            .iter()
            .enumerate()
            .take_while(|&(_, &start)| start < offset)
            .count();
        self.char_byte_starts.get(trans_char_idx).copied().unwrap_or(self.orig_len)
    }
}

impl VisualTransform for IncreasedVisualTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        let mut out = String::new();
        let mut char_byte_starts = Vec::new();
        let mut transformed_starts = Vec::new();
        for (byte_idx, ch) in text.char_indices() {
            char_byte_starts.push(byte_idx);
            transformed_starts.push(out.len());
            out.push(ch);
            out.push('a');
        }
        let trans_len = out.len();
        TransformedText {
            text: out,
            offset_mapping: Rc::new(IncreasedMapping {
                char_byte_starts,
                transformed_starts,
                orig_len: text.len(),
                trans_len,
            }),
        }
    }
}

/// Mirrors Compose's `ReducedVisualTransformation`: keeps only even-indexed characters.
struct ReducedVisualTransformation;

struct ReducedMapping {
    orig_char_starts: Vec<usize>,
    kept_trans_starts: Vec<usize>,
    orig_len: usize,
    trans_len: usize,
}

impl OffsetMapping for ReducedMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        let orig_idx = self.orig_char_starts.partition_point(|&start| start < offset);
        let kept_idx = orig_idx / 2;
        self.kept_trans_starts.get(kept_idx).copied().unwrap_or(self.trans_len)
    }
    fn transformed_to_original(&self, offset: usize) -> usize {
        let kept_idx = self.kept_trans_starts.partition_point(|&start| start < offset);
        let orig_idx = (kept_idx * 2).min(self.orig_char_starts.len());
        self.orig_char_starts.get(orig_idx).copied().unwrap_or(self.orig_len)
    }
}

impl VisualTransform for ReducedVisualTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        let mut out = String::new();
        let mut orig_char_starts = Vec::new();
        let mut kept_trans_starts = Vec::new();
        for (char_idx, (byte_idx, ch)) in text.char_indices().enumerate() {
            orig_char_starts.push(byte_idx);
            if char_idx % 2 == 0 {
                kept_trans_starts.push(out.len());
                out.push(ch);
            }
        }
        let trans_len = out.len();
        TransformedText {
            text: out,
            offset_mapping: Rc::new(ReducedMapping {
                orig_char_starts,
                kept_trans_starts,
                orig_len: text.len(),
                trans_len,
            }),
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/TextFieldVisualTransformationMagnifierTest.kt, magnifier_appears_whileHandleTouched, checkMagnifierFollowsHandleHorizontally
#[test]
fn value_based_text_field_magnifier_with_increased_and_reduced_visual_transformations() {
    for increased in [false, true] {
        for rtl in [false, true] {
            for handle in [Handle::Cursor, Handle::SelectionStart, Handle::SelectionEnd] {
                let transform = if increased {
                    VisualTransformation::new(IncreasedVisualTransformation)
                } else {
                    VisualTransformation::new(ReducedVisualTransformation)
                };
                let ch = if rtl { RTL_CHAR } else { 'a' };
                // 5-char words (stride 6 with spaces) keep spaces between words under both
                // `ReducedVisualTransformation` and `IncreasedVisualTransformation` so a center
                // `long_click` selects the middle word with room on both sides on one line.
                let word: String = std::iter::repeat_n(ch, 5).collect();
                let initial = format!("{word} {word} {word}");
                let direction = if rtl { LayoutDirection::Rtl } else { LayoutDirection::Ltr };
                let drag = Offset::new(if rtl { -1.0 } else { 1.0 }, 0.0);

                let mut ui = touch_rule(direction, move || {
                    let text = remember(|| mutable_state_of(initial.clone()));
                    let t = text;
                    BasicTextField!(
                        text.get(),
                        move |next: String| t.set(next),
                        modifier = Modifier.fill_max_size().wrap_content_size().test_tag(TAG),
                        text_style = TextStyle { font_size: 10.0.sp(), ..TextStyle::DEFAULT },
                        visual_transformation = transform.clone(),
                    );
                });

                if handle == Handle::Cursor {
                    ui.on_node_with_tag(TAG).perform_click();
                } else {
                    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.long_click());
                }
                ui.wait_for_idle();
                assert_no_magnifier_exists(&mut ui);

                ui.on_node(is_selection_handle(Some(handle))).perform_touch_input(|touch| {
                    let center = touch.center();
                    touch.down(center);
                    if handle == Handle::Cursor {
                        move_past_slop_by(touch, drag);
                    }
                });
                let start_pos = magnifier_center(&mut ui);

                ui.on_node(is_selection_handle(Some(handle))).perform_touch_input(|touch| {
                    if handle == Handle::Cursor {
                        touch.move_by(drag);
                    } else {
                        move_past_slop_by(touch, drag);
                    }
                });
                assert_offset_near(magnifier_center(&mut ui), start_pos + drag, 0.01);

                ui.on_node(is_selection_handle(Some(handle)))
                    .perform_touch_input(|touch| touch.up());
                assert_no_magnifier_exists(&mut ui);
            }
        }
    }
}

// ---- `OutputTransformation` & value-based scrolled `BasicTextField` -------------------------

/// In `BasicTextFieldOfState` with an `OutputTransformation` inserting `"$ "` before the state's
/// text, the magnifier centers on the transformed visual line (accounting for the 2-character
/// prefix width); and in a value-based `BasicTextField` inside a padded `decoration_box`, the
/// magnifier stays clamped to the visible inner field bounds when scrolled.
#[test]
fn output_transformation_prefix_and_scrolled_value_field_magnifier() {
    // 1. `OutputTransformation` inserting `"$ "` (40 px wide at 20 sp).
    let state = **Box::leak(Box::new(TextFieldState::with_selection("1234", TextRange::new(2, 2))));
    let output = OutputTransformation::from_fn(|buffer| {
        buffer.insert(0, "$ ");
    });
    let mut ui = touch_rule(LayoutDirection::Ltr, move || {
        BasicTextFieldOfState!(
            state,
            modifier = Modifier.test_tag(TAG),
            text_style = style(),
            output_transformation = Some(output.clone()),
        );
    });
    // Tap after `"$ 12"` (x = 80 px) to show the cursor handle, then drag 5 px right.
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.click_at(Offset::new(80.0, 10.0)));
    ui.wait_for_idle();
    ui.on_node(is_selection_handle(Some(Handle::Cursor))).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
        move_past_slop_by(touch, Offset::new(5.0, 0.0));
    });
    // Cursor at visual index 4 (`"$ 12|34"`, x = 80..82, center = 81), dragged +5 -> x = 86, y = 10.
    assert_offset_near(magnifier_center(&mut ui), Offset::new(86.0, 10.0), 0.5);

    // 2. Value-based `BasicTextField` with `single_line = true` and 8 dp decoration padding,
    //    dragged far past the right edge of a 100 dp wide box.
    let long_text = "aaaa bbbb cccc dddd eeee";
    let mut ui_val = touch_rule(LayoutDirection::Ltr, move || {
        let value = remember(|| mutable_state_of(long_text.to_string()));
        let v = value;
        BasicTextField!(
            value.get(),
            move |next: String| v.set(next),
            modifier = Modifier.width(100.dp()).test_tag(TAG),
            text_style = style(),
            single_line = true,
            decoration_box = |inner: compose_runtime::Callback| {
                Box!(modifier = Modifier.padding(8.dp()), { inner() });
            },
        );
    });
    ui_val
        .on_node_with_tag(TAG)
        .perform_touch_input(|touch| touch.click_at(Offset::new(20.0, 18.0)));
    ui_val.wait_for_idle();
    ui_val.on_node(is_selection_handle(Some(Handle::Cursor))).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
        move_past_slop_by(touch, Offset::new(200.0, 0.0));
    });
    // Inner field width is 100 - 16 = 84 px; plus 8 px left/top padding -> (92, 18).
    assert_offset_near(magnifier_center(&mut ui_val), Offset::new(92.0, 18.0), 1.0);
}

// ---- Exact half-width horizontal hide threshold & vertical spring animation -----------------

/// The selection magnifier's default size is `100 x 48 dp` (`width / 2 = 50 px`). Dragging `49 px`
/// past the line's right edge keeps the magnifier shown at `line_right`; dragging to `51 px` past
/// `line_right` hides it (`> width / 2`); and dragging back to `49 px` past `line_right` without
/// lifting the finger shows it again.
#[test]
fn magnifier_hides_just_past_half_width_off_line_and_reappears_when_dragged_back() {
    let layout_cell: Rc<RefCell<Option<TextLayoutResult>>> = Rc::default();
    let kept = layout_cell.clone();
    let mut ui = touch_rule(LayoutDirection::Ltr, move || {
        let kept = kept.clone();
        SelectionContainer!(
            modifier = Modifier.fill_max_size().wrap_content_size().test_tag(TAG),
            {
                let kept = kept.clone();
                BasicText!(
                    "aaaa bbbb\naaaa\naaaa bbbb",
                    style =
                        TextStyle { text_align: Some(compose_ui::TextAlign::Center), ..style() },
                    on_text_layout = Some(move |res: TextLayoutResult| {
                        *kept.borrow_mut() = Some(res);
                    }),
                );
            }
        );
    });

    let layout = layout_cell.borrow().clone().expect("laid out");
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_snapshot().bounds;
    let placed = Offset::new(bounds.left, bounds.top);
    // Long-press on the middle line (`"aaaa"`, 80 px wide inside a 180 px wide paragraph:
    // `line_left = 50`, `line_right = 130`).
    let line1_right = layout.get_line_right(1);
    let line1_y = (layout.get_line_top(1) + layout.get_line_bottom(1)) / 2.0;

    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        long_press(touch, Offset::new(line1_right - 10.0, line1_y));
    });
    assert_offset_near(
        magnifier_center(&mut ui) - placed,
        Offset::new(line1_right - 10.0, line1_y),
        0.01,
    );

    // 49 px past `line_right` (<= 50 px half-width): clamped at `line_right`, still shown.
    ui.on_node_with_tag(TAG)
        .perform_touch_input(|touch| touch.move_to(Offset::new(line1_right + 49.0, line1_y)));
    assert_offset_near(magnifier_center(&mut ui) - placed, Offset::new(line1_right, line1_y), 0.01);

    // 51 px past `line_right` (> 50 px half-width): hidden.
    ui.on_node_with_tag(TAG)
        .perform_touch_input(|touch| touch.move_to(Offset::new(line1_right + 51.0, line1_y)));
    assert_no_magnifier_exists(&mut ui);

    // Drag back to 49 px past `line_right` during the same gesture: reappears at `line_right`.
    ui.on_node_with_tag(TAG)
        .perform_touch_input(|touch| touch.move_to(Offset::new(line1_right + 49.0, line1_y)));
    assert_offset_near(magnifier_center(&mut ui) - placed, Offset::new(line1_right, line1_y), 0.5);
}

/// Moving horizontally on the same line snaps without spring lag, while crossing to another line
/// animates via `MAGNIFIER_SPRING` through intermediate vertical coordinates before settling, and
/// removing a container or field mid-drag immediately dismisses the platform magnifier.
#[test]
fn line_crossing_springs_through_intermediate_y_and_mid_drag_detach_dismisses() {
    let mounted = mutable_state_of(true);
    let m_read = mounted.handle();
    let mut ui = touch_rule(LayoutDirection::Ltr, move || {
        if m_read.get() {
            SelectionContainer!(
                modifier = Modifier.fill_max_size().wrap_content_size().test_tag(TAG),
                {
                    BasicText!("aaaa bbbb\ncccc dddd\neeee ffff", style = style());
                }
            );
        }
    });

    ui.set_auto_advance(false);
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        long_press(touch, Offset::new(10.0, 10.0));
    });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let initial = ui.magnifiers().last().unwrap().shown().unwrap().source_center;

    // Horizontal move on line 0: snaps on the very next frame (below `OFFSET_DISPLACEMENT_THRESHOLD`
    // vertically, `delta.y == 0`).
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.move_to(Offset::new(30.0, 10.0)));
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let horiz = ui.magnifiers().last().unwrap().shown().unwrap().source_center;
    assert_offset_near(horiz, initial + Offset::new(20.0, 0.0), 0.01);

    // Move down two lines (`+40 px` vertically): 2 frames (32 ms) in, `y` is strictly mid-flight.
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.move_to(Offset::new(30.0, 50.0)));
    ui.advance_time_by(Duration::from_millis(32));
    ui.wait_for_idle();
    let mid = ui.magnifiers().last().unwrap().shown().unwrap().source_center;
    assert!(
        mid.y > initial.y + 1.0 && mid.y < initial.y + 39.0,
        "expected intermediate spring y between {} and {}, got {:?}",
        initial.y,
        initial.y + 40.0,
        mid
    );

    // Detach the `SelectionContainer` mid-drag while the spring and gesture are still active!
    let live = ui.magnifiers().last().unwrap();
    assert!(live.is_showing());
    mounted.set(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert!(!live.is_showing(), "mid-drag unmount must dismiss platform magnifier immediately");
    assert!(ui.magnifiers().showing().is_empty());

    // Whitespace-only text shows a magnifier (unlike empty text, `"  "` is non-empty; tapped near
    // start so the drag stays within `width / 2` of `FixedTextMeasurer`'s `0.0` `line_right`).
    let flag = Rc::new(Cell::new(false));
    let seen = flag.clone();
    let mut ui_ws = touch_rule(LayoutDirection::Ltr, move || {
        seen.set(true);
        let state = remember(|| TextFieldState::new("  "));
        BasicTextFieldOfState!(state, modifier = Modifier.test_tag(TAG), text_style = style());
    });
    assert!(flag.get());
    ui_ws.on_node_with_tag(TAG).perform_touch_input(|touch| touch.click_at(Offset::new(5.0, 10.0)));
    ui_ws.wait_for_idle();
    ui_ws.on_node(is_selection_handle(Some(Handle::Cursor))).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
        move_past_slop_by(touch, Offset::new(5.0, 0.0));
    });
    assert!(magnifier_center(&mut ui_ws).is_specified());
}
