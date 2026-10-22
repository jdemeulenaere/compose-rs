//! Adversarial review of retained drawing per repaint boundary (docs/06-ui.md, "Repaint
//! boundaries"): each frame of the gallery's interactions draws what drawing everything again at
//! that moment does.

use compose::test::*;
use compose::ui::{Offset, PointerEvent};

fn described(description: &'static str) -> Matcher {
    Matcher::new("described", move |config| {
        config.content_description.iter().any(|text| text.as_str() == description)
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
    for frame in 0..frames {
        ui.advance_time_by_frame();
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
    let list = ui.on_node_with_text("Card 15").fetch_semantics_node().bounds;
    let inside = Offset::new(list.left + 10.0, list.top + 5.0);
    for _ in 0..4 {
        ui.host().send_pointer(PointerEvent::scroll(inside, Offset::new(0.0, 37.0)));
        check_frames(&mut ui, 2, "list scrolled");
    }
    check_frames(&mut ui, 30, "scroll settling");
    let moved = ui.on_node_with_text("Card 15").fetch_semantics_node().bounds;
    assert!(moved.top < list.top, "scrolled: {list:?} {moved:?}");
    ui.on_node(described("Search")).perform_click();
    check_frames(&mut ui, 10, "search open");
    ui.on_node(has_text("Search the cards")).perform_text_input("1");
    check_frames(&mut ui, 10, "filtered");
    ui.on_node(described("Close the search")).perform_click();
    check_frames(&mut ui, 10, "search closed");
    ui.on_node(described("Menu")).perform_click();
    check_frames(&mut ui, 30, "drawer opening");
}
