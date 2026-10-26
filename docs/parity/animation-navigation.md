# Parity ledger: animation and navigation

The inventory of Compose's animation and navigation tests and public API against compose-rs's `compose-animation` and `compose-navigation`, for the agents porting them. Read-only survey; nothing here is implemented yet.

## Scope

- **Reference:** the androidx checkout at `androidx-main/frameworks/support`, commit `d153d87ed76` (its tip). Every path below is relative to `frameworks/support`.
- **Compose modules:**
  - `compose/animation/animation-core` (Animatable, `animate*AsState`, Transition, specs, easing, converters, decay, infinite transitions);
  - `compose/animation/animation` (AnimatedVisibility, AnimatedContent, Crossfade, `animateContentSize`, enter/exit transitions, shared transitions, `animateBounds`, spline decay);
  - `compose/animation/animation-graphics` (animated vector drawables).
- **Navigation:** both libraries exist in the checkout:
  - Navigation Compose: `navigation/navigation-compose`, on top of `navigation-runtime`'s `NavController`;
  - Navigation 3: `navigation3/navigation3-runtime` and `navigation3/navigation3-ui`.

  **compose-rs models Navigation Compose**: `NavController`, `NavHost`, `NavBackStackEntry`, `NavOptions`, `remember_nav_controller`, `pop_up_to`, `launch_single_top` (D291). Its destinations are values of the app's own type, though, an enum `match`ed in `NavHost`, with no routes or graph. That is closer to Navigation 3's data model, where the back stack is a list of keys and `entryProvider` maps them to content. So Navigation 3 is listed in full, as features compose-rs would have to choose to adopt (see [navigation3](#navigation3)).
- **Test source sets:** all of them: `commonTest`, `androidHostTest`, `androidDeviceTest`. Helper files with no `@Test` are left out of the table (9 of them: `AnimationTestUtils.kt`, the 4 `IgnoreAndroidHostTestTarget*.kt`, the 2 `TestUtils.kt`, `TestNestedTransitionScenes.kt`, `TestDualPaneScenes.kt`). Counts are `grep -cE '^\s*@Test\b'` per file. `SharedTransitionTest.kt` has 71 `@Test` lines for 69 named methods.
- **API:** each module's `api/current.txt`. `restricted_current.txt` was used only to tell what isn't public. Experimental means an `@Experimental*` opt-in annotation on the declaration. At this tip, almost nothing in animation is experimental:
  - `ArcAnimationSpec` (`@ExperimentalAnimationSpecApi`);
  - `LookaheadAnimationVisualDebugging` and `CustomizedLookaheadAnimationVisualDebugging` (`@ExperimentalLookaheadAnimationVisualDebugApi`).

  `ExperimentalAnimationApi`, `ExperimentalSharedTransitionApi`, `ExperimentalTransitionApi` and `ExperimentalAnimationGraphicsApi` still exist but annotate nothing public. Shared transitions, `SeekableTransitionState`, `animateBounds` and the new `Deferred*` APIs are all stable.
- **compose-rs side:**
  - `crates/compose-animation`: 12 source files; 54 tests in `tests/suite` (`animation.rs`, `content.rs`, `crossfade.rs`, `infinite.rs`, `visibility.rs` and eight `review_*.rs`);
  - `crates/compose-navigation`: 18 tests in `tests/suite/navigation.rs` and `review.rs`;
  - the harness: `crates/compose-test`.

## Totals

**95 test files, 1,021 `@Test` methods.**

| Status | Files | @Test |
|---|---|---|
| `covered` | 1 | 1 |
| `ported` | 1 | 1 |
| `port` | 7 | 16 |
| `partial` | 14 | 192 |
| `feature:<name>` | 69 | 799 |
| `n/a` | 3 | 12 |

By module:

| Module | Files | @Test | covered | ported | port | partial | feature | n/a |
|---|---|---|---|---|---|---|---|---|
| animation-core | 30 | 212 | 0 | 1 / 1 | 7 / 16 | 6 / 73 | 15 / 119 | 1 / 3 |
| animation | 15 | 238 | 0 | 0 | 0 | 5 / 51 | 10 / 187 | 0 |
| animation-graphics | 5 | 22 | 0 | 0 | 0 | 0 | 5 / 22 | 0 |
| navigation-compose | 9 | 121 | 1 / 1 | 0 | 0 | 3 / 68 | 4 / 50 | 1 / 2 |
| navigation3-runtime | 21 | 307 | 0 | 0 | 0 | 0 | 20 / 300 | 1 / 7 |
| navigation3-ui | 15 | 121 | 0 | 0 | 0 | 0 | 15 / 121 | 0 |

`@Test` methods by missing feature (a file counts under its main blocker; the notes name the others):

| Feature | Files | @Test |
|---|---|---|
| nav3-deep-links | 8 | 211 |
| nav3-ui | 15 | 121 |
| shared-transition | 2 | 86 |
| nav3-runtime | 8 | 79 |
| deferred-transition | 4 | 62 |
| seekable-transition | 2 | 49 |
| nav-graphs | 1 | 33 |
| animation-objects | 2 | 14 |
| animated-image-vector | 5 | 22 |
| lookahead-visual-debug | 1 | 16 |
| arc-animation | 2 | 14 |
| enter-exit-params | 1 | 13 |
| path-easing | 1 | 12 |
| keyframes-with-spline | 2 | 10 |
| nav-dialog-destinations | 1 | 10 |
| nav3-results | 4 | 10 |
| captured-animated-visibility | 1 | 9 |
| animate-bounds | 1 | 8 |
| suspend-animate | 1 | 5 |
| nav-transitions | 1 | 5 |
| motion-duration-scale | 2 | 4 |
| decay-animation | 2 | 3 |
| predictive-back | 1 | 2 |
| deferred-target-animation | 1 | 1 |

Without Navigation 3 (whose adoption is a decision, not a gap in what compose-rs models), the area is 59 files and 593 tests. Of those:

- 1 file is `covered`, 7 are `port` and 13 are `partial` (198 tests), so most of the animation and Navigation Compose basics are portable now;
- 36 files (390 tests) are blocked on features;
- 2 files (5 tests) are `n/a`.

The partial files' notes count about 60 tests portable today.

## Test files

Status meanings:

- `covered`: compose-rs tests already exercise this.
- `port`: the feature exists, so adapt the tests.
- `partial`: some of it exists; the notes say what is portable now and what is missing.
- `feature:<name>`: blocked on a missing feature (see [Missing features](#missing-features)).
- `n/a`: an adapted test wouldn't make compose-rs more robust.

The "covered" notes cite compose-rs test files under `crates/compose-animation/tests/suite/` and `crates/compose-navigation/tests/suite/`.

| # | Compose test file (under `frameworks/support`) | @Test | Covers | Status | Notes |
|---|---|---|---|---|---|
| 1 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt` | 9 | `Animatable`: animateTo (float, generic), interruption, animateDecay, updateBounds, IntSize bounds, toString of results | partial | animateTo/generic/interruption portable now (interruption covered by `animation.rs` `an_animatable_interrupted_by_another_animation_says_so`); missing `animateDecay` (decay-animation), `updateBounds`/`BoundReached` (animatable-bounds), IntSize converter (more-converters), `InterruptionAwareAnimationSpec`; the 2 `toString` tests are n/a (Rust `Debug`) |
| 2 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationTest.kt` | 6 | `TargetBasedAnimation` values/velocities for snap, keyframes, tween, spring; infinite repeatable velocity across repetitions | `feature:animation-objects` | the spec math exists (`AnimationSpec::frame`, crate-private); needs the public `TargetBasedAnimation`/vectorized evaluation API |
| 3 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationVectorTest.kt` | 3 | `AnimationVector1D..4D`: reset, hashCode, factory functions | n/a: compose-rs's vectors are `[f32; 4]` (`Animate::to_vector`); reset/hashCode/factories have no analogue |  |
| 4 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt` | 12 | `ArcAnimationSpec` (ArcAbove/Below/Linear) interpolation in 1-4 dimensions, equality, curve shape | `feature:arc-animation` | experimental (`@ExperimentalAnimationSpecApi`) |
| 5 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DecayAnimationTest.kt` | 2 | `exponentialDecay` values and the velocity threshold | `feature:decay-animation` |  |
| 6 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DeferredTargetAnimationTest.kt` | 1 | `DeferredTargetAnimation.updateTarget` value/idle/pendingTarget | `feature:deferred-target-animation` |  |
| 7 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DelayedAnimationTest.kt` | 5 | Compose's internal `delayed(spec, nanos)` wrapper: duration, values, velocity, zero, equality | `feature:seekable-transition` | tests an internal helper of Compose's seekable transitions; adapt only if the port has the same wrapper |
| 8 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DurationScaleTest.kt` | 1 | `animate()` honours `MotionDurationScale` (5x, and 0 = finish at once, infinite too) | `feature:motion-duration-scale` | also needs suspend-animate's `animate()` |
| 9 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/EasingUnitTest.kt` | 6 | `CubicBezierEasing`: 0 and 1 ends, overshoot, equality, small fractions | port | partly covered by `animation.rs` `easings_follow_their_curves` (values only) |
| 10 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/IsInfiniteTest.kt` | 8 | `isInfinite` of every vectorized spec, exponential decay and `DecayAnimation` | `feature:animation-objects` | 2 tests also need decay-animation |
| 11 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt` | 12 | keyframes: start/end override, easing per stretch, multi-dimensional, equality, `atFraction`, out-of-range times | partial | 8 portable now through `Animatable` sampling (covered in part by `animation.rs` keyframe tests and `review_round14.rs`); `atFraction` (2) needs spec-gaps; negative times can't be written (`at(u32)`); exact `getValueFromMillis` checks are easier after animation-objects |
| 12 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeArcAnimationTest.kt` | 2 | keyframes `using ArcMode` in 2D, with easings | `feature:arc-animation` |  |
| 13 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt` | 8 | `keyframesWithSpline`: interpolation, periodic bias, easing, delay, equality | `feature:keyframes-with-spline` |  |
| 14 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/MonoSplineTest.kt` | 2 | the monotonic spline under `keyframesWithSpline` | `feature:keyframes-with-spline` | internal class; port with the feature |
| 15 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt` | 11 | springs: velocity, under/over/critically damped durations, end snapping, vector springs, zero damping, interruption-aware spec | partial | 10 ported (`physics.rs`: velocities, the three damping regimes' durations, end snapping, vector durations and values, zero damping); `testInterruptionAwareAnimationSpec` needs interruption-aware-spec |
| 16 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt` | 11 | repeatable/infiniteRepeatable values, durations, Reverse, StartOffset, zero-duration inner specs | partial | values/Reverse/infinite portable (Reverse covered by `review_animation.rs` reversed-repetition tests; finite end by `review_round14.rs`); StartOffset (2) and `snap()` inside a repeatable need spec-gaps; spline/arc zero-duration need their features; durations need animation-objects |
| 17 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SnapAnimationTest.kt` | 1 | `snap()` is finished at play time 0 | port | `animation.rs` `snap_jumps_after_its_delay` covers the delayed case |
| 18 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SuspendAnimationTest.kt` | 5 | top-level `animate()` (float, generic), `animateDecay`, `AnimationState.animateTo/animateDecay` | `feature:suspend-animate` | decay ones also need decay-animation |
| 19 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/TweenAnimationTest.kt` | 3 | tween delay, custom easing, value past the duration | port | custom easing must be a `fn` (`Easing::Function`); covered in part by `animation.rs` `a_tween_moves_with_the_clock_and_ends_at_its_target` |
| 20 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/TypeConverterTest.kt` | 2 | Float and Int vector converters | port | add a fractional case: Compose's Int converter truncates, compose-rs rounds (see notes) |
| 21 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt` | 8 | `DeferredTransitionState`: defer/animateTo, pending state, child transitions, interruption | `feature:deferred-transition` | new at the tip, not experimental |
| 22 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DurationScaleTransitionTest.kt` | 3 | child transitions (plain, seekable, seeking) under `MotionDurationScale` | `feature:motion-duration-scale` | also needs transition-state (child transitions) and seekable-transition |
| 23 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/EasingTest.kt` | 1 | cubic Bezier evaluation matches a drawn path (bitmap comparison) | port | compare `Easing::CubicBezier` against a densely sampled parametric curve instead of bitmaps |
| 24 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/InfiniteTransitionTest.kt` | 1 | `rememberInfiniteTransition`: keyframes Reverse float and tween color, frame by frame | port | `infinite.rs` checks against compose-rs's own earlier implementation, never against Compose's expected values; expected values need keyframe/tween evaluation (easier after animation-objects) |
| 25 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt` | 12 | `PathEasing`: bounds, invalid paths, compare to cubic, quads, overshoot | `feature:path-easing` |  |
| 26 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt` | 44 | `SeekableTransitionState`: seekTo, animateTo, snapTo, interruptions, durations, isRunning, cleanup | `feature:seekable-transition` |  |
| 27 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt` | 13 | `animate*AsState`: 1D/2D/4D/color frame by frame, interruption, visibility thresholds, spec and threshold updates | partial | 12 ported (`single_value.rs`: 1D/2D/4D/color frame by frame, a target moved every frame, the visibility threshold tests, spec and threshold updates); `interruptionAwareAnimationSpec_usesInterruptionSpecOnInterruption` needs interruption-aware-spec (its interruption frames are ported with one spec: `an_interrupting_animation_starts_where_the_other_left_off`) |
| 28 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt` | 17 | `updateTransition`/`Transition`: values per segment, animations added mid-flight, MutableTransitionState, child transitions, totalDurationNanos, recomposition counts, interruption-aware spec | partial | ~7 portable now (`content.rs` and `review_round8.rs`/`review_round10.rs` cover isRunning, interruption spring, current state); MutableTransitionState/child transitions/totalDurationNanos need transition-state; 2 need interruption-aware-spec; label test n/a (tooling) |
| 29 | `compose/animation/animation-core/src/androidHostTest/kotlin/androidx/compose/animation/core/EasingTest.android.kt` | 2 | cubic solver near 0/1 and with overshooting curves | port |  |
| 30 | `compose/animation/animation-core/src/androidHostTest/kotlin/androidx/compose/animation/core/SpringEstimationTest.kt` | 1 | spring duration estimation against simulation over a grid of mass/stiffness | ported | `spring_estimation.rs`: every fifth mass and spring constant of Compose's grid (2.9 million of its 70 million cases) |
| 31 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimateBoundsTest.kt` | 8 | `Modifier.animateBounds`: position, size, bounds, movable content, lazy layouts, scrolling | `feature:animate-bounds` | needs ui lookahead |
| 32 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt` | 17 | `AnimatedContent`: SizeTransform, alignment, slideInto/OutOfContainer, contentKey, subcomposition, interruption, KeepUntilTransitionsFinished, lookahead, cleanup | partial | ~6 portable now (alignment, interruption, right transition on interruption, subcomposition, cleanup, size continuity); compose-rs covers basics in `content.rs`, `review_round8.rs`, `review_round10.rs`, `review_round11.rs`; the rest need animated-content-scope (SizeTransform spec, `using`, slideIntoContainer, contentKey, KeepUntilTransitionsFinished, zIndex), transition-state (`rememberTransition`), ui lookahead (2) |
| 33 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt` | 17 | `AnimatedVisibility`: expand/shrink, slide, fade, scale with exact values, None, Transition overload, seeking, lookahead, veil, settle time | partial | 3 portable now (fade, None/None, interrupted exit; `visibility.rs`, `review_animation.rs`, `review_round11.rs`/`review_round12.rs` cover related cases); expand/slide/scale with custom specs and lambdas need enter-exit-params; veil (4); transition-state; seekable; lookahead; settle time needs spring-estimation |
| 34 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt` | 6 | `animateContentSize`: frame-by-frame size, alignment LTR/RTL, lookahead, inspector, re-attach | partial | 2 portable (size test, re-attach; `crossfade.rs` `a_content_size_animates_to_its_new_size` and `review_animation.rs` default spring cover parts); alignment (2) needs animate-content-size-extras; lookahead (1); inspector n/a |
| 35 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CapturedAnimatedVisibilityTest.kt` | 9 | `CapturedAnimatedVisibility`: content disposed at once, exit animates a captured layer, MutableTransitionState, interruptions, lookahead | `feature:captured-animated-visibility` | needs ui graphics-layer recording |
| 36 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CrossfadeTest.kt` | 6 | `Crossfade`: shows content, disposes old content, custom spec, null state, rememberSaveable per screen, contentKey | partial | 5 portable (`crossfade.rs` and `review_animation.rs` `crossfading_back_reuses_the_contents_of_the_state` cover parts); contentKey needs crossfade-extras |
| 37 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt` | 29 | `DeferredAnimatedContent`: gates, preview transforms, hand-off velocity, interruption specs, veil | `feature:deferred-transition` |  |
| 38 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt` | 21 | `DeferredAnimatedVisibility`: enter/exit gates, preview scale/offset, hand-off, interruption | `feature:deferred-transition` |  |
| 39 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredSharedElementTest.kt` | 15 | shared elements under deferred transitions: previews, hand-off, cancellation | `feature:shared-transition` | also deferred-transition |
| 40 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/LookaheadAnimationVisualDebugHelperTest.kt` | 16 | debug overlays for shared elements and animateBounds | `feature:lookahead-visual-debug` | experimental (`@ExperimentalLookaheadAnimationVisualDebugApi`) |
| 41 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/SharedTransitionTest.kt` | 71 | `SharedTransitionLayout`, `sharedElement`, `sharedBounds`, overlay, clip, placeholder size, scale-to-bounds, lazy lists, isTransitionActive, velocity | `feature:shared-transition` | needs ui lookahead |
| 42 | `compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/AndroidFlingSplineTest.kt` | 1 | the fling spline clamps positions out of range | `feature:decay-animation` | compose-rs has the spline privately in `compose-foundation/src/fling.rs` |
| 43 | `compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/ConverterTest.kt` | 5 | Color (Oklab, clamping), Rect, Dp, Offset converters | partial | Color covered by `animation.rs` `colors_go_through_oklab_and_back`; clamp test portable and expected to fail (see notes); Rect needs more-converters |
| 44 | `compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt` | 13 | `EnterTransition/ExitTransition.config`: fade, slide, change size, scale, veil, combined; SizeTransform defaults | `feature:enter-exit-params` | veil (2) also needs veil; SizeTransform default needs animated-content-scope |
| 45 | `compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/SharedMutableTransformStateTest.kt` | 4 | internal state machine of mutable transforms | `feature:deferred-transition` | internal; port with the feature |
| 46 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/res/AnimatedVectorPainterResourcesTest.kt` | 2 | `rememberAnimatedVectorPainter` from an AVD resource: config, duplicate targets | `feature:animated-image-vector` | resources become bundled bytes in compose-rs |
| 47 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/vector/AnimatorAnimationSpecsTest.kt` | 5 | reversed and combined specs used by vector animators | `feature:animated-image-vector` | internal specs |
| 48 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/vector/AnimatorTest.kt` | 10 | object animators, keyframes, sets, path data morphing, delays, repeats | `feature:animated-image-vector` |  |
| 49 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/vector/compat/XmlAnimatedVectorParserTest.kt` | 1 | parsing an animated-vector XML | `feature:animated-image-vector` | needs an XML parser; compose-rs parses no vector XML today |
| 50 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/vector/compat/XmlAnimatorParserTest.kt` | 4 | parsing objectAnimator/set/propertyValuesHolder XML | `feature:animated-image-vector` |  |
| 51 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/ComposeNavigatorTest.kt` | 1 | navigating and popping updates the navigator's back stack | covered | `navigation.rs` `navigate_shows_the_destination_and_pop_goes_back` checks the same stack changes (there's no Navigator layer) |
| 52 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/DialogNavigatorTest.kt` | 10 | dialog destinations: shown, dismissed on pop, nested NavHost, transition completion order | `feature:nav-dialog-destinations` |  |
| 53 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavBackStackEntryProviderTest.kt` | 5 | an entry provides ViewModelStore/Lifecycle/SavedStateRegistry owners; saveable values saved, plain ones not | partial | the 2 saveable tests are covered (`navigation.rs` `an_entry_keeps_its_state_while_on_the_stack`, `the_back_stack_and_its_states_survive_the_process`); 3 owner tests need nav-entry-owners |
| 54 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavGraphBuilderTest.kt` | 33 | graph builder: composable/dialog/navigation with routes, KClass and objects, nested graphs, default args, deep links | `feature:nav-graphs` | 1 portable now (current entry after navigate); ~14 are n/a (string routes, `typeMap`/`NavType` arguments, deep links: compose-rs destinations are typed values); nested-graph ones need nav-graphs, dialog ones nav-dialog-destinations |
| 55 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostControllerTest.kt` | 27 | `rememberNavController`, current entry, navigate/pop, singleTop, KClass routes with args, dialogs, getBackStackEntry | partial | ~6 covered (`navigation.rs`: navigate/pop, pop_up_to, single top, current entry); singleTop with other arguments is a documented difference (D291); getBackStackEntry (3) needs nav-controller-extras; dialog KClass (6) nav-dialog-destinations; Bundle/SavedStateHandle argument tests (~10) n/a |
| 56 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostPredictiveBackTest.kt` | 2 | NavHost animations driven by a predictive back gesture; disabling in the same frame | `feature:predictive-back` |  |
| 57 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostPreviewTest.kt` | 2 | NavHost renders in an Android Studio preview (no lifecycle owner) | n/a: Android Studio preview (`LocalInspectionMode`), no analogue |  |
| 58 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostScreenShotTest.kt` | 5 | z-order of entering/leaving entries, predictive back frames, custom transitions, size transform (screenshots) | `feature:nav-transitions` | z-order can be asserted on the display list instead of goldens; predictive ones need predictive-back |
| 59 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostTest.kt` | 36 | NavHost: navigate, pop, state saving/restoring, dialogs, ViewModels, graphs, animations, lifecycles, nested hosts, back handlers | partial | ~9 covered (`navigation.rs`, `review.rs`: navigate, pop, saveable state kept/cleared, process death, back handler order, fade); ~5 portable now (navigate outside a state change, same controller after disposing the host, nested host back, nested host restore, back interrupting the animation); the rest need nav-entry-owners (lifecycle, ViewModel: ~10), nav-graphs (7), nav-dialog-destinations (2), nav-transitions; the deep link test is n/a |
| 60 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt` | 42 | `rememberDecoratedNavEntries`: decorators called once/in order, onPop, re-decoration, state per contentKey, multiple back stacks | `feature:nav3-runtime` |  |
| 61 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavBackStackSerializerTest.kt` | 5 | `NavBackStack` kotlinx.serialization round trips | `feature:nav3-runtime` | adapt to `Saveable`, not serialization |
| 62 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavEntryTest.kt` | 2 | `NavEntry.contentKey` for objects and data classes | `feature:nav3-runtime` |  |
| 63 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/RememberNavBackStackTest.kt` | 10 | `rememberNavBackStack` restores objects, classes, sealed and polymorphic keys | `feature:nav3-runtime` | the serializer-configuration cases are n/a; restoration maps to `Saveable` |
| 64 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/SaveableStateHolderNavEntryDecoratorTest.kt` | 1 | equal data-class keys don't duplicate saveable state | `feature:nav3-runtime` |  |
| 65 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusAndroidTest.kt` | 2 | `ResultEventBus.conflateAsState` | `feature:nav3-results` |  |
| 66 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusNavEntryDecoratorTest.kt` | 5 | results between entries and dialogs through the decorator | `feature:nav3-results` |  |
| 67 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt` | 40 | decoding URI arguments into keys (all primitive types, enums, nesting, errors) | `feature:nav3-deep-links` | optional: kotlinx.serialization decoding; a Rust port would use its own decoding trait |
| 68 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt` | 11 | matcher filters and ordering of match results | `feature:nav3-deep-links` |  |
| 69 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.android.kt` | 7 | `DeepLinkRequest` from an Android `Intent` (uri, mime type, action, extras) | n/a: Android `Intent`s |  |
| 70 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkSerializerTest.kt` | 4 | `DeepLinkSerializer` custom string encodings | `feature:nav3-deep-links` |  |
| 71 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt` | 69 | URI pattern matching: paths, queries, fragments, wildcards, exactness, ranking | `feature:nav3-deep-links` |  |
| 72 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt` | 70 | URI pattern parsing into path/query/fragment arguments | `feature:nav3-deep-links` |  |
| 73 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WithBackStackTest.kt` | 5 | `withBackStack` builds a synthetic back stack for a deep link | `feature:nav3-deep-links` |  |
| 74 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WrappedMatchResultTest.kt` | 4 | wrapped match results compare like the wrapped one | `feature:nav3-deep-links` |  |
| 75 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt` | 9 | `entryProvider` DSL: entries by key/class, metadata, duplicates, fallback | `feature:nav3-runtime` |  |
| 76 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryTest.kt` | 2 | `NavEntry` key and metadata | `feature:nav3-runtime` |  |
| 77 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt` | 8 | typed `NavMetadataKey`s: object/class keys, lambdas, override, plus, contains | `feature:nav3-runtime` |  |
| 78 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt` | 8 | `DeepLinkRequest` URIs and `RequestExtras` | `feature:nav3-deep-links` |  |
| 79 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/result/ResultEffectTest.kt` | 1 | `ResultEffect` sees the latest callback | `feature:nav3-results` |  |
| 80 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusTest.kt` | 2 | `ResultEventBus` results flow and removal | `feature:nav3-results` |  |
| 81 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/NavDisplayInfoTest.kt` | 1 | a predictive back swipe fills `SceneInfo` for the navigation event | `feature:nav3-ui` | also predictive-back |
| 82 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/NavigationBackHandlerTest.kt` | 3 | `NavigationBackHandler`: single pop, multi pop, cancelled back | `feature:nav3-ui` | also predictive-back |
| 83 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/OverlaySceneTest.kt` | 1 | shared transition with a dialog scene | `feature:nav3-ui` | also shared-transition |
| 84 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/RememberNavigationEventStateTest.kt` | 1 | `rememberNavigationEventState` follows the scene state | `feature:nav3-ui` | also predictive-back (navigationevent) |
| 85 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/SceneStateTest.kt` | 8 | `rememberSceneState`: scene changes, previous scenes, strategy chaining, recalculation | `feature:nav3-ui` |  |
| 86 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/AnimateOverlaySceneTest.kt` | 2 | overlay scene exit animation, nested bottom sheet | `feature:nav3-ui` |  |
| 87 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/BottomNavSceneDecoratorTest.kt` | 7 | a bottom-nav scene decorator with dialogs and list-detail | `feature:nav3-ui` |  |
| 88 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/HierarchicalSceneTest.kt` | 7 | multi-column hierarchical scene: content, back, state, predictive back | `feature:nav3-ui` |  |
| 89 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/ListDetailSceneTest.kt` | 5 | list-detail scene: second detail, replace, full screen, back | `feature:nav3-ui` |  |
| 90 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt` | 26 | NavDisplay transitions: navigate/pop/interrupted, custom specs, centre pops, duplicates, shared elements, lifecycle | `feature:nav3-ui` | shared-element test also needs shared-transition |
| 91 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt` | 8 | entry lifecycle order across navigation, panes, dialogs, overlays | `feature:nav3-ui` | also nav-entry-owners (lifecycles) |
| 92 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayPredictiveBackTest.kt` | 5 | state restored after predictive back, z-index, back during forward animation, hand-off | `feature:nav3-ui` | also predictive-back |
| 93 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt` | 22 | z-order and frames of predictive back, pops, duplicates, scenes (screenshots) | `feature:nav3-ui` | z-order via the display list rather than goldens |
| 94 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt` | 21 | NavDisplay: content, dialogs, back, saved state per entry, back stack swaps, duplicates, multi-pane | `feature:nav3-ui` |  |
| 95 | `navigation3/navigation3-ui/src/commonTest/kotlin/androidx/navigation3/scene/SceneInfoTest.kt` | 4 | `SceneInfo` title/url metadata | `feature:nav3-ui` |  |

## Public API surface

What compose-rs lacks or only partly has, from each `api/current.txt`, non-deprecated, experimental included. Each item names its feature. Rust-shaped equivalents count as present:

- a `TwoWayConverter` is the `Animate` trait;
- an `AnimationVector` is `[f32; 4]`;
- a cancellation is `AnimationEndReason::Interrupted`;
- default arguments are `#[default]` parameters or `_with` forms.

Deprecated and left out:

- `defaultDecayAnimationSpec()`;
- `KeyframeEntity.with(easing)`, replaced by `using`;
- `updateTransition(MutableTransitionState)`;
- the hidden overloads of `rememberSceneState`, `SceneStrategy.then` and two `NavDisplay`s.

### What compose-rs already has (for reference)

**animation-core:**

- `Animatable`: `new`, `value`, `as_state`, `target_value`, `velocity`, `is_running`, `animate_to`, `animate_to_with_velocity`, `snap_to`, `stop`.
- `AnimationResult`, with `end_value` in place of `endState`, and `AnimationEndReason::Finished`.
- `animate_{value,float,dp,color,int,offset,int_offset,size}_as_state`, with `label` and `finished_listener`.
- The specs and constants:
  - `tween` (`.delay`, `.easing`), `spring`/`spring_with`, `snap(delay)`, `repeatable`, `infinite_repeatable`, `RepeatMode`;
  - `keyframes(duration, |f| f.at(t, v).with(easing))` with `delay_millis`;
  - `Spring::*` constants, `DEFAULT_DURATION_MILLIS`.
- `Easing::{Linear, CubicBezier, Function}`, with the four standard curves and `EASE`, `EASE_IN`, `EASE_OUT`, `EASE_IN_OUT`.
- `remember_infinite_transition` with `animate_value/float/color`.
- `update_transition` with `current_state`, `target_state`, `is_running`, `animate_value/float/dp/color/offset`. The generic `animate_value` also covers `i32`, `IntOffset`, `Size`.
- `Animate` for `f32`, `i32`, `Dp`, `Offset`, `IntOffset`, `Size`, `Color` (Oklab).

**animation:**

- `AnimatedVisibility(visible, modifier, enter, exit, label, content)`.
- `EnterTransition`/`ExitTransition` with `+` and `NONE`.
- The transitions:
  - `fade_in`/`fade_in_with(spec, alpha)` and the `fade_out` pair;
  - `scale_in(scale)`/`scale_in_with(spec, scale)` and the `scale_out` pair;
  - `expand_{in,horizontally,vertically}()` and `shrink_{out,horizontally,vertically}()`, defaults only;
  - `slide_in((fx, fy))`/`slide_out` (fractions of the size), `slide_in_vertically()`/`slide_out_vertically()`.
- `AnimatedContent(target_state, modifier, transition_spec(initial, target), content_alignment, label, content)`, with `ContentTransform` (`together_with`, `without_size_transform`) and `default_content_transform()`.
- `Crossfade(target_state, modifier, animation_spec, label, content)`.
- `Modifier.animate_content_size()`/`animate_content_size_with(spec)`.

**navigation:** see [Navigation Compose](#navigation-compose-androidxnavigationcompose-and-navcontroller).

### animation-core (`androidx.compose.animation.core`)

| API | compose-rs | Feature |
|---|---|---|
| `Animatable(initialValue, typeConverter, visibilityThreshold, label)`; `Animatable(Float, visibilityThreshold)` | no threshold or label parameter: springs without a threshold use `Animate::visibility_threshold()` (see notes) | animatable-bounds |
| `Animatable.updateBounds`, `lowerBound`, `upperBound`, `AnimationEndReason.BoundReached` | missing | animatable-bounds |
| `Animatable.label`, `typeConverter`, `velocityVector` | missing (trivial) | animatable-bounds |
| `Animatable.animateTo(…, block)`: the per-frame callback on `Animatable` | missing | suspend-animate |
| `Animatable.animateDecay(initialVelocity, DecayAnimationSpec, block)` | missing | decay-animation |
| `AnimationResult.endState: AnimationState` | `end_value` only | suspend-animate |
| `animateIntSizeAsState`, `animateRectAsState`; `animateValueAsState(…, visibilityThreshold, …)` | missing; `animate_value_as_state` has no threshold parameter | more-converters |
| `Animation` interface (`durationNanos`, `getValueFromNanos`, `getVelocityVectorFromNanos`, `isFinishedFromNanos`, `isInfinite`, `targetValue`, `typeConverter`); `TargetBasedAnimation`; `Animation.getVelocityFromNanos` | missing (the math is crate-private `AnimationSpec::frame`/`value_at`) | animation-objects |
| `AnimationSpec.vectorize`, `FiniteAnimationSpec`, `DurationBasedAnimationSpec` (an enum of tween and keyframes in compose-rs, without snap) | partial: `AnimationSpec` enum; no vectorized form | animation-objects, spec-gaps |
| `VectorizedAnimationSpec`, `VectorizedFiniteAnimationSpec`, `VectorizedDurationBasedAnimationSpec`, `VectorizedTweenSpec`, `VectorizedSpringSpec`, `VectorizedSnapSpec`, `VectorizedKeyframesSpec`, `VectorizedRepeatableSpec`, `VectorizedInfiniteRepeatableSpec`, `VectorizedFloatAnimationSpec` (`getDurationNanos`, `getValueFromNanos`, `getVelocityFromNanos`, `getEndVelocity`, `isInfinite`) | missing | animation-objects |
| `FloatAnimationSpec`, `FloatTweenSpec`, `FloatSpringSpec` | missing | animation-objects |
| `AnimationVector`, `AnimationVector1D..4D`, `AnimationVector(…)` factories | `[f32; 4]` (Rust-shaped; no 1D..4D types) | animation-objects (only if the evaluation API needs them) |
| `AnimationConstants.UnspecifiedTime` | missing | animation-objects |
| `SpringSpec` duration (Compose's spring estimation), and ending an animation at that duration | compose-rs ends a spring when value and speed are under the threshold, frame by frame | spring-estimation |
| `AnimationState` (ctor, `copy`, `isFinished`, `createZeroVectorFrom`), `AnimationScope` (`cancelAnimation`, times, value, velocity), `animate(…)` (float and generic), `AnimationState.animateTo`, `animateDecay(…)` (float and on `AnimationState`), `sequentialAnimation` | missing | suspend-animate (decay forms: decay-animation) |
| `DecayAnimationSpec`, `FloatDecayAnimationSpec`, `VectorizedDecayAnimationSpec`, `exponentialDecay`, `FloatExponentialDecaySpec`, `generateDecayAnimationSpec`, `calculateTargetValue` (both), `DecayAnimation` (3 ctors), `DecayAnimation(FloatDecayAnimationSpec, …)` | missing | decay-animation |
| `ArcAnimationSpec` (experimental), `ArcMode` (`ArcAbove`, `ArcBelow`, `ArcLinear`), `KeyframesSpecConfig.using(entity, ArcMode)` | missing | arc-animation |
| `KeyframesSpecBaseConfig.atFraction`, `using(entity, easing)` (compose-rs names it `with`, Compose's deprecated name), `durationMillis` settable in the builder | partial | spec-gaps |
| `KeyframesWithSplineSpec`, `keyframesWithSpline(init)`, `keyframesWithSpline(periodicBias, init)` | missing | keyframes-with-spline |
| `StartOffset`, `StartOffsetType` (`Delay`, `FastForward`); `initialStartOffset` on `repeatable`, `infiniteRepeatable`, `RepeatableSpec`, `InfiniteRepeatableSpec` | missing | spec-gaps |
| `SnapSpec` as a `DurationBasedAnimationSpec` (so `repeatable(snap())` works) | `AnimationSpec::Snap` only, not repeatable | spec-gaps |
| `InterruptionAwareAnimationSpec(animationSpec, interruptionSpec)` | missing (transitions hard-code a default spring on interruption) | interruption-aware-spec |
| `Easing` as a `fun interface` (any object, capturing lambdas) | `Easing::Function(fn(f32) -> f32)` (no captures) | easing-functions |
| `EaseInBack`, `EaseInBounce`, `EaseInCirc`, `EaseInCubic`, `EaseInElastic`, `EaseInExpo`, `EaseInOutBack`, `EaseInOutBounce`, `EaseInOutCirc`, `EaseInOutCubic`, `EaseInOutElastic`, `EaseInOutExpo`, `EaseInOutQuad`, `EaseInOutQuart`, `EaseInOutQuint`, `EaseInOutSine`, `EaseInQuad`, `EaseInQuart`, `EaseInQuint`, `EaseInSine`, `EaseOutBack`, `EaseOutBounce`, `EaseOutCirc`, `EaseOutCubic`, `EaseOutElastic`, `EaseOutExpo`, `EaseOutQuad`, `EaseOutQuart`, `EaseOutQuint`, `EaseOutSine` | missing (only `Ease`, `EaseIn`, `EaseOut`, `EaseInOut`) | easing-functions |
| `PathEasing(path)` | missing | path-easing |
| `TransitionState`, `MutableTransitionState` (`currentState`, `targetState`, `isIdle`), `rememberTransition(transitionState, label)` | missing | transition-state |
| `Transition.segment`, `Transition.Segment` (`initialState`, `targetState`, `isTransitioningTo`), `pendingTargetState`, `label`, `animations`, `transitions`, `totalDurationNanos`, `Transition.TransitionAnimationState` (`animation`, `animationSpec`, `label`, `typeConverter`) | missing (`transition_spec` gets `(&initial, &target)`) | transition-state |
| `Transition.createChildTransition(label, transform)` | missing (docs/11: "no child transitions") | transition-state |
| `Transition.animateInt`, `animateIntOffset`, `animateIntSize`, `animateRect`, `animateSize` (and `label` on all `animate*`) | generic `animate_value` covers `i32`/`IntOffset`/`Size`; no named forms, no `IntSize`/`Rect` | transition-state, more-converters |
| `SeekableTransitionState` (`seekTo`, `animateTo(spec)`, `snapTo`, `fraction`, `currentState`, `targetState`) | missing | seekable-transition |
| `DeferredTransitionState` (`defer`, `animateTo`, `pendingTargetState`), `DeferredTransition`, `rememberDeferredTransition` | missing (new at this tip) | deferred-transition |
| `DeferredTargetAnimation` (`updateTarget`, `value`, `pendingTarget`, `isIdle`) | missing | deferred-target-animation |
| `InfiniteTransition.animations`, `label`, `InfiniteTransition.TransitionAnimationState` (`animation`, `animationSpec`, `label`, `typeConverter`); `label` on `animateFloat`/`animateValue` | missing (tooling-facing; `label` is accepted and ignored elsewhere) | transition-state |
| `withInfiniteAnimationFrameNanos`, `withInfiniteAnimationFrameMillis` (and the `InfiniteAnimationPolicy` they consult); `MotionDurationScale` honoured by every animation | missing | motion-duration-scale |
| `Rect.VectorConverter`, `IntSize.VectorConverter` (clamped at 0), `DpOffset.VectorConverter`; `TwoWayConverter(to, from)` factory | `Animate` impls missing for `Rect`, `IntSize`; compose-ui has no `DpOffset` | more-converters |
| `VisibilityThreshold` for `Int`, `IntOffset`, `IntSize`, `Offset`, `Size`, `Rect`, `Dp`, `DpOffset` | private per-type `visibility_threshold()`, with different values (see notes) | more-converters |

### animation (`androidx.compose.animation`)

| API | compose-rs | Feature |
|---|---|---|
| `fadeIn`/`fadeOut` | present | |
| `expandIn(spec, expandFrom, clip, initialSize)`, `expandHorizontally(spec, expandFrom, clip, initialWidth)`, `expandVertically(spec, expandFrom, clip, initialHeight)`, `shrinkOut`, `shrinkHorizontally`, `shrinkVertically` with their parameters | defaults only | enter-exit-params |
| `slideIn(spec, initialOffset: (IntSize) -> IntOffset)`, `slideOut`, `slideInHorizontally(spec, initialOffsetX)`, `slideOutHorizontally`, `slideInVertically(spec, initialOffsetY)`, `slideOutVertically` | fractions of the size, default spec; no horizontal forms | enter-exit-params |
| `scaleIn(spec, initialScale, transformOrigin)`, `scaleOut(…, transformOrigin)` | no `transformOrigin`; `scale_in` has no default scale | enter-exit-params |
| `EnterTransition.config`, `ExitTransition.config`, `EnterExitTransitionConfig`, `FadeConfig`, `SlideConfig`, `ChangeSizeConfig`, `ScaleConfig` | private `Parts` | enter-exit-params |
| `unveilIn(spec, initialColor, matchParentSize)`, `veilOut(…)`, `VeilConfig` | missing (new at this tip) | veil |
| `AnimatedVisibility(MutableTransitionState<Boolean>, …)`, `Transition<T>.AnimatedVisibility(visible: (T) -> Boolean, …)`, `ColumnScope.AnimatedVisibility` (×2), `RowScope.AnimatedVisibility` (×2) | missing; the scope overloads' defaults (vertical or horizontal expand and shrink) differ from the plain one's | animated-visibility-scope |
| `AnimatedVisibilityScope` (`transition`, `Modifier.animateEnterExit(enter, exit, label)`), `EnterExitState` (`PreEnter`, `Visible`, `PostExit`) | missing: content gets no scope | animated-visibility-scope |
| `AnimatedContent(…, contentKey, content: AnimatedContentScope.(S) -> Unit)`, `Transition<S>.AnimatedContent(…)` | no `contentKey`, no scope, no `Transition` overload | animated-content-scope |
| `AnimatedContentTransitionScope` (`initialState`, `targetState`, `isTransitioningTo`, `contentAlignment`, `slideIntoContainer(towards, spec, initialOffset)`, `slideOutOfContainer`, `using(SizeTransform?)`, `ExitTransition.KeepUntilTransitionsFinished`), `SlideDirection` (`Left`, `Right`, `Up`, `Down`, `Start`, `End`) | `transition_spec(initial, target)` only | animated-content-scope |
| `AnimatedContentScope` | missing | animated-content-scope |
| `ContentTransform(targetContentEnter, initialContentExit, targetContentZIndex, sizeTransform)`, `SizeTransform(clip, sizeAnimationSpec)`, `SizeTransform` interface | `size_transform: bool` (on or off, default spring) | animated-content-scope |
| `Transition<T>.Crossfade(modifier, animationSpec, contentKey, content)` (the plain overload has no `contentKey`) | missing | crossfade-extras |
| `animateContentSize(animationSpec, alignment, finishedListener)` (both overloads) | spec only | animate-content-size-extras |
| `CapturedAnimatedVisibility(visible, …)`, `CapturedAnimatedVisibility(MutableTransitionState, …)` | missing | captured-animated-visibility |
| `DeferredAnimatedContent`, `DeferredAnimatedVisibility`, `MutableContentTransform(…)`, `MutableTransform`, `TransformScope` (`alpha`, `scale`, `offset`, `transformOrigin`, `veil`) | missing (new at this tip) | deferred-transition |
| `SharedTransitionLayout`, `SharedTransitionScope(content)`, the `SharedTransitionScope` interface:<br>• the modifiers: `sharedElement`, `sharedBounds`, `sharedElementWithCallerManagedVisibility`, `skipToLookaheadSize`, `skipToLookaheadPosition`, `renderInSharedTransitionScopeOverlay`;<br>• the state: `rememberSharedContentState(key[, config])`, `SharedContentConfig(…)`, `OverlayClip`, `isTransitionActive`;<br>• the nested types: `SharedContentState` (`key`, `isMatchFound`, `clipPathInOverlay`, `parentSharedContentState`, `prepareTransitionWithInitialVelocity`), `SharedContentConfig` (`isEnabled`, `shouldKeepEnabledForOngoingAnimation`, `permitTransformDuringDeferredTransition`, `alternativeTargetBoundsInTransitionScopeAfterRemoval`), `PlaceholderSize` (`ContentSize`, `AnimatedSize`), `ResizeMode` (`RemeasureToBounds`, `scaleToBounds(contentScale, alignment)`);<br>• `SharedTransitionDefaults` (`BoundsTransform`, `SharedContentConfig`) and `BoundsTransform` | missing | shared-transition |
| `Modifier.animateBounds(lookaheadScope, modifier, boundsTransform, animateMotionFrameOfReference)` | missing | animate-bounds |
| `LookaheadAnimationVisualDebugging(…)`, `CustomizedLookaheadAnimationVisualDebugging(…)` (experimental) | missing | lookahead-visual-debug |
| `splineBasedDecay(density)`, `rememberSplineBasedDecay()`, `SplineBasedFloatDecayAnimationSpec` | missing (foundation has the spline privately in `fling.rs`) | decay-animation |
| `Color.VectorConverter(colorSpace)` | `Animate for Color` (sRGB only: compose-rs colors have no color spaces) | (none: n/a until color spaces exist) |
| `Animatable(Color)`, `animateColorAsState`, `InfiniteTransition.animateColor`, `Transition.animateColor` | present | |

### animation-graphics (`androidx.compose.animation.graphics`)

| API | compose-rs | Feature |
|---|---|---|
| `AnimatedImageVector` (`imageVector`, `totalDuration`), `AnimatedImageVector.animatedVectorResource(id)`, `rememberAnimatedVectorPainter(animatedImageVector, atEnd)` | missing. The resource loader is Android's; its analogue loads AVD XML from bundled bytes, as `remember_image_resource` does for images. compose-ui's `ImageVector` is paths with fills only: no groups, transforms, strokes or `VectorPainter` | animated-image-vector |

### Navigation Compose (`androidx.navigation.compose`) and `NavController`

What compose-rs has:

- `remember_nav_controller(start)`, `NavController`:
  - `navigate`, `navigate_with(NavOptions { pop_up_to, pop_up_to_inclusive, launch_single_top })`;
  - `pop_back_stack`, `pop_back_stack_to(dest, inclusive)`;
  - the reads `current_back_stack`, `current_back_stack_entry`, `current_destination`, `previous_back_stack_entry` (state reads, so `currentBackStackEntryAsState` is just a read).
- `NavBackStackEntry { id, destination }`.
- `NavHost!(controller, modifier, content_alignment, enter_transition, exit_transition, content)`: saveable state per entry, back pops.

| API | compose-rs | Feature |
|---|---|---|
| `NavHost(…, enterTransition, exitTransition, popEnterTransition, popExitTransition, sizeTransform)` as functions of `AnimatedContentTransitionScope<NavBackStackEntry>`; per-destination transitions on `composable(…)`/`ComposeNavigatorDestinationBuilder`; `DefaultNavTransitions` (`fadeIn(tween(700))`/`fadeOut(tween(700))`, `popEnterTransition`, `popExitTransition`, `sizeTransform = null`) | fixed `EnterTransition`/`ExitTransition` values, the same both ways, `NONE` by default | nav-transitions |
| `DefaultNavTransitions.predictivePopEnterTransition`/`predictivePopExitTransition`, predictive back seeking the NavHost's transition, `NavBackStackEntryInfo` (navigation events) | missing: back has no progress | predictive-back |
| `NavHost(navController, startDestination, …) { composable<T>(); dialog<T>(); navigation<T>(startDestination) {} }`, `NavHost(navController, graph: NavGraph, …)`; `NavGraphBuilder.composable/dialog/navigation` (route strings, `KClass`, objects; `typeMap`, `deepLinks`, `arguments`) | a `match` on the app's enum takes the place of the builder (D291); no nested graphs | nav-graphs (routes, `typeMap`, `NavType` and `arguments`: n/a, since destinations are typed values) |
| `dialog(…)` destinations, `DialogNavigator`, `DialogNavigator.Destination`, `DialogNavigatorDestinationBuilder`, `DialogHost` | missing | nav-dialog-destinations |
| `NavBackStackEntry.lifecycle` and `visibleEntries`, `LocalOwnersProvider(entry, saveableStateHolder, content)`, the per-entry `ViewModelStoreOwner` and `SavedStateRegistryOwner`, `savedStateHandle` | missing: compose-rs has no lifecycle or ViewModel model | nav-entry-owners |
| `NavController.navigateUp`, `popBackStack(route, inclusive, saveState)`, `clearBackStack`, `NavOptions.saveState`/`restoreState` (multiple back stacks), `getBackStackEntry(route)`; `launchSingleTop` replacing the arguments of an entry of the same route | missing; single top keeps the top only when `==` (documented) | nav-controller-extras |
| `ComposeNavigator`, `ComposeNavigator.Destination`, `ComposeNavigatorDestinationBuilder`, `rememberNavController(vararg navigators)` | no navigator layer | n/a: the Navigator plug-in architecture has no analogue in a typed-destination design |
| deep links (`NavDeepLink` into Activities, `handleDeepLink(Intent)`) | missing | n/a: Android intents |

### Navigation 3

Navigation 3 is a second, newer Navigation library for Compose. compose-rs doesn't model it, but its data model (a list of typed keys the app owns, mapped to content) is closer to compose-rs's typed destinations than Navigation Compose's routes are. All of it is missing.

**navigation3-runtime** (`androidx.navigation3.runtime`):

| API | Feature |
|---|---|
| `NavEntry(key, contentKey, metadata, content)`, `NavEntry(navEntry, content)`, `Content()`, `contentKey`, `metadata` | nav3-runtime |
| `entryProvider(fallback) { entry<K>(…); entry(key, …); addEntryProvider(…) }`, `EntryProviderScope`, `EntryDsl` | nav3-runtime |
| `NavEntryDecorator(onPop, decorate)`, `rememberDecoratedNavEntries(entries, decorators)`, `rememberDecoratedNavEntries(backStack, decorators, entryProvider)` | nav3-runtime |
| `SaveableStateHolderNavEntryDecorator`, `rememberSaveableStateHolderNavEntryDecorator` | nav3-runtime |
| `NavKey`, `NavBackStack` (a snapshot-state list of keys), `rememberNavBackStack(vararg)`, `rememberNavBackStack(configuration, vararg)`, `NavBackStackSerializer`, `NavKeySerializer` | nav3-runtime (serialization becomes `Saveable`) |
| `NavMetadataKey`, `metadata { put(key, value) }`, `MetadataScope`, `Map.get/contains(NavMetadataKey)` | nav3-runtime |
| `ResultEventBus` (`sendResult`, `conflateAsState`, `removeResult`), `rememberResultEventBus`, `LocalResultEventBus`, `ResultEffect`, `ResultEventBusNavEntryDecorator`, `rememberResultEventBusNavEntryDecorator` | nav3-results |
| `deeplink`: `DeepLinkMatcher` (+`Filter`, `MatchResult`, `mimeTypeFilter`), `DeepLinkRequest` (uri, `RequestExtras`, `mimeTypeExtra`), `UriDeepLinkMatcher`, `UriMatchResult`, `StaticKeyDeepLinkMatcher`, `BackStackMatcher`/`withBackStack`, `WrappedMatchResult`, `DeepLinkSerializer`, `RequestExtras`/`requestExtras {}`/`RequestExtrasKey`, `DeepLinkUri` | nav3-deep-links (optional) |
| `DeepLinkRequest(Intent)`, `actionExtra`, `actionFilter`, `IntentExtrasKey`, `ActionExtrasKey` | n/a: Android `Intent`s |

**navigation3-ui** (`androidx.navigation3.ui`, `androidx.navigation3.scene`):

| API | Feature |
|---|---|
| `NavDisplay(backStack, modifier, contentAlignment, onBack, entryDecorators, sceneStrategies, sceneDecoratorStrategies, sharedTransitionScope, sizeTransform, transitionSpec, popTransitionSpec, predictivePopTransitionSpec, entryProvider)`, `NavDisplay(entries, …)`, `NavDisplay(sceneState, navigationEventState, …)` | nav3-ui |
| `NavDisplay.transitionSpec/popTransitionSpec/predictivePopTransitionSpec` metadata (`TransitionKey`, `PopTransitionKey`, `PredictivePopTransitionKey`), `defaultTransitionSpec`, `defaultPopTransitionSpec`, `defaultPredictivePopTransitionSpec`, `LocalNavAnimatedContentScope` | nav3-ui (needs animated-content-scope) |
| `Scene` (`key`, `entries`, `previousEntries`, `content`, `metadata`), `OverlayScene` (`overlaidEntries`, `onRemove`), `SceneStrategy`, `SceneStrategyScope` (`onBack`), `SinglePaneSceneStrategy`, `DialogSceneStrategy` (+`dialog(properties)`, `DialogKey`), `SceneDecoratorStrategy`, `SceneDecoratorStrategyScope`, `SceneState` (`currentScene`, `previousScenes`, `overlayScenes`, `entries`), `rememberSceneState`, `SceneInfo` (`title`, `url`), `TitleMetadataKey`, `UrlMetadataKey` | nav3-ui |
| `NavigationBackHandler(sceneState, state, onBackCancelled, onBackCompleted)`, `rememberNavigationEventState(sceneState)` | nav3-ui and predictive-back (the `navigationevent` library) |

## Missing features

One entry per feature. Sizes:

- S: under a day;
- M: days;
- L: one to two weeks;
- XL: multi-week.

"Exp." says whether the API is experimental in Compose.

### Animation core

#### spring-estimation (S–M, not exp.): done (D800)

- **API:** `SpringSpec`'s duration: `VectorizedSpringSpec.getDurationNanos` through Compose's `SpringEstimation` (`estimateAnimationDurationMillis`).
- **Behaviour:** an animation ends at play time ≥ that duration, at the target.
- **What it does:** gives a spring a computed duration and ends it exactly there. compose-rs instead tests the value and speed against the threshold every frame (`spec.rs`, `VELOCITY_THRESHOLD_MULTIPLIER`), so its springs end on a different frame from Compose's.
- **Tests:** `SpringEstimationTest`, `PhysicsAnimationTest` (duration and end-snapping tests), `AnimatedVisibilityTest.testAnimationSettleExactTime`, `SeekableTransitionStateTest` (durations).
- **Dependencies:** none. Do it together with the visibility-threshold fixes (see the notes).

#### animation-objects (M, not exp.)

- **API:**
  - `Animation`, `TargetBasedAnimation`;
  - the `Vectorized*AnimationSpec` evaluation API (`value_from_nanos`, `velocity_from_nanos`, `duration_nanos`, `end_velocity`, `is_infinite`) on every spec;
  - `FloatAnimationSpec`, `FloatTweenSpec`, `FloatSpringSpec`;
  - `AnimationConstants.UnspecifiedTime`;
  - optionally `AnimationVector1D..4D` as thin `[f32; N]` types.
- **What it does:** public, pure evaluation of a spec at a play time. Needed by tests, by `SeekableTransitionState` (durations), and by tooling.
- **Tests:** `AnimationTest`, `IsInfiniteTest`, `PhysicsAnimationTest`, and the exact-value parts of `TweenAnimationTest`, `KeyframeAnimationTest`, `RepeatableAnimationTest` and `InfiniteTransitionTest`.
- **Dependencies:** spring-estimation for spring durations.

#### spec-gaps (S, not exp.)

- **API:**
  - `StartOffset`/`StartOffsetType` (`Delay`, `FastForward`), with `initialStartOffset` on `repeatable`/`infiniteRepeatable`;
  - `SnapSpec` as a duration-based spec (`repeatable(snap())`);
  - keyframes `atFraction`, and `using(easing)` as the name of `with`;
  - a settable `duration_millis` in the keyframes builder (it is an argument today).
- **Tests:** `RepeatableAnimationTest` (3), `KeyframeAnimationTest` (2).
- **Dependencies:** none.

#### more-converters (S, not exp.)

- **API:**
  - `Animate` for `Rect`, `IntSize` (rounded, clamped at 0) and `DpOffset` (a new compose-ui type);
  - `animate_rect_as_state`, `animate_int_size_as_state`;
  - a `visibility_threshold` parameter on `animate_value_as_state`;
  - public `VisibilityThreshold` values, set to Compose's.
- **Tests:** `SingleValueAnimationTest.animate4DRectTest`, `ConverterTest.testRectConverter`, `AnimatableTest.testIntSize_alwaysWithinValidBounds`.
- **Dependencies:** compose-ui `DpOffset`.

#### animatable-bounds (S, not exp.)

- **API:**
  - `Animatable::update_bounds`, `lower_bound`, `upper_bound`, and `AnimationEndReason::BoundReached`;
  - a visibility threshold and `label` in the constructor;
  - `type_converter`/`velocity_vector` accessors.
- **Tests:** `AnimatableTest.testUpdateBounds`.
- **Dependencies:** none (bounds matter most for decay-animation).

#### suspend-animate (M, not exp.)

- **API:** `animate(initial, target, initial_velocity, spec, block)` (`f32` and generic), `AnimationState`, `AnimationScope`, `AnimationState::animate_to`, `animate_decay`, `sequential_animation`, the per-frame `block` on `Animatable::animate_to`, and `AnimationResult::end_state`.
- **What it does:** low-level animations without an `Animatable`.
- **Tests:** `SuspendAnimationTest`, `DurationScaleTest`.
- **Dependencies:** animation-objects; decay-animation for the decay forms.

#### decay-animation (M, not exp.)

- **API:**
  - the specs: `DecayAnimationSpec`, `FloatDecayAnimationSpec`, `VectorizedDecayAnimationSpec`, `exponential_decay`, `FloatExponentialDecaySpec`, `generate_decay_animation_spec`;
  - `calculate_target_value`, `DecayAnimation`, `Animatable::animate_decay`;
  - the spline family: `spline_based_decay(density)`, `remember_spline_based_decay`, `SplineBasedFloatDecayAnimationSpec`.
- **What it does:** flings that slow to a stop.
- **Tests:** `DecayAnimationTest`, `IsInfiniteTest` (2), `AnimatableTest.animateDecayTest`, `SuspendAnimationTest` (2), `AndroidFlingSplineTest`.
- **Dependencies:**
  - animation-objects and animatable-bounds;
  - the Android spline exists privately in `compose-foundation/src/fling.rs` and should move into compose-animation;
  - compose-foundation doesn't depend on compose-animation today, while Compose's foundation does depend on animation-core. Foundation's flings using `spline_based_decay` would add that crate edge, a D292 decision.

#### easing-functions (S, not exp.)

- **API:**
  - the 30 missing `Ease*` curves (Back, Bounce, Circ, Cubic, Elastic, Expo, Quad, Quart, Quint, Sine, each in In, Out and InOut);
  - an `Easing` that can hold any curve: a capturing closure (`Rc<dyn Fn>`) or a trait. Compose's is a `fun interface`, needed by `PathEasing` and custom easings.
- **Tests:** none target the `Ease*` constants; `TweenAnimationTest.easingIsApplied` uses a custom easing.
- **Dependencies:** none.

#### path-easing (S–M, not exp.)

- **API:** `PathEasing(path)`.
- **Tests:** `PathEasingTest`.
- **Dependencies:**
  - easing-functions (an easing that holds data);
  - a compose-ui path iterator or flattener, which compose-ui's `Path` lacks (ui graphics area).

#### keyframes-with-spline (M, not exp.)

- **API:** `keyframes_with_spline(periodic_bias, init)`, `KeyframesWithSplineSpec`, and Compose's monotonic spline.
- **Tests:** `KeyframeSplineAnimationTest`, `MonoSplineTest`, `RepeatableAnimationTest` (1).
- **Dependencies:** spec-gaps (`atFraction`).

#### arc-animation (M; `ArcAnimationSpec` is exp., `ArcMode` and keyframes `using(ArcMode)` are not)

- **API:** `ArcAnimationSpec`, `ArcMode` (`ArcAbove`, `ArcBelow`, `ArcLinear`), keyframes `using(arc_mode)`.
- **Tests:** `ArcAnimationTest`, `KeyframeArcAnimationTest`, `RepeatableAnimationTest` (1).
- **Dependencies:** spec-gaps.

#### interruption-aware-spec (S–M, not exp.)

- **API:** `InterruptionAwareAnimationSpec(spec, interruption_spec)`, honoured by `Animatable`, `animate*AsState` and `Transition`. compose-rs's transitions hard-code Compose's default interruption spring.
- **Tests:** `PhysicsAnimationTest` (1), `AnimatableTest` (1), `SingleValueAnimationTest` (1), `TransitionTest` (2).
- **Dependencies:** none.

#### motion-duration-scale (S–M, not exp.)

- **API:** `MotionDurationScale` (a runtime/ui coroutine-context element: a scale of 0 finishes animations at once, as Android's animator scale), honoured by every animation; `with_infinite_animation_frame_nanos`/`_millis` and the `InfiniteAnimationPolicy` they consult.
- **Tests:** `DurationScaleTest`, `DurationScaleTransitionTest`.
- **Dependencies:**
  - the runtime and ui areas: a context element on compose-rs's executor and frame clock;
  - transition-state and seekable-transition for the transition tests.

#### transition-state (M–L, not exp.)

- **API:**
  - the states: `TransitionState`, `MutableTransitionState` (`is_idle`), `remember_transition(state, label)`;
  - on `Transition`: `Segment` with `is_transitioning_to` (the `transition_spec` closure taking a segment), `pending_target_state`, `label`, `animations`, `total_duration_nanos`;
  - child transitions: `create_child_transition`, `transitions`;
  - `animate_int`, `animate_int_offset`, `animate_int_size`, `animate_rect`, `animate_size`;
  - the tooling getters on `InfiniteTransition`.
- **What it does:** the shared machinery of `AnimatedVisibility`, `AnimatedContent` and `Crossfade` overloads, and of state hoisting. D423 kept a task per value; seeking needs Compose's single play-time loop.
- **Tests:** `TransitionTest` (~10). It also unblocks parts of `AnimatedVisibilityTest`, `AnimatedContentTest`, `CrossfadeTest` and `DurationScaleTransitionTest`.
- **Dependencies:** animation-objects (durations).

#### seekable-transition (L, not exp.)

- **API:** `SeekableTransitionState` (`seek_to(fraction, target)`, `animate_to(target, spec)`, `snap_to`, `fraction`), and Transition support for seeking (Compose's internal `delayed` wrapper).
- **What it does:** drives a transition from a gesture. This is the basis of predictive back.
- **Tests:** `SeekableTransitionStateTest` (44), `DelayedAnimationTest`, `DurationScaleTransitionTest` (2), `AnimatedVisibilityTest.testSeekingAnimatedVisibility`.
- **Dependencies:** transition-state, animation-objects, spring-estimation.

#### deferred-target-animation (S, not exp.)

- **API:** `DeferredTargetAnimation(converter)`: `update_target(target, scope, spec)`, `value`, `pending_target`, `is_idle`.
- **Tests:** `DeferredTargetAnimationTest`.
- **Dependencies:** none.

#### deferred-transition (XL, not exp., new at this tip)

- **API:**
  - the states: `DeferredTransitionState` (`defer`, `animate_to`, `pending_target_state`), `DeferredTransition`, `remember_deferred_transition`;
  - the composables: `DeferredAnimatedContent`, `DeferredAnimatedVisibility`;
  - the transforms: `MutableContentTransform`, `MutableTransform`, `TransformScope`.
- **What it does:** a transition whose start waits on a gate while the content is previewed by gesture-driven transforms, then handed off with velocity.
- **Tests:** `DeferredTransitionTest`, `DeferredAnimatedVisibilityTest`, `DeferredAnimatedContentTest`, `SharedMutableTransformStateTest`, and with shared-transition `DeferredSharedElementTest`.
- **Dependencies:** transition-state, enter-exit-params, animated-visibility-scope, animated-content-scope, veil.

### Animation components

#### enter-exit-params (M, not exp.)

- **API:**
  - the full parameter lists of `expand_*`/`shrink_*` (spec, alignment, `clip`, size lambdas), `slide_in`/`slide_out` (spec, `IntSize -> IntOffset`), `slide_in_horizontally`/`slide_out_horizontally` and the vertical pair (spec, offset lambda), `scale_in`/`scale_out` (spec, `transform_origin`);
  - the `config` getters and their types (`EnterExitTransitionConfig`, `FadeConfig`, `SlideConfig`, `ChangeSizeConfig`, `ScaleConfig`).
- **Behaviour to change:** animate the parts in their own units with Compose's thresholds: pixels for size and offset, the float itself for alpha and scale. compose-rs animates a 0..1 fraction with threshold 0.001 for every part.
- **Tests:** `EnterExitTransitionConfigTest`, and most of `AnimatedVisibilityTest` and `AnimatedContentTest`.
- **Dependencies:** none.

#### veil (S–M, not exp., new at this tip)

- **API:** `unveil_in(spec, initial_color, match_parent_size)`, `veil_out`, `VeilConfig`.
- **Tests:** `AnimatedVisibilityTest` (4), `EnterExitTransitionConfigTest` (2), parts of the deferred tests.
- **Dependencies:** enter-exit-params.

#### animated-visibility-scope (M, not exp.)

- **API:**
  - `AnimatedVisibilityScope` (`transition`, `Modifier.animate_enter_exit`), `EnterExitState`;
  - the overloads: `AnimatedVisibility(MutableTransitionState)`, `Transition::AnimatedVisibility(visible_fn)`, and `ColumnScope`/`RowScope` with their defaults (`fade_in() + expand_vertically()` in a column, horizontal in a row).
- **Tests:** `AnimatedVisibilityTest` (transition overload, the `MutableTransitionState` cases), `CapturedAnimatedVisibilityTest`, `SharedTransitionTest`.
- **Dependencies:** transition-state, enter-exit-params.

#### animated-content-scope (M–L, not exp.)

- **API:**
  - the scopes: `AnimatedContentScope` (an `AnimatedVisibilityScope`) and `AnimatedContentTransitionScope`, holding `initial_state`, `target_state`, `content_alignment` and the methods `slide_into_container(SlideDirection, spec, offset)`, `slide_out_of_container`, `using(SizeTransform)`, `ExitTransition::KEEP_UNTIL_TRANSITIONS_FINISHED`;
  - the `ContentTransform` parts: `target_content_z_index`, `SizeTransform { clip, size_animation_spec(initial, target) }`;
  - `AnimatedContent(…, content_key)` and the `Transition::AnimatedContent` overload.
- **Behaviour to change:** remove the cap of three contents (Compose keeps every state still animating) and key contents by `content_key(state)`.
- **Tests:** `AnimatedContentTest` (~9), `EnterExitTransitionConfigTest` (SizeTransform default); NavHost and NavDisplay transitions build on it.
- **Dependencies:** transition-state, animated-visibility-scope, enter-exit-params.

#### crossfade-extras (S, not exp.)

- **API:** `Transition::Crossfade(modifier, animation_spec, content_key, content)`. Compose's plain `Crossfade` has no `contentKey`.
- **Tests:** `CrossfadeTest.crossfadeTest_contentKey`.
- **Dependencies:** transition-state.

#### animate-content-size-extras (S, not exp.)

- **API:** `alignment` (the content placed within the animating size) and `finished_listener(initial, target)` on `animate_content_size`.
- **Tests:** `AnimationModifierTest` (2 alignment).
- **Dependencies:** none.

#### captured-animated-visibility (M, not exp., new at this tip)

- **API:** `CapturedAnimatedVisibility(visible | MutableTransitionState, …)`.
- **What it does:** disposes the content at once and animates a recorded layer of its last frame.
- **Tests:** `CapturedAnimatedVisibilityTest`.
- **Dependencies:**
  - recording a subtree into a graphics layer and drawing it later (`rememberGraphicsLayer`/`record`/`drawLayer`), which compose-ui lacks (ui drawing area);
  - enter-exit-params and transition-state; ui lookahead for 2 tests.

#### shared-transition (XL, not exp.)

- **API:** `SharedTransitionLayout`, `SharedTransitionScope` and its whole surface (see the API table), `BoundsTransform`, `SharedTransitionDefaults`.
- **What it does:** matches elements across content changes and animates their bounds in an overlay.
- **Tests:** `SharedTransitionTest` (71), `DeferredSharedElementTest` (15), `OverlaySceneTest`, `AnimatedTest.testSharedElement`, `NavDisplayScreenshotTest` (1).
- **Dependencies:**
  - **ui lookahead** (`LookaheadScope`, approach and intermediate layout, lookahead coordinates), which compose-rs lacks entirely;
  - movable content (tests use `movableContentOf`), which compose-runtime lacks;
  - an overlay with z-ordering, animated-visibility-scope, transition-state, and lazy layouts (present).

#### animate-bounds (L, not exp.)

- **API:** `Modifier.animate_bounds(lookahead_scope, modifier, bounds_transform, animate_motion_frame_of_reference)`.
- **Tests:** `AnimateBoundsTest`.
- **Dependencies:** ui lookahead; `BoundsTransform` (shared with shared-transition); movable content for 1 test.

#### lookahead-visual-debug (M, exp.)

- **API:** `LookaheadAnimationVisualDebugging`, `CustomizedLookaheadAnimationVisualDebugging`.
- **Tests:** `LookaheadAnimationVisualDebugHelperTest`.
- **Dependencies:** shared-transition, animate-bounds.

#### lookahead in animations (S–M after ui lookahead; no separate API)

- **What it does:** `AnimatedVisibility`, `AnimatedContent` and `animate_content_size` take the lookahead size and placement inside a `LookaheadScope`.
- **Tests:** `AnimatedVisibilityTest` (1), `AnimatedContentTest` (2), `AnimationModifierTest` (1), `CapturedAnimatedVisibilityTest` (2).
- **Dependencies:** ui lookahead.

#### animated-image-vector (L, not exp.)

- **API:** `AnimatedImageVector`, `remember_animated_vector_painter(avd, at_end)`, and a loader of animated-vector XML from bundled bytes (the analogue of `animatedVectorResource`).
- **What it does:** the internal `Animator` model (object animators, sets, path morphing, repeats).
- **Tests:** the five animation-graphics files (22).
- **Dependencies:**
  - a full `ImageVector` in compose-ui (groups with rotation, scale and translation, strokes, trim, `VectorPainter`, path data interpolation): ui graphics area;
  - an XML parser (a new crate dependency, or a hand-written one);
  - spec-gaps (keyframes).

### Navigation

#### nav-transitions (M, not exp.)

- **API:**
  - `enter_transition`, `exit_transition`, `pop_enter_transition`, `pop_exit_transition` and `size_transform`, as functions of the change on `NavHost`;
  - per-destination overrides (Rust shape: a function of `(from, to)`, since there is no graph builder);
  - `DefaultNavTransitions` (700 ms fades);
  - the z-order of a pop (the entry leaving stays on top).
- **Behaviour to change:** the default should become Compose's 700 ms fade, a behaviour change from D291's none.
- **Tests:** `NavHostScreenShotTest`, `NavHostTest.testNavHostAnimations`/`testNavHostAnimationsBackInterrupt`.
- **Dependencies:** animated-content-scope.

#### predictive-back (L, not exp.)

- **API:**
  - back with progress, swipe edge and cancel: `PredictiveBackHandler` in activity-compose, `NavigationEventHandler` in the `navigationevent` library;
  - NavHost seeking its transition by it: `predictive_pop_enter_transition`/`_exit_transition`, `NavBackStackEntryInfo`.
- **What it does:** on Android, the back gesture previews the screen underneath and commits or cancels.
- **Tests:** `NavHostPredictiveBackTest`, `NavHostScreenShotTest` (3), `NavHostTest.testNavBackStackEntryInfoHistory`, and the predictive-back tests of navigation3-ui.
- **Dependencies:**
  - the ui/platform area: the Android host delivering `BackEvent` progress, and `BackHandler` growing a progress form. docs/11 notes there's no `PredictiveBackHandler`;
  - seekable-transition, nav-transitions.

#### nav-dialog-destinations (M, not exp.)

- **API:** dialog destinations shown in a `Dialog` over the current entry, popped on dismiss (`DialogNavigator`, `DialogHost`); Rust shape: a destination flag or a separate `dialog` arm.
- **Tests:** `DialogNavigatorTest`, `NavHostTest` (2), `NavGraphBuilderTest` (dialog ones), `NavHostControllerTest` (6 dialog `KClass` ones, minus the argument checks).
- **Dependencies:** compose-ui `Dialog` (present).

#### nav-controller-extras (S–M, not exp.)

- **API:** `navigate_up`, `get_back_stack_entry(dest)`, `clear_back_stack`, and `save_state`/`restore_state` in `NavOptions` and `pop_back_stack` (multiple back stacks for bottom navigation, whose entries keep their saveable state).
- **Tests:** `NavHostControllerTest` (3), and the multiple-back-stack scenarios in `NavHostTest`.
- **Dependencies:** none.

#### nav-graphs (M, not exp.)

- **API:**
  - nested graphs: a destination grouping a sub-stack with its own start, popped and navigated to as a unit;
  - changing the start destination;
  - graph-scoped state.
- **What it does:** a Rust shape would keep the `match` and add graph nodes to the destination type.
- **Tests:** `NavGraphBuilderTest` (about 19 of 33; the route-string, `typeMap` and deep-link ones are n/a), `NavHostTest` (`setSameGraph*` (7), `testChangeStartDestination`).
- **Dependencies:** none.

#### nav-entry-owners (L, not exp.)

- **API:**
  - per-entry lifecycles (`CREATED`, `STARTED`, `RESUMED`, following transitions), `visible_entries`;
  - a per-entry retained store (the `ViewModelStoreOwner` analogue, cleared on pop);
  - a per-entry saved-state registry.
- **Tests:** `NavBackStackEntryProviderTest` (3), `NavHostTest` (~10), `NavDisplayLifecycleTest`.
- **Dependencies:** a lifecycle model in compose-rs (ui/platform area; there is none).

<a id="navigation3"></a>
#### navigation3: four features (a decision for the user before any work)

- **nav3-runtime (L):** `NavEntry`, `entryProvider`, decorators and `rememberDecoratedNavEntries`, the saveable-state decorator, `NavKey`/`NavBackStack`/`rememberNavBackStack` over `Saveable`, metadata keys.
  - Tests: 8 runtime files (79 tests).
  - Dependencies: none beyond what NavHost uses.
- **nav3-ui (L):** `NavDisplay`, scenes and strategies, dialog and overlay scenes, scene decorators, transition metadata, `LocalNavAnimatedContentScope`, `NavigationBackHandler`.
  - Tests: 15 ui files (121 tests).
  - Dependencies: nav3-runtime, animated-content-scope, predictive-back, shared-transition (2 tests), nav-entry-owners (lifecycle test).
- **nav3-results (S):** `ResultEventBus`, `ResultEffect` and the decorator.
  - Tests: 4 files (10 tests).
  - Dependencies: nav3-runtime.
- **nav3-deep-links (L, optional):** URI patterns, matchers, argument decoding and synthetic back stacks.
  - Tests: 8 files (211 tests).
  - Dependencies: a URI type and a decoding trait (the analogue of kotlinx.serialization), and a platform path delivering links: on Android an `Intent`, which is n/a, or a desktop URL handler.

Navigation 3 is a separate library from Navigation Compose. Adopting it would mean either replacing compose-navigation's model (D291) or offering both. Its back-stack-as-a-list design fits compose-rs's typed destinations well. That choice is the user's.

### Suggested order

1. **spring-estimation and the threshold fixes (S–M).** They change existing behaviour, so do them before porting tests that check exact frames.
2. **animation-objects (M).** It unblocks exact-value ports across animation-core.
3. **The small ones, in any order:**
   - spec-gaps (S), more-converters (S), animatable-bounds (S), easing-functions (S);
   - animate-content-size-extras (S);
   - interruption-aware-spec (S–M), deferred-target-animation (S).

   Then port everything marked `port` and the portable parts of `partial`.
4. **enter-exit-params (M).**
5. **transition-state (M–L).** Revisit D423: a single play-time loop.
6. **animated-visibility-scope (M)**, then **animated-content-scope (M–L)** and **crossfade-extras (S)**.
7. **The animation-core additions:** decay-animation (M), then suspend-animate (M) and motion-duration-scale (S–M).
8. **The Navigation Compose basics:** nav-transitions (M), nav-controller-extras (S–M), nav-dialog-destinations (M).
9. **seekable-transition (L)**, then **predictive-back (L)**, with the platform back-progress work.
10. **The remaining specs:** keyframes-with-spline (M), path-easing (S–M, after the ui path iterator), arc-animation (M, experimental).
11. **veil (S–M)**, then **deferred-transition (XL)**.
12. **After ui lookahead (and movable content) lands in its own area:**
    - lookahead in animations (S–M);
    - animate-bounds (L);
    - shared-transition (XL);
    - lookahead-visual-debug (M, experimental).
13. **captured-animated-visibility (M)**, after ui layer recording.
14. **nav-graphs (M)** and **nav-entry-owners (L)**, after a lifecycle model.
15. **animated-image-vector (L)**, after a full `ImageVector`.
16. **Navigation 3**, if the user chooses it: nav3-runtime, then nav3-ui, nav3-results, and nav3-deep-links last or never.

## Notes for the porting agents

### Mapping Compose's test patterns

| Compose | compose-rs (`compose_test`, `crates/compose-test/src/lib.rs`) |
|---|---|
| `createComposeRule()` + `rule.setContent { … }` | `compose_test(|| …)` or `ComposeTestRule::with_manual_clock(|| …)`. The content is fixed at creation, so drive changes through `mutable_state_of` handles created outside (see `visibility.rs` `panel()`) |
| `rule.mainClock.autoAdvance = false` before `setContent` | `ComposeTestRule::with_manual_clock`, or `set_auto_advance(false)` |
| `rule.mainClock.advanceTimeByFrame()` | `advance_time_by_frame()` (16 ms) |
| `rule.mainClock.advanceTimeBy(ms)` | `advance_time_by(Duration::from_millis(ms))`, rounded up to whole frames as Compose's; `advance_time_by_with(d, true)` for `ignoreFrameDuration = true` |
| `rule.runOnIdle { … }`, `runOnUiThread { … }` | plain code after `wait_for_idle()`: everything runs on the test thread |
| `rule.waitForIdle()` | `wait_for_idle()`. With auto-advance on it moves the clock while animations ask for frames, up to 60 s of test time |
| `rule.waitUntil { cond }`, `mainClock.advanceTimeUntil { cond }` | missing: loop on `advance_time_by_frame()` with a bound |
| `LaunchedEffect(Unit) { withFrameNanos { … } }` inside content, asserting per frame | works as is (`compose_runtime::with_frame_nanos`, `LaunchedEffect`). Compose's `SingleValueAnimationTest` and `InfiniteTransitionTest` use this pattern. Or sample from the test: read the state after each `advance_time_by_frame()` |
| `onGloballyPositioned { coords.localToRoot(…) }`, `size` | `Modifier.on_globally_positioned` and `on_size_changed` exist (`compose-ui/src/layout_callbacks.rs`); or `on_node_with_tag(t).fetch_semantics_node().bounds` (root coordinates, used by `visibility.rs`) |
| `assertPositionInRootIsEqualTo`, `getBoundsInRoot`, `assertWidthIsEqualTo` | missing: compare `fetch_semantics_node().bounds` |
| `captureToImage()` and pixel or screenshot assertions | missing: assert on `dump_display_list()` or `draw_snapshot()` (alphas and layer transforms, as `crossfade.rs` `alphas()` does), or on z-order in the display list |
| `CompositionLocalProvider(LocalDensity provides Density(1f))` | the default test host is at density 1; else `compose_test_with(size, Density(…), …)` |
| `StateRestorationTester` / activity recreation | `host.save_state()` → `to_bytes` → a new rule with `restore_state` (see `recreate()` in `compose-navigation/tests/suite/navigation.rs`) |
| `Espresso.pressBack()` / `onBackPressedDispatcher.onBackPressed()` | `ui.host().on_back_pressed()`; no predictive-back progress events |
| `performTouchInput { swipe… }` | missing as a DSL (only `perform_click`); inject `PointerEvent`s through the host |
| recomposition counts (`TransitionTest.testRecompositionCountWhenPassingTransition`) | `recomposition_counts()` / `track_recompositions` |
| `DetectLeaksAfterTestSuccess` | n/a |
| `TestNavHostController`, `createNavController(context)` | `NavController::new(start)` (returns its owner) or `remember_nav_controller(start)` |
| an exception from an interrupted `animateTo` | `AnimationResult { end_reason: AnimationEndReason::Interrupted, .. }` (D120) |
| `assertEquals(expected, actual, delta)` over frames | assert per frame with a tolerance. Spring results agree with Compose's math (`spec.rs`), but end frames don't (see below) |

**Pure spec tests (`commonTest` `Vectorized*`):** until animation-objects exists, sample through an `Animatable` in a rule. `review_round14.rs` `samples()` is the pattern: the first value is at play time 0, then one every 16 ms. That only reaches 16 ms granularity, so prefer waiting for animation-objects for exact checks.

### What the harness lacks

Since the harness's milestone 1 (D900–D905), compose-test has Compose's clock model (rounding, frames on the test clock, `advance_time_until`, `wait_until`, the infinite-animation policy), bounds and displayed assertions, matchers over the tree and selectors (`on_children`, `on_parent`, collections), the touch, mouse (primary button) and key injection scopes, `perform_scroll_to`, `capture_to_image` with `assert_pixels`, `print_to_string`, and `set_content`; `perform_click` is a touch click. docs/09 lists the API, and [ui-text-and-test](ui-text-and-test.md) what is still missing. Existing tests that relied on the old timing gained a frame where they wait for a recomposition (D900).

- **`advance_time_by` rounds up to whole frames**, as Compose's `advanceTimeBy(ms)` (`ceil(ms / 16) × 16`, D900); `advance_time_by_ignoring_frame_duration` is `ignoreFrameDuration = true`. A frame comes a frame duration after something needs one, as Compose's.
- **No `MotionDurationScale`** to put in the test context (feature motion-duration-scale).

### compose-rs behaviour suspected to differ from Compose's

Each item names the Compose test that would catch it, in the file's notes or listed here.

1. **Springs end on a different frame.** Fixed (D800): a spring ends at Compose's estimated duration.
   - Caught by: `PhysicsAnimationTest.testCriticallydampedDuration`, `testOverdampedDuration`, `testUnderdampedDuration`, `testEndSnapping`; `AnimatedVisibilityTest.testAnimationSettleExactTime`.
2. **Visibility thresholds.** Fixed (D800): a spring without a threshold uses 0.01 per component, the types' defaults are the tip's (0.4 dp, 1 px), and `animate*AsState` resolves a given threshold as Compose's.
   - **Plain springs.** Compose's `Animatable` and an explicit `spring()` without a threshold use `Spring.DefaultDisplacementThreshold` (0.01) on every dimension. Only `animate*AsState`'s default specs carry the type's threshold. compose-rs's `Animatable` uses `T::visibility_threshold()` for every spring without one, so an `Animatable<Dp>` or `<Offset>` with `spring()` ends earlier than Compose's.
   - **Default values.** At this tip `Dp.VisibilityThreshold` is 0.4 dp and `Offset`/`Size`/`Rect` use 1 px (`VisibilityThresholds.kt`). compose-rs uses 0.1 dp and 0.5 px, which look like older Compose values; check them against the version parity targets (1.12.1) before changing.
   - **`animateFloatAsState`.** Compose's applies a non-default `visibilityThreshold` to any `SpringSpec`, replacing the spec's own. compose-rs fills it only when the spring has none.
   - Caught by: `SingleValueAnimationTest.visibilityThresholdTest`, `defaultVisibilityThresholdTest`, `customSpringSpecVisibilityThresholdTest`, `customSpringSpecLargeVisibilityThresholdTest`, `updateVisibilityThresholdTest`.
3. **Enter and exit transitions animate a 0..1 fraction, not the values.** Every part springs a fraction with threshold 0.001 (`visibility.rs` `default_spec`). Compose animates alpha and scale as floats (threshold 0.01 for the default spring), and size and offset in pixels (1 px). So compose-rs's default fades end later, and expansions and slides of content smaller than 1,000 px end later too.
   - Also, slide offsets are fractions times the full size, recomputed each measure, while Compose calls the offset lambda with the full size.
   - Caught by: `AnimatedVisibilityTest.testScaleInAndScaleOutDefaultVisibilityThreshold`, `testAnimationSettleExactTime`, and the frame-by-frame expand and slide tests.
4. **Colors aren't clamped in Oklab.** Compose clamps L to 0..1, a and b to ±0.5 and alpha to 0..1 before converting back. compose-rs converts first and clamps the sRGB channels, so a bouncy color spring overshoots differently.
   - Caught by: `ConverterTest.testColorConverterClampValuesOutOfRange`.
5. **`i32` rounds where Compose truncates.** `Int.VectorConverter` uses `toInt()`; compose-rs's `Animate for i32` rounds. `IntOffset` and `IntSize` round in both.
   - Caught by: `TypeConverterTest` only checks whole numbers, so add a mid-animation `animate_int_as_state` case.
6. **`AnimatedVisibility` inside a `Column` or `Row` has the wrong defaults.** Compose's scope overloads default to vertical or horizontal expand and shrink. compose-rs has only the plain overload (`expand_in`/`shrink_out`), so the same call animates differently.
   - Caught by: no Compose test targets it. Add one with animated-visibility-scope.
7. **`AnimatedContent` keeps at most three contents and keys them by an internal id.** Compose keeps all and keys by `contentKey(state)`.
   - Caught by: `AnimatedContentTest.AnimatedContentWithKeysTest` and `AnimatedContentWithContentKey`. Write a rapid four-state test for the cap.
8. **The animation containers expose no shared `Transition`.** Content can't read `transition.isRunning` (`AnimatedContentTest.AnimatedContentWithInterruption` does) and nothing can be seeked.
9. **`NavHost` has no default transition** (Compose fades for 700 ms) and never pops its start destination. `launch_single_top` compares with `==`, where Compose compares the route and replaces the arguments. All are documented in D291, but ported tests must expect compose-rs's behaviour or the decision must change.
   - Caught by: `NavHostTest.testNavHostAnimations`, `NavHostControllerTest.testNavigateOptionSingleTopDifferentArguments`.
10. **Keyframe times are `u32`.** Compose's out-of-range negative keyframes can't be written.
    - Also: at the end, compose-rs's frame returns the target; Compose's *vectorized* spec returns a keyframe placed at the duration, while its `TargetBasedAnimation` returns the target. Compare at the matching level.
    - Caught by: `KeyframeAnimationTest.outOfRangeValues_withForcedInitialAndTarget`.
11. **`InfiniteTransition` timing.** Its values accept finite `RepeatableSpec`s (Compose requires `InfiniteRepeatableSpec`), and run on frame time rather than the transition's play time (documented, D420).
    - Caught by: `InfiniteTransitionTest.transitionTest` checks the phase from the first frame. It may still pass, since both start there.
12. **A transition value added mid-flight jumps to its target.** In Compose, a `Transition.animateFloat` first composed while the transition runs starts at its value for `currentState` and animates to the target's.
    - compose-rs's `Transition::animate_value` remembers an `Animatable` at the target state's value (`transition.rs`), so it never animates.
    - Caught by: `TransitionTest.addNewAnimationInFlightTest`. It reads Compose's internal `playTimeNanos`; sample frames instead.
