//! `Crossfade` against Compose's `CrossfadeTest`: the content shown, the old content disposed
//! after the fade (default or given), a `None` state, and saved state kept per screen.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;

const FIRST: &str = "first";
const SECOND: &str = "second";

fn millis(millis: u32) -> Duration {
    Duration::from_millis(u64::from(millis))
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CrossfadeTest.kt, crossfadeTest_showsContent
#[test]
fn a_crossfade_shows_its_content() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        Crossfade!(true, |first| {
            BasicText!("{}", if first { FIRST } else { SECOND });
        });
    });
    ui.advance_time_by(millis(DEFAULT_DURATION_MILLIS));
    ui.on_node_with_text(FIRST).assert_exists();
}

/// A crossfade between `FIRST` and `SECOND` along `spec`, `disposed` set when a content
/// leaves the composition.
fn disposing(
    show_first: MutableState<bool>,
    spec: AnimationSpec,
    disposed: Rc<Cell<bool>>,
) -> ComposeTestRule {
    let handle = show_first;
    ComposeTestRule::with_manual_clock(move || {
        let disposed = disposed.clone();
        Crossfade!(handle.get(), animation_spec = spec, move |first| {
            BasicText!("{}", if first { FIRST } else { SECOND });
            let disposed = disposed.clone();
            DisposableEffect((), move |scope| scope.on_dispose(move || disposed.set(true)));
        });
    })
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CrossfadeTest.kt, crossfadeTest_disposesContentOnChange
#[test]
fn a_crossfade_disposes_the_old_content() {
    let show_first = mutable_state_of(true);
    let disposed = Rc::new(Cell::new(false));
    let spec = tween(DEFAULT_DURATION_MILLIS).into();
    let mut ui = disposing(show_first.handle(), spec, disposed.clone());
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(DEFAULT_DURATION_MILLIS));
    show_first.set(false);
    let mut frames = 0;
    while !disposed.get() {
        ui.advance_time_by_frame();
        frames += 1;
        assert!(frames < 100, "the old content never left");
    }
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_exists();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CrossfadeTest.kt, crossfadeTest_durationCanBeModifierUsingAnimationSpec
#[test]
fn a_crossfades_spec_sets_when_the_old_content_leaves() {
    let duration = 100;
    let show_first = mutable_state_of(true);
    let disposed = Rc::new(Cell::new(false));
    let mut ui = disposing(show_first.handle(), tween(duration).into(), disposed.clone());
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(duration));
    show_first.set(false);
    ui.advance_time_by(millis(duration));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(disposed.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CrossfadeTest.kt, nullInitialValue
#[test]
fn a_crossfade_from_none() {
    let current = mutable_state_of(None::<String>);
    let handle = current.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Crossfade!(handle.get(), |value: Option<String>| {
            BasicText!("{}", if value.is_none() { FIRST } else { SECOND });
        });
    });
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(DEFAULT_DURATION_MILLIS));
    ui.on_node_with_text(FIRST).assert_exists();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    current.set(Some("other".into()));
    ui.advance_time_by(millis(DEFAULT_DURATION_MILLIS));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_exists();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CrossfadeTest.kt, crossfadeTest_rememberSaveableIsNotRecreatedForScreens
#[test]
fn a_crossfade_keeps_each_screens_saved_state() {
    let duration = 100;
    let show_first = mutable_state_of(true);
    let handle = show_first.handle();
    let counter = Rc::new(Cell::new(1u32));
    let counters = Rc::new([Cell::new(0u32), Cell::new(0u32)]);
    let (next, seen) = (counter.clone(), counters.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let holder = remember_saveable_state_holder();
        let (next, seen) = (next.clone(), seen.clone());
        Crossfade!(handle.get(), animation_spec = tween(duration), move |first: bool| {
            let (next, seen) = (next.clone(), seen.clone());
            holder.SaveableStateProvider(first, move || {
                let value = remember_saveable(|| {
                    let value = next.get();
                    next.set(value + 1);
                    mutable_state_of(value)
                });
                seen[usize::from(!first)].set(value.get());
            });
        });
    });
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(duration));
    show_first.set(false);
    ui.advance_time_by(millis(duration));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // And back to the first screen.
    show_first.set(true);
    ui.advance_time_by(millis(duration));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(counters[0].get(), 1);
    assert_eq!(counters[1].get(), 2);
}
