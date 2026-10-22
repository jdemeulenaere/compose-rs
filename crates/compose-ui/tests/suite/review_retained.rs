//! Adversarial review of retained drawing per repaint boundary (docs/06-ui.md, "Repaint
//! boundaries"): after a change, the incremental host's display list must be what a fresh host
//! with the final values draws (as `incremental_frames_draw_what_a_fresh_draw_does`).

#![allow(non_snake_case)]

use std::rc::Rc;

use crate::support::*;

use compose_runtime::*;
use compose_ui::*;

/// A display list's dump, without its paragraph handles (a fresh measurer numbers them anew).
fn dump(list: &DisplayList) -> String {
    list.dump()
        .lines()
        .map(|line| match line.find("paragraph ") {
            Some(at) => {
                let rest = &line[at + "paragraph ".len()..];
                let after = rest.find(' ').map_or("", |space| &rest[space..]);
                format!("{}paragraph _{after}", &line[..at])
            }
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fill(color: Color) -> Paint {
    Paint::fill(color)
}

fn tint(value: i32) -> Color {
    Color::from_argb(0xFF00_0000 | (value as u32).wrapping_mul(0x0012_3457) & 0x00FF_FFFF)
}

/// A column writing the height of its children to `published` while it measures them, as a
/// lazy list writes its layout info and a scroll its maximum.
struct Publishing(MutableState<i32>);

impl MeasurePolicy for Publishing {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let loose = Constraints::new(0, constraints.max_width, 0, Constraints::INFINITY);
        let (mut width, mut y) = (0, 0);
        for &measurable in measurables {
            let placeable = scope.measure(measurable, loose);
            scope.place(&placeable, 0, y);
            y += placeable.height;
            width = width.max(placeable.width);
        }
        self.0.set(y);
        scope.layout(constraints.constrain_width(width), constraints.constrain_height(y))
    }
}

/// A state written while laying out (a lazy list's layout info, a scroll's maximum), read while
/// drawing a repaint boundary that nothing else changed (a scrollbar in its own layer): the frame
/// that laid out draws it with the new value, as a fresh draw does.
#[test]
fn a_state_written_while_laying_out_is_drawn_by_a_clean_boundary_in_that_frame() {
    fn scene(height: MutableState<i32>, published: MutableState<i32>) {
        let policy = remember(|| Rc::new(Publishing(published)) as Rc<dyn MeasurePolicy>);
        Column!({
            Layout(
                Modifier,
                policy.clone(),
                callback(move || Leaf(Modifier.size_wh(10.dp(), height.get().dp()))),
            );
            Leaf(
                Modifier
                    .repaint_boundary()
                    .draw_behind(move |scope| {
                        let bounds = scope.bounds();
                        scope.draw_rect(bounds, fill(tint(published.get())));
                    })
                    .size(10.dp()),
            );
        });
    }
    let height = mutable_state_of(10);
    let published = mutable_state_of(0);
    let (height_handle, published_handle) = (height.handle(), published.handle());
    let mut host = host(move || scene(height_handle, published_handle));
    // The first frame wrote it while laying out: the next one draws it.
    host.frame();
    height.set(17);
    host.frame();
    let incremental = dump(&host.display_list());
    let fresh = {
        let height = mutable_state_of(17);
        let published = mutable_state_of(0);
        let (height_handle, published_handle) = (height.handle(), published.handle());
        let mut fresh = crate::support::host(move || scene(height_handle, published_handle));
        fresh.frame();
        dump(&fresh.display_list())
    };
    assert_eq!(incremental, fresh, "the boundary drew the value from before the layout");
}

/// Content moved under a resting mouse: the hover the frame's layout finds (`refresh_hover`) is
/// published to the interaction source before drawing, and a repaint boundary drawing it (a
/// pill showing another node's hover) draws it in that frame.
#[test]
fn a_hover_found_by_the_frames_layout_is_drawn_by_a_clean_boundary_in_that_frame() {
    // Made once, so the recomposition moving them gives them the same modifiers.
    fn scene(gap: MutableState<i32>, clickable: Modifier, pill: Modifier) {
        Column!({
            Leaf(Modifier.size_wh(10.dp(), gap.get().dp()));
            Leaf(clickable.clone());
            Leaf(pill.clone());
        });
    }
    let gap = mutable_state_of(20);
    let gap_handle = gap.handle();
    let source = MutableInteractionSource::new();
    let clickable = Modifier
        .clickable_interaction(true, None, Color::UNSPECIFIED, source.clone(), || {})
        .size(10.dp());
    let shown = source.clone();
    let pill = Modifier
        .repaint_boundary()
        .draw_behind(move |scope| {
            let color = if shown.is_hovered() { Color::RED } else { Color::BLUE };
            let bounds = scope.bounds();
            scope.draw_rect(bounds, fill(color));
        })
        .size(10.dp());
    let mut host = host(move || scene(gap_handle, clickable.clone(), pill.clone()));
    // The mouse rests at y 5, over the gap.
    host.send_pointer(PointerEvent::moved(Offset::new(5.0, 5.0), false));
    host.frame();
    assert!(!source.is_hovered());
    // The clickable moves under it.
    gap.set(0);
    host.frame();
    assert!(source.is_hovered(), "the layout found the hover");
    let list = host.display_list().dump();
    assert!(list.contains("#FFFF0000"), "the pill drew the hover from before the layout\n{list}");
}

/// Switching the layout direction mirrors what boundaries draw from it, as a fresh draw in that
/// direction does.
#[test]
fn switching_to_right_to_left_draws_what_a_fresh_right_to_left_draw_does() {
    fn scene() {
        Column!(modifier = Modifier.repaint_boundary(), {
            Row!({
                Leaf(Modifier.repaint_boundary().background(Color::RED).size(10.dp()));
                Leaf(Modifier.background(Color::GREEN).size(5.dp()));
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .draw_behind(|scope| {
                            let size = scope.size();
                            let x = if scope.layout_direction() == LayoutDirection::Rtl {
                                size.width - 2.0
                            } else {
                                0.0
                            };
                            scope.draw_rect(
                                Rect::new(x, 0.0, x + 2.0, size.height),
                                fill(Color::BLUE),
                            );
                        })
                        .padding_sides(1.dp(), 0.dp(), 3.dp(), 0.dp())
                        .background(Color::BLACK)
                        .size(8.dp()),
                );
            });
        });
    }
    let mut host = host(scene);
    host.set_layout_direction(LayoutDirection::Rtl);
    host.frame();
    let mut fresh = UiHost::headless(200, 100);
    fresh.set_layout_direction(LayoutDirection::Rtl);
    fresh.set_content(scene);
    fresh.frame();
    assert_eq!(dump(&host.display_list()), dump(&fresh.display_list()));
}

/// A popup whose host is inside a repaint boundary: its content drawing state, the boundary
/// moved, the popup's own state, each drawn as a fresh draw does.
#[test]
fn a_popup_inside_a_boundary_draws_what_a_fresh_draw_does() {
    fn scene(knobs: [MutableState<i32>; 3]) {
        let [inside, moved, popup] = knobs;
        Column!(modifier = Modifier.repaint_boundary(), {
            Leaf(Modifier.size_wh(10.dp(), moved.get().dp()));
            Column!(modifier = Modifier.repaint_boundary(), {
                Leaf(
                    Modifier
                        .draw_behind(move |scope| {
                            let bounds = scope.bounds();
                            scope.draw_rect(bounds, fill(tint(inside.get())));
                        })
                        .size(6.dp()),
                );
                Popup!(offset = IntOffset::new(3, 4), {
                    Column!(modifier = Modifier.repaint_boundary(), {
                        Leaf(
                            Modifier
                                .draw_behind(move |scope| {
                                    let bounds = scope.bounds();
                                    scope.draw_rect(bounds, fill(tint(popup.get())));
                                })
                                .size(5.dp()),
                        );
                    });
                });
            });
        });
    }
    let owned: Vec<OwnedState<i32>> = [1, 5, 2].iter().map(|v| mutable_state_of(*v)).collect();
    let knobs: [MutableState<i32>; 3] = std::array::from_fn(|index| owned[index].handle());
    let mut host = host(move || scene(knobs));
    let mut values = [1, 5, 2];
    for (knob, value) in [(0, 7), (2, 9), (1, 11), (2, 4), (0, 3), (1, 2)] {
        values[knob] = value;
        owned[knob].set(value);
        host.frame();
        let fresh = {
            let owned: Vec<OwnedState<i32>> = values.iter().map(|v| mutable_state_of(*v)).collect();
            let knobs: [MutableState<i32>; 3] = std::array::from_fn(|index| owned[index].handle());
            let fresh = crate::support::host(move || scene(knobs));
            let list = dump(&fresh.display_list());
            drop(fresh);
            list
        };
        assert_eq!(dump(&host.display_list()), fresh, "{values:?}");
    }
}

/// A recomposition giving a node inside a boundary (and the boundary's own node) a new drawing
/// closure, capturing a plain value: the update draws the boundary again.
#[test]
fn a_new_drawing_closure_from_a_recomposition_draws_its_boundary_again() {
    fn scene(value: i32) {
        let (inner, own) = (tint(value), tint(value + 1));
        Column!({
            Column!(modifier = Modifier.repaint_boundary(), {
                Leaf(
                    Modifier
                        .draw_behind(move |scope| {
                            let bounds = scope.bounds();
                            scope.draw_rect(bounds, fill(inner));
                        })
                        .size(4.dp()),
                );
            });
            Leaf(
                Modifier
                    .repaint_boundary()
                    .draw_behind(move |scope| {
                        let bounds = scope.bounds();
                        scope.draw_rect(bounds, fill(own));
                    })
                    .size(4.dp()),
            );
            // The same closure, its node moved by nothing: a modifier with a fixed value.
            Leaf(Modifier.repaint_boundary().background(tint(value + 2)).size(4.dp()));
        });
    }
    let value = mutable_state_of(1);
    let handle = value.handle();
    let mut host = host(move || scene(handle.get()));
    for next in [2, 3, 10] {
        value.set(next);
        host.frame();
        assert_eq!(
            dump(&host.display_list()),
            dump(&crate::support::host(move || scene(next)).display_list())
        );
    }
}

/// A window resized: boundaries whose constraints change draw again, as a fresh draw at that
/// size.
#[test]
fn resizing_the_window_draws_what_a_fresh_draw_at_that_size_does() {
    fn scene() {
        Column!(modifier = Modifier.repaint_boundary().fill_max_size(), {
            Leaf(
                Modifier.repaint_boundary().fill_max_width().background(Color::RED).height(5.dp()),
            );
            Leaf(
                Modifier
                    .repaint_boundary()
                    .draw_behind(|scope| {
                        let size = scope.size();
                        scope.draw_rect(
                            Rect::new(0.0, 0.0, size.width / 2.0, size.height),
                            fill(Color::BLUE),
                        );
                    })
                    .fill_max_size(),
            );
        });
    }
    let mut host = host(scene);
    host.resize(IntSize::new(150, 70));
    host.frame();
    let mut fresh = UiHost::headless(150, 70);
    fresh.set_content(scene);
    fresh.frame();
    assert_eq!(dump(&host.display_list()), dump(&fresh.display_list()));
}

/// Layers lifted out of their content, nested: the inner one's properties change, then the
/// outer's, then both with the inner's content; each frame draws what a fresh one does, the
/// outer's version stays while only the inner's properties change.
#[test]
fn nested_lifted_layers_draw_what_a_fresh_draw_does() {
    fn scene(knobs: [MutableState<i32>; 3]) {
        let [outer, inner, content] = knobs;
        Column!(
            modifier = Modifier.graphics_layer(move |layer| {
                layer.alpha = outer.get() as f32 / 10.0;
                layer.translation_x = outer.get() as f32;
            }),
            {
                Leaf(Modifier.background(Color::GREEN).size(3.dp()));
                Leaf(
                    Modifier
                        .padding(1.dp())
                        .graphics_layer(move |layer| {
                            layer.alpha = inner.get() as f32 / 10.0;
                            layer.rotation_z = inner.get() as f32;
                            layer.clip = inner.get() % 2 == 0;
                        })
                        .draw_behind(move |scope| {
                            let bounds = scope.bounds();
                            scope.draw_rect(bounds, fill(tint(content.get())));
                        })
                        .size_wh(4.dp(), (content.get() % 3 + 2).dp()),
                );
            }
        );
    }
    let owned: Vec<OwnedState<i32>> = [10, 10, 1].iter().map(|v| mutable_state_of(*v)).collect();
    let knobs: [MutableState<i32>; 3] = std::array::from_fn(|index| owned[index].handle());
    let mut host = host(move || scene(knobs));
    let mut values = [10, 10, 1];
    let mut previous = host.display_list();
    for (knob, value) in [(1, 5), (0, 4), (2, 2), (1, 8), (0, 10), (1, 10), (2, 6)] {
        values[knob] = value;
        owned[knob].set(value);
        host.frame();
        let list = host.display_list();
        let fresh = {
            let owned: Vec<OwnedState<i32>> = values.iter().map(|v| mutable_state_of(*v)).collect();
            let knobs: [MutableState<i32>; 3] = std::array::from_fn(|index| owned[index].handle());
            let fresh = crate::support::host(move || scene(knobs));
            let list = fresh.display_list();
            drop(fresh);
            list
        };
        assert_eq!(list.dump(), fresh.dump(), "{values:?}");
        crate::review_groups::check_groups(&list);
        if knob == 1 {
            // Only the inner layer's properties: no group's version changes.
            for group in &list.groups {
                let before = previous.groups.iter().find(|before| before.key == group.key);
                assert_eq!(before.map(|before| before.version), Some(group.version), "{values:?}");
            }
        }
        previous = list;
    }
}

/// A boundary not placed for a while (its parent placing it no more) while what it draws from
/// changes, placed again: it draws the new value.
#[test]
fn a_boundary_placed_again_draws_what_changed_while_it_was_not_placed() {
    struct Hiding(MutableState<bool>);
    impl MeasurePolicy for Hiding {
        fn measure(
            &self,
            scope: &mut MeasureScope<'_>,
            measurables: &[Measurable],
            constraints: Constraints,
        ) -> MeasureResult {
            let loose = constraints.loosen();
            for &measurable in measurables {
                let placeable = scope.measure(measurable, loose);
                if self.0.get() {
                    scope.place(&placeable, 0, 0);
                }
            }
            scope.layout(20, 20)
        }
    }
    fn scene(shown: MutableState<bool>, color: MutableState<i32>) {
        let policy = remember(|| Rc::new(Hiding(shown)) as Rc<dyn MeasurePolicy>);
        Layout(
            Modifier,
            policy,
            callback(move || {
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .draw_behind(move |scope| {
                            let bounds = scope.bounds();
                            scope.draw_rect(bounds, fill(tint(color.get())));
                        })
                        .size(5.dp()),
                );
            }),
        );
    }
    let shown = mutable_state_of(true);
    let color = mutable_state_of(1);
    let (shown_handle, color_handle) = (shown.handle(), color.handle());
    let mut host = host(move || scene(shown_handle, color_handle));
    shown.set(false);
    host.frame();
    assert!(!host.display_list().dump().contains("rect"), "{}", host.display_list().dump());
    color.set(2);
    host.frame();
    color.set(3);
    host.frame();
    shown.set(true);
    host.frame();
    let fresh = {
        let shown = mutable_state_of(true);
        let color = mutable_state_of(3);
        let (shown_handle, color_handle) = (shown.handle(), color.handle());
        let fresh = crate::support::host(move || scene(shown_handle, color_handle));
        dump(&fresh.display_list())
    };
    assert_eq!(dump(&host.display_list()), fresh);
}

/// A text replaced inside a boundary, its old paragraph released: what the boundary keeps
/// doesn't draw a released paragraph (a fresh measurer's handle for it may be another text's).
#[test]
fn a_text_replaced_inside_a_boundary_draws_its_new_paragraph() {
    let text = mutable_state_of(1);
    let handle = text.handle();
    let mut host = host(move || {
        Column!({
            Column!(modifier = Modifier.repaint_boundary(), {
                Text!("first {}", handle.get());
            });
            Column!(modifier = Modifier.repaint_boundary(), {
                Text!("second");
            });
        });
    });
    for next in 2..6 {
        text.set(next);
        host.frame();
        let list = host.display_list();
        let texts: Vec<String> = host.with_tree(|tree| {
            let measurer: &dyn TextMeasurer = tree.text_measurer_ref();
            list.ops
                .iter()
                .filter_map(|op| match op {
                    DrawOp::DrawParagraph(paragraph, _) => {
                        Some(measurer.text(*paragraph).to_string())
                    }
                    _ => None,
                })
                .collect()
        });
        assert_eq!(texts, [format!("first {next}"), "second".to_string()]);
    }
}

/// What a removed boundary kept is freed with its node: an image it drew isn't kept alive. A
/// replaced image (its node drawn again) isn't either.
#[test]
fn what_a_removed_or_redrawn_boundary_kept_is_freed() {
    let image = |shade: u8| ImageBitmap::from_rgba_premultiplied(1, 1, vec![shade, 0, 0, 255]);
    let shown = mutable_state_of(true);
    let current = mutable_state_of(0u8);
    let (shown_handle, current_handle) = (shown.handle(), current.handle());
    let images: Rc<Vec<ImageBitmap>> = Rc::new((0..3).map(image).collect());
    let weak: Vec<_> = images.iter().map(ImageBitmap::downgrade).collect();
    let given = images.clone();
    let mut host = host(move || {
        let images = given.clone();
        Column!({
            if shown_handle.get() {
                let images = images.clone();
                Column!(modifier = Modifier.repaint_boundary(), {
                    let images = images.clone();
                    Leaf(
                        Modifier
                            .repaint_boundary()
                            .draw_behind(move |scope| {
                                let image = &images[current_handle.get() as usize];
                                let rect = Rect::new(0.0, 0.0, 1.0, 1.0);
                                scope.draw_image(image, rect, scope.bounds(), 1.0);
                            })
                            .size(4.dp()),
                    );
                });
            }
        });
    });
    drop(images);
    // Held by the list the host keeps (the last frame's) and the boundary's recording.
    assert!(weak[0].upgrade().is_some());
    current.set(1);
    host.frame();
    // Redrawn: only the new image is kept, by the boundary and the host's list.
    assert!(weak[1].upgrade().is_some());
    assert_eq!(std::sync::Weak::strong_count(&weak[0]), 1, "the replaced image is still kept");
    shown.set(false);
    host.frame();
    host.frame();
    assert!(!host.display_list().dump().contains("image"), "{}", host.display_list().dump());
    // The composition's lambda holds the images; what matters is that no drawing does: the
    // count of strong references is the lambda's only.
    let strong = std::sync::Weak::strong_count(&weak[1]);
    assert_eq!(strong, 1, "a removed boundary's recording keeps the image it drew");
}
