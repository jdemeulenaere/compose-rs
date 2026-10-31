//! Dialogs and selectable lists (docs/guide/kotlin-to-rust.md, "Dialogs", "Selectable lists").

use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;

// ANCHOR: dialog
#[composable]
pub fn DeleteNote(on_delete: impl Fn() + 'static) {
    let confirming = remember(|| mutable_state_of(false));
    TextButton!(on_click = move || confirming.set(true), {
        Text!("Delete…");
    });
    // In the composition while it shows, as in Compose.
    if confirming.get() {
        AlertDialog!(
            on_dismiss_request = move || confirming.set(false),
            confirm_button = {
                TextButton!(
                    on_click = move || {
                        confirming.set(false);
                        on_delete();
                    },
                    {
                        Text!("Delete");
                    }
                );
            },
            dismiss_button = {
                TextButton!(on_click = move || confirming.set(false), {
                    Text!("Cancel");
                });
            },
            title = {
                Text!("Delete the note?");
            },
            text = {
                Text!("It can't be restored.");
            },
        );
    }
}
// ANCHOR_END: dialog

// ANCHOR: selectable
// ANCHOR: date_picker
#[composable]
pub fn PickDate(month: i64, on_dismiss: impl Fn() + 'static, on_confirm: impl Fn() + 'static) {
    let state = remember_date_picker_state!(initial_displayed_month_millis = Some(month));
    DatePickerDialog!(
        on_dismiss,
        confirm_button = {
            TextButton!(on_click = on_confirm, {
                Text!("OK");
            });
        },
        |_| {
            DatePicker!(state);
        }
    );
}
// ANCHOR_END: date_picker

const SIZES: [&str; 3] = ["Small", "Medium", "Large"];

#[composable]
pub fn SizePicker(selected: usize, on_select: impl Fn(usize) + 'static) {
    // One group for accessibility: "1 of 3"-style navigation between the options.
    Column!(modifier = Modifier.selectable_group(), {
        for (index, size) in SIZES.into_iter().enumerate() {
            // The whole item selects, as a radio button.
            SelectableListItem!(
                index == selected,
                move || on_select(index),
                // `None`: the item handles the click, not the button.
                leading_content = Some(move || RadioButton!(index == selected, None::<fn()>)),
                {
                    Text!(size);
                }
            );
        }
    });
}
// ANCHOR_END: selectable
