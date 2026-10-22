//! The counter app, as Compose's first sample, written as in sketches/counter.rs.

#![allow(non_snake_case)]

use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

#[composable]
pub fn Counter() {
    // remember_saveable keeps the OwnedState in this group's slot and returns its Copy handle;
    // its value is saved too, so it survives an Android activity's recreation (a rotation).
    let count = remember_saveable(|| mutable_state_of(0));

    Column!(
        modifier = Modifier.padding(16.dp()),
        vertical_arrangement = Arrangement::spaced_by(8.dp()),
        {
            // `move` closures capture the Copy handle; the macro memoizes each lambda on its
            // captures (count), so Button receives the same Callback every pass and can skip.
            Button!(on_click = move || count.update(|c| c + 1), {
                Text!("Count: {}", count.get()); // a read: registers this content's scope
            });
            Button!(on_click = move || count.set(0), enabled = count.get() > 0, {
                Text!("Reset");
            });
        }
    );
}
