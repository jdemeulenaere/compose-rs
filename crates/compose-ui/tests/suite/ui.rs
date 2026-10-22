//! UI rules (docs/06-ui.md): layout, modifier order, modifier node state, input, focus,
//! semantics, incremental measurement and releasing.

#![allow(non_snake_case)]

use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

// ---- Layout ------------------------------------------------------------------------------------

#[test]
fn layouts_measure_and_place_with_modifiers() {
    let host = host(|| {
        Column!(modifier = Modifier.padding(10.dp()), {
            Leaf(Modifier.size_wh(30.dp(), 20.dp()));
            Row!({
                Leaf(Modifier.size(5.dp()));
                Leaf(Modifier.padding_horizontal_vertical(2.dp(), 1.dp()).size(10.dp()));
            });
        });
    });
    assert_eq!(
        host.dump_tree(),
        "\
StackMeasurePolicy [0,0 200x100]
  ColumnPolicy [0,0 50x52] Modifier.padding(10.dp, 10.dp, 10.dp, 10.dp)
    EmptyMeasurePolicy [0,0 30x20] Modifier.size(30.dp x 20.dp)
    RowPolicy [0,20 19x12]
      EmptyMeasurePolicy [0,0 5x5] Modifier.size(5.dp x 5.dp)
      EmptyMeasurePolicy [5,0 14x12] Modifier.padding(2.dp, 1.dp, 2.dp, 1.dp).size(10.dp x 10.dp)
"
    );
    let column = host.with_tree(|tree| tree.children_of(tree.root_id()).next().unwrap());
    let row = host.with_tree(|tree| tree.children_of(column).nth(1).unwrap());
    // In the root: the column's padding, then the row's place in the column.
    assert_eq!(host.with_tree(|tree| tree.bounds_in_root(row)), Rect::new(10.0, 30.0, 29.0, 42.0));
}

#[test]
fn size_modifiers_follow_compose() {
    let host = host(|| {
        Column!({
            // Within the incoming constraints: max width 200.
            Leaf(Modifier.width(300.dp()).height(4.dp()));
            // Fills the width.
            Leaf(Modifier.fill_max_width().height(4.dp()));
            Leaf(Modifier.fill_max_width_fraction(0.25).height(4.dp()));
            // Required: measured at 300, seen at 200, centered.
            Leaf(Modifier.required_size(300.dp(), 4.dp()));
            // Bounds: at least 10 wide.
            Leaf(Modifier.size_in(SizeSpec::bounds(
                10.dp(),
                4.dp(),
                Dp::UNSPECIFIED,
                Dp::UNSPECIFIED,
            )));
        });
    });
    let sizes = host.with_tree(|tree| {
        let column = tree.children_of(tree.root_id()).next().unwrap();
        tree.children_of(column).map(|child| tree.bounds_in_root(child)).collect::<Vec<_>>()
    });
    assert_eq!(
        sizes,
        [
            Rect::new(0.0, 0.0, 200.0, 4.0),
            Rect::new(0.0, 4.0, 200.0, 8.0),
            Rect::new(0.0, 8.0, 50.0, 12.0),
            Rect::new(-50.0, 12.0, 250.0, 16.0),
            Rect::new(0.0, 16.0, 10.0, 20.0),
        ]
    );
}

#[test]
fn density_scales_dp() {
    let mut host =
        UiHost::new(IntSize::new(100, 100), Density::new(2.0), Box::new(FixedTextMeasurer::new()));
    host.set_content(|| Leaf(Modifier.padding(1.dp()).size(10.dp())));
    host.frame();
    let leaf = host.with_tree(|tree| tree.children_of(tree.root_id()).next().unwrap());
    assert_eq!(host.with_tree(|tree| tree.bounds_in_root(leaf)).width(), 24.0);
}

// ---- Drawing -----------------------------------------------------------------------------------

#[test]
fn modifier_order_decides_what_is_drawn_where() {
    let host = host(|| {
        Column!({
            // Background inside the padding.
            Leaf(Modifier.padding(5.dp()).background(Color::RED).size(10.dp()));
            // Background under the padding.
            Leaf(Modifier.background(Color::BLUE).padding(5.dp()).size(10.dp()));
            // A clipped, bordered, rounded box.
            Leaf(
                Modifier
                    .size(20.dp())
                    .clip(Shape::Rounded([CornerSize::Dp(4.dp()); 4]))
                    .background(Color::GREEN)
                    .border(2.dp(), Color::BLACK, RECTANGLE_SHAPE),
            );
        });
    });
    assert_eq!(
        host.display_list().dump(),
        "\
rect 5,5 10x10 #FFFF0000
translate 0 20
rect 0,0 20x20 #FF0000FF
translate 0 20
save
  clip 0,0 20x20 radii [4.0, 4.0, 4.0, 4.0]
  rect 0,0 20x20 #FF00FF00
  rect 1,1 18x18 #FF000000 stroke 2
restore
translate 0 -40
"
    );
}

/// A repaint boundary's operations are a group, in its own coordinates (after its position):
/// a translation outside it isn't merged into one inside, so the group stays the same when
/// only its position changes.
#[test]
fn repaint_boundaries_are_groups_of_their_own_operations() {
    let host = host(|| {
        Column!({
            Leaf(Modifier.size(10.dp()));
            Leaf(Modifier.repaint_boundary().padding(5.dp()).background(Color::RED).size(10.dp()));
            Leaf(
                Modifier
                    .graphics_layer(|layer| layer.alpha = 0.5)
                    .background(Color::BLUE)
                    .size(10.dp()),
            );
        });
    });
    let list = host.display_list();
    assert_eq!(list.groups.len(), 2, "{}", list.dump());
    let ops = |group: &DrawGroup| {
        let mut part = DisplayList::default();
        part.ops = list.ops[group.start..group.end].to_vec();
        part.dump()
    };
    // The first: its rect, in its own coordinates (not the move to it).
    assert_eq!(ops(&list.groups[0]), "rect 5,5 10x10 #FFFF0000\n");
    assert!(matches!(list.ops[list.groups[0].start - 1], DrawOp::Translate(_)));
    // The layer's: its content, its alpha a property applied just outside it.
    let layer = list.groups[1];
    assert_eq!(ops(&layer), "rect 0,0 10x10 #FF0000FF\n");
    assert_eq!(layer.layer.map(|layer| layer.alpha), Some(0.5));
    assert_eq!(list.ops[layer.layer_start..layer.start], [DrawOp::SaveLayerAlpha(0.5)]);
    assert_eq!(list.ops[layer.end..layer.layer_end], [DrawOp::Restore]);
    assert_eq!(list.groups[0].layer, None);
    assert_ne!(list.groups[0].key, list.groups[1].key);
}

#[test]
fn text_elements_measure_and_draw_paragraphs_and_release_them() {
    let mut host = UiHost::headless(200, 100);
    let label = mutable_state_of("hello");
    let shown = mutable_state_of(true);
    let (label_handle, shown_handle) = (label.handle(), shown.handle());
    host.set_content(move || {
        if shown_handle.get() {
            Text!("{}", label_handle.get());
        }
    });
    host.frame();
    // 14 sp: 7 px per character, 17.5 px per line.
    assert_eq!(host.display_list().dump(), "paragraph 0 at 0 0\n");
    let text = host.with_tree(|tree| tree.children_of(tree.root_id()).next().unwrap());
    assert_eq!(host.with_tree(|tree| tree.bounds_in_root(text)), Rect::new(0.0, 0.0, 35.0, 18.0));
    assert_eq!(live_paragraphs(&host), 1);
    label.set("hi");
    host.frame();
    assert_eq!(host.with_tree(|tree| tree.bounds_in_root(text)).width(), 14.0);
    assert_eq!(live_paragraphs(&host), 1, "the old paragraph is released");
    shown.set(false);
    host.frame();
    assert_eq!(live_paragraphs(&host), 0, "a removed text releases its paragraph");
    assert_eq!(host.display_list().dump(), "");
}

#[test]
fn an_idle_frame_does_nothing() {
    let mut host = host(|| Leaf(Modifier.size(10.dp()).background(Color::RED)));
    let stats = host.frame();
    assert_eq!(stats, FrameStats::default());
    assert!(!host.needs_frame());
}

// ---- Input -------------------------------------------------------------------------------------

#[test]
fn a_click_is_a_press_released_inside() {
    let clicks = mutable_state_of(0);
    let handle = clicks.handle();
    let mut host = host(move || {
        Leaf(Modifier.size(50.dp()).clickable(move || handle.update(|count| count + 1)));
    });
    click(&mut host, 10.0, 10.0);
    assert_eq!(clicks.get(), 1);
    // Released outside: cancelled.
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    host.send_pointer(PointerEvent::release(Offset::new(80.0, 10.0)));
    assert_eq!(clicks.get(), 1);
    // Moved out and back: cancelled, as Compose's tap detection.
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    host.send_pointer(PointerEvent::moved(Offset::new(80.0, 10.0), true));
    host.send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), true));
    host.send_pointer(PointerEvent::release(Offset::new(10.0, 10.0)));
    assert_eq!(clicks.get(), 1);
    // Outside entirely.
    click(&mut host, 80.0, 80.0);
    assert_eq!(clicks.get(), 1);
}

#[test]
fn the_innermost_clickable_takes_the_click_and_the_topmost_sibling_is_hit() {
    let mut host = host(|| {
        Stack!(modifier = Modifier.size(100.dp()).clickable(|| log("outer")), {
            Leaf(Modifier.size(50.dp()).clickable(|| log("inner")));
            // Drawn after, so on top, where they overlap.
            Leaf(Modifier.offset(25.dp(), 0.dp()).size(50.dp()).clickable(|| log("top")));
        });
    });
    click(&mut host, 10.0, 10.0);
    click(&mut host, 40.0, 10.0);
    click(&mut host, 90.0, 90.0);
    assert_eq!(take_log(), ["inner", "top", "outer"]);
}

#[test]
fn a_disabled_clickable_ignores_clicks_and_its_parent_gets_them() {
    let mut host = host(|| {
        Stack!(modifier = Modifier.clickable(|| log("parent")), {
            Leaf(Modifier.size(20.dp()).clickable_with(false, None, || log("child")));
        });
    });
    click(&mut host, 5.0, 5.0);
    assert_eq!(take_log(), ["parent"]);
}

#[test]
fn padding_before_clickable_shrinks_the_click_area() {
    let mut host = host(|| {
        Leaf(Modifier.padding(10.dp()).clickable(|| log("click")).size(20.dp()));
    });
    click(&mut host, 5.0, 5.0);
    assert!(take_log().is_empty());
    click(&mut host, 15.0, 15.0);
    assert_eq!(take_log(), ["click"]);
}

#[test]
fn a_pressed_state_survives_a_recomposition_and_the_new_handler_runs() {
    let version = mutable_state_of(1);
    let handle = version.handle();
    let mut host = host(move || {
        let version = handle.get();
        Leaf(Modifier.size(20.dp()).clickable(move || log(format!("v{version}"))));
    });
    host.send_pointer(PointerEvent::press(Offset::new(5.0, 5.0)));
    version.set(2);
    host.frame();
    host.send_pointer(PointerEvent::release(Offset::new(5.0, 5.0)));
    assert_eq!(take_log(), ["v2"], "the node, with its press, was updated in place");
}

#[test]
fn modifier_nodes_keep_their_state_when_elements_are_added_around_them() {
    let extra = mutable_state_of(false);
    let handle = extra.handle();
    let mut host = host(move || {
        let modifier = if handle.get() { Modifier.padding(0.dp()) } else { Modifier };
        Leaf(modifier.size(20.dp()).clickable(|| log("click")).then(if handle.get() {
            Modifier.background(Color::RED)
        } else {
            Modifier
        }));
    });
    host.send_pointer(PointerEvent::press(Offset::new(5.0, 5.0)));
    extra.set(true);
    host.frame();
    host.send_pointer(PointerEvent::release(Offset::new(5.0, 5.0)));
    assert_eq!(take_log(), ["click"]);
}

#[test]
fn toggleable_calls_back_with_the_new_value() {
    let checked = mutable_state_of(false);
    let handle = checked.handle();
    let mut host = host(move || {
        Leaf(Modifier.size(20.dp()).toggleable(
            handle.get(),
            true,
            Some(Role::Checkbox),
            move |value| handle.set(value),
        ));
    });
    click(&mut host, 5.0, 5.0);
    assert!(checked.get());
    let tree = host.semantics_tree(true);
    assert_eq!(tree.children[0].config.toggleable_state, Some(ToggleableState::On));
    assert_eq!(tree.children[0].config.role, Some(Role::Checkbox));
    click(&mut host, 5.0, 5.0);
    assert!(!checked.get());
}

// ---- Focus and keys ----------------------------------------------------------------------------

#[test]
fn tab_moves_focus_through_clickables_and_enter_clicks() {
    let mut host = host(|| {
        Column!({
            Leaf(
                Modifier
                    .size(10.dp())
                    .on_focus_changed(|focused| log(format!("a focused {focused}")))
                    .clickable(|| log("a")),
            );
            Leaf(Modifier.size(10.dp()).clickable(|| log("b")));
            Leaf(Modifier.size(10.dp()).clickable_with(false, None, || log("disabled")));
        });
    });
    assert!(host.send_key(KeyEvent::down(Key::Tab)));
    host.send_key(KeyEvent::down(Key::Enter));
    host.send_key(KeyEvent::up(Key::Enter));
    assert!(host.send_key(KeyEvent::down(Key::Tab)));
    host.send_key(KeyEvent::down(Key::Space));
    host.send_key(KeyEvent::up(Key::Space));
    // Past the last focusable: wraps to the first, skipping the disabled one.
    host.send_key(KeyEvent::down(Key::Tab));
    let shift_tab = KeyEvent {
        modifiers: KeyModifiers { shift: true, ..KeyModifiers::default() },
        ..KeyEvent::down(Key::Tab)
    };
    host.send_key(shift_tab);
    assert_eq!(
        take_log(),
        // The initial state first, as Compose reports it on attach.
        [
            "a focused false",
            "a focused true",
            "a",
            "a focused false",
            "b",
            "a focused true",
            "a focused false"
        ]
    );
    let focused =
        host.semantics_tree(true).flatten().iter().filter(|node| node.config.focused).count();
    assert_eq!(focused, 1);
}

#[test]
fn key_events_bubble_from_the_focused_node_until_consumed() {
    let mut host = host(|| {
        Stack!(
            modifier = Modifier.on_key_event(|event: KeyEvent| {
                log(format!("outer {:?}", event.key));
                true
            }),
            {
                Leaf(
                    Modifier
                        .on_key_event(|event: KeyEvent| {
                            log(format!("inner {:?}", event.key));
                            event.key == Key::Character('a')
                        })
                        .focusable(true)
                        .size(10.dp()),
                );
            }
        );
    });
    let leaf = host.with_tree(|tree| tree.focus_order()[0]);
    host.request_focus(leaf);
    host.send_key(KeyEvent::down(Key::Character('a')));
    host.send_key(KeyEvent::down(Key::Character('b')));
    assert_eq!(
        take_log(),
        ["inner Character('a')", "inner Character('b')", "outer Character('b')"]
    );
}

#[test]
fn removing_the_focused_node_clears_focus() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut host = host(move || {
        if handle.get() {
            Leaf(Modifier.size(10.dp()).clickable(|| {}));
        }
    });
    host.send_key(KeyEvent::down(Key::Tab));
    assert!(host.with_tree(|tree| tree.focused()).is_some());
    shown.set(false);
    host.frame();
    assert!(host.with_tree(|tree| tree.focused()).is_none());
}

// ---- Semantics ---------------------------------------------------------------------------------

#[test]
fn a_clickable_merges_its_descendants_semantics() {
    let host = host(|| {
        Column!({
            Row!(modifier = Modifier.clickable(|| {}).test_tag("button"), {
                Text!("Count");
                Text!(": 3");
            });
            Text!("Label");
        });
    });
    let merged = host.semantics_tree(true);
    assert_eq!(
        merged.dump(),
        "\
[0,0 200x100] 
  [0,0 56x18] text=[\"Count\", \": 3\"] tag=\"button\" clickable focusable merged
  [0,18 35x18] text=[\"Label\"]
"
    );
    let unmerged = host.semantics_tree(false);
    assert_eq!(unmerged.children[0].children.len(), 2, "unmerged, the texts are nodes");
    assert_eq!(merged.children[0].center(), Offset::new(28.0, 9.0));
}

// ---- Incremental layout and releasing ----------------------------------------------------------

#[test]
fn a_size_change_remeasures_up_to_the_first_ancestor_whose_size_holds() {
    let width = mutable_state_of(10);
    let handle = width.handle();
    let host_ = host(move || {
        Column!({
            // A fixed-size box: its size holds whatever its content does.
            Stack!(modifier = Modifier.size(50.dp()), {
                Column!({
                    Leaf(Modifier.size_wh((handle.get() as f32).dp(), 5.dp()));
                    Leaf(Modifier.size(5.dp()));
                });
            });
            Column!({
                Leaf(Modifier.size(5.dp()));
            });
        });
    });
    let mut host = host_;
    take_measures();
    width.set(20);
    host.frame();
    // The inner column (its width changed), not the stack (fixed size) nor the outer column,
    // nor the sibling column.
    assert_eq!(take_measures(), 1);
    host.frame();
    assert_eq!(take_measures(), 0);
}

#[test]
fn removed_nodes_are_freed_with_their_modifier_nodes() {
    let count = mutable_state_of(3);
    let handle = count.handle();
    let mut host = host(move || {
        Column!({
            for index in 0..handle.get() {
                Text!("{index}", modifier = Modifier.clickable(|| {}));
            }
        });
    });
    let full = host.with_tree(|tree| tree.node_count());
    count.set(1);
    host.frame();
    assert_eq!(host.with_tree(|tree| tree.node_count()), full - 2);
    assert_eq!(live_paragraphs(&host), 1);
    count.set(3);
    host.frame();
    assert_eq!(host.with_tree(|tree| tree.node_count()), full);
}

#[test]
fn dropping_the_host_frees_every_handler_and_state() {
    let dropped = Rc::new(Cell::new(false));
    struct Flag(Rc<Cell<bool>>);
    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let flag = Rc::new(Flag(dropped.clone()));
    let before = live_state_count();
    {
        let host = host(move || {
            let flag = flag.clone();
            let state = remember(|| mutable_state_of(0));
            Leaf(Modifier.clickable(move || {
                let _ = &flag;
                state.set(1);
            }));
        });
        drop(host);
    }
    assert!(dropped.get(), "the handler, holding the flag, was dropped");
    assert_eq!(live_state_count(), before);
}

#[test]
fn a_modifier_closure_is_memoized_so_the_receiver_skips() {
    thread_local! {
        static RUNS: Cell<u32> = const { Cell::new(0) };
    }
    #[composable]
    fn Receiver(modifier: Modifier) {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        Leaf(modifier);
    }
    #[composable]
    fn Screen(tick: MutableState<u32>, count: MutableState<u32>) {
        tick.get();
        Receiver(Modifier.size(10.dp()).clickable(move || count.update(|count| count + 1)));
    }
    let tick = mutable_state_of(0);
    let count = mutable_state_of(0);
    let (tick_handle, count_handle) = (tick.handle(), count.handle());
    let mut host = host(move || Screen(tick_handle, count_handle));
    for round in 1..4 {
        tick.set(round);
        host.frame();
    }
    assert_eq!(RUNS.with(Cell::get), 1, "the same modifier every pass: the receiver skips");
    click(&mut host, 5.0, 5.0);
    assert_eq!(count.get(), 1);
}

// ---- State read in layout and drawing (D71) -----------------------------------------------------

#[test]
fn state_read_by_a_policy_or_a_draw_node_measures_or_draws_again_without_recomposing() {
    thread_local! {
        static WIDTH: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
        static SHADE: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
    }
    struct StateWidth(MutableState<i32>);
    impl MeasurePolicy for StateWidth {
        fn measure(
            &self,
            scope: &mut MeasureScope<'_>,
            _: &[Measurable],
            _: Constraints,
        ) -> MeasureResult {
            scope.layout(self.0.get(), 10)
        }
    }
    struct Shade(MutableState<u32>);
    impl ModifierNodeElement for Shade {
        fn create(&self) -> Box<dyn ModifierNode> {
            Box::new(ShadeNode(self.0))
        }
        fn update(&self, _: &mut dyn ModifierNode) {}
        fn same(&self, _: &dyn ModifierNodeElement) -> bool {
            true
        }
    }
    struct ShadeNode(MutableState<u32>);
    impl ModifierNode for ShadeNode {
        fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
            Some(self)
        }
    }
    impl DrawModifierNode for ShadeNode {
        fn draw(&mut self, scope: &mut DrawScope<'_>) {
            let color = Color::from_argb(0xFF00_0000 | self.0.get());
            scope.draw_rect(scope.bounds(), Paint::fill(color));
        }
    }
    #[composable]
    fn Screen() {
        let width = remember(|| mutable_state_of(10));
        let shade = remember(|| mutable_state_of(0x11));
        WIDTH.with(|cell| cell.set(Some(width)));
        SHADE.with(|cell| cell.set(Some(shade)));
        Layout(Modifier.then_custom(Shade(shade)), Rc::new(StateWidth(width)), callback(|| {}));
    }
    let mut host = host(Screen);
    let bodies = compose_runtime::bodies_run();
    WIDTH.with(Cell::get).unwrap().set(42);
    assert!(host.needs_frame(), "the write asks for a frame");
    host.frame();
    let leaf = host.with_tree(|tree| tree.children_of(tree.root_id()).next().unwrap());
    assert_eq!(host.with_tree(|tree| tree.bounds_in_root(leaf)).width(), 42.0);
    SHADE.with(Cell::get).unwrap().set(0x22);
    host.frame();
    assert!(host.display_list().dump().contains("#FF000022"), "{}", host.display_list().dump());
    assert_eq!(compose_runtime::bodies_run(), bodies, "no recomposition");
}
