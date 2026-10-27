//! The failure pipeline's files under `$COMPOSE_TEST_ARTIFACTS`, with a Skia screenshot. Its
//! own binary, of one test: the variable is the process's, and setting it while other tests'
//! threads (or Skia's) may read the environment isn't sound.

#![allow(non_snake_case)]

use compose::foundation::layout::*;
use compose::test::*;
use compose::ui::{Color, Modifier, UnitExt};

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/FailurePipelineTest.kt, customHandler_isCalled_withArtifacts_onAssertionFailure
/// A failure writes the screenshot and the hierarchy into `$COMPOSE_TEST_ARTIFACTS`, creating
/// the directory, the files named after the test.
#[test]
fn a_failure_writes_its_artifacts_into_the_directory() {
    let scratch = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("compose-test-artifacts-{}", std::process::id()));
    let directory = scratch.join("nested");
    let _ = std::fs::remove_dir_all(&scratch);
    // SAFETY: this binary's only test sets the variable before any thread reads it.
    unsafe { std::env::set_var("COMPOSE_TEST_ARTIFACTS", &directory) };
    set_capture_renderer(compose_skia::raster_colors);
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_compose_ui_test(|ui| {
            ui.set_content(|| {
                Box!(modifier = Modifier.size(20.dp()).background(Color::from_argb(0xFF0000FF)))
            });
            ui.on_node_with_tag("non_existent").assert_exists();
        })
    }));
    assert!(failed.is_err(), "the failure goes on");
    let mut names: Vec<String> = std::fs::read_dir(&directory)
        .expect("the directory was created")
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names.len(), 2, "{names:?}");
    let prefix = "a_failure_writes_its_artifacts_into_the_directory_";
    assert!(names.iter().all(|name| name.starts_with(prefix)), "{names:?}");
    let screenshot = names.iter().find(|name| name.ends_with("_screenshot.ppm")).expect("a PPM");
    let ui = names.iter().find(|name| name.ends_with("_ui.txt")).expect("the hierarchy");
    let image = std::fs::read(directory.join(screenshot)).unwrap();
    assert!(image.starts_with(b"P6\n"), "a binary PPM");
    // The box's blue is in the picture.
    assert!(image.windows(3).any(|pixel| pixel == [0, 0, 255]));
    let text = std::fs::read_to_string(directory.join(ui)).unwrap();
    assert!(text.contains("Compose Hierarchy"), "{text}");
    let _ = std::fs::remove_dir_all(&scratch);
}
