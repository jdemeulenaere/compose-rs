//! `DeferredTargetAnimation` against Compose's: the first target at once, later ones animated
//! to from the next frame.

use compose_animation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::Offset;

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DeferredTargetAnimationTest.kt, testDeferredTargetAnimationValue
#[test]
fn a_deferred_target_animates_from_the_next_frame() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let scope = CoroutineScope::new();
    let animation = DeferredTargetAnimation::<Offset>::new();
    assert_eq!(animation.value(), None);
    // The first target is the value at once.
    assert_eq!(
        animation.update_target(Offset::new(10.0, 20.0), *scope, spring()),
        Offset::new(10.0, 20.0)
    );
    assert_eq!(animation.value(), Some(Offset::new(10.0, 20.0)));
    // No `wait_for_idle` between the targets: run at the current time, the test rule would
    // give the task launched a frame then (compose-test, not Compose, frames between ticks).
    ui.advance_time_by_frame();
    assert!(animation.is_idle());
    // A new target: nothing moves before a frame.
    animation.update_target(Offset::new(100.0, 200.0), *scope, spring());
    assert_eq!(animation.value(), Some(Offset::new(10.0, 20.0)));
    assert_eq!(animation.pending_target(), Some(Offset::new(100.0, 200.0)));
    // The first frame starts the animation, the second moves it.
    ui.advance_time_by_frame();
    assert_eq!(animation.value(), Some(Offset::new(10.0, 20.0)));
    ui.advance_time_by_frame();
    let moving = animation.value().unwrap();
    assert!(moving != Offset::new(10.0, 20.0) && moving != Offset::new(100.0, 200.0), "{moving:?}");
    assert!(!animation.is_idle());
    for _ in 0..100 {
        ui.advance_time_by_frame();
    }
    assert_eq!(animation.value(), Some(Offset::new(100.0, 200.0)));
    assert!(animation.is_idle());
}
