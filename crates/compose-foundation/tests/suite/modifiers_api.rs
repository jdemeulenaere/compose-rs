//! Compose's common modifiers: `aspect_ratio`, `wrap_content_*`, `width_in`, `height_in`,
//! `default_min_size`, `on_size_changed`, `on_globally_positioned`, `z_index`,
//! `combined_clickable`, `selectable`, `selectable_group`, `clip_to_bounds`, `layout`,
//! `hoverable`.

#![allow(non_snake_case)]

use compose_ui::ClickableOptions;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Constraints, IntSize, LayoutCoordinates, MutableInteractionSource, Offset, PointerEvent, Rect,
    UnitExt,
};

type Log = Rc<RefCell<Vec<String>>>;

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

fn click(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.wait_for_idle();
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
    ui.wait_for_idle();
}

/// Compose's overloads as one method each: `padding(all)`, `padding(horizontal, vertical)`,
/// `padding(start, top, end, bottom)` (zero defaults), `size(size)`, `size(width, height)`.
#[test]
fn padding_and_size_take_each_overloads_arguments() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Column!({
            Box!(modifier = Modifier.test_tag("all").padding(4.dp()).size(10.dp()));
            Box!(
                modifier =
                    Modifier.test_tag("hv").padding((8.dp(), 2.dp())).size((20.dp(), 10.dp()))
            );
            Box!(
                modifier = Modifier
                    .test_tag("start")
                    .padding(PaddingValues { start: 6.dp(), ..PaddingValues::ZERO })
                    .size(compose_ui::DpSize::new(10.dp(), 5.dp()))
            );
            Box!(modifier = Modifier.test_tag("sides").padding((1.dp(), 2.dp(), 3.dp(), 4.dp())));
        });
    });
    let size = |ui: &mut ComposeTestRule, tag| {
        let bounds = bounds(ui, tag);
        (bounds.width(), bounds.height())
    };
    assert_eq!(size(&mut ui, "all"), (18.0, 18.0));
    assert_eq!(size(&mut ui, "hv"), (36.0, 14.0));
    assert_eq!(size(&mut ui, "start"), (16.0, 5.0));
    assert_eq!(size(&mut ui, "sides"), (4.0, 6.0));
}

#[test]
fn aspect_ratio_sizes_from_the_width_or_the_height_first() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Column!({
            Box!(modifier = Modifier.width(100.dp()).aspect_ratio(2.0).test_tag("wide"));
            Box!(
                modifier = Modifier
                    .size((100.dp(), 30.dp()))
                    .wrap_content_size_with(Alignment::TOP_START, false)
                    .aspect_ratio_with(2.0, true)
                    .test_tag("height first")
            );
            Box!(
                modifier = Modifier
                    .size_in(SizeSpec::bounds(0.dp(), 0.dp(), 100.dp(), 30.dp()))
                    .aspect_ratio(2.0)
                    .test_tag("bounded")
            );
        });
    });
    let wide = bounds(&mut ui, "wide");
    assert_eq!((wide.width(), wide.height()), (100.0, 50.0));
    let height_first = bounds(&mut ui, "height first");
    assert_eq!((height_first.width(), height_first.height()), (60.0, 30.0));
    // The width's 100 would be 50 high: too high, so the height's 30 decides.
    let bounded = bounds(&mut ui, "bounded");
    assert_eq!((bounded.width(), bounded.height()), (60.0, 30.0));
}

#[test]
fn wrap_content_aligns_the_content_in_the_size_imposed() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Column!({
            Box!(
                modifier = Modifier
                    .size(100.dp())
                    .wrap_content_size_with(Alignment::CENTER, false)
                    .size(20.dp())
                    .test_tag("centered")
            );
            Box!(
                modifier = Modifier
                    .size(100.dp())
                    .wrap_content_width_with(Alignment::END, false)
                    .width(20.dp())
                    .test_tag("end")
            );
            Box!(
                modifier = Modifier
                    .size(50.dp())
                    .wrap_content_height_with(Alignment::TOP, true)
                    .height(80.dp())
                    .test_tag("unbounded")
            );
        });
    });
    assert_eq!(bounds(&mut ui, "centered"), Rect::new(40.0, 40.0, 60.0, 60.0));
    assert_eq!(bounds(&mut ui, "end"), Rect::new(80.0, 100.0, 100.0, 200.0));
    let unbounded = bounds(&mut ui, "unbounded");
    assert_eq!((unbounded.top, unbounded.height()), (200.0, 80.0));
}

#[test]
fn width_in_height_in_and_default_min_size_bound_the_size() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Column!({
            Box!(
                modifier = Modifier.width_in(50.dp(), 80.dp()).height(10.dp()).test_tag("narrow"),
                {
                    Box!(modifier = Modifier.size(10.dp()));
                }
            );
            Box!(
                modifier = Modifier.height_in(0.dp(), 20.dp()).width(10.dp()).test_tag("short"),
                {
                    Box!(modifier = Modifier.size(50.dp()));
                }
            );
            Box!(modifier = Modifier.default_min_size(30.dp(), 40.dp()).test_tag("default"));
            // A minimum already set wins.
            Box!(
                modifier =
                    Modifier.size(10.dp()).default_min_size(30.dp(), 40.dp()).test_tag("set")
            );
        });
    });
    assert_eq!(bounds(&mut ui, "narrow").width(), 50.0);
    assert_eq!(bounds(&mut ui, "short").height(), 20.0);
    let default = bounds(&mut ui, "default");
    assert_eq!((default.width(), default.height()), (30.0, 40.0));
    assert_eq!(bounds(&mut ui, "set").width(), 10.0);
}

#[test]
fn on_size_changed_reports_the_first_size_and_each_change() {
    let log: Log = Rc::default();
    let width = mutable_state_of(10.0f32);
    let (seen, handle) = (log.clone(), width.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier
                .padding(5.dp())
                .on_size_changed(move |size: IntSize| {
                    seen.borrow_mut().push(format!("{}x{}", size.width, size.height));
                })
                .size((handle.get().dp(), 20.dp()))
        );
    });
    assert_eq!(*log.borrow(), vec!["10x20"]);
    width.set(30.0);
    ui.wait_for_idle();
    width.set(30.0);
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), vec!["10x20", "30x20"]);
}

#[test]
fn a_size_written_by_on_size_changed_is_composed_in_the_same_frame() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        let measured = remember(|| mutable_state_of(0));
        let handle = measured;
        Column!({
            Box!(
                modifier = Modifier
                    .size(42.dp())
                    .on_size_changed(move |size: IntSize| handle.set(size.width))
            );
            BasicText!("width {}", measured.get());
        });
    });
    ui.host().frame();
    ui.on_node_with_text("width 42").assert_exists();
}

#[test]
fn on_globally_positioned_reports_the_layer_after_it_and_follows_a_scroll() {
    let log: Rc<RefCell<Vec<LayoutCoordinates>>> = Rc::default();
    let seen = log.clone();
    let state = ScrollState::new(0);
    let scroll = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let seen = seen.clone();
        Column!(modifier = Modifier.size((100.dp(), 100.dp())).vertical_scroll(scroll), {
            let seen = seen.clone();
            Box!(modifier = Modifier.size((100.dp(), 80.dp())));
            Box!(
                modifier = Modifier
                    .padding(10.dp())
                    .on_globally_positioned(move |coordinates: LayoutCoordinates| {
                        seen.borrow_mut().push(coordinates)
                    })
                    .size((50.dp(), 40.dp()))
            );
        });
    });
    let first = log.borrow()[0];
    assert_eq!(first.size(), IntSize::new(50, 40));
    assert_eq!(first.position_in_root(), Offset::new(10.0, 90.0));
    // Clipped by the scroll's viewport: 10 px of it shows.
    assert_eq!(first.bounds_in_root(), Rect::new(10.0, 90.0, 60.0, 100.0));
    assert_eq!(log.borrow().len(), 1);
    state.scroll_to(30);
    ui.wait_for_idle();
    let moved = *log.borrow().last().unwrap();
    assert_eq!(moved.position_in_window(), Offset::new(10.0, 60.0));
    assert_eq!(log.borrow().len(), 2);
}

#[test]
fn z_index_draws_and_hits_a_sibling_over_the_others() {
    let log: Log = Rc::default();
    let (first, second) = (log.clone(), log.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (first, second) = (first.clone(), second.clone());
        Box!({
            let (first, second) = (first.clone(), second.clone());
            Box!(
                modifier = Modifier
                    .z_index(1.0)
                    .size(50.dp())
                    .background(Color::RED)
                    .clickable(move || first.borrow_mut().push("first".into()))
            );
            Box!(
                modifier = Modifier
                    .size(50.dp())
                    .background(Color::BLUE)
                    .clickable(move || second.borrow_mut().push("second".into()))
            );
        });
    });
    let display = ui.dump_display_list();
    let red = display.find("#FFFF0000").expect("red");
    let blue = display.find("#FF0000FF").expect("blue");
    assert!(red > blue, "{display}");
    click(&mut ui, 10.0, 10.0);
    assert_eq!(*log.borrow(), vec!["first"]);
}

#[composable]
fn Combined(log: Log, double: bool) {
    let (click, long, double_click) = (log.clone(), log.clone(), log.clone());
    Box!(
        modifier = Modifier
            .size(50.dp())
            .combined_clickable(
                Some(callback(move || long.borrow_mut().push("long".into()))),
                double.then(|| callback(move || double_click.borrow_mut().push("double".into()))),
                move || click.borrow_mut().push("click".into())
            )
            .test_tag("target")
    );
}

#[test]
fn combined_clickable_clicks_long_clicks_and_double_clicks() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || Combined(seen.clone(), true));
    click(&mut ui, 10.0, 10.0);
    // A click waits to be no double click.
    assert!(log.borrow().is_empty());
    ui.advance_time_by(Duration::from_millis(350));
    assert_eq!(*log.borrow(), vec!["click"]);
    click(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(100));
    click(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(350));
    assert_eq!(*log.borrow(), vec!["click", "double"]);
    ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    ui.advance_time_by(Duration::from_millis(500));
    ui.host().send_pointer(PointerEvent::release(Offset::new(10.0, 10.0)));
    ui.advance_time_by(Duration::from_millis(500));
    assert_eq!(*log.borrow(), vec!["click", "double", "long"]);
    // Accessibility clicks at once.
    ui.on_node_with_tag("target").perform_semantics_action(compose_ui::SemanticsAction::OnClick);
    assert_eq!(log.borrow().last().unwrap(), "click");
    let semantics = ui.dump_semantics();
    assert!(semantics.contains("long_clickable"), "{semantics}");
    ui.on_node_with_tag("target")
        .perform_semantics_action(compose_ui::SemanticsAction::OnLongClick);
    assert_eq!(log.borrow().last().unwrap(), "long");
}

#[test]
fn combined_clickable_without_double_click_clicks_at_the_release() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || Combined(seen.clone(), false));
    click(&mut ui, 10.0, 10.0);
    assert_eq!(*log.borrow(), vec!["click"]);
}

#[test]
fn combined_clickable_shows_its_indication_while_pressed() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        Box!(
            modifier = Modifier.size(50.dp()).combined_clickable_with(
                ClickableOptions { indication: Color::BLACK.into(), ..ClickableOptions::DEFAULT },
                None,
                None,
                || {}
            )
        );
    });
    assert!(!ui.dump_display_list().contains("#1A000000"));
    ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    ui.wait_for_idle();
    assert!(ui.dump_display_list().contains("#1A000000"), "{}", ui.dump_display_list());
    ui.host().send_pointer(PointerEvent::release(Offset::new(10.0, 10.0)));
    ui.wait_for_idle();
    assert!(!ui.dump_display_list().contains("#1A000000"));
}

#[test]
fn selectable_and_selectable_group_set_their_semantics() {
    let selected = mutable_state_of(0);
    let handle = selected.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Column!(modifier = Modifier.selectable_group().test_tag("group"), {
            for index in 0..2 {
                Box!(
                    modifier = Modifier
                        .size(20.dp())
                        .selectable_with(
                            handle.get() == index,
                            ClickableOptions {
                                role: Some(Role::RadioButton),
                                ..ClickableOptions::DEFAULT
                            },
                            move || { handle.set(index) }
                        )
                        .test_tag(format!("option {index}"))
                );
            }
        });
    });
    let semantics = ui.dump_semantics();
    assert!(semantics.contains("selectable_group"), "{semantics}");
    assert!(semantics.contains("selected=true"), "{semantics}");
    ui.on_node_with_tag("option 1").perform_click();
    let option = ui.on_node_with_tag("option 1").fetch_semantics_node();
    assert_eq!(option.config.selected, Some(true));
    assert_eq!(option.config.role, Some(Role::RadioButton));
}

#[test]
fn clip_to_bounds_clips_the_content() {
    let ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!(modifier = Modifier.size(20.dp()).clip_to_bounds(), {
            Box!(modifier = Modifier.size(40.dp()).background(Color::RED));
        });
    });
    assert!(ui.dump_display_list().contains("clip 0,0 20x20"), "{}", ui.dump_display_list());
}

#[test]
fn the_layout_modifier_measures_and_places_with_a_closure() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!(
            modifier = Modifier
                .layout(|scope, measurable, constraints: Constraints| {
                    // Twice the content's width, the content at its end.
                    let placeable = scope.measure(measurable, constraints);
                    scope.place(&placeable, placeable.width, 0);
                    scope.layout(placeable.width * 2, placeable.height)
                })
                .size(30.dp())
                .test_tag("content")
        );
    });
    assert_eq!(bounds(&mut ui, "content"), Rect::new(30.0, 0.0, 60.0, 30.0));
    assert!(ui.dump_tree().contains("layout"), "{}", ui.dump_tree());
}

#[test]
fn hoverable_reports_the_mouse_over_it() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Box!(modifier = Modifier.size(50.dp()).hoverable(given.clone()));
    });
    assert!(!source.is_hovered());
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), false));
    ui.wait_for_idle();
    assert!(source.is_hovered());
    ui.host().send_pointer(PointerEvent::moved(Offset::new(100.0, 100.0), false));
    ui.wait_for_idle();
    assert!(!source.is_hovered());
}

#[test]
fn a_combined_clickable_disabled_mid_press_lets_its_press_go() {
    let enabled = mutable_state_of(true);
    let handle = enabled.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Box!(
            modifier = Modifier.size(50.dp()).combined_clickable_with(
                ClickableOptions {
                    enabled: handle.get(),
                    indication: Color::BLACK.into(),
                    ..ClickableOptions::DEFAULT
                },
                None,
                None,
                || {}
            )
        );
    });
    ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    ui.wait_for_idle();
    assert!(ui.dump_display_list().contains("#1A000000"));
    enabled.set(false);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(20));
    assert!(!ui.dump_display_list().contains("#1A000000"), "{}", ui.dump_display_list());
}

/// `ClickableOptions::interaction_source` reaches every member of the clickable family:
/// `toggleable_with`, `selectable_with` and `combined_clickable_with` report their press too.
#[test]
fn the_clickable_family_reports_presses_to_its_interaction_source() {
    for kind in 0..3 {
        let source = MutableInteractionSource::new();
        let given = source.clone();
        let mut ui = compose_test(move || {
            let options = ClickableOptions {
                interaction_source: Some(given.clone()),
                ..ClickableOptions::DEFAULT
            };
            let modifier = match kind {
                0 => Modifier.toggleable_with(false, options, |_| {}),
                1 => Modifier.selectable_with(false, options, || {}),
                _ => Modifier.combined_clickable_with(options, None, None, || {}),
            };
            Box!(modifier = modifier.size(40.dp()));
        });
        ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
        ui.wait_for_idle();
        assert!(source.is_pressed(), "kind {kind}: pressed");
        ui.host().send_pointer(PointerEvent::release(Offset::new(10.0, 10.0)));
        ui.wait_for_idle();
        assert!(!source.is_pressed(), "kind {kind}: released");
    }
}
