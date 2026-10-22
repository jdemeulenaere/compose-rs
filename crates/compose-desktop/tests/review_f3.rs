//! Third phase F review: failing tests for compose-desktop (docs/07-platforms.md, "Desktop").
//! As in `review_f.rs`, the private mapping module is compiled here (`#[path]`) and fed winit
//! events built by hand; no window is opened. winit's `KeyEvent` can't be built outside winit,
//! so keyboard findings are written up in the review instead.

#![allow(dead_code, non_snake_case)]

#[path = "../src/input.rs"]
mod input;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use compose_ui::{Density, IntSize, Modifier, UiHost, UnitExt};
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};

// ---- Counting allocations -------------------------------------------------------------------

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

// SAFETY: forwards every call to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations(f: impl FnOnce()) -> u64 {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}

// ---- A field ----------------------------------------------------------------------------------

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

/// A 300 x 100 px window showing a single-line `BasicTextField`, `width` px wide and 40 px
/// high, holding `initial` at its top-left corner, focused, with the cursor at the start.
fn focused_field(initial: &'static str, width: i32) -> UiHost {
    let mut host = UiHost::new(
        IntSize::new(300, 100),
        Density::default(),
        Box::new(compose_ui::FixedTextMeasurer::new()),
    );
    host.set_content(move || {
        let text =
            compose_runtime::remember(|| compose_runtime::mutable_state_of(String::from(initial)));
        compose_foundation::BasicTextField(
            text.get(),
            compose_runtime::callback(move |value: String| text.set(value)),
            Modifier.size_wh(width.dp(), 40.dp()).test_tag("field"),
            true,
            false,
            compose_ui::TextStyle::DEFAULT,
            compose_foundation::KeyboardOptions::DEFAULT,
            compose_foundation::KeyboardActions::new(),
            true,
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

fn moved(x: f64, y: f64) -> WindowEvent {
    WindowEvent::CursorMoved { device_id: DeviceId::dummy(), position: PhysicalPosition::new(x, y) }
}

fn button(state: ElementState) -> WindowEvent {
    WindowEvent::MouseInput { device_id: DeviceId::dummy(), state, button: MouseButton::Left }
}

/// Sixty "a"s: 420 px of text in the 300 px field, which scrolls.
const LONG: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Dragging a selection past the window's edge keeps selecting in Compose Desktop: AWT keeps
/// sending the drag (`mouseDragged`) outside the window while the button is held, so a field
/// whose text runs past the window's edge selects on as the pointer goes further. On macOS and
/// X11 winit reports `CursorLeft` when the pointer crosses the window's edge during the drag
/// (and goes on sending `CursorMoved` outside it), and the host treats it as losing the pointer:
/// it cancels every gesture (`cancel_pointers`) and forgets the button is held (`pressed =
/// false`), so the moves after it are hovers. In a 300 px window with a 300 px field holding
/// sixty "a"s (420 px), pressing at the start, dragging out of the window's right edge to
/// x = 450 and releasing there selects all sixty in Compose, and "X" typed then gives "X".
/// Here the selection stops at the edge (41 "a"s), and the text becomes "X" and 19 "a"s.
#[test]
fn a_selection_drag_continues_outside_the_window() {
    let mut host = focused_field(LONG, 300);
    let mut state = input::InputState::default();
    input::handle(&mut state, &mut host, &moved(1.0, 8.0));
    input::handle(&mut state, &mut host, &button(ElementState::Pressed));
    input::handle(&mut state, &mut host, &moved(150.0, 8.0));
    input::handle(&mut state, &mut host, &moved(290.0, 8.0));
    input::handle(&mut state, &mut host, &WindowEvent::CursorLeft { device_id: DeviceId::dummy() });
    input::handle(&mut state, &mut host, &moved(350.0, 8.0));
    input::handle(&mut state, &mut host, &moved(450.0, 8.0));
    input::handle(&mut state, &mut host, &button(ElementState::Released));
    host.settle(10);
    host.send_text("X");
    host.settle(10);
    assert_eq!(field_text(&host), "X");
}

/// The control: the same drag without the `CursorLeft` selects everything.
#[test]
fn a_selection_drag_outside_the_window_without_cursor_left_selects_all() {
    let mut host = focused_field(LONG, 300);
    let mut state = input::InputState::default();
    input::handle(&mut state, &mut host, &moved(1.0, 8.0));
    input::handle(&mut state, &mut host, &button(ElementState::Pressed));
    input::handle(&mut state, &mut host, &moved(150.0, 8.0));
    input::handle(&mut state, &mut host, &moved(290.0, 8.0));
    input::handle(&mut state, &mut host, &moved(350.0, 8.0));
    input::handle(&mut state, &mut host, &moved(450.0, 8.0));
    input::handle(&mut state, &mut host, &button(ElementState::Released));
    host.settle(10);
    host.send_text("X");
    host.settle(10);
    assert_eq!(field_text(&host), "X");
}

/// docs/07 promises that a warm frame allocates nothing on the desktop ("Render and
/// present"). After every frame `redraw` also asks the host where the focused field's cursor
/// is, for the input method's candidate window (`host.text_input_area()`), and that collects
/// the focused node's key path into a new `Vec` (`UiTree::key_path`): with a text field
/// focused, every frame (an animation elsewhere in the window, a blinking caret later)
/// allocates. The frame itself allocates nothing here; the question about the cursor does.
#[test]
fn asking_for_the_text_input_area_allocates_nothing() {
    let mut host = focused_field("hello", 200);
    for _ in 0..3 {
        host.frame();
        let _ = host.text_input_area();
    }
    let frame = allocations(|| {
        host.frame();
    });
    assert_eq!(frame, 0, "the warm frame itself allocates");
    let area = allocations(|| {
        let _ = host.text_input_area();
    });
    assert_eq!(area, 0, "text_input_area allocates");
}
