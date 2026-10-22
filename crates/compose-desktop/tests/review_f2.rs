//! Second phase F review: failing tests for compose-desktop's input mapping
//! (docs/07-platforms.md, "Desktop"). As in `review_f.rs`, the private mapping module is
//! compiled here (`#[path]`) and fed winit events built by hand; no window is opened.

#![allow(dead_code, non_snake_case)]

#[path = "../src/input.rs"]
mod input;

use compose_ui::{Density, IntSize, Modifier, UiHost, UnitExt};
use winit::event::{Ime, WindowEvent};

/// The field's editable text, from the semantics tree.
fn field_text(host: &UiHost) -> String {
    let tree = host.semantics_tree(true);
    let field = tree
        .flatten()
        .into_iter()
        .find(|node| node.config.test_tag.as_deref() == Some("field"))
        .expect("the field")
        .clone();
    field.config.editable_text.as_deref().unwrap_or_default().to_string()
}

/// A host showing one empty `BasicTextField`, focused.
fn focused_field() -> UiHost {
    let mut host = UiHost::new(
        IntSize::new(300, 100),
        Density::default(),
        Box::new(compose_ui::FixedTextMeasurer::new()),
    );
    host.set_content(|| {
        let text = compose_runtime::remember(|| compose_runtime::mutable_state_of(String::new()));
        compose_foundation::BasicTextField(
            text.get(),
            compose_runtime::callback(move |value: String| text.set(value)),
            Modifier.size_wh(200.dp(), 40.dp()).test_tag("field"),
            true,
            false,
            compose_ui::TextStyle::DEFAULT,
            compose_foundation::KeyboardOptions::DEFAULT,
            compose_foundation::KeyboardActions::new(),
            false,
            u32::MAX,
            1,
            compose_foundation::VisualTransformation::NONE,
            compose_ui::Color::BLACK,
            compose_runtime::callback(compose_foundation::inner_text_field_only),
        );
    });
    host.settle(10);
    let field = host.with_tree(|tree| tree.focus_order()[0]);
    host.request_focus(field);
    host.settle(10);
    host
}

/// Typing with an input method (pinyin, kana, Hangul, dead keys on macOS): the platform sends
/// the text being composed (`Ime::Preedit`) before committing it. Compose Desktop shows it in
/// the field as it's composed (`SetComposingTextCommand`: the composition is part of the
/// field's value, underlined, and the value-based `BasicTextField` reports it through
/// `onValueChange`), and docs/07 lists "IME preedit" among the events the host maps. Here
/// `Ime::Preedit` falls through to `Outcome::Ignored`: while composing "ni" (before choosing
/// 你) the field shows nothing, and nothing in `UiHost` or the key input nodes could receive
/// a composition. (The host also never calls `Window::set_ime_cursor_area`, so the candidate
/// window opens at the window's corner rather than at the cursor.)
#[test]
fn the_text_being_composed_shows_in_the_field() {
    let mut host = focused_field();
    let mut state = input::InputState::default();
    input::handle(
        &mut state,
        &mut host,
        &WindowEvent::Ime(Ime::Preedit("ni".to_string(), Some((2, 2)))),
    );
    host.settle(10);
    assert_eq!(field_text(&host), "ni", "the composition isn't shown");
}

/// The committed text does reach the field (the control for the test above).
#[test]
fn committed_text_reaches_the_field() {
    let mut host = focused_field();
    let mut state = input::InputState::default();
    input::handle(&mut state, &mut host, &WindowEvent::Ime(Ime::Commit("你".to_string())));
    host.settle(10);
    assert_eq!(field_text(&host), "你");
}
