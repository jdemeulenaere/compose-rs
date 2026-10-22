//! Gestures (docs/guide/kotlin-to-rust.md, "Gestures").

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: tap
#[composable]
pub fn Photo() {
    let liked = remember(|| mutable_state_of(false));
    Box!(
        modifier = Modifier.size(160.dp()).background(Color::LIGHT_GRAY).pointer_input(
            (),
            move |scope| async move {
                // `detectTapGestures(onDoubleTap = { … })`, in a coroutine per pointer input.
                let gestures =
                    TapGestures::default().on_double_tap(move |_| liked.set(!liked.get()));
                scope.detect_tap_gestures(gestures).await;
            }
        ),
        content_alignment = Alignment::CENTER,
        {
            if liked.get() {
                Icon!(icons::filled::favorite(), Some("Liked"));
            }
        }
    );
}
// ANCHOR_END: tap

// ANCHOR: drag
#[composable]
pub fn SwipeAway(on_dismiss: impl Fn() + 'static) {
    let offset = remember(|| mutable_state_of(0.0f32));
    let state = remember_draggable_state!(move |delta: f32| offset.update(|x| x + delta));
    Card!(
        modifier = Modifier
            .fill_max_width()
            // Read while drawing: dragging moves the card without recomposing it.
            .graphics_layer(move |layer| layer.translation_x = offset.get())
            .draggable(
                state,
                Orientation::Horizontal,
                true,
                false,
                |_start| {},
                move |_velocity| {
                    if offset.get().abs() > 150.0 {
                        on_dismiss();
                    }
                    offset.set(0.0);
                }
            ),
        {
            Text!("Swipe me away", modifier = Modifier.padding(16.dp()));
        }
    );
}
// ANCHOR_END: drag
