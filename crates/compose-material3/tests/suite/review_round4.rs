//! Fourth adversarial review of Material: progress indicators, `ListItem`, `NavigationBar`, against
//! Compose's Material 3.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, IntSize, Modifier, Offset, UnitExt, platform::DrawOp};

fn lines(ui: &mut ComposeTestRule) -> Vec<(Offset, Offset)> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawLine(from, to, _) => Some((*from, *to)),
            _ => None,
        })
        .collect()
}

fn arcs(ui: &mut ComposeTestRule) -> Vec<(f32, f32)> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawArc { start_angle, sweep_angle, .. } => Some((*start_angle, *sweep_angle)),
            _ => None,
        })
        .collect()
}

/// Compose's `drawLinearIndicator` keeps a capped line in its bounds: with a `StrokeCap` other
/// than `Butt` (and room for it), both ends are coerced into `strokeWidth / 2 ..
/// width - strokeWidth / 2`, so the round caps end at the indicator's edges. Here the lines run
/// from 0 to the full width, their caps drawn 2 dp outside the 240 dp indicator on each side.
#[test]
fn a_round_capped_linear_indicator_keeps_its_caps_in_its_bounds() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        MaterialTheme!({
            LinearProgressIndicator!(0.5, stroke_cap = StrokeCap::Round);
        });
    });
    let found = lines(&mut ui);
    let ends: Vec<(f32, f32)> = found.iter().map(|(from, to)| (from.x, to.x)).collect();
    // The track from 128 (half of 240, the 4 dp gap and the 4 dp stroke for the caps) to 238,
    // the progress from 2 to 120.
    assert_eq!(ends, vec![(128.0, 238.0), (2.0, 120.0)], "{found:?}");
}

/// Material 3 1.4's determinate linear indicator draws its track after the progress, past a
/// gap (`gapSize`, 4 dp by default) grown by the stroke for capped lines (`adjustedGapSize`):
/// with no gap, the progress's round end and the track's touch, half a stroke past the
/// progress. Before, the track ran under the progress, its corners showing around the
/// progress's round end (the stress screen's action cards, D572).
#[test]
fn a_linear_indicator_without_a_gap_starts_its_track_where_the_progress_ends() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        MaterialTheme!({
            LinearProgressIndicator!(0.5, gap_size = 0.dp());
        });
    });
    let found = lines(&mut ui);
    let ends: Vec<(f32, f32)> = found.iter().map(|(from, to)| (from.x, to.x)).collect();
    // The track's line from 124 (its cap from 122), the progress's to 120 (its cap to 122).
    assert_eq!(ends, vec![(124.0, 238.0), (2.0, 120.0)], "{found:?}");
    // Full, no track; empty, only the track.
    for (progress, expected) in [(1.0, vec![(2.0, 238.0)]), (0.0, vec![(2.0, 238.0)])] {
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            MaterialTheme!({
                LinearProgressIndicator!(progress, gap_size = 0.dp());
            });
        });
        let found = lines(&mut ui);
        let ends: Vec<(f32, f32)> = found.iter().map(|(from, to)| (from.x, to.x)).collect();
        assert_eq!(ends, expected, "at {progress}: {found:?}");
    }
}

/// Compose draws a capped linear progress only when it has a length (`abs(endFraction -
/// startFraction) > 0`): at progress 0 only the track shows. Here a zero-length line is drawn,
/// which a round cap turns into a dot of the indicator's color at its start.
#[test]
fn a_round_capped_linear_indicator_at_zero_draws_no_progress_dot() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        MaterialTheme!({
            LinearProgressIndicator!(0.0, stroke_cap = StrokeCap::Round);
        });
    });
    let found = lines(&mut ui);
    assert_eq!(found.len(), 1, "only the track: {found:?}");
}

/// Compose's `drawIndeterminateCircularIndicator` moves the arc's start forward by the stroke
/// cap's offset when the cap isn't `Butt` (the default, `Round`, is one):
/// `(180 / PI) * (strokeWidth / (CircularIndicatorDiameter / 2)) / 2`, 5.73 degrees for the
/// default 4 dp stroke, so the arc's capped start shows where the animation puts it. At the start
/// of the animation, the arc starts at -90 + 5.73 = -84.27 degrees; here at -90.
#[test]
fn the_indeterminate_circular_arc_starts_past_its_stroke_cap() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        MaterialTheme!({
            IndeterminateCircularProgressIndicator!();
        });
    });
    let found = arcs(&mut ui);
    let (start, sweep) = found[0];
    let expected = -90.0 + (180.0 / std::f32::consts::PI) * (4.0 / 20.0) / 2.0;
    assert!((start - expected).abs() < 0.01, "start {start}, expected {expected} ({found:?})");
    assert!((sweep - 0.1).abs() < 0.001, "the minimum sweep: {sweep}");
}

/// Material 3 1.4's `ProgressIndicatorDefaults` (`LinearStrokeCap`,
/// `CircularDeterminateStrokeCap`, `CircularIndeterminateStrokeCap`) are all `StrokeCap.Round`,
/// and every overload taking a cap defaults to them (checked with `javap` on material3 1.4.0).
/// A round arc is also what Skia's `CircleOp` draws directly: a square one is a stroked path.
#[test]
fn every_indicator_defaults_to_a_round_cap() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        MaterialTheme!({
            Column!({
                CircularProgressIndicator!(0.25);
                IndeterminateCircularProgressIndicator!();
                LinearProgressIndicator!(0.5);
                IndeterminateLinearProgressIndicator!();
            });
        });
    });
    // Into the animations, where both indeterminate lines and the arc have a length.
    ui.advance_time_by(std::time::Duration::from_millis(1100));
    let caps: Vec<(&str, compose_ui::StrokeCap)> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawArc { paint, .. } => Some(("arc", paint.cap)),
            DrawOp::DrawLine(_, _, paint) => Some(("line", paint.cap)),
            _ => None,
        })
        .collect();
    assert!(caps.iter().filter(|(kind, _)| *kind == "arc").count() >= 2, "{caps:?}");
    assert!(caps.iter().filter(|(kind, _)| *kind == "line").count() >= 3, "{caps:?}");
    assert!(caps.iter().all(|(_, cap)| *cap == compose_ui::StrokeCap::Round), "{caps:?}");
}

/// Lock-in: a determinate indicator given a new progress draws it.
#[test]
fn passes_a_determinate_indicator_draws_a_new_progress() {
    let progress = mutable_state_of(0.25f32);
    let handle = progress.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            CircularProgressIndicator!(handle.get());
        });
    });
    assert_eq!(arcs(&mut ui), vec![(-90.0, 90.0)]);
    progress.set(0.5);
    ui.wait_for_idle();
    assert_eq!(arcs(&mut ui), vec![(-90.0, 180.0)]);
}

/// Compose's `ListItem` is one merged semantics node (`Modifier.semantics(mergeDescendants =
/// true) {}` on its `Surface`, before the given modifier): a screen reader reads the headline
/// and supporting text together, and the item's tag finds a node with both texts. Here the item
/// doesn't merge: its node has no text, each line is its own node.
#[test]
fn a_list_item_merges_its_texts_into_one_node() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            ListItem!(
                { Text!("Headline") },
                modifier = Modifier.test_tag("item"),
                supporting_content = Some(|| Text!("Supporting")),
            );
        });
    });
    let node = ui.on_node_with_tag("item").fetch_semantics_node();
    let texts: Vec<&str> = node
        .config
        .get(&compose_ui::SemanticsProperties::TEXT)
        .into_iter()
        .flatten()
        .map(|text| text.as_str())
        .collect();
    assert_eq!(texts, vec!["Headline", "Supporting"], "{}", ui.dump_semantics());
}

/// Compose's `NavigationBarItemDefaults.colors()` derives the disabled colors from the
/// unselected ones: `disabledIconColor = unselectedIconColor.copy(alpha = DisabledAlpha)` and
/// `disabledTextColor = unselectedTextColor.copy(alpha = DisabledAlpha)`, i.e. on-surface-variant
/// at 38%. Here they're on-surface at 38%.
#[test]
fn navigation_bar_item_disabled_colors_are_the_unselected_ones_faded() {
    let found: Rc<Cell<Option<(NavigationBarItemColors, Color)>>> = Rc::new(Cell::new(None));
    let sink = found.clone();
    let _ui = compose_test(move || {
        let sink = sink.clone();
        MaterialTheme!({
            let scheme = MaterialTheme::color_scheme();
            sink.set(Some((NavigationBarItemDefaults::colors(), scheme.on_surface_variant)));
        });
    });
    let (colors, on_surface_variant) = found.get().expect("composed");
    assert_eq!(colors.disabled_icon_color, on_surface_variant.with_alpha(0.38));
    assert_eq!(colors.disabled_text_color, on_surface_variant.with_alpha(0.38));
}

/// Compose's `NavigationBarItem` with `alwaysShowLabel = false` still composes the label of an
/// unselected item, only drawn transparent (`Modifier.alpha(animationProgress)`): its text stays
/// in the item's merged semantics, so a screen reader still announces the tab's name. Here the
/// label isn't composed at all: the unselected tab has no text for accessibility.
#[test]
fn a_hidden_navigation_label_still_names_its_tab() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            NavigationBar!(|row: RowScope| {
                for (index, name) in ["Home", "Search"].into_iter().enumerate() {
                    NavigationBarItem!(
                        row,
                        index == 0,
                        || {},
                        { Icon!(icons::filled::star(), None) },
                        modifier = Modifier.test_tag(name),
                        label = Some(move || Text!("{}", name)),
                        always_show_label = false,
                    );
                }
            });
        });
    });
    let search = ui.on_node_with_tag("Search").fetch_semantics_node();
    let texts: Vec<&str> = search
        .config
        .get(&compose_ui::SemanticsProperties::TEXT)
        .into_iter()
        .flatten()
        .map(|text| text.as_str())
        .collect();
    assert_eq!(texts, vec!["Search"], "{}", ui.dump_semantics());
}

/// Compose's `NavigationBarItem` clears the icon's semantics when a label shows
/// (`clearAndSetSemantics {}` on the icon's box when `label != null && (alwaysShowLabel ||
/// selected)`): "If there's a label, don't have a11y services repeat the icon description". Here
/// the icon's description merges into the item next to the label, read twice.
#[test]
fn a_labelled_navigation_item_does_not_repeat_its_icon_description() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            NavigationBar!(|row: RowScope| {
                NavigationBarItem!(
                    row,
                    true,
                    || {},
                    { Icon!(icons::filled::star(), Some("Home")) },
                    modifier = Modifier.test_tag("home"),
                    label = Some(|| Text!("Home")),
                );
            });
        });
    });
    let node = ui.on_node_with_tag("home").fetch_semantics_node();
    assert!(
        node.config
            .get(&compose_ui::SemanticsProperties::CONTENT_DESCRIPTION)
            .cloned()
            .unwrap_or_default()
            .is_empty(),
        "the icon's description is cleared: {:?}",
        node.config
            .get(&compose_ui::SemanticsProperties::CONTENT_DESCRIPTION)
            .cloned()
            .unwrap_or_default()
    );
    let texts: Vec<&str> = node
        .config
        .get(&compose_ui::SemanticsProperties::TEXT)
        .into_iter()
        .flatten()
        .map(|text| text.as_str())
        .collect();
    assert_eq!(texts, vec!["Home"]);
}

/// Compose's `NavigationBarItem` is `selectable(…, indication = null)`: the press ripple shows
/// only on the 64 by 32 dp indicator pill (`indicatorRipple`, clipped to the pill shape, its
/// interactions offset onto it), never across the item's whole share of the bar. Here the
/// state layer of a pressed item fills the whole item (here the whole 400 by 80 dp bar).
#[test]
fn a_pressed_navigation_item_shows_its_ripple_on_the_pill_only() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            NavigationBar!(|row: RowScope| {
                NavigationBarItem!(
                    row,
                    false,
                    || {},
                    { Icon!(icons::filled::star(), None) },
                    modifier = Modifier.test_tag("home"),
                    label = Some(|| Text!("Home")),
                );
            });
        });
    });
    let center = ui.on_node_with_tag("home").fetch_semantics_node().center();
    ui.host().send_pointer(compose_ui::PointerEvent::press(center));
    ui.wait_for_idle();
    let rects: Vec<compose_ui::Rect> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(rect, paint) if paint.color.alpha() < 255 => Some(*rect),
            _ => None,
        })
        .collect();
    assert!(
        rects.iter().all(|rect| rect.width() <= 64.0 && rect.height() <= 32.0),
        "a state layer wider than the pill: {rects:?}"
    );
}
