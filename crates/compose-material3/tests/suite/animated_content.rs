//! `AnimatedContent` around Material layouts, against Compose's `AnimatedContentTest`.

#![allow(non_snake_case)]

use compose_animation::*;
use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, Modifier, UnitExt};

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, AnimatedContentWithSubcomposition
#[test]
fn an_animated_content_switches_to_and_from_a_scaffold_with_tabs() {
    let target = mutable_state_of(true);
    let handle = target.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            AnimatedContent!(handle.get(), |scaffold: bool| {
                if scaffold {
                    Scaffold!(modifier = Modifier.fill_max_size(), |_| {
                        TabRow!(0, {
                            for index in 0..15 {
                                Text!("{}", index, modifier = Modifier.width(100.dp()));
                            }
                        });
                    });
                    Box!(modifier = Modifier.fill_max_size().background(Color::BLUE), {
                        Text!("test");
                    });
                } else {
                    Box!(modifier = Modifier.size(200.dp()));
                }
            });
        });
    });
    ui.on_node_with_text("test").assert_exists();
    target.set(false);
    ui.wait_for_idle();
    target.set(true);
    ui.wait_for_idle();
    ui.on_node_with_text("test").assert_exists();
}
