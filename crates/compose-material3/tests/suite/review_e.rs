//! Phase E review: failing tests for compose-material3 (docs/06-ui.md). Each test states the
//! input, what Jetpack Compose's material3 does, and what happens instead.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, IntSize, Modifier, Offset, PointerEvent, Rect, UnitExt, platform::DrawOp};

thread_local! {
    static SEEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static CLICKS: Cell<u32> = const { Cell::new(0) };
}

fn seen(entry: impl Into<String>) {
    SEEN.with(|seen| seen.borrow_mut().push(entry.into()));
}

fn take_seen() -> Vec<String> {
    SEEN.with(|seen| std::mem::take(&mut *seen.borrow_mut()))
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds
}

/// The bounds of the field's inner text (its focus target): the field's semantics node is the
/// whole decoration, as Compose's.
fn inner_text_bounds(ui: &mut ComposeTestRule) -> compose_ui::Rect {
    ui.on_node(Matcher::new("editable", |config| {
        config.contains(&compose_ui::SemanticsProperties::EDITABLE_TEXT)
    }))
    .request_focus();
    let bounds = ui.host().with_tree(|tree| tree.bounds_in_root(tree.focused().expect("focused")));
    ui.host().clear_focus();
    ui.wait_for_idle();
    bounds
}

fn click_at(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
    ui.wait_for_idle();
}

// ---- MaterialTheme -----------------------------------------------------------------------------

/// Compose's `MaterialTheme(colorScheme = MaterialTheme.colorScheme, shapes = …, typography =
/// …)` defaults each argument to the *current* theme, so a nested `MaterialTheme(shapes = …)`
/// keeps the outer dark scheme. Here the defaults are the baseline values
/// (`light_color_scheme()`), so the nested theme silently switches back to light.
#[test]
fn a_nested_theme_keeps_the_outer_color_scheme() {
    take_seen();
    let _ui = compose_test(|| {
        MaterialTheme!(color_scheme = dark_color_scheme(), {
            MaterialTheme!(shapes = Shapes::default(), {
                let primary = MaterialTheme::color_scheme().primary;
                seen(format!("{primary:?}"));
            });
        });
    });
    assert_eq!(take_seen(), [format!("{:?}", dark_color_scheme().primary)]);
}

/// Compose's material3 `MaterialTheme` provides the color scheme, shapes, typography and
/// `LocalTextStyle`, but not `LocalContentColor`: content outside a `Surface` keeps the
/// default `Color.Black` (the well-known reason to wrap a screen in a `Surface`). Here
/// `MaterialTheme` provides `on_surface` as the content color, so text outside any surface
/// reads E6E0E9 in the dark theme where Compose draws black.
#[test]
fn material_theme_does_not_provide_a_content_color() {
    take_seen();
    let _ui = compose_test(|| {
        MaterialTheme!(color_scheme = dark_color_scheme(), {
            seen(format!("{:?}", LOCAL_CONTENT_COLOR.current()));
        });
    });
    assert_eq!(take_seen(), [format!("{:?}", Color::BLACK)]);
}

// ---- Surface -----------------------------------------------------------------------------------

/// Compose's non-clickable `Surface` ends its modifier with `pointerInput(Unit) {}`, so it is
/// hit and blocks clicks from reaching what's beneath it ("prevent clicks from going through
/// the surface"). Here a `Surface` has no pointer input, so hit testing passes over it to the
/// button underneath, which clicks.
#[test]
fn a_surface_blocks_clicks_to_what_is_beneath_it() {
    CLICKS.with(|clicks| clicks.set(0));
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Box!({
            Button!(on_click = || CLICKS.with(|clicks| clicks.set(clicks.get() + 1)), {
                Text!("Under");
            });
            // A sheet over the button.
            Surface!(modifier = Modifier.size(150.dp()), {});
        });
    });
    click_at(&mut ui, 30.0, 24.0);
    assert_eq!(CLICKS.with(Cell::get), 0, "the click went through the surface");
}

// ---- Checkbox ----------------------------------------------------------------------------------

/// Compose's `Checkbox` applies `minimumInteractiveComponentSize()` only when `onCheckedChange`
/// is given (`if (onClick != null) Modifier.minimumInteractiveComponentSize() else
/// Modifier`): a display-only checkbox is 24 dp (20 dp plus 2 dp padding). Here it is always
/// 48 dp.
#[test]
fn a_checkbox_without_a_callback_has_no_touch_target() {
    let mut ui = compose_test(|| {
        Checkbox!(checked = true, on_checked_change = None, modifier = Modifier.test_tag("box"));
    });
    let box_bounds = bounds(&mut ui, "box");
    assert_eq!((box_bounds.width(), box_bounds.height()), (24.0, 24.0));
}

/// Compose draws the checkbox at `requiredSize(20.dp)` inside `wrapContentSize(Center)`: a
/// `Checkbox(modifier = Modifier.size(60.dp))` still draws a 20 dp box, centered. Here the box
/// is `size(20.dp)`, which the incoming fixed constraints override, so it's drawn 56 px wide.
#[test]
fn a_larger_checkbox_still_draws_a_20_dp_box() {
    let mut ui = compose_test(|| {
        Checkbox!(
            checked = true,
            on_checked_change = |_| {},
            modifier = Modifier.size(60.dp()).test_tag("box")
        );
    });
    let list = ui.host().display_list();
    let filled: Vec<Rect> = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRoundRect(rect, _, paint)
                if paint.style == compose_ui::PaintStyle::Fill =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(filled.len(), 1, "{}", list.dump());
    assert_eq!((filled[0].width(), filled[0].height()), (20.0, 20.0), "{}", list.dump());
}

/// Compose's `minimumInteractiveComponentSize` centers its content with
/// `((width - placeable.width) / 2f).roundToInt()`: a 41 px box in the 48 px target sits at 4
/// (3.5 rounded half up). Here integer division puts it at 3.
#[test]
fn the_minimum_touch_target_centers_as_compose_rounds() {
    let mut ui = compose_test(|| {
        Box!(modifier = minimum_interactive_component_size(Modifier).size(41.dp()).test_tag("in"));
    });
    assert_eq!(bounds(&mut ui, "in").left, 4.0);
}

// ---- TextField ---------------------------------------------------------------------------------

/// Compose's material `TextField` is a `BasicTextField` whose decoration box draws the
/// container, label and indicator: the whole 56 dp container is the text field, and a tap
/// anywhere in it (on the label, in the padding) focuses it. Here the container is a `Box`
/// around a smaller `BasicTextField`, so a tap in the container's padding does nothing.
#[test]
fn a_tap_anywhere_in_a_text_field_container_focuses_it() {
    #[composable]
    fn Field() {
        let text = remember(|| mutable_state_of(String::new()));
        MaterialTheme!({
            TextField!(
                value = text.get(),
                on_value_change = move |value| text.set(value),
                modifier = Modifier.test_tag("container"),
                label = { Text!("Name") },
            );
        });
    }
    let mut ui = compose_test(Field);
    // Inside the container, in its 16 dp start padding.
    click_at(&mut ui, 4.0, 28.0);
    let focused = ui.host().with_tree(|tree| tree.focused());
    assert!(focused.is_some(), "the tap in the container focused nothing");
}

/// Without a label, Compose's `TextField` uses `contentPaddingWithoutLabel()` (16 dp on every
/// side) and places the input at the top padding (multi-line) or centered (single line): in
/// the 56 dp container the text (24 px line) starts at y = 16. Here the padding is always the
/// with-label one (8 dp vertically) and the column sits at the bottom, so the text starts at
/// y = 24.
#[test]
fn a_text_field_without_a_label_places_its_text_as_compose() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TextField!(value = String::new(), on_value_change = |_| {});
        });
    });
    // The text field itself, inside the container (the field's node is the whole container).
    let input = inner_text_bounds(&mut ui);
    assert_eq!(input.top, 16.0);
}

// ---- Skipping ----------------------------------------------------------------------------------

/// A `Button` called again with the same arguments skips, as in Compose: when its parent
/// recomposes, only the parent's body runs. With a plain `Modifier` it does (checked). With
/// `Modifier.width(120.dp())` it doesn't: `width` leaves the height bounds `Dp::UNSPECIFIED`
/// (NaN), `SizeSpec`'s derived `PartialEq` finds NaN != NaN, so the modifier is never `same`
/// and the button and everything inside it run again (6 bodies, not 1).
#[test]
fn a_button_with_an_unchanged_width_modifier_skips() {
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Parent(handle);
        });
    });
    tick.set(1);
    let stats = ui.host().frame();
    assert_eq!(stats.bodies_run, 1, "only the parent should run");
}

#[composable]
fn Parent(tick: MutableState<u32>) {
    let _ = tick.get();
    Button!(on_click = || {}, modifier = Modifier.width(120.dp()), { Text!("OK") });
}
