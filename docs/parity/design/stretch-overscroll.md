# Stretch and glow overscroll on Android

Design note, nothing built. Written from Compose's `AndroidOverscroll.android.kt` (1,078 lines) and the replayer, with nothing run. AOSP's `EdgeEffect` and HWUI aren't in androidx-main, so statements about them come from Compose's code and comments and are unverified. Line counts are estimates.

## Decision

Use the real `android.widget.EdgeEffect` on Android, as Compose does. Port Compose's event logic to Rust behind a small trait, and do the drawing in the replayer. Don't port the physics or draw with an own shader: `RuntimeShader` is API 33 while stretch starts at API 31, and a copy would drift from the platform. Desktop stays `None`, as Compose's non-Android stub does.

## Design

**Events, in `compose-foundation` (about 500 lines).** A port of `AndroidEdgeEffectOverscrollEffect`: `applyToScroll`, `applyToFling`, the pulls, releasing the opposite edge, destretch by source, `updateSize`, `displacement`. It runs over a trait:

```rust
trait EdgeEffect {
    fn on_pull_distance(&self, delta: f32, displacement: f32) -> f32;
    fn on_release(&self);
    fn on_absorb(&self, velocity: i32);
    fn distance(&self) -> f32;
    fn is_finished(&self) -> bool;
    fn finish(&self);
    fn set_size(&self, width: i32, height: i32);
}
```

The factory comes from the host. `LOCAL_OVERSCROLL_FACTORY` stays `None` until a host provides one, so `remember_overscroll_effect` needs no change.

**Android, in `compose-android` and Kotlin (about 250 lines).** A JNI implementation, the pattern of `services.rs`. Effects are created lazily at the first pull, as `EdgeEffectWrapper` does. A scroller that never hits an edge makes no JNI call and creates no Java object. Rust mirrors "stretched" after each pull, so a normal scroll delta has no JNI cost; it reads Kotlin only while an effect is active. API 31+ gets stretch, API 29 and 30 get glow.

**Drawing.** A pair of display-list ops wraps the content, like `SaveLayerAlpha`: `BeginEdgeEffects { id, size, stamp }` and `End`. The Rust node emits them only while an effect is active; at rest it draws its content with no op, as Compose's `shouldDraw…` early return. For stretch, the replayer records the content into a `RenderNode` extended across the axis by `MaxSupportedElevation`, calls `EdgeEffect.draw` on the recording canvas per edge, then draws the node back, translated. For glow, it draws the content, then each edge with rotation and offset. In software rendering it finishes the effects and draws plainly. The Skia renderer ignores the ops.

**Keeping it animating.** The replayer skips a group whose key and version are unchanged, and a releasing effect's bytes don't change, so `stamp` changes each active frame to make a new version. The enclosing group is re-recorded each frame only while an effect is active; lazy items stay kept. Rust asks for the next frame while `!is_finished()`, read when recording, which may run one frame longer than Compose's `redrawSignal`. `recordAhead` must skip these ops, because an edge effect can only advance in the real draw.

## Decisions taken (agent)

- **Clearing the stretch:** port Compose's negation effects (`AndroidOverscroll.android.kt:255-264`), because `RenderNode.clearStretch()` is hidden. A fresh node per active frame would be simpler, but it depends on HWUI behaviour no one can verify without a device, and negation is known to work on phones.
- **Default on Android:** yes, as Compose does (D1464 says nothing overscrolls on Android until this lands). `bench-rs` gains a node per scroller and renders only at the edge. Parity with Compose's default is right by the benchmark-fairness rule, and the jank numbers get re-read on the device.
- **Testing:** host tests run the event logic over a fake `EdgeEffect` in `compose-test`, covering `StretchOverscrollIntegrationTest`. The fake's curves are simple arithmetic, so tests depending on the exact release curve are ported only as far as the fake is honest, and the ledger says which. Display-list tests check ops only while active, a changing stamp, and no op at rest. `StretchOverscrollScreenshotTest` stays device-only. It isn't done until a phone check by the user: the stretch shape on API 31+, no clipped shadows across the axis, a clean end to the stretch, and glow on API 29/30 if a device is available.
- **The list's end belongs in this work, in phase 1.**

## Risks

1. **A list's estimated room hides the edge.** Compose's `performScroll` measures synchronously and returns the real consumption. A lazy list here takes the whole delta while its end is only estimated (D476), and the rest goes to nested parents after the layout (D479), not to the effect. The delta or fling first reaching the end gives the effect nothing that frame. Drags should lag about a frame; a fling's leftover velocity could be overstated or lost, which changes `onAbsorb`. The place handing the rest to nested parents also tells the effect, and the first test of this phase is a fling into a `LazyColumn`'s end. This could grow beyond a small change.
2. **Scrollables with no lazy layout** consume synchronously, so `StretchOverscrollIntegrationTest` can pass without exercising risk 1. Lazy lists need tests of their own.
3. **One effect at two places.** D1464: an effect's modifier applied at two places draws twice, where Compose attaches once. Check pagers and `without_visual_effect`.
4. **Pointer position.** `displacement()` needs Compose's small pointer-input node, one more node per scroller with an effect, only with a factory present.

## Phases

1. Trait, event port, fake, `StretchOverscrollIntegrationTest` on the host, the list-end fix. About 800 lines including tests. No Android.
2. JNI, ops, replayer, stretch and glow, default factory. About 500 lines. `assembleRelease` proves it builds, not that it looks right.
3. Phone check by the user. Then ledger: `rememberPlatformOverscrollFactory`, the rest of `OverscrollTest`.
