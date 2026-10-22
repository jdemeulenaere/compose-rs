//! Getting started: the first app (docs/guide/README.md).

// ANCHOR: hello
use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

#[composable]
pub fn Hello() {
    let name = remember_saveable(|| mutable_state_of(String::new()));
    MaterialTheme!({
        Surface!(modifier = Modifier.fill_max_size(), {
            Column!(
                modifier = Modifier.padding(16.dp()),
                vertical_arrangement = Arrangement::spaced_by(8.dp()),
                {
                    TextField!(
                        value = name.get(),
                        on_value_change = move |text: String| name.set(text),
                        modifier = Modifier.test_tag("name"),
                        label = Some(|| Text!("Your name")),
                        single_line = true,
                    );
                    let shown = name.get();
                    Text!("Hello, {}!", if shown.is_empty() { "world" } else { &shown });
                }
            );
        });
    });
}
// ANCHOR_END: hello
