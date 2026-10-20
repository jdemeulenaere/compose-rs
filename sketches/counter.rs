// Sketch, not compiled: the counter app, as Compose's first sample, written against the
// designed API.

use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

#[composable]
fn Counter() {
    // remember keeps the OwnedState in this group's slot and returns its Copy handle.
    let count = remember(|| mutable_state_of(0));

    Column!(
        modifier = Modifier.padding(16.dp()),
        vertical_arrangement = Arrangement::spaced_by(8.dp()),
        {
            // `move` closures capture the Copy handle; the macro memoizes each lambda on its
            // captures (count), so Button receives the same Callback every pass and can skip.
            Button!(on_click = move || count.update(|c| c + 1), {
                Text!("Count: {}", count.get()); // a read: registers Button's restart scope
            });
            Button!(on_click = move || count.set(0), enabled = count.get() > 0, {
                Text!("Reset");
            });
        }
    );
}

fn main() {
    compose::desktop::run_app(
        WindowOptions { title: "Counter".into(), size: (320, 200).into(), ..Default::default() },
        || Counter(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose::test::*;

    #[test]
    fn counting_to_three() {
        let mut ui = compose_test(|| Counter());
        for _ in 0..3 {
            ui.on_node_with_text("Count").perform_click();
        }
        ui.on_node_with_text("Count: 3").assert_exists();
        ui.on_node_with_text("Reset").assert_is_enabled();
    }
}
