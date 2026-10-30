//! Adversarial review of retained drawing per repaint boundary (docs/06-ui.md, "Repaint
//! boundaries"): each frame of the gallery's interactions draws what drawing everything again at
//! that moment does.

use compose::test::*;
use compose::ui::{Offset, PointerEvent};

fn described(description: &'static str) -> Matcher {
    Matcher::new("described", move |config| {
        config
            .get(&compose::ui::SemanticsProperties::CONTENT_DESCRIPTION)
            .into_iter()
            .flatten()
            .any(|text| text.as_str() == description)
    })
}

/// The display list the last frame drew, and the one drawing every boundary again draws now.
fn retained_and_redrawn(ui: &mut ComposeTestRule) -> (String, String) {
    let retained = ui.host().display_list().dump();
    let redrawn = ui.host().with_tree(|tree| {
        tree.invalidate_draw();
        tree.draw();
        tree.display_list().dump()
    });
    (retained, redrawn)
}

fn check_frames(ui: &mut ComposeTestRule, frames: usize, what: &str) {
    let mut skipped = false;
    let mut frame = 0;
    // One more frame after a skipped last one, to see it draw what was left.
    while frame < frames || skipped {
        ui.advance_time_by_frame();
        // A drawing left for the next frame (a focus the frame's effect asked for, moved after
        // it, D906) changes what drawing everything draws now: compare the frames that leave none.
        // The next frame draws it: two in a row would be a drawing never done.
        if ui.host().with_tree(|tree| tree.needs_draw()) {
            assert!(
                !skipped,
                "{what}, frame {frame}: the frame after a skipped one left a drawing"
            );
            skipped = true;
            frame += 1;
            continue;
        }
        skipped = false;
        let (retained, redrawn) = retained_and_redrawn(ui);
        if retained != redrawn {
            let diff: Vec<String> = retained
                .lines()
                .zip(redrawn.lines())
                .filter(|(a, b)| a != b)
                .take(6)
                .map(|(a, b)| format!("kept:   {a}\ndrawn:  {b}"))
                .collect();
            panic!("{what}, frame {frame}: a boundary kept a stale drawing\n{}", diff.join("\n"));
        }
        frame += 1;
    }
}

#[test]
fn the_gallerys_frames_draw_what_drawing_everything_does() {
    let mut ui = ComposeTestRule::with_manual_clock(gallery::Gallery);
    check_frames(&mut ui, 5, "at rest");
    ui.on_node(described("Add a card")).perform_click();
    check_frames(&mut ui, 40, "card added, snackbar showing");
    ui.on_node(has_role(compose::ui::Role::Switch)).perform_click();
    check_frames(&mut ui, 40, "switch off, panel hiding");
    let list = ui.on_node_with_text("Card 15").fetch_semantics_snapshot().bounds;
    let inside = Offset::new(list.left + 10.0, list.top + 5.0);
    for _ in 0..4 {
        ui.host().send_pointer(PointerEvent::scroll(inside, Offset::new(0.0, 37.0)));
        check_frames(&mut ui, 2, "list scrolled");
    }
    check_frames(&mut ui, 30, "scroll settling");
    let moved = ui.on_node_with_text("Card 15").fetch_semantics_snapshot().bounds;
    assert!(moved.top < list.top, "scrolled: {list:?} {moved:?}");
    // The top bar slid away with the scroll; scrolling back brings it back at once.
    ui.host().send_pointer(PointerEvent::scroll(inside, Offset::new(0.0, -80.0)));
    check_frames(&mut ui, 10, "top bar back");
    ui.on_node(described("Search")).perform_click();
    check_frames(&mut ui, 10, "search open");
    ui.on_node(has_text("Search the cards")).perform_text_input("1");
    check_frames(&mut ui, 10, "filtered");
    ui.on_node(described("Close the search")).perform_click();
    check_frames(&mut ui, 10, "search closed");
    ui.on_node(described("Menu")).perform_click();
    check_frames(&mut ui, 30, "drawer opening");
}
