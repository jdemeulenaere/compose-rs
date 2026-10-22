//! Animation (docs/guide/kotlin-to-rust.md, "Animation").

use compose::animation::*;
use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: as_state
#[composable]
pub fn Expandable(title: SharedString, body: SharedString) {
    let expanded = remember_saveable(|| mutable_state_of(false));
    // `animateFloatAsState(if (expanded) 180f else 0f)`: a `State<f32>` moving to its target.
    let rotation = animate_float_as_state!(if expanded.get() { 180.0 } else { 0.0 });
    Card!(modifier = Modifier.fill_max_width().clickable(move || expanded.set(!expanded.get())), {
        Row!(
            modifier = Modifier.padding(16.dp()),
            vertical_alignment = Alignment::CENTER_VERTICALLY,
            |row| {
                Text!(title, modifier = Modifier.weight(row, 1.0));
                Icon!(
                    icons::filled::keyboard_arrow_down(),
                    Some("Expand"),
                    // Read while drawing: the animation's frames don't recompose.
                    modifier =
                        Modifier.graphics_layer(move |layer| layer.rotation_z = rotation.get())
                );
            }
        );
        // Fades and expands in, shrinks and fades out, as Compose's defaults.
        AnimatedVisibility!(expanded.get(), {
            Text!(body, modifier = Modifier.padding(16.dp()));
        });
    });
}
// ANCHOR_END: as_state

// ANCHOR: crossfade
#[composable]
pub fn Loading(loading: bool) {
    Crossfade!(loading, |loading: bool| {
        if loading {
            IndeterminateCircularProgressIndicator!();
        } else {
            Text!("Loaded");
        }
    });
}
// ANCHOR_END: crossfade
