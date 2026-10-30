# Parity ledger: animation and navigation

The inventory of Compose's animation and navigation tests and public API against compose-rs's `compose-animation` and `compose-navigation`, for the agents porting them.

## Scope

- **Reference:** the androidx checkout at `androidx-main/frameworks/support`, commit `d153d87ed76` (its tip). Every path below is relative to `frameworks/support`.
- **Compose modules:**
  - `compose/animation/animation-core` (Animatable, `animate*AsState`, Transition, specs, easing, converters, decay, infinite transitions);
  - `compose/animation/animation` (AnimatedVisibility, AnimatedContent, Crossfade, `animateContentSize`, enter/exit transitions, shared transitions, `animateBounds`, spline decay);
  - `compose/animation/animation-graphics` (animated vector drawables).
- **Back:** `navigationevent/navigationevent`, `navigationevent/navigationevent-compose` and `activity/activity-compose`'s `BackHandler` and `PredictiveBackHandler`, what Navigation 3's back goes through.
- **Navigation:** both libraries exist in the checkout, and **Navigation 3 is the one in scope** (D704, the user's decision):
  - Navigation Compose: `navigation/navigation-compose`, on top of `navigation-runtime`'s `NavController`: inventoried below, all `n/a: replaced by Navigation 3 (D704)`;
  - Navigation 3: `navigation3/navigation3-runtime` and `navigation3/navigation3-ui`.

  `crates/compose-navigation` is Navigation 3 (D704): the Navigation Compose model of D291 is gone.
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

**108 test files, 1,219 `@Test` methods.**

| Status | Files | @Test |
|---|---|---|
| `covered` | 0 | 0 |
| `ported` | 62 | 565 |
| `port` | 0 | 0 |
| `partial` | 9 | 116 |
| `feature:<name>` | 23 | 384 |
| `n/a` | 14 | 154 |

By module:

| Module | Files | @Test | covered | ported | port | partial | feature | n/a |
|---|---|---|---|---|---|---|---|---|
| animation-core | 30 | 212 | 0 | 26 / 181 | 0 | 3 / 19 | 1 / 12 | 0 |
| animation | 15 | 238 | 0 | 7 / 79 | 0 | 3 / 40 | 5 / 119 | 0 |
| animation-graphics | 5 | 22 | 0 | 0 | 0 | 0 | 5 / 22 | 0 |
| navigation-compose | 9 | 121 | 0 | 0 | 0 | 0 | 0 | 9 / 121 |
| navigation3-runtime | 21 | 307 | 0 | 12 / 89 | 0 | 0 | 8 / 211 | 1 / 7 |
| navigation3-ui | 15 | 121 | 0 | 9 / 53 | 0 | 2 / 48 | 4 / 20 | 0 |
| navigationevent, activity-compose's back | 13 | 198 | 0 | 8 / 163 | 0 | 1 / 9 | 0 | 4 / 26 |

`@Test` methods by missing feature (a file counts under its main blocker; the notes name the others):

| Feature | Files | @Test |
|---|---|---|
| nav3-deep-links | 8 | 211 |
| shared-transition | 3 | 87 |
| movable-content | 3 | 19 |
| animated-image-vector | 5 | 22 |
| lookahead-visual-debug | 1 | 16 |
| path-easing | 1 | 12 |
| captured-animated-visibility | 1 | 9 |
| animate-bounds | 1 | 8 |

Four parts:

- **Animation** (50 files, 472 tests): 33 files `ported` (260 tests), 6 `partial` (59), 11 waiting on features (153), of which what's left needs lookahead (shared-transition, animate-bounds, lookahead-visual-debug), the ui path iterator (path-easing), ui layer recording (captured-animated-visibility) or a full `ImageVector` (animated-image-vector).
- **Navigation 3** (36 files, 428 tests): 21 files `ported` (142 tests), 2 `partial` (48), 12 waiting on features (231: deep links, movable content, shared elements), 1 `n/a` (7). It's what compose-rs is to model (D704), a step of the order below, after S3.
- **Navigation events and back** (13 files, 198 tests), what Navigation 3's back is built on: 8 files `ported` (163 tests), 1 `partial` (9), 4 `n/a` (26), in `compose-navigation-event` (D845).
- **Navigation Compose** (9 files, 121 tests): all `n/a: replaced by Navigation 3 (D704)`. Their notes say which Navigation 3 tests check the same behaviour.

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
| 1 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt` | 9 | `Animatable`: animateTo (float, generic), interruption, animateDecay, updateBounds, IntSize bounds, toString of results | ported | `animation_objects.rs` and `interruption_aware.rs` (16 ms frames, not 50); the 2 `toString` tests are n/a (Rust `Debug`) |
| 2 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationTest.kt` | 6 | `TargetBasedAnimation` values/velocities for snap, keyframes, tween, spring; infinite repeatable velocity across repetitions | ported | `animation_objects.rs` (`testVectorizedInfiniteRepeatableSpec_velocityOnRepetitions` with a `DurationBasedSpec::custom`, D816) |
| 3 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationVectorTest.kt` | 3 | `AnimationVector1D..4D`: reset, hashCode, factory functions | partial | 2 ported (`animation_vector.rs`: `reset`, the factory); `testHashCode` n/a (`AnimationVector` holds floats: no `Hash`) |
| 4 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt` | 12 | `ArcAnimationSpec` (ArcAbove/Below/Linear) interpolation in 1-4 dimensions, equality, curve shape | ported | `arc_animation.rs`; the 3 curve plots in `arc_spline.rs`'s unit tests (D844) |
| 5 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DecayAnimationTest.kt` | 2 | `exponentialDecay` values and the velocity threshold | ported | `animation_objects.rs` |
| 6 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DeferredTargetAnimationTest.kt` | 1 | `DeferredTargetAnimation.updateTarget` value/idle/pendingTarget | ported | `deferred_target.rs` |
| 7 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DelayedAnimationTest.kt` | 5 | Compose's internal `delayed(spec, nanos)` wrapper: duration, values, velocity, zero, equality | partial | 4 ported (`animation.rs`'s unit tests, on `TargetBasedAnimation::delayed`: compose-rs delays the animation, not the spec); `comparison` n/a: no delayed specs to compare (D821) |
| 8 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DurationScaleTest.kt` | 1 | `animate()` honours `MotionDurationScale` (5x, and 0 = finish at once, infinite too) | ported | `duration_scale.rs` (16 ms frames, not 10) |
| 9 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/EasingUnitTest.kt` | 6 | `CubicBezierEasing`: 0 and 1 ends, overshoot, equality, small fractions | ported | `easing.rs` |
| 10 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/IsInfiniteTest.kt` | 8 | `isInfinite` of every vectorized spec, exponential decay and `DecayAnimation` | ported | `animation_objects.rs` |
| 11 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt` | 12 | keyframes: start/end override, easing per stretch, multi-dimensional, equality, `atFraction`, out-of-range times | ported | `keyframe_animation.rs`; `testNotEquals0` inverted there: keyframes built alike are equal (D816) |
| 12 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeArcAnimationTest.kt` | 2 | keyframes `using ArcMode` in 2D, with easings | ported | `arc_animation.rs` |
| 13 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt` | 8 | `keyframesWithSpline`: interpolation, periodic bias, easing, delay, equality | ported | `keyframe_spline_animation.rs`; `testNotEquals0`'s specs built alike are equal (D816, D843) |
| 14 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/MonoSplineTest.kt` | 2 | the monotonic spline under `keyframesWithSpline` | ported | unit tests of `mono_spline.rs`, an internal class, with its text plot |
| 15 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt` | 11 | springs: velocity, under/over/critically damped durations, end snapping, vector springs, zero damping, interruption-aware spec | ported | `physics.rs`, `interruption_aware.rs` |
| 16 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt` | 11 | repeatable/infiniteRepeatable values, durations, Reverse, StartOffset, zero-duration inner specs | partial | 9 ported (`repeatable_animation.rs`); the zero-duration spline (keyframes-with-spline) and arc (arc-animation) specs need their features |
| 17 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SnapAnimationTest.kt` | 1 | `snap()` is finished at play time 0 | ported | `easing.rs` |
| 18 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SuspendAnimationTest.kt` | 5 | top-level `animate()` (float, generic), `animateDecay`, `AnimationState.animateTo/animateDecay` | ported | `suspend_animation.rs` (16 ms frames, not 50) |
| 19 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/TweenAnimationTest.kt` | 3 | tween delay, custom easing, value past the duration | ported | `easing.rs` (the custom easing an `Easing::Function`) |
| 20 | `compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/TypeConverterTest.kt` | 2 | Float and Int vector converters | ported | `converters.rs`, with fractions: Compose's `Int` converter truncates, as compose-rs's now does |
| 21 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt` | 8 | `DeferredTransitionState`: defer/animateTo, pending state, child transitions, interruption | ported | `deferred_transition.rs` (the 2 child tests in one; D828) |
| 22 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DurationScaleTransitionTest.kt` | 3 | child transitions (plain, seekable, seeking) under `MotionDurationScale` | ported | `duration_scale.rs`, the rule's `set_motion_duration_scale` for `effectContext` (D827) |
| 23 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/EasingTest.kt` | 1 | cubic Bezier evaluation matches a drawn path (bitmap comparison) | ported | `easing.rs`: the evaluation against the curve sampled by its parameter, not bitmaps |
| 24 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/InfiniteTransitionTest.kt` | 1 | `rememberInfiniteTransition`: keyframes Reverse float and tween color, frame by frame | ported | `infinite_transition.rs`, through 2.1 s of frames (Compose's loop compares nanoseconds with 2100: it checks one frame) |
| 25 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt` | 12 | `PathEasing`: bounds, invalid paths, compare to cubic, quads, overshoot | `feature:path-easing` |  |
| 26 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt` | 44 | `SeekableTransitionState`: seekTo, animateTo, snapTo, interruptions, durations, isRunning, cleanup | ported | `seekable.rs` (41) and `seekable.rs`'s unit tests (3: the two tests giving `animate_to` a frame clock of its own check those frames against the fraction's animation; `testCleanupAfterDispose` the observer of the total duration); `animateAfterSeekToZero` checks the content's states and display, not its pixels; a call can end a frame earlier (D817, D821) |
| 27 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt` | 13 | `animate*AsState`: 1D/2D/4D/color frame by frame, interruption, visibility thresholds, spec and threshold updates | ported | `single_value.rs`, `interruption_aware.rs` |
| 28 | `compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt` | 17 | `updateTransition`/`Transition`: values per segment, animations added mid-flight, MutableTransitionState, child transitions, totalDurationNanos, recomposition counts, interruption-aware spec | ported | `transition.rs`, `interruption_aware.rs` (`interruptionAwareAnimationSpec_preservedOnInterruption` with keyframes for its spec of its own), `seekable.rs` (`childTransitionStartsUninterrupted_usingSeekableTransition`) |
| 29 | `compose/animation/animation-core/src/androidHostTest/kotlin/androidx/compose/animation/core/EasingTest.android.kt` | 2 | cubic solver near 0/1 and with overshooting curves | ported | `easing.rs` |
| 30 | `compose/animation/animation-core/src/androidHostTest/kotlin/androidx/compose/animation/core/SpringEstimationTest.kt` | 1 | spring duration estimation against simulation over a grid of mass/stiffness | ported | `spring_estimation.rs`: every fifth mass and spring constant of Compose's grid (2.9 million of its 70 million cases) |
| 31 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimateBoundsTest.kt` | 8 | `Modifier.animateBounds`: position, size, bounds, movable content, lazy layouts, scrolling | `feature:animate-bounds` | needs ui lookahead |
| 32 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt` | 17 | `AnimatedContent`: SizeTransform, alignment, slideInto/OutOfContainer, contentKey, subcomposition, interruption, KeepUntilTransitionsFinished, lookahead, cleanup | partial | 12 ported (`animated_content.rs`: the size transform frame by frame, to and from an empty content, the alignment, an exit finished then interrupted, a transition recreated; `animated_content_scope.rs`: slides into and out of the container, `contentKey` (2), `KeepUntilTransitionsFinished` (2), the enter and exit an interruption picks, the last with a wait one frame later (D817); `compose-material3/tests/suite/animated_content.rs`: a scaffold with tabs); lookahead and `animateBounds` (3); n/a: `AnimatedContentWithInterruption` (the test ends as it sets the flag, so its layout-callback assertions never see the interruption, under which Compose's content slides too) and `testAnimatedContentCleanup` (an internal map's size) |
| 33 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt` | 17 | `AnimatedVisibility`: expand/shrink, slide, fade, scale with exact values, None, Transition overload, seeking, lookahead, veil, settle time | partial | 15 ported (`animated_visibility.rs`: expand and shrink, slide, a content size change, fade, scale, the scale spring's threshold, `None`/`None`, the `Transition` overload, an interrupted exit, the settle time, with a test of the column and row defaults; `veil.rs`: the five veil tests); seeking (1) seekable-transition; lookahead (1) |
| 34 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt` | 6 | `animateContentSize`: frame-by-frame size, alignment LTR/RTL, lookahead, inspector, re-attach | partial | 3 ported (`content_size.rs`: the frame-by-frame size with its finished listener, alignment left to right and right to left); `properFinalStateAfterReAttach` needs movable content (compose-runtime has no `movableContentOf`); lookahead (1) needs lookahead; inspector n/a |
| 35 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CapturedAnimatedVisibilityTest.kt` | 9 | `CapturedAnimatedVisibility`: content disposed at once, exit animates a captured layer, MutableTransitionState, interruptions, lookahead | `feature:captured-animated-visibility` | needs ui graphics-layer recording |
| 36 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CrossfadeTest.kt` | 6 | `Crossfade`: shows content, disposes old content, custom spec, null state, rememberSaveable per screen, contentKey | ported | `crossfade_ported.rs` (`crossfadeTest_contentKey` with a wait one frame later, D817) |
| 37 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt` | 29 | `DeferredAnimatedContent`: gates, preview transforms, hand-off velocity, interruption specs, veil | ported | `deferred_content.rs` (`immediateExitTest` and `verifyExitAnimationAfterGateOpens` in one; the veil's pixel from the display list; D828; widths through `on_globally_positioned`, as Compose's, since D1187) |
| 38 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt` | 21 | `DeferredAnimatedVisibility`: enter/exit gates, preview scale/offset, hand-off, interruption | ported | `deferred_visibility.rs` (D828; widths through `on_globally_positioned`, as Compose's, since D1187) |
| 39 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredSharedElementTest.kt` | 15 | shared elements under deferred transitions: previews, hand-off, cancellation | `feature:shared-transition` | deferred-transition is done (D828) |
| 40 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/LookaheadAnimationVisualDebugHelperTest.kt` | 16 | debug overlays for shared elements and animateBounds | `feature:lookahead-visual-debug` | experimental (`@ExperimentalLookaheadAnimationVisualDebugApi`) |
| 41 | `compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/SharedTransitionTest.kt` | 71 | `SharedTransitionLayout`, `sharedElement`, `sharedBounds`, overlay, clip, placeholder size, scale-to-bounds, lazy lists, isTransitionActive, velocity | `feature:shared-transition` | needs ui lookahead |
| 42 | `compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/AndroidFlingSplineTest.kt` | 1 | the fling spline clamps positions out of range | ported | `spline_decay.rs`, with flings checked against values computed in Kotlin's numeric semantics |
| 43 | `compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/ConverterTest.kt` | 5 | Color (Oklab, clamping), Rect, Dp, Offset converters | ported | `converters.rs` (the color clamp included: compose-rs clamps in Oklab first, as Compose's) |
| 44 | `compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt` | 13 | `EnterTransition/ExitTransition.config`: fade, slide, change size, scale, veil, combined; SizeTransform defaults | ported | `enter_exit_config.rs` (the offset and size functions checked by what they give) |
| 45 | `compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/SharedMutableTransformStateTest.kt` | 4 | internal state machine of mutable transforms | ported | `deferred.rs`'s unit tests (D828) |
| 46 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/res/AnimatedVectorPainterResourcesTest.kt` | 2 | `rememberAnimatedVectorPainter` from an AVD resource: config, duplicate targets | `feature:animated-image-vector` | resources become bundled bytes in compose-rs |
| 47 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/vector/AnimatorAnimationSpecsTest.kt` | 5 | reversed and combined specs used by vector animators | `feature:animated-image-vector` | internal specs |
| 48 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/vector/AnimatorTest.kt` | 10 | object animators, keyframes, sets, path data morphing, delays, repeats | `feature:animated-image-vector` |  |
| 49 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/vector/compat/XmlAnimatedVectorParserTest.kt` | 1 | parsing an animated-vector XML | `feature:animated-image-vector` | needs an XML parser; compose-rs parses no vector XML today |
| 50 | `compose/animation/animation-graphics/src/androidDeviceTest/kotlin/androidx/compose/animation/graphics/vector/compat/XmlAnimatorParserTest.kt` | 4 | parsing objectAnimator/set/propertyValuesHolder XML | `feature:animated-image-vector` |  |
| 51 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/ComposeNavigatorTest.kt` | 1 | navigating and popping updates the navigator's back stack | n/a: replaced by Navigation 3 (D704) | `nav_back_stack.rs` and `nav_stack_state.rs` check the same stack changes |
| 52 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/DialogNavigatorTest.kt` | 10 | dialog destinations: shown, dismissed on pop, nested NavHost, transition completion order | n/a: replaced by Navigation 3 (D704) | `NavDisplayTest` (dialogs), the scene tests and `NavDisplayLifecycleTest` take over |
| 53 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavBackStackEntryProviderTest.kt` | 5 | an entry provides ViewModelStore/Lifecycle/SavedStateRegistry owners; saveable values saved, plain ones not | n/a: replaced by Navigation 3 (D704) | the entry decorators of nav3-runtime (`DecoratedNavEntriesTest`, per `contentKey`), with `lifecycle-viewmodel-navigation3` |
| 54 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavGraphBuilderTest.kt` | 33 | graph builder: composable/dialog/navigation with routes, KClass and objects, nested graphs, default args, deep links | n/a: replaced by Navigation 3 (D704) | no graphs in Navigation 3: the back stack is the app's list of keys, `entryProvider` maps them |
| 55 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostControllerTest.kt` | 27 | `rememberNavController`, current entry, navigate/pop, singleTop, KClass routes with args, dialogs, getBackStackEntry | n/a: replaced by Navigation 3 (D704) | the app owns the back stack: no controller, no `NavOptions` |
| 56 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostPredictiveBackTest.kt` | 2 | NavHost animations driven by a predictive back gesture; disabling in the same frame | n/a: replaced by Navigation 3 (D704) | `NavDisplayPredictiveBackTest` takes over (`feature:nav3-ui`, also predictive-back) |
| 57 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostPreviewTest.kt` | 2 | NavHost renders in an Android Studio preview (no lifecycle owner) | n/a: replaced by Navigation 3 (D704) | also Android Studio's preview (`LocalInspectionMode`), no analogue |
| 58 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostScreenShotTest.kt` | 5 | z-order of entering/leaving entries, predictive back frames, custom transitions, size transform (screenshots) | n/a: replaced by Navigation 3 (D704) | `NavDisplayScreenshotTest` takes over |
| 59 | `navigation/navigation-compose/src/androidDeviceTest/kotlin/androidx/navigation/compose/NavHostTest.kt` | 36 | NavHost: navigate, pop, state saving/restoring, dialogs, ViewModels, graphs, animations, lifecycles, nested hosts, back handlers | n/a: replaced by Navigation 3 (D704) | `NavDisplayTest`, `AnimatedTest` and `NavDisplayLifecycleTest` take over |
| 60 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/DecoratedNavEntriesTest.kt` | 42 | `rememberDecoratedNavEntries`: decorators called once/in order, onPop, re-decoration, state per contentKey, multiple back stacks | ported | `nav_decorated_entries.rs`; `decorator_calledOnce` shows the entry, as its decorator there doesn't and so can't tell (and a popped entry still shown, and a reordered stack, are its own) |
| 61 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavBackStackSerializerTest.kt` | 5 | `NavBackStack` kotlinx.serialization round trips | ported | `nav_back_stack.rs`, through `Saveable` and bytes; `encode_withCustomModuleAndMissingSubtype` has no analogue (D1400) |
| 62 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavEntryTest.kt` | 2 | `NavEntry.contentKey` for objects and data classes | ported | `nav_entry.rs` |
| 63 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/RememberNavBackStackTest.kt` | 10 | `rememberNavBackStack` restores objects, classes, sealed and polymorphic keys | ported | `nav_back_stack.rs`; `noSerializerFail`, `configurationRestore` and `defaultConfigurationFails` have no analogue (D1400), and a key is saved as it is when it's saved, Rust's not being changed behind the stack |
| 64 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/SaveableStateHolderNavEntryDecoratorTest.kt` | 1 | equal data-class keys don't duplicate saveable state | ported | `nav_entry.rs` |
| 65 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusAndroidTest.kt` | 2 | `ResultEventBus.conflateAsState` | ported | `nav_results.rs` |
| 66 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusNavEntryDecoratorTest.kt` | 5 | results between entries and dialogs through the decorator | ported | `nav_results.rs` |
| 67 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt` | 40 | decoding URI arguments into keys (all primitive types, enums, nesting, errors) | `feature:nav3-deep-links` | optional: kotlinx.serialization decoding; a Rust port would use its own decoding trait |
| 68 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt` | 11 | matcher filters and ordering of match results | `feature:nav3-deep-links` |  |
| 69 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.android.kt` | 7 | `DeepLinkRequest` from an Android `Intent` (uri, mime type, action, extras) | n/a: Android `Intent`s |  |
| 70 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkSerializerTest.kt` | 4 | `DeepLinkSerializer` custom string encodings | `feature:nav3-deep-links` |  |
| 71 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt` | 69 | URI pattern matching: paths, queries, fragments, wildcards, exactness, ranking | `feature:nav3-deep-links` |  |
| 72 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt` | 70 | URI pattern parsing into path/query/fragment arguments | `feature:nav3-deep-links` |  |
| 73 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WithBackStackTest.kt` | 5 | `withBackStack` builds a synthetic back stack for a deep link | `feature:nav3-deep-links` |  |
| 74 | `navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WrappedMatchResultTest.kt` | 4 | wrapped match results compare like the wrapped one | `feature:nav3-deep-links` |  |
| 75 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryProviderTest.kt` | 9 | `entryProvider` DSL: entries by key/class, metadata, duplicates, fallback | ported | `nav_entry.rs`; `Debug` prints a key, so a string in a message has its quotes |
| 76 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/EntryTest.kt` | 2 | `NavEntry` key and metadata | ported | `nav_entry.rs` |
| 77 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/NavMetadataTest.kt` | 8 | typed `NavMetadataKey`s: object/class keys, lambdas, override, plus, contains | ported | `nav_entry.rs` |
| 78 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt` | 8 | `DeepLinkRequest` URIs and `RequestExtras` | `feature:nav3-deep-links` |  |
| 79 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/result/ResultEffectTest.kt` | 1 | `ResultEffect` sees the latest callback | ported | `nav_results.rs` |
| 80 | `navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusTest.kt` | 2 | `ResultEventBus` results flow and removal | ported | `nav_results.rs`, by what's heard: `channelMap` isn't public |
| 81 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/NavDisplayInfoTest.kt` | 1 | a predictive back swipe fills `SceneInfo` for the navigation event | ported | `nav_scene.rs` |
| 82 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/NavigationBackHandlerTest.kt` | 3 | `NavigationBackHandler`: single pop, multi pop, cancelled back | ported | `nav_scene.rs` |
| 83 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/OverlaySceneTest.kt` | 1 | shared transition with a dialog scene | `feature:shared-transition` | | |
| 84 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/RememberNavigationEventStateTest.kt` | 1 | `rememberNavigationEventState` follows the scene state | ported | `nav_scene.rs` |
| 85 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/SceneStateTest.kt` | 8 | `rememberSceneState`: scene changes, previous scenes, strategy chaining, recalculation | ported | `nav_scene.rs`; `testSceneStrategyThen*` are of a deprecated operator, and a list of strategies is `nav_display.rs`'s |
| 86 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/AnimateOverlaySceneTest.kt` | 2 | overlay scene exit animation, nested bottom sheet | ported | `nav_scene.rs`, with an overlay leaving by a delay for Material's sheet |
| 87 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/BottomNavSceneDecoratorTest.kt` | 7 | a bottom-nav scene decorator with dialogs and list-detail | `feature:movable-content` | an entry moving between panes keeps its state (D1404) |
| 88 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/HierarchicalSceneTest.kt` | 7 | multi-column hierarchical scene: content, back, state, predictive back | `feature:movable-content` | the same (D1404) |
| 89 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/ListDetailSceneTest.kt` | 5 | list-detail scene: second detail, replace, full screen, back | `feature:movable-content` | the same (D1404) |
| 90 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt` | 26 | NavDisplay transitions: navigate/pop/interrupted, custom specs, centre pops, duplicates, shared elements, lifecycle | partial | `nav_animated.rs` and `nav_card_stack.rs`, 25 of 26: `testSharedElement` waits for shared elements |
| 91 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayLifecycleTest.kt` | 8 | entry lifecycle order across navigation, panes, dialogs, overlays | ported | `nav_lifecycle.rs` |
| 92 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayPredictiveBackTest.kt` | 5 | state restored after predictive back, z-index, back during forward animation, hand-off | ported | `nav_predictive_back.rs` |
| 93 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt` | 22 | z-order and frames of predictive back, pops, duplicates, scenes (screenshots) | partial | 21 of 22: `nav_z_index.rs`, the 9 z-index tests by clicks on the overlap, and 4 of scenes and a swapped stack by bounds (D1410); `nav_predictive_back.rs`, its 3 frames of a gesture, and `nav_card_stack.rs`, its 5 of a scene with its own transitions, by bounds (D1411); the default fade and scale by the display list. `testSharedElementCorrectlyDisplayedOnPredictiveBack` waits for shared elements |
| 94 | `navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt` | 21 | NavDisplay: content, dialogs, back, saved state per entry, back stack swaps, duplicates, multi-pane | ported | `nav_display.rs` |
| 95 | `navigation3/navigation3-ui/src/commonTest/kotlin/androidx/navigation3/scene/SceneInfoTest.kt` | 4 | `SceneInfo` title/url metadata | ported | `nav_scene.rs` |
| 96 | `navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt` | 102 | the dispatcher: gestures, priorities, trees, fallbacks, disposal, transition state and history | ported | `dispatcher.rs` of `compose-navigation-event`'s suite |
| 97 | `navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt` | 10 | an input with no dispatcher, and what it hears | ported | `input.rs` |
| 98 | `navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/DirectNavigationEventInputTest.kt` | 9 | a direct input's calls | partial | 8 ported, `input.rs`; `navigationMethod_ifNameChanged_shouldFail` checks signatures by reflection: n/a |
| 99 | `navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventHistoryTest.kt` | 7 | the history's merged places, equality, printing | ported | `input.rs`; its hash isn't there, an info being any value with `==` |
| 100 | `navigationevent/navigationevent/src/androidDeviceTest/kotlin/androidx/navigationevent/NavigationEventTest.kt` | 4 | `BackEvent` to `NavigationEvent` and back | n/a: `Back.kt` passes the fields on; checked on the Pixel (D845) |  |
| 101 | `navigationevent/navigationevent/src/androidDeviceTest/kotlin/androidx/navigationevent/OnBackInvokedInputTest.kt` | 14 | `OnBackInvokedInput` registering and dispatching | n/a: `Back.kt`'s `PlatformBack`, in Kotlin; checked on the Pixel (D845) |  |
| 102 | `navigationevent/navigationevent/src/androidDeviceTest/kotlin/androidx/navigationevent/ViewTreeNavigationEventDispatcherTest.kt` | 6 | the owner on Android's view tree | n/a: a host provides its dispatcher |  |
| 103 | `navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt` | 10 | the composable handler | ported | `composition.rs` |
| 104 | `navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt` | 9 | `rememberNavigationEventDispatcherOwner` | ported | `composition.rs` |
| 105 | `navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/RememberNavigationEventStateTest.kt` | 3 | `rememberNavigationEventState` | ported | `composition.rs` |
| 106 | `navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/LocalNavigationEventDispatcherOwnerTest.kt` | 2 | the local's fallback to the context and the view tree | n/a: a host provides its dispatcher |  |
| 107 | `activity/activity-compose/src/androidTest/java/androidx/activity/compose/BackHandlerTest.kt` | 4 | `BackHandler` | ported | `predictive_back.rs` |
| 108 | `activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt` | 18 | `PredictiveBackHandler`: start, progress, completion, cancellation, order, lifecycle | ported | `predictive_back.rs`; a task's cancelled wait as D845 says, its task's end as D847 |

## Public API surface

What compose-rs lacks or only partly has, from each `api/current.txt`, non-deprecated, experimental included. Each item names its feature. Rust-shaped equivalents count as present:

- a `TwoWayConverter` is the `Animate` trait;
- `AnimationVector1D` to `AnimationVector4D` are one `AnimationVector` of one to four components;
- a cancellation is `AnimationEndReason::Interrupted`;
- default arguments are `#[default]` parameters or `_with` forms.

Deprecated and left out:

- `defaultDecayAnimationSpec()`;
- `KeyframeEntity.with(easing)`, replaced by `using`;
- `updateTransition(MutableTransitionState)`;
- the hidden overloads of `rememberSceneState`, `SceneStrategy.then` and two `NavDisplay`s.

### What compose-rs already has (for reference)

**animation-core:**

- `Animatable`: `new`, `new_with(initial, visibility_threshold, label)`, `value`, `as_state`, `target_value`, `velocity`, `velocity_vector`, `is_running`, `label`, `lower_bound`/`upper_bound`/`update_bounds`, `animate_to`, `animate_to_with_velocity`, `animate_decay`, `snap_to`, `stop`.
- `AnimationResult`, with `end_value` and `end_velocity` in place of `endState` (D825), and `AnimationEndReason` (`Finished`, `BoundReached`, and `Interrupted` for a cancellation).
- `animate_{value,float,dp,color,int,offset,int_offset,size,int_size,rect}_as_state`, with `label`, `finished_listener` and (on `animate_value_as_state`) `visibility_threshold`.
- The specs and constants:
  - `tween` (`.delay`, `.easing`), `spring`/`spring_with` (`.visibility_threshold`), `snap(delay)`, `repeatable`/`infinite_repeatable` (their `_with` forms take a `StartOffset`), `RepeatMode`, `InterruptionAwareAnimationSpec`;
  - `keyframes(|f| { f.duration_millis = …; f.delay_millis = …; f.at(t, v).using(easing); f.at_fraction(fraction, v); })`;
  - `Spring::*` constants, `DEFAULT_DURATION_MILLIS`, `AnimationConstants`;
  - every spec a `VectorizedAnimationSpec`; `FloatAnimationSpec` (`FloatTweenSpec`, `FloatSpringSpec`); the decays (`exponential_decay`, `FloatExponentialDecaySpec`, `generate_decay_animation_spec`).
- `Animation`, `TargetBasedAnimation`, `DecayAnimation`, `AnimationVector`.
- `Easing::{Linear, CubicBezier, Function}`, with the standard curves, `EASE`, `EASE_IN`, `EASE_OUT`, `EASE_IN_OUT` and Compose's thirty `EasingFunctions` curves.
- `remember_infinite_transition` with `animate_value/float/color`.
- `update_transition` and `remember_transition(&MutableTransitionState)`, with `current_state`, `target_state`, `segment`, `is_running`, `label`, `total_duration_nanos`, `create_child_transition` and `animate_value/float/dp/color/offset/size/int/int_offset/int_size/rect`.
- `DeferredTargetAnimation`.
- `DeferredTransitionState`, `DeferredTransition`, `remember_deferred_transition` (D828); the motion duration scale (`compose_ui::motion_duration_scale`, `with_motion_duration_scale`, `set_motion_duration_scale`, D827).
- `Animate` for `f32`, `i32`, `Dp`, `DpOffset`, `Offset`, `IntOffset`, `Size`, `IntSize`, `Rect`, `Color` (Oklab) and `AnimationVector`; the `VisibilityThreshold` constants.

**animation:**

- `AnimatedVisibility(visible, modifier, enter, exit, label, content)`, `visible` a `bool`, a `MutableTransitionState<bool>`, or either with the `ColumnScope` or `RowScope` it's in; `transition.AnimatedVisibility(visible, modifier, enter, exit, content)`; the content gets an `AnimatedVisibilityScope`.
- `EnterTransition`/`ExitTransition` with `+`, `NONE` and `config()` (`EnterExitTransitionConfig`, `FadeConfig`, `SlideConfig`, `ChangeSizeConfig`, `ScaleConfig`).
- The transitions, with Compose's parameters in their `_with` forms: `fade_in`/`fade_out`, `scale_in`/`scale_out`, `expand_{in,horizontally,vertically}`/`shrink_{out,horizontally,vertically}`, `slide_in`/`slide_out` and their horizontal and vertical forms.
- `AnimatedContent(target_state, modifier, transition_spec(scope), content_alignment, label, content_key, content(scope, state))` and `transition.AnimatedContent(modifier, transition_spec, content_alignment, content_key, content)`, the scopes `AnimatedContentTransitionScope` (`slide_into_container`, `slide_out_of_container`, `SlideDirection`) and `AnimatedContentScope`, `ExitTransition::KEEP_UNTIL_TRANSITIONS_FINISHED`, `transition.Crossfade(…)`, with `ContentTransform` (`together_with`, `using`, `target_content_z_index`), `size_transform(clip, spec)` and `default_content_transform()`.
- `Crossfade(target_state, modifier, animation_spec, label, content)`.
- `Modifier.animate_content_size()`/`animate_content_size_with(spec, alignment, finished_listener)`.
- `DeferredAnimatedVisibility`, `DeferredAnimatedContent`, `MutableTransform`, `MutableContentTransform`, `TransformScope` (D828); `BoundsTransform`, `SharedTransitionDefaults`, `PlaceholderSize`, `ResizeMode`, `SharedContentConfig` (D829).

**navigation:** [Navigation Compose](#navigation-compose-androidxnavigationcompose-and-navcontroller), until [Navigation 3](#navigation-3) replaces it.

### animation-core (`androidx.compose.animation.core`)

| API | compose-rs | Feature |
|---|---|---|
| `Animatable(initialValue, typeConverter, visibilityThreshold, label)`; `Animatable(Float, visibilityThreshold)` | present: `Animatable::new_with(initial, visibility_threshold, label)`, the converter being the type's `Animate` (D800) | animatable-bounds |
| `Animatable.updateBounds`, `lowerBound`, `upperBound`, `AnimationEndReason.BoundReached` | present (D800) | animatable-bounds |
| `Animatable.label`, `typeConverter`, `velocityVector` | present (`label`, `velocity_vector`; the converter is the type's `Animate`) | animatable-bounds |
| `Animatable.animateTo(…, block)`: the per-frame callback on `Animatable` | present (`animate_to_with`, `animate_decay_with`, D819) | suspend-animate |
| `Animatable.animateDecay(initialVelocity, DecayAnimationSpec, block)` | present without the block: `animate_decay(velocity, &spec)` (D807) | decay-animation |
| `AnimationResult.endState: AnimationState` | `end_value` and `end_velocity` (D819, D825), not the `AnimationState` itself | suspend-animate |
| `animateIntSizeAsState`, `animateRectAsState`; `animateValueAsState(…, visibilityThreshold, …)` | present (D800) | more-converters |
| `Animation` interface (`durationNanos`, `getValueFromNanos`, `getVelocityVectorFromNanos`, `isFinishedFromNanos`, `isInfinite`, `targetValue`, `typeConverter`); `TargetBasedAnimation`; `Animation.getVelocityFromNanos` | present: the `Animation` trait, `TargetBasedAnimation` (D800) | animation-objects |
| `AnimationSpec.vectorize`, `FiniteAnimationSpec`, `DurationBasedAnimationSpec` | the `AnimationSpec` enum and the spec types are each a `VectorizedAnimationSpec` (no separate vectorized types); `DurationBasedSpec` holds tween, keyframes and snap. `AnimationSpec::custom` and `DurationBasedSpec::custom` take specs of an app's own (D816) | animation-objects, spec-gaps |
| `VectorizedAnimationSpec`, `VectorizedFiniteAnimationSpec`, `VectorizedDurationBasedAnimationSpec`, `VectorizedTweenSpec`, `VectorizedSpringSpec`, `VectorizedSnapSpec`, `VectorizedKeyframesSpec`, `VectorizedRepeatableSpec`, `VectorizedInfiniteRepeatableSpec`, `VectorizedFloatAnimationSpec` (`getDurationNanos`, `getValueFromNanos`, `getVelocityFromNanos`, `getEndVelocity`, `isInfinite`) | present as one `VectorizedAnimationSpec` trait the spec types implement (D800) | animation-objects |
| `FloatAnimationSpec`, `FloatTweenSpec`, `FloatSpringSpec` | present (D800) | animation-objects |
| `AnimationVector`, `AnimationVector1D..4D`, `AnimationVector(…)` factories | one `AnimationVector` of one to four components (no 1D..4D types) | animation-objects |
| `AnimationConstants.UnspecifiedTime` | present (D800) | animation-objects |
| `SpringSpec` duration (Compose's spring estimation), and ending an animation at that duration | present (D800) | spring-estimation |
| `AnimationState` (ctor, `copy`, `isFinished`, `createZeroVectorFrom`), `AnimationScope` (`cancelAnimation`, times, value, velocity), `animate(…)` (float and generic), `AnimationState.animateTo`, `animateDecay(…)` (float and on `AnimationState`), `sequentialAnimation` | present (`animate`, `animate_value`, `animate_decay`, `AnimationState`, `AnimationScope`, D819) | suspend-animate (decay forms: decay-animation) |
| `DecayAnimationSpec`, `FloatDecayAnimationSpec`, `VectorizedDecayAnimationSpec`, `exponentialDecay`, `FloatExponentialDecaySpec`, `generateDecayAnimationSpec`, `calculateTargetValue` (both), `DecayAnimation` (3 ctors), `DecayAnimation(FloatDecayAnimationSpec, …)` | present (D807) | decay-animation |
| `ArcAnimationSpec` (experimental), `ArcMode` (`ArcAbove`, `ArcBelow`, `ArcLinear`), `KeyframesSpecConfig.using(entity, ArcMode)` | present: `ArcAnimationSpec::new`, `ArcMode`, `using_arc` (D844) | arc-animation |
| `KeyframesSpecBaseConfig.atFraction`, `using(entity, easing)`, `durationMillis` settable in the builder | present (`at_fraction`, `using`, `duration_millis` settable) | spec-gaps |
| `KeyframesWithSplineSpec`, `keyframesWithSpline(init)`, `keyframesWithSpline(periodicBias, init)` | present: `keyframes_with_spline`, `periodic_keyframes_with_spline`, `KeyframesWithSplineBuilder` (D843, D846) | keyframes-with-spline |
| `StartOffset`, `StartOffsetType` (`Delay`, `FastForward`); `initialStartOffset` on `repeatable`, `infiniteRepeatable`, `RepeatableSpec`, `InfiniteRepeatableSpec` | present (`repeatable_with`, `infinite_repeatable_with`) | spec-gaps |
| `SnapSpec` as a `DurationBasedAnimationSpec` (so `repeatable(snap())` works) | present (D800) | spec-gaps |
| `InterruptionAwareAnimationSpec(animationSpec, interruptionSpec)` | present (D809) | interruption-aware-spec |
| `Easing` as a `fun interface` (any object, capturing lambdas) | present: `Easing::custom(closure)` (D816) | easing-functions |
| `EaseInBack`, `EaseInBounce`, `EaseInCirc`, `EaseInCubic`, `EaseInElastic`, `EaseInExpo`, `EaseInOutBack`, `EaseInOutBounce`, `EaseInOutCirc`, `EaseInOutCubic`, `EaseInOutElastic`, `EaseInOutExpo`, `EaseInOutQuad`, `EaseInOutQuart`, `EaseInOutQuint`, `EaseInOutSine`, `EaseInQuad`, `EaseInQuart`, `EaseInQuint`, `EaseInSine`, `EaseOutBack`, `EaseOutBounce`, `EaseOutCirc`, `EaseOutCubic`, `EaseOutElastic`, `EaseOutExpo`, `EaseOutQuad`, `EaseOutQuart`, `EaseOutQuint`, `EaseOutSine` | present (D808) | easing-functions |
| `PathEasing(path)` | missing | path-easing |
| `TransitionState`, `MutableTransitionState` (`currentState`, `targetState`, `isIdle`), `rememberTransition(transitionState, label)` | present (`TransitionState` a trait of `MutableTransitionState` and `SeekableTransitionState`, D821; `DeferredTransitionState` too, D828) | transition-state |
| `Transition.segment`, `Transition.Segment` (`initialState`, `targetState`, `isTransitioningTo`), `pendingTargetState`, `label`, `animations`, `transitions`, `totalDurationNanos`, `Transition.TransitionAnimationState` (`animation`, `animationSpec`, `label`, `typeConverter`) | present (`animations`, `transitions`: `TransitionAnimation`, `AnyTransition`, D813; `pending_target_state`, D828) | transition-state |
| `Transition.createChildTransition(label, transform)` | present | transition-state |
| `Transition.animateInt`, `animateIntOffset`, `animateIntSize`, `animateRect`, `animateSize` (and `label` on all `animate*`) | present (the labels in `_with` forms, D818) | transition-state, more-converters |
| `SeekableTransitionState` (`seekTo`, `animateTo(spec)`, `snapTo`, `fraction`, `currentState`, `targetState`) | present (an interrupted call cancels its task, as Compose's `CancellationException`, D823) | seekable-transition |
| `DeferredTransitionState` (`defer`, `animateTo`, `pendingTargetState`), `DeferredTransition`, `rememberDeferredTransition` | present (D828) | deferred-transition |
| `DeferredTargetAnimation` (`updateTarget`, `value`, `pendingTarget`, `isIdle`) | present (D810) | deferred-target-animation |
| `InfiniteTransition.animations`, `label`, `InfiniteTransition.TransitionAnimationState` (`animation`, `animationSpec`, `label`, `typeConverter`); `label` on `animateFloat`/`animateValue` | present (`animations()`, `label()`, `animate_*_with(…, label)`, D818) | transition-state |
| `withInfiniteAnimationFrameNanos`, `withInfiniteAnimationFrameMillis` (and the `InfiniteAnimationPolicy` they consult, D901); `MotionDurationScale` honoured by `Animatable`, `animate`, transitions and infinite transitions | present (`motion_duration_scale`, `with_motion_duration_scale`, `set_motion_duration_scale`, D827) | motion-duration-scale |
| `Rect.VectorConverter`, `IntSize.VectorConverter` (clamped at 0), `DpOffset.VectorConverter`; `TwoWayConverter(to, from)` factory | present (`Animate` impls; compose-ui has `DpOffset`); `TwoWayConverter(to, from)` is an `Animate` impl | more-converters |
| `VisibilityThreshold` for `Int`, `IntOffset`, `IntSize`, `Offset`, `Size`, `Rect`, `Dp`, `DpOffset` | present: the `VisibilityThreshold` constants, Compose's values (D800) | more-converters |

### animation (`androidx.compose.animation`)

| API | compose-rs | Feature |
|---|---|---|
| `fadeIn`/`fadeOut` | present | |
| `expandIn(spec, expandFrom, clip, initialSize)`, `expandHorizontally(spec, expandFrom, clip, initialWidth)`, `expandVertically(spec, expandFrom, clip, initialHeight)`, `shrinkOut`, `shrinkHorizontally`, `shrinkVertically` with their parameters | present (`_with` forms, D804) | enter-exit-params |
| `slideIn(spec, initialOffset: (IntSize) -> IntOffset)`, `slideOut`, `slideInHorizontally(spec, initialOffsetX)`, `slideOutHorizontally`, `slideInVertically(spec, initialOffsetY)`, `slideOutVertically` | present (`_with` forms, D804) | enter-exit-params |
| `scaleIn(spec, initialScale, transformOrigin)`, `scaleOut(…, transformOrigin)` | present (`_with` forms, D804) | enter-exit-params |
| `EnterTransition.config`, `ExitTransition.config`, `EnterExitTransitionConfig`, `FadeConfig`, `SlideConfig`, `ChangeSizeConfig`, `ScaleConfig` | present (D804) | enter-exit-params |
| `unveilIn(spec, initialColor, matchParentSize)`, `veilOut(…)`, `VeilConfig` | present (`_with` forms, D814); `matchParentSize` draws over the content's bounds | veil |
| `AnimatedVisibility(MutableTransitionState<Boolean>, …)`, `Transition<T>.AnimatedVisibility(visible: (T) -> Boolean, …)`, `ColumnScope.AnimatedVisibility` (×2), `RowScope.AnimatedVisibility` (×2) | present: one `AnimatedVisibility` over `AnimatedVisibilityState` (`(column, visible)` for a scope's), and `transition.AnimatedVisibility(…)` (D804, D812) | animated-visibility-scope |
| `AnimatedVisibilityScope` (`transition`, `Modifier.animateEnterExit(enter, exit, label)`), `EnterExitState` (`PreEnter`, `Visible`, `PostExit`) | present (`scope.animate_enter_exit(enter, exit, label)`, D804, D817) | animated-visibility-scope |
| `AnimatedContent(…, contentKey, content: AnimatedContentScope.(S) -> Unit)`, `Transition<S>.AnimatedContent(…)` | present (`content_key: ContentKey`, the content taking an `AnimatedContentScope` and the state, D812, D817) | animated-content-scope |
| `AnimatedContentTransitionScope` (`initialState`, `targetState`, `isTransitioningTo`, `contentAlignment`, `slideIntoContainer(towards, spec, initialOffset)`, `slideOutOfContainer`, `using(SizeTransform?)`, `ExitTransition.KeepUntilTransitionsFinished`), `SlideDirection` (`Left`, `Right`, `Up`, `Down`, `Start`, `End`) | present (D817) | animated-content-scope |
| `AnimatedContentScope` | present (D817) | animated-content-scope |
| `ContentTransform(targetContentEnter, initialContentExit, targetContentZIndex, sizeTransform)`, `SizeTransform(clip, sizeAnimationSpec)`, `SizeTransform` interface | present (`ContentTransform`, `size_transform(clip, spec)`, D805) | animated-content-scope |
| `Transition<T>.Crossfade(modifier, animationSpec, contentKey, content)` (the plain overload has no `contentKey`) | present (`transition.Crossfade(…)`, D817) | crossfade-extras |
| `animateContentSize(animationSpec, alignment, finishedListener)` (both overloads) | present (`animate_content_size_with(spec, alignment, finished_listener)`, D811) | animate-content-size-extras |
| `CapturedAnimatedVisibility(visible, …)`, `CapturedAnimatedVisibility(MutableTransitionState, …)` | missing | captured-animated-visibility |
| `DeferredAnimatedContent`, `DeferredAnimatedVisibility`, `MutableContentTransform(…)`, `MutableTransform`, `TransformScope` (`alpha`, `scale`, `offset`, `transformOrigin`, `veil`) | present: methods of `DeferredTransition`; `TransformScope`'s setters `set_alpha` and the like (D828) | deferred-transition |
| `BoundsTransform`, `SharedTransitionDefaults` (`BoundsTransform`, `SharedContentConfig`), `SharedTransitionScope.PlaceholderSize` (`ContentSize`, `AnimatedSize`), `ResizeMode` (`RemeasureToBounds`, `scaleToBounds(contentScale, alignment)`), `SharedContentConfig`'s `shouldKeepEnabledForOngoingAnimation` and `permitTransformDuringDeferredTransition` | present (D829); what takes a `SharedContentState` comes with the scope | shared-transition |
| `SharedTransitionLayout`, `SharedTransitionScope(content)`, the rest of the `SharedTransitionScope` interface:<br>• the modifiers: `sharedElement`, `sharedBounds`, `sharedElementWithCallerManagedVisibility`, `skipToLookaheadSize`, `skipToLookaheadPosition`, `renderInSharedTransitionScopeOverlay`;<br>• the state: `rememberSharedContentState(key[, config])`, `SharedContentConfig(…)`, `OverlayClip`, `isTransitionActive`;<br>• the nested types: `SharedContentState` (`key`, `isMatchFound`, `clipPathInOverlay`, `parentSharedContentState`, `prepareTransitionWithInitialVelocity`), `SharedContentConfig` (`isEnabled`, `shouldKeepEnabledForOngoingAnimation`, `permitTransformDuringDeferredTransition`, `alternativeTargetBoundsInTransitionScopeAfterRemoval`), `PlaceholderSize` (`ContentSize`, `AnimatedSize`), `ResizeMode` (`RemeasureToBounds`, `scaleToBounds(contentScale, alignment)`);<br>• `SharedTransitionDefaults` (`BoundsTransform`, `SharedContentConfig`) and `BoundsTransform` | missing | shared-transition |
| `Modifier.animateBounds(lookaheadScope, modifier, boundsTransform, animateMotionFrameOfReference)` | missing | animate-bounds |
| `LookaheadAnimationVisualDebugging(…)`, `CustomizedLookaheadAnimationVisualDebugging(…)` (experimental) | missing | lookahead-visual-debug |
| `splineBasedDecay(density)`, `rememberSplineBasedDecay()`, `SplineBasedFloatDecayAnimationSpec` | present (D820; foundation's flings decay along it, D826) | decay-animation |
| `Color.VectorConverter(colorSpace)` | `Animate for Color` (sRGB only: compose-rs colors have no color spaces) | (none: n/a until color spaces exist) |
| `Animatable(Color)`, `animateColorAsState`, `InfiniteTransition.animateColor`, `Transition.animateColor` | present | |

### animation-graphics (`androidx.compose.animation.graphics`)

| API | compose-rs | Feature |
|---|---|---|
| `AnimatedImageVector` (`imageVector`, `totalDuration`), `AnimatedImageVector.animatedVectorResource(id)`, `rememberAnimatedVectorPainter(animatedImageVector, atEnd)` | missing. The resource loader is Android's; its analogue loads AVD XML from bundled bytes, as `remember_image_resource` does for images. compose-ui's `ImageVector` is paths with fills only: no groups, transforms, strokes or `VectorPainter` | animated-image-vector |

### Navigation Compose (`androidx.navigation.compose`) and `NavController`

`n/a: replaced by Navigation 3 (D704)`. `crates/compose-navigation` had what follows, and has it no more.

- `remember_nav_controller(start)`, `NavController`:
  - `navigate`, `navigate_with(NavOptions { pop_up_to, pop_up_to_inclusive, launch_single_top })`;
  - `pop_back_stack`, `pop_back_stack_to(dest, inclusive)`;
  - the reads `current_back_stack`, `current_back_stack_entry`, `current_destination`, `previous_back_stack_entry` (state reads, so `currentBackStackEntryAsState` is just a read).
- `NavBackStackEntry { id, destination }`.
- `NavHost!(controller, modifier, content_alignment, enter_transition, exit_transition, content)`: saveable state per entry, back pops.

### Navigation 3

What compose-rs is to model (D704): a list of typed keys the app owns, mapped to content, which fits its typed destinations. The runtime and results are in (D1400, D1401), and `NavDisplay` with its scenes (D1405).

**navigation3-runtime** (`androidx.navigation3.runtime`):

| API | Feature |
|---|---|
| `NavEntry(key, contentKey, metadata, content)`, `NavEntry(navEntry, content)`, `Content()`, `contentKey`, `metadata` | present: `NavEntry::new`, `with_content_key`, `with_metadata`, `from_entry` (D1400) |
| `entryProvider(fallback) { entry<K>(…); entry(key, …); addEntryProvider(…) }`, `EntryProviderScope`, `EntryDsl` | present: `entry_provider`, `entry_provider_with_fallback`, `entry`, `entry_of` with `KeyClass` (D1400) |
| `NavEntryDecorator(onPop, decorate)`, `rememberDecoratedNavEntries(entries, decorators)`, `rememberDecoratedNavEntries(backStack, decorators, entryProvider)` | present: `NavEntryDecorator::new`, `with_on_pop`, `remember_decorated_nav_entries`, `remember_decorated_nav_entries_from` (D1400) |
| `SaveableStateHolderNavEntryDecorator`, `rememberSaveableStateHolderNavEntryDecorator` | present (D1400) |
| `NavKey`, `NavBackStack` (a snapshot-state list of keys), `rememberNavBackStack(vararg)`, `NavBackStackSerializer`, `NavKeySerializer` | present: `NavKey`, `NavBackStack`, `remember_nav_back_stack`, `Saveable` for the serializers (D1400) |
| `rememberNavBackStack(configuration, vararg)` | no analogue: nothing's registered, an enum holding its kinds of key (D1400) |
| `NavMetadataKey`, `metadata { put(key, value) }`, `MetadataScope`, `Map.get/contains(NavMetadataKey)` | present: `NavMetadata` for the map (D1400) |
| `ResultEventBus` (`sendResult`, `conflateAsState`, `removeResult`), `rememberResultEventBus`, `LocalResultEventBus`, `ResultEffect`, `ResultEventBusNavEntryDecorator`, `rememberResultEventBusNavEntryDecorator` | present (D1401) |
| `deeplink`: `DeepLinkMatcher` (+`Filter`, `MatchResult`, `mimeTypeFilter`), `DeepLinkRequest` (uri, `RequestExtras`, `mimeTypeExtra`), `UriDeepLinkMatcher`, `UriMatchResult`, `StaticKeyDeepLinkMatcher`, `BackStackMatcher`/`withBackStack`, `WrappedMatchResult`, `DeepLinkSerializer`, `RequestExtras`/`requestExtras {}`/`RequestExtrasKey`, `DeepLinkUri` | nav3-deep-links (optional) |
| `DeepLinkRequest(Intent)`, `actionExtra`, `actionFilter`, `IntentExtrasKey`, `ActionExtrasKey` | n/a: Android `Intent`s |

**navigation3-ui** (`androidx.navigation3.ui`, `androidx.navigation3.scene`):

| API | Feature |
|---|---|
| `NavDisplay(backStack, modifier, contentAlignment, onBack, entryDecorators, sceneStrategies, sceneDecoratorStrategies, sharedTransitionScope, sizeTransform, transitionSpec, popTransitionSpec, predictivePopTransitionSpec, entryProvider)`, `NavDisplay(entries, …)`, `NavDisplay(sceneState, navigationEventState, …)` | present: `NavDisplay`, `NavDisplayOfEntries`, `NavDisplayOfState`, restartable, their lists `NavListArg`s (D1408); `sharedTransitionScope` with shared elements (D1405) |
| `NavDisplay.transitionSpec/popTransitionSpec/predictivePopTransitionSpec` metadata (`TransitionKey`, `PopTransitionKey`, `PredictivePopTransitionKey`), `defaultTransitionSpec`, `defaultPopTransitionSpec`, `defaultPredictivePopTransitionSpec`, `LocalNavAnimatedContentScope` | present: `TransitionKey`, `PopTransitionKey`, `PredictivePopTransitionKey`, `LOCAL_NAV_ANIMATED_CONTENT_SCOPE` (D1405) |
| `Scene` (`key`, `entries`, `previousEntries`, `content`, `metadata`), `OverlayScene` (`overlaidEntries`, `onRemove`), `SceneStrategy`, `SceneStrategyScope` (`onBack`), `SinglePaneSceneStrategy`, `DialogSceneStrategy` (+`dialog(properties)`, `DialogKey`), `SceneDecoratorStrategy`, `SceneDecoratorStrategyScope`, `SceneState` (`currentScene`, `previousScenes`, `overlayScenes`, `entries`), `rememberSceneState`, `SceneInfo` (`title`, `url`), `TitleMetadataKey`, `UrlMetadataKey` | present (D1405) |
| `NavigationBackHandler(sceneState, state, onBackCancelled, onBackCompleted)`, `rememberNavigationEventState(sceneState)` | present: `SceneNavigationBackHandler`, `remember_scene_navigation_event_state` (D1405) |

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

#### animation-objects (M, not exp.): done (D800, D807, D816)

- **API:**
  - `Animation`, `TargetBasedAnimation`;
  - the `Vectorized*AnimationSpec` evaluation API (`value_from_nanos`, `velocity_from_nanos`, `duration_nanos`, `end_velocity`, `is_infinite`) on every spec;
  - `FloatAnimationSpec`, `FloatTweenSpec`, `FloatSpringSpec`;
  - `AnimationConstants.UnspecifiedTime`;
  - optionally `AnimationVector1D..4D` as thin `[f32; N]` types.
- **What it does:** public, pure evaluation of a spec at a play time. Needed by tests, by `SeekableTransitionState` (durations), and by tooling.
- **Tests:** `AnimationTest`, `IsInfiniteTest`, `PhysicsAnimationTest`, and the exact-value parts of `TweenAnimationTest`, `KeyframeAnimationTest`, `RepeatableAnimationTest` and `InfiniteTransitionTest`.
- **Dependencies:** spring-estimation for spring durations.

#### spec-gaps (S, not exp.): done (D800)

- **API:**
  - `StartOffset`/`StartOffsetType` (`Delay`, `FastForward`), with `initialStartOffset` on `repeatable`/`infiniteRepeatable`;
  - `SnapSpec` as a duration-based spec (`repeatable(snap())`);
  - keyframes `atFraction`, and `using(easing)` as the name of `with`;
  - a settable `duration_millis` in the keyframes builder (it is an argument today).
- **Tests:** `RepeatableAnimationTest` (3), `KeyframeAnimationTest` (2).
- **Dependencies:** none.

#### more-converters (S, not exp.): done (D800)

- **API:**
  - `Animate` for `Rect`, `IntSize` (rounded, clamped at 0) and `DpOffset` (a new compose-ui type);
  - `animate_rect_as_state`, `animate_int_size_as_state`;
  - a `visibility_threshold` parameter on `animate_value_as_state`;
  - public `VisibilityThreshold` values, set to Compose's.
- **Tests:** `SingleValueAnimationTest.animate4DRectTest`, `ConverterTest.testRectConverter`, `AnimatableTest.testIntSize_alwaysWithinValidBounds`.
- **Dependencies:** compose-ui `DpOffset`.

#### animatable-bounds (S, not exp.): done (D800, D807)

- **API:**
  - `Animatable::update_bounds`, `lower_bound`, `upper_bound`, and `AnimationEndReason::BoundReached`;
  - a visibility threshold and `label` in the constructor;
  - `type_converter`/`velocity_vector` accessors.
- **Tests:** `AnimatableTest.testUpdateBounds`.
- **Dependencies:** none (bounds matter most for decay-animation).

#### suspend-animate (M, not exp.): done but `AnimationResult.endState` as an `AnimationState` (D819, D825)

- **API:** `animate(initial, target, initial_velocity, spec, block)` (`f32` and generic), `AnimationState`, `AnimationScope`, `AnimationState::animate_to`, `animate_decay`, `sequential_animation`, the per-frame `block` on `Animatable::animate_to`, and `AnimationResult::end_state`.
- **What it does:** low-level animations without an `Animatable`.
- **Tests:** `SuspendAnimationTest`, `DurationScaleTest`.
- **Dependencies:** animation-objects; decay-animation for the decay forms.

#### decay-animation (M, not exp.): done (D807, D820)

- **Done:** `FloatDecayAnimationSpec`, `FloatExponentialDecaySpec`, `VectorizedDecayAnimationSpec`, `DecayAnimationSpec`, `exponential_decay`, `generate_decay_animation_spec`, `calculate_target_value`, `DecayAnimation` (and `float_decay_animation` for its `FloatDecayAnimationSpec` constructor), `Animatable::animate_decay`. The spline family (`spline_based_decay`, `remember_spline_based_decay`, D820); foundation's flings decay along it (D820, D826).

- **API:**
  - the specs: `DecayAnimationSpec`, `FloatDecayAnimationSpec`, `VectorizedDecayAnimationSpec`, `exponential_decay`, `FloatExponentialDecaySpec`, `generate_decay_animation_spec`;
  - `calculate_target_value`, `DecayAnimation`, `Animatable::animate_decay`;
  - the spline family: `spline_based_decay(density)`, `remember_spline_based_decay`, `SplineBasedFloatDecayAnimationSpec`.
- **What it does:** flings that slow to a stop.
- **Tests:** `DecayAnimationTest`, `IsInfiniteTest` (2), `AnimatableTest.animateDecayTest`, `SuspendAnimationTest` (2), `AndroidFlingSplineTest`.
- **Dependencies:**
  - animation-objects and animatable-bounds;
  - the Android spline moved from `compose-foundation/src/fling.rs` into compose-animation, and compose-foundation depends on compose-animation, as Compose's foundation on animation-core (D820, D826).

#### easing-functions (S, not exp.): done (D808, D816)

- **Done:** the 30 `Ease*` curves; cubic Béziers solved as Compose's. `Easing::custom` holds any closure (D816).

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

#### keyframes-with-spline (M, not exp.): done (D843)

- **API:** `keyframes_with_spline(periodic_bias, init)`, `KeyframesWithSplineSpec`, and Compose's monotonic spline.
- **Tests:** `KeyframeSplineAnimationTest`, `MonoSplineTest`, `RepeatableAnimationTest` (1).
- **Dependencies:** spec-gaps (`atFraction`).

#### arc-animation (M; `ArcAnimationSpec` is exp., `ArcMode` and keyframes `using(ArcMode)` are not): done (D844)

- **API:** `ArcAnimationSpec`, `ArcMode` (`ArcAbove`, `ArcBelow`, `ArcLinear`), keyframes `using(arc_mode)`.
- **Tests:** `ArcAnimationTest`, `KeyframeArcAnimationTest`, `RepeatableAnimationTest` (1).
- **Dependencies:** spec-gaps.

#### interruption-aware-spec (S–M, not exp.): done (D809)

- **API:** `InterruptionAwareAnimationSpec(spec, interruption_spec)`, honoured by `Animatable`, `animate*AsState` and `Transition`. compose-rs's transitions hard-code Compose's default interruption spring.
- **Tests:** `PhysicsAnimationTest` (1), `AnimatableTest` (1), `SingleValueAnimationTest` (1), `TransitionTest` (2).
- **Dependencies:** none.

#### motion-duration-scale (S–M, not exp.): done (D827, D836)

- **Done:** the scale of the thread and of a task's future, read by `Animatable`, `animate`, `Transition`, `SeekableTransitionState` and `InfiniteTransition`; the test rule's and `ComposeUiTestConfig`'s; Android's `ANIMATOR_DURATION_SCALE` (D836).

- **API:** `MotionDurationScale` (a runtime/ui coroutine-context element: a scale of 0 finishes animations at once, as Android's animator scale), honoured by every animation; `with_infinite_animation_frame_nanos`/`_millis` and the `InfiniteAnimationPolicy` they consult.
- **Tests:** `DurationScaleTest`, `DurationScaleTransitionTest`.
- **Dependencies:**
  - the runtime and ui areas: a context element on compose-rs's executor and frame clock;
  - transition-state and seekable-transition for the transition tests.

#### transition-state (M–L, not exp.): done (D803, D813, D818)

- **Done:** `MutableTransitionState`, `remember_transition`, `Segment` (the spec closures take it), `pending_target_state` aside; child transitions; `total_duration_nanos`, `label`; `animate_int`, `animate_int_offset`, `animate_int_size`, `animate_rect`, `animate_size`; the tooling getters `animations`, `transitions` (`TransitionAnimation`, `AnyTransition`, D813). `InfiniteTransition`'s tooling getters and the `animate_*` calls' labels (D818). `pending_target_state` (D828).

- **API:**
  - the states: `TransitionState`, `MutableTransitionState` (`is_idle`), `remember_transition(state, label)`;
  - on `Transition`: `Segment` with `is_transitioning_to` (the `transition_spec` closure taking a segment), `pending_target_state`, `label`, `animations`, `total_duration_nanos`;
  - child transitions: `create_child_transition`, `transitions`;
  - `animate_int`, `animate_int_offset`, `animate_int_size`, `animate_rect`, `animate_size`;
  - the tooling getters on `InfiniteTransition`.
- **What it does:** the shared machinery of `AnimatedVisibility`, `AnimatedContent` and `Crossfade` overloads, and of state hoisting. D423 kept a task per value; seeking needs Compose's single play-time loop.
- **Tests:** `TransitionTest` (~10). It also unblocks parts of `AnimatedVisibilityTest`, `AnimatedContentTest`, `CrossfadeTest` and `DurationScaleTransitionTest`.
- **Dependencies:** animation-objects (durations).

#### seekable-transition (L, not exp.): done (D821)

- **Done:** `SeekableTransitionState` (`seek_to`, `seek_to_with`, `animate_to`, `animate_to_with`, `snap_to`, `fraction`, `current_state`, `target_state`; an interrupted call cancels its task, D823), `remember_transition` over the `TransitionState` trait, the transition's seeking (initial-value animations, the observed total duration). Left: `AnimatedVisibilityTest.testSeekingAnimatedVisibility` (Compose's tooling `setPlaytimeAfterInitialAndTargetStateEstablished`).

- **API:** `SeekableTransitionState` (`seek_to(fraction, target)`, `animate_to(target, spec)`, `snap_to`, `fraction`), and Transition support for seeking (Compose's internal `delayed` wrapper).
- **What it does:** drives a transition from a gesture. This is the basis of predictive back.
- **Tests:** `SeekableTransitionStateTest` (44), `DelayedAnimationTest`, `DurationScaleTransitionTest` (2), `AnimatedVisibilityTest.testSeekingAnimatedVisibility`.
- **Dependencies:** transition-state, animation-objects, spring-estimation.

#### deferred-target-animation (S, not exp.): done (D810)

- **API:** `DeferredTargetAnimation(converter)`: `update_target(target, scope, spec)`, `value`, `pending_target`, `is_idle`.
- **Tests:** `DeferredTargetAnimationTest`.
- **Dependencies:** none.

#### deferred-transition (XL, not exp., new at this tip): done (D828)

- **Done:** all of it but what shared elements take from it (`SharedMutableTransformState`'s modifier local, `parentLayoutCoordinates`), which comes with shared-transition.

- **API:**
  - the states: `DeferredTransitionState` (`defer`, `animate_to`, `pending_target_state`), `DeferredTransition`, `remember_deferred_transition`;
  - the composables: `DeferredAnimatedContent`, `DeferredAnimatedVisibility`;
  - the transforms: `MutableContentTransform`, `MutableTransform`, `TransformScope`.
- **What it does:** a transition whose start waits on a gate while the content is previewed by gesture-driven transforms, then handed off with velocity.
- **Tests:** `DeferredTransitionTest`, `DeferredAnimatedVisibilityTest`, `DeferredAnimatedContentTest`, `SharedMutableTransformStateTest`, and with shared-transition `DeferredSharedElementTest`.
- **Dependencies:** transition-state, enter-exit-params, animated-visibility-scope, animated-content-scope, veil.

### Animation components

#### enter-exit-params (M, not exp.): done (D804, D814)

- **API:**
  - the full parameter lists of `expand_*`/`shrink_*` (spec, alignment, `clip`, size lambdas), `slide_in`/`slide_out` (spec, `IntSize -> IntOffset`), `slide_in_horizontally`/`slide_out_horizontally` and the vertical pair (spec, offset lambda), `scale_in`/`scale_out` (spec, `transform_origin`);
  - the `config` getters and their types (`EnterExitTransitionConfig`, `FadeConfig`, `SlideConfig`, `ChangeSizeConfig`, `ScaleConfig`).
- **Behaviour to change:** animate the parts in their own units with Compose's thresholds: pixels for size and offset, the float itself for alpha and scale. compose-rs animates a 0..1 fraction with threshold 0.001 for every part.
- **Tests:** `EnterExitTransitionConfigTest`, and most of `AnimatedVisibilityTest` and `AnimatedContentTest`.
- **Dependencies:** none.

#### veil (S–M, not exp., new at this tip): done but `match_parent_size`'s parent bounds (D814)

- **API:** `unveil_in(spec, initial_color, match_parent_size)`, `veil_out`, `VeilConfig`.
- **Tests:** `AnimatedVisibilityTest` (4), `EnterExitTransitionConfigTest` (2), parts of the deferred tests.
- **Dependencies:** enter-exit-params.

#### animated-visibility-scope (M, not exp.): done (D804, D812, D817)

- **Done:** `AnimatedVisibilityScope` with its `transition`, `EnterExitState`, the `MutableTransitionState`, `ColumnScope` and `RowScope` overloads (one function over `AnimatedVisibilityState`), the `Transition` overload (`transition.AnimatedVisibility`, D812). `scope.animate_enter_exit` (D817).

- **API:**
  - `AnimatedVisibilityScope` (`transition`, `Modifier.animate_enter_exit`), `EnterExitState`;
  - the overloads: `AnimatedVisibility(MutableTransitionState)`, `Transition::AnimatedVisibility(visible_fn)`, and `ColumnScope`/`RowScope` with their defaults (`fade_in() + expand_vertically()` in a column, horizontal in a row).
- **Tests:** `AnimatedVisibilityTest` (transition overload, the `MutableTransitionState` cases), `CapturedAnimatedVisibilityTest`, `SharedTransitionTest`.
- **Dependencies:** transition-state, enter-exit-params.

#### animated-content-scope (M–L, not exp.): done (D805, D812, D817)

- **Done:** `SizeTransform` (`size_transform(clip, spec)`), `target_content_z_index`, `ContentTransform::using`, no cap on the contents shown (Compose's rule: a content shows while its state is the transition's current or target), the `Transition` overload (`transition.AnimatedContent`, D812). The scopes (`AnimatedContentScope`, `AnimatedContentTransitionScope` and its methods), `content_key`, `KeepUntilTransitionsFinished` (D817).

- **API:**
  - the scopes: `AnimatedContentScope` (an `AnimatedVisibilityScope`) and `AnimatedContentTransitionScope`, holding `initial_state`, `target_state`, `content_alignment` and the methods `slide_into_container(SlideDirection, spec, offset)`, `slide_out_of_container`, `using(SizeTransform)`, `ExitTransition::KEEP_UNTIL_TRANSITIONS_FINISHED`;
  - the `ContentTransform` parts: `target_content_z_index`, `SizeTransform { clip, size_animation_spec(initial, target) }`;
  - `AnimatedContent(…, content_key)` and the `Transition::AnimatedContent` overload.
- **Behaviour to change:** remove the cap of three contents (Compose keeps every state still animating) and key contents by `content_key(state)`.
- **Tests:** `AnimatedContentTest` (~9), `EnterExitTransitionConfigTest` (SizeTransform default); NavHost and NavDisplay transitions build on it.
- **Dependencies:** transition-state, animated-visibility-scope, enter-exit-params.

#### crossfade-extras (S, not exp.): done (D817)

- **API:** `Transition::Crossfade(modifier, animation_spec, content_key, content)`. Compose's plain `Crossfade` has no `contentKey`.
- **Tests:** `CrossfadeTest.crossfadeTest_contentKey`.
- **Dependencies:** transition-state.

#### animate-content-size-extras (S, not exp.): done (D811)

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

- **Done, without lookahead (D829):** `BoundsTransform`, `SharedTransitionDefaults`, `PlaceholderSize`, `ResizeMode` (`ContentScale` moved to compose-ui, as Compose's is in ui), the state-free part of `SharedContentConfig`. From deferred-transition (D828): `pending_target_state`, the manual transforms' state machine, forced initial values on deferred animations. **Left:** everything with a `SharedContentState` or a scope.
- **API:** `SharedTransitionLayout`, `SharedTransitionScope` and its whole surface (see the API table), `BoundsTransform`, `SharedTransitionDefaults`.
- **What it does:** matches elements across content changes and animates their bounds in an overlay.
- **Tests:** `SharedTransitionTest` (71), `DeferredSharedElementTest` (15), `OverlaySceneTest`, `AnimatedTest.testSharedElement`, `NavDisplayScreenshotTest` (1).
- **Dependencies:**
  - **ui lookahead** (`LookaheadScope`, approach and intermediate layout, lookahead coordinates), which compose-rs lacks entirely;
  - movable content (tests use `movableContentOf`), which compose-runtime lacks;
  - an overlay with z-ordering: its `LayerRenderer`s draw `GraphicsLayer`s recorded in place, so it waits for ui-core's **graphics-layer-object** (compose-ui has graphics layer modifiers, no layer object to record and draw elsewhere); `renderInSharedTransitionScopeOverlay` with it;
  - animated-visibility-scope, transition-state, and lazy layouts (present).

#### animate-bounds (L, not exp.)

- **API:** `Modifier.animate_bounds(lookahead_scope, modifier, bounds_transform, animate_motion_frame_of_reference)`.
- **Tests:** `AnimateBoundsTest`.
- **Dependencies:** ui lookahead; `BoundsTransform` (done, D829); movable content for 1 test.

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

#### predictive-back (L, not exp.): done (D845, D847, D848, D1405)

- **Done:** `compose-navigation-event` (`NavigationEventDispatcher`, `NavigationEventHandler`, `NavigationBackHandler`, `NavigationEventState`, inputs), `BackHandler` and `PredictiveBackHandler` on it, and the Android host's back progress (`OnBackAnimationCallback`, `OnBackPressedCallback`).
- **To run on a device, not checked yet:** a real `androidx.activity` before 1.11, for `AndroidXBack`'s `frameTimeMillis` (F1 of its review: only 1.12.4 and 1.13.0 are in the Gradle cache); a cancel by swiping back to the edge, rerun since `AndroidXPresence` went in front of `BackRegistration`; API 33 and below; a right-edge swipe.
- **Left:** Material 3's own use of the progress, in its ledger. `NavDisplay`'s is done (D1405).

#### Navigation Compose's features: n/a

nav-transitions, nav-controller-extras, nav-dialog-destinations, nav-graphs and nav-entry-owners were this ledger's features for extending `compose-navigation`. They're `n/a: replaced by Navigation 3 (D704)`: their behaviours come back as Navigation 3's, derived from its own tests, `NavDisplay`'s transition metadata and z-order (`AnimatedTest`, `NavDisplayScreenshotTest`), `DialogSceneStrategy` (`NavDisplayTest`), and entry lifecycles and view models through decorators (`NavDisplayLifecycleTest`, `lifecycle-viewmodel-navigation3`, on `compose-lifecycle`: D837, D838). The local branch `parked/nav-compose-extras`, not merged, is reference for a seekable transition's mechanism and its back-interrupt fix, not a source of behaviours.

<a id="navigation3"></a>
#### navigation3: four features (D704, after S3)

- **nav3-runtime (L): done (D1400).** `NavEntry`, `entryProvider`, decorators and `rememberDecoratedNavEntries`, the saveable-state decorator, `NavKey`/`NavBackStack`/`rememberNavBackStack` over `Saveable`, metadata keys.
  - Tests: 8 runtime files (79 tests), `ported` but for the 4 of serializer configuration, which have no analogue.
- **nav3-ui (L): done but what follows (D1405).** `NavDisplay`, scenes and strategies, dialog and overlay scenes, scene decorators, transition metadata, `LocalNavAnimatedContentScope`, `NavigationBackHandler`. Left: what needs shared elements.
  - Tests: 15 ui files (121 tests), and `ResultEventBusNavEntryDecoratorTest` (5).
  - Dependencies: nav3-runtime, animated-content-scope, predictive-back, shared-transition (2 tests), the entry decorators over `compose-lifecycle` (lifecycle test), and movable content (`SceneSetupNavEntryDecorator`'s `movableContentOf`).
  - Without movable content (D1404): single-pane scenes, dialogs and overlays ship first, the decorator keeping its shape. An entry moving between panes waits for it, from the composition context work, with the tests of `ListDetailSceneTest`, `HierarchicalSceneTest`, `BottomNavSceneDecoratorTest`'s list-detail, and the two-pane cases of `SceneStateTest`, `NavDisplayTest`, `NavDisplayLifecycleTest` and `NavDisplayScreenshotTest`.
- **nav3-results (S): done but its decorator's tests (D1401).** `ResultEventBus`, `ResultEffect` and the decorator.
  - Tests: 3 files (5 tests) `ported`; `ResultEventBusNavEntryDecoratorTest` (5) needs `NavDisplay`, under nav3-ui.
- **nav3-deep-links (L), last of Navigation 3 (D1404):** URI patterns, matchers, argument decoding and synthetic back stacks.
  - Tests: 8 files (211 tests).
  - Dependencies: a URI type and a decoding trait (the analogue of kotlinx.serialization), and a platform path delivering links: on Android an `Intent`, which is n/a, or a desktop URL handler.

Navigation 3 has replaced `compose-navigation`'s model (D704): its `NavController` and `NavHost` are gone, and its users, the gallery and the guide, are on `NavDisplay`.

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
8. **seekable-transition (L)**.
9. **The remaining specs:** keyframes-with-spline (M, done), path-easing (S–M, after the ui path iterator), arc-animation (M, experimental, done).
10. **predictive-back (L)**, in the form Navigation 3 needs, with the platform back-progress work: the host's side done (D845), `NavDisplay`'s with nav3-ui.
11. **veil (S–M)**, then **deferred-transition (XL)**.
12. **Navigation 3 (D704), after S3 (the runtime's state collections):** nav3-runtime, then nav3-ui with the entry decorators over `compose-lifecycle`, nav3-results, and nav3-deep-links last or never. It replaces `compose-navigation`.
13. **After ui lookahead (and movable content) lands in its own area:**
    - lookahead in animations (S–M);
    - animate-bounds (L);
    - shared-transition (XL);
    - lookahead-visual-debug (M, experimental).
14. **captured-animated-visibility (M)**, after ui layer recording.
15. **animated-image-vector (L)**, after a full `ImageVector`.

## Notes for the porting agents

### Mapping Compose's test patterns

| Compose | compose-rs (`compose_test`, `crates/compose-test/src/lib.rs`) |
|---|---|
| `createComposeRule()` + `rule.setContent { … }` | `compose_test(|| …)` or `ComposeTestRule::with_manual_clock(|| …)` (or `set_content`); drive changes through `mutable_state_of` handles created outside (see `visibility.rs` `panel()`) |
| `rule.mainClock.autoAdvance = false` before `setContent` | `ComposeTestRule::with_manual_clock`, or `set_auto_advance(false)` |
| `rule.mainClock.advanceTimeByFrame()` | `advance_time_by_frame()` (16 ms) |
| `rule.mainClock.advanceTimeBy(ms)` | `advance_time_by(Duration::from_millis(ms))`, rounded up to whole frames as Compose's; `advance_time_by_ignoring_frame_duration(d)` for `ignoreFrameDuration = true` |
| `rule.runOnIdle { … }`, `runOnUiThread { … }` | plain code after `wait_for_idle()`: everything runs on the test thread |
| `rule.waitForIdle()` | `wait_for_idle()`. With auto-advance on it moves the clock while a frame is needed, up to 60 s of test time; with it off, a state change composes only at the next `advance_time_by…` frame, as Compose's (D900) |
| `rule.waitUntil { cond }`, `mainClock.advanceTimeUntil { cond }` | `wait_until(|rule| …)`, `advance_time_until(|rule| …)` |
| `LaunchedEffect(Unit) { withFrameNanos { … } }` inside content, asserting per frame | works as is (`compose_runtime::with_frame_nanos`, `LaunchedEffect`). Compose's `SingleValueAnimationTest` and `InfiniteTransitionTest` use this pattern. Or sample from the test: read the state after each `advance_time_by_frame()` |
| `onGloballyPositioned { coords.localToRoot(…) }`, `size` | `Modifier.on_globally_positioned` and `on_size_changed` exist (`compose-ui/src/layout_callbacks.rs`); or `on_node_with_tag(t).fetch_semantics_node().bounds` (root coordinates, used by `visibility.rs`) |
| `assertPositionInRootIsEqualTo`, `getBoundsInRoot`, `assertWidthIsEqualTo` | the bounds assertions (docs/09), or compare `fetch_semantics_node().bounds` |
| `captureToImage()` and pixel or screenshot assertions | `capture_to_image` with `assert_pixels` (D904), or the display list (alphas and layer transforms, as `crossfade.rs` `alphas()` does; z-order) |
| `CompositionLocalProvider(LocalDensity provides Density(1f))` | the default test host is at density 1; else `compose_test_with(size, Density(…), …)` |
| `StateRestorationTester` / activity recreation | `host.save_state()` → `to_bytes` → a new rule with `restore_state` (see `recreate()` in `compose-navigation/tests/suite/navigation.rs`) |
| `Espresso.pressBack()` / `onBackPressedDispatcher.onBackPressed()` | `ui.host().on_back_pressed()`; a gesture with `on_back_started`, `on_back_progressed`, `on_back_cancelled`, or a `DirectNavigationEventInput` |
| `performTouchInput { swipe… }` | the touch injection scope (D903) |
| recomposition counts (`TransitionTest.testRecompositionCountWhenPassingTransition`) | `recomposition_counts()` / `track_recompositions` |
| `DetectLeaksAfterTestSuccess` | n/a |
| an exception from an interrupted `animateTo` | `AnimationResult { end_reason: AnimationEndReason::Interrupted, .. }` (D120) |
| `assertEquals(expected, actual, delta)` over frames | assert per frame; specs evaluate as Compose's to the bit (D800), so exact comparisons hold where Compose's do |

**Pure spec tests (`commonTest` `Vectorized*`):** evaluate the specs directly: every spec is a `VectorizedAnimationSpec` (`value_from_millis(play_time, start, end, velocity)` is Compose's `getValueFromMillis`; `at()` in `keyframe_animation.rs` is Compose's `at` helper), and `TargetBasedAnimation` is Compose's.

### What the harness lacks

Since the harness's milestone 1 (D900–D905), compose-test has Compose's clock model (rounding, frames on the test clock, `advance_time_until`, `wait_until`, the infinite-animation policy), bounds and displayed assertions, matchers over the tree and selectors (`on_children`, `on_parent`, collections), the touch, mouse (primary button) and key injection scopes, `perform_scroll_to`, `capture_to_image` with `assert_pixels`, `print_to_string`, and `set_content`; `perform_click` is a touch click. docs/09 lists the API, and [ui-text-and-test](ui-text-and-test.md) what is still missing. Existing tests that relied on the old timing gained a frame where they wait for a recomposition (D900).

- **`advance_time_by` rounds up to whole frames**, as Compose's `advanceTimeBy(ms)` (`ceil(ms / 16) × 16`, D900); `advance_time_by_ignoring_frame_duration` is `ignoreFrameDuration = true`. A frame comes a frame duration after something needs one, as Compose's.

### compose-rs behaviour suspected to differ from Compose's

Each item names the Compose test that would catch it, in the file's notes or listed here.

1. **Springs end on a different frame.** Fixed (D800): a spring ends at Compose's estimated duration.
   - Caught by: `PhysicsAnimationTest.testCriticallydampedDuration`, `testOverdampedDuration`, `testUnderdampedDuration`, `testEndSnapping`; `AnimatedVisibilityTest.testAnimationSettleExactTime`.
2. **Visibility thresholds.** Fixed (D800): a spring without a threshold uses 0.01 per component, the types' defaults are the tip's (0.4 dp, 1 px), and `animate*AsState` resolves a given threshold as Compose's.
   - **Plain springs.** Compose's `Animatable` and an explicit `spring()` without a threshold use `Spring.DefaultDisplacementThreshold` (0.01) on every dimension. Only `animate*AsState`'s default specs carry the type's threshold. compose-rs's `Animatable` uses `T::visibility_threshold()` for every spring without one, so an `Animatable<Dp>` or `<Offset>` with `spring()` ends earlier than Compose's.
   - **Default values.** At this tip `Dp.VisibilityThreshold` is 0.4 dp and `Offset`/`Size`/`Rect` use 1 px (`VisibilityThresholds.kt`). compose-rs uses 0.1 dp and 0.5 px, which look like older Compose values; check them against the version parity targets (1.12.1) before changing.
   - **`animateFloatAsState`.** Compose's applies a non-default `visibilityThreshold` to any `SpringSpec`, replacing the spec's own. compose-rs fills it only when the spring has none.
   - Caught by: `SingleValueAnimationTest.visibilityThresholdTest`, `defaultVisibilityThresholdTest`, `customSpringSpecVisibilityThresholdTest`, `customSpringSpecLargeVisibilityThresholdTest`, `updateVisibilityThresholdTest`.
3. **Enter and exit transitions animate a 0..1 fraction, not the values.** Fixed (D804). Every part springs a fraction with threshold 0.001 (`visibility.rs` `default_spec`). Compose animates alpha and scale as floats (threshold 0.01 for the default spring), and size and offset in pixels (1 px). So compose-rs's default fades end later, and expansions and slides of content smaller than 1,000 px end later too.
   - Also, slide offsets are fractions times the full size, recomputed each measure, while Compose calls the offset lambda with the full size.
   - Caught by: `AnimatedVisibilityTest.testScaleInAndScaleOutDefaultVisibilityThreshold`, `testAnimationSettleExactTime`, and the frame-by-frame expand and slide tests.
4. **Colors aren't clamped in Oklab.** Fixed: compose-rs clamps L to 0..1, a and b to ±0.5 and alpha to 0..1 before converting back, as Compose.
   - Caught by: `ConverterTest.testColorConverterClampValuesOutOfRange`.
5. **`i32` rounds where Compose truncates.** Fixed: `Animate for i32` truncates, as `Int.VectorConverter`'s `toInt()`. `IntOffset` and `IntSize` round half up in both.
   - Caught by: `TypeConverterTest` only checks whole numbers, so add a mid-animation `animate_int_as_state` case.
6. **`AnimatedVisibility` inside a `Column` or `Row` has the wrong defaults.** Fixed (D804): `AnimatedVisibility!((column, visible), …)`. Compose's scope overloads default to vertical or horizontal expand and shrink. compose-rs has only the plain overload (`expand_in`/`shrink_out`), so the same call animates differently.
   - Caught by: no Compose test targets it. Add one with animated-visibility-scope.
7. **`AnimatedContent` keeps at most three contents and keys them by an internal id.** Fixed (D805, and `contentKey` D817): no cap; a content shows while its state is the transition's current or target state, as Compose's `AnimatedEnterExitImpl` (so after a second interruption the first content leaves at once, not "Compose keeps all" as this note said).
   - Caught by: `AnimatedContentTest.AnimatedContentWithKeysTest` and `AnimatedContentWithContentKey`. Write a rapid four-state test for the cap.
8. **The animation containers expose no shared `Transition`.** `AnimatedVisibility`'s content has its scope's `transition` now (D804), and both run on a transition of the app's (`transition.AnimatedVisibility`, `transition.AnimatedContent`, D812); `AnimatedContent`'s content gets an `AnimatedContentScope` (D817). A transition remembered with a `SeekableTransitionState` is seeked (D821).
   - A task resumed by a frame continues within that frame, before its composition, where Compose dispatches it after: a frame callback that changes a transition's target sees the transition run a frame later than Compose's (D817; `AnimatedContentWithKeysTest`, `crossfadeTest_contentKey` wait a frame more).
9. *(Navigation Compose's differences, D291: `n/a` with its model, D704. What `NavDisplay` does by default comes from its own tip when Navigation 3 starts.)*
10. **Keyframe times are `u32`.** Fixed: they're `i32`, and out-of-range keyframes are ignored as Compose's (`keyframe_animation.rs`).
    - Also: at the end, compose-rs's frame returns the target; Compose's *vectorized* spec returns a keyframe placed at the duration, while its `TargetBasedAnimation` returns the target. Compare at the matching level.
    - Caught by: `KeyframeAnimationTest.outOfRangeValues_withForcedInitialAndTarget`.
11. **`InfiniteTransition` timing.** Its values accept finite `RepeatableSpec`s (Compose requires `InfiniteRepeatableSpec`), and run on frame time rather than the transition's play time (documented, D420).
    - Caught by: `InfiniteTransitionTest.transitionTest` checks the phase from the first frame. It passes (`infinite_transition.rs`, through 2.1 s of frames), since both start there.
12. **A transition value added mid-flight jumps to its target.** Fixed (D803): `Transition` is Compose's single play-time loop. In Compose, a `Transition.animateFloat` first composed while the transition runs starts at its value for `currentState` and animates to the target's.
    - compose-rs's `Transition::animate_value` remembers an `Animatable` at the target state's value (`transition.rs`), so it never animates.
    - Caught by: `TransitionTest.addNewAnimationInFlightTest`. It reads Compose's internal `playTimeNanos`; sample frames instead.
