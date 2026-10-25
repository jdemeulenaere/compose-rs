# 13. Drawing on Android: today's path and the direct alternatives

How compose-rs draws on Android, why it doesn't call HWUI or Skia directly, what a device's `libhwui.so` actually exposes, and the options for drawing "directly", with what each costs. Written for people asking "can compose-rs use libhwui or Skia directly?".

**Short answer.** On a production (`user`) Android build, an app can't call HWUI's or Skia's drawing code. Skia is linked statically inside `libhwui.so` and none of its drawing calls are exported. The few exported HWUI functions can't draw what a UI needs, and an app's linker namespace can't load `libhwui.so` anyway. compose-rs therefore draws through the public `Canvas` and `RenderNode` APIs, as Jetpack Compose does, and HWUI renders with Skia on the GPU from there. There are two realistic ways to draw "directly":
- **Ship our own Skia** and draw into a `SurfaceView` with public NDK APIs (option D below).
- **On the platform side, give compose-rs access** to HWUI's internals in a platform build (option C).

Reaching HWUI's internal C++ from an ordinary app on a production build (option B) is possible only through workarounds that break with every build, and it would save about 0.2 ms a frame.

## Today: public `Canvas` and `RenderNode` (D152, D157, D270)

1. Each frame, Rust records the UI into a display list. Parts that didn't change are kept from earlier frames (repaint boundaries, D155 and D270). Rust serializes the list into a byte buffer it owns. Kept groups are copied from a cache by key and version (D157), not serialized again.
2. In `View.onDraw`, one JNI call (`takeDrawing`) hands the buffer to Kotlin. `Replayer.kt` replays it as `android.graphics.Canvas` calls (`drawRect`, `drawRoundRect`, `clipPath`, `drawBitmap`, …).
3. Each repaint boundary and each graphics layer is recorded into a public `RenderNode` (API 29), as Compose's layers are, and kept while its version is unchanged. Repeated frames draw the kept node by reference. A graphics layer's properties are set as `RenderNode` properties, as Compose sets them: alpha, scale, rotation and translation, the outline it clips to (`clip`, `clip_to_bounds`), its elevation and shadow colors (`shadow`: HWUI draws the elevation shadow) and its compositing strategy. So a layer-only animation records nothing (D522-D525). Translations aren't merged across a group's edge, and a software draw (a snapshot) replays everything, applying each layer as operations. Known difference: a library keeping a render node recorded from the view (a blur, a ghost copy) sees the live group nodes, not a frozen copy (D154).
4. Text is laid out by `StaticLayout` (or `BoringLayout` for a single line, D330) and drawn into the recording that draws it, as Compose's; with the `paragraph_render_nodes` optimization (off by default, D567), each paragraph is recorded into a `RenderNode` of its own instead. Small icons are drawn from cached alpha-mask bitmaps (D190): a filled path drawn at most 256 px each way (after its scale) is rasterized once by the CPU (anti-aliased) into an `ALPHA_8` mask per path and scale, its corners on whole pixels from the drawn origin, and drawn with `drawBitmap` tinted by the paint's color, as Compose's `VectorPainter` caches an icon's bitmap at its drawn size; stroked and larger paths stay `drawPath`. Masks aren't keyed by subpixel offset (on whole pixels, as layout places icons, the mask is pixel-aligned; elsewhere, or under a scaling layer, it's filtered, as Compose's bitmap), go with their path, number at most 4 per path (a size animating makes one per frame, as Compose's `DrawCache` redraws its bitmap, without filling the cache), are immutable copies (`setImmutable` is hidden API), so the render thread uploads them once, and are kept within 4 MB, least recently recorded first. The trade-off is memory, one byte per pixel per drawn size (a 24 dp icon at 3.5x: about 7 KB), against the GPU rendering the path every frame (Ganesh's scratch atlas: an extra render pass, a `PathStencilCoverOp` and `DrawAtlasPathOp`s). Kotlin decides, as it knows the paint's style and the path's bounds. Paths, gradients and image bitmaps are resources defined once by id.
5. Render nodes are reused, not left to the garbage collector (D195): a released paragraph's node and a gone group's node go to a pool (64 and 32 at most; a group's layer properties reset first) that new paragraphs and groups take from, and a `TextPaint` is shared by the paragraphs whose paint-relevant style is the same (color, size, weight, italic, family, letter spacing, decoration: not line height or background, which are spans; 64 at most, then cleared). A dropped `RenderNode` frees its native side, the recording the render thread last drew included (`discardDisplayList` drops only the one not yet drawn), only when collected, and ART collects by Java heap growth (8 MB at least on a Pixel 8 Pro), which compose-rs's frames rarely reach: before pooling, 10 flings left 8.9 MB of native heap waiting, 7.2 MB of it render nodes. A reused node's old recording is freed the first time its new one is drawn, so at most a pool's worth waits. Reuse is safe because releasing a paragraph gives every group that drew it a new version before Kotlin draws again, so no kept node draws a reused one; `crates/compose-android/tests/released_paragraphs.rs` checks it from the bytes across text changes, item removal, a scrolled lazy list with prefetch, and a text field's edits. Kept groups' children are held by reference and walked by index. Pooling took the native heap after 10 flings from 20.8 to 12.4 MB and PSS from 92.1 to 81.2 MB (Compose: 85–110 MB).
6. From there it's the normal Android pipeline. HWUI records those calls, and its RenderThread draws them with Skia on the GPU (Vulkan on recent Pixels).

What this costs per frame, measured on a Pixel 8 Pro: the animated feed of [12](12-android-performance.md), averages of `compose-rs:` Perfetto sections after D270.

| Section | Main thread |
|---|---|
| `compose-rs: frame` (composition, layout, draw in Rust, serialize) | 0.22 ms |
| of which `serialize` | 0.03 ms |
| `compose-rs: replay` (JNI plus the Kotlin replay on the `Canvas`) | 0.20 ms |
| `Record View#draw()` (HWUI's recording around it) | about 0.31 ms |

So the whole "indirection" this path adds is about 0.2 ms of the main thread's work per frame. Even with it, compose-rs's frames are shorter than Jetpack Compose's in every scenario of [12](12-android-performance.md).

## What `libhwui.so` exposes

Checked on a Pixel 8 Pro running a `user`, release-keys build (`google/husky_beta/husky:CANARY/ZP11.251212.007/14649019`). The library is world-readable, so anyone can reproduce this; nothing on the device was changed:

```bash
adb shell getprop ro.build.type                      # user
adb shell ls /system/lib64/libhwui.so /system/lib64/libskia*.so
adb pull /system/lib64/libhwui.so
$NDK/toolchains/llvm/prebuilt/*/bin/llvm-readelf --dyn-syms -W libhwui.so \
  | awk '$5=="GLOBAL" && $7!="UND" {print $8}' | llvm-cxxfilt
```

- **No `libskia.so`.** Skia is linked statically into `libhwui.so`, which is 13 MB.
- **166 exported symbols**, under two version nodes:
  - **`LIBHWUI` (41):**
    - `ABitmap_*` (pixels, info, hardware buffer, compress);
    - `ACanvas_*`: `createCanvas`, `destroyCanvas`, `setBuffer`, `getNativeHandleFromJava`, `isSupportedPixelFormat`, `clipRect`, `clipOutRect`, `drawRect`, `drawBitmap`. That's all of its drawing.
    - `APaint_*`: `create`, `destroy`, `setBlendMode`, `setImageFilter`;
    - `AMatrix_*`, `ARegionIterator_*`;
    - the framework's own `init_android_graphics`, `register_android_graphics_classes`, `zygote_preload_graphics` and `hwui_uses_vulkan`.
  - **`LIBHWUI_PLATFORM` (125):**
    - image decoding (`android::ImageDecoder`, `SkAndroidCodec`, `SkCodec`);
    - colour (`SkColorSpace`, `SkColorInfo`, `SkImageInfo`, `skcms`);
    - streams and data (`SkStream`, `SkData`);
    - `SkCanvasStateUtils::ReleaseCanvasState`;
    - `GraphicsJNI::getNativeCanvas(JNIEnv*, jobject)`.
- **No Skia drawing symbols.** None for `SkCanvas`, `SkPaint`, `SkPath`, text (`SkFont`, `SkTextBlob`, SkParagraph), `SkPicture` or GPU contexts.

So the C drawing API is four calls: rectangles, bitmaps and two kinds of rectangular clip. It has no paths, rounded rectangles, text, layers or render nodes. The one way into general drawing is `GraphicsJNI::getNativeCanvas`, which returns HWUI's internal C++ `android::Canvas` for a Java `Canvas`. That class's drawing methods are virtual, so they could be called through its vtable without exported symbols. But that only works against the exact class layout of the build it was compiled for.

## The options

### A. Today's path (public `Canvas` and `RenderNode`)
- **Works on:** every device, API 29 and later.
- **Cost:** about 0.2 ms of the main thread per frame for the JNI call and the replay.
- **Keeps:** HWUI's whole pipeline, including the RenderThread, GPU backend selection, damage, hardware layers, the system's text rendering and font fallback, and the View hierarchy, so compose-rs works inside `AndroidView` and Kotlin Compose.
- **Remaining costs:** the Kotlin replay, and HWUI's own recording (`Record View#draw()`). Both are already below Jetpack Compose's frame costs.

### B. HWUI's internal C++ from an ordinary app
Two obstacles, either of which rules it out for anything shipped:
- **Loading.** Since Android 7, an app's native libraries live in an isolated linker namespace that can load only the NDK's public libraries (`public.libraries.txt`). `libhwui.so` is already loaded in every app process (the zygote preloads it), but outside the app's namespace, so `dlopen("libhwui.so")` and `dlsym` from app code don't find it. Getting at it means working around the linker's namespace checks; known techniques exist but are undocumented and unsupported.
- **ABI.** Calling `android::Canvas`'s virtual methods, or constructing its `Paint` (an `SkPaint` subclass), needs headers and object layouts matching the exact AOSP build and Skia milestone on the device. The platform's libc++ also uses the `std::__1` namespace while the NDK's uses `std::__ndk1`, so no standard-library type can cross the boundary. Every monthly build can move a vtable slot or a field and crash the app.

**Benefit if it worked:** most of the 0.2 ms replay, and perhaps some of HWUI's recording. Not worth the fragility, and not shippable.

### C. Platform-side access
For someone working on the platform, the clean version of B is to give compose-rs access in a platform build:
- **As a platform component:** build compose-rs's Android glue in-tree, as a system app or a library on the system image, linked against HWUI with matching headers.
- **Or make the libraries public on a development image:** add `libhwui.so` to `public.libraries.txt` on a `userdebug` or `eng` image, to experiment from an app.
- **Or, longer term, add a proper public API:** for example, an NDK display-list or `RenderNode` recording API covering paths, text and layers. That would let any native UI toolkit skip a Java replay.

All three need a device running a build you make. On a production device that means unlocking the bootloader (which wipes it) and flashing a `userdebug` image. The useful first experiment would be measuring how much of `Record View#draw()` and the replay disappears when compose-rs records into a `RenderNode`'s native canvas directly from Rust.

### D. Ship our own Skia (public NDK APIs only)
Draw with a Skia built into the app, into a `SurfaceView` (or `TextureView`), through `ANativeWindow` and Vulkan or EGL/GLES, all of them public NDK APIs. This is roughly how Flutter draws.
- **Most of it exists already.** compose-skia's renderer (the display-list walk, gradients, shadows, layers) and its SkParagraph text measurer run on desktop today, and `skia-safe` builds for `aarch64-linux-android`. The repaint-boundary groups and versions (D155, D270) map onto cached `SkPicture` recordings, the same way they map onto `RenderNode`s now.
- **Gains:** no JNI call per frame, no serialization and no Kotlin replay. Rust would drive Skia directly, on a render thread of its own.
- **Costs** (estimates, to be measured):
  - **APK size:** +5–8 MB for Skia and its text stack. Today's library is 3 MB uncompressed.
  - **Startup:** creating a Vulkan or GL context and Skia's GPU context, likely tens of milliseconds, where compose-rs now starts 19–28 % faster than Compose (round 8) ([12](12-android-performance.md)).
  - **Memory:** the app's own glyph caches and GPU resource cache, on top of HWUI's own.
  - **Text:** shaped by SkParagraph with fonts compose-rs loads, instead of the system's `StaticLayout`. System font fallback and locale-specific behaviour would need care to match.
  - **Integration:** a `SurfaceView` composites as its own layer, with known limitations for z-order, clipping, animations and transforms inside a View hierarchy. A `TextureView` composes like a View but adds a copy. Drawing inside Kotlin Compose screens (`AndroidView`) and the interaction with HWUI's damage and frame pacing would need checking.
  - **Pipeline:** compose-rs would own frame pacing (Choreographer), the swap chain, and GPU memory under pressure (`onTrimMemory`).

## Recommendation

- **For shipping:** keep A. Its indirection costs about 0.2 ms a frame, and it already beats Jetpack Compose on startup, memory, scrolling and animation ([12](12-android-performance.md)).
- **For "direct Skia" on a production device:** D is the only route that uses supported APIs. It's worth building as an experiment (an experimental `ComposeRsSurfaceView` next to `ComposeRsView`) and measuring with the same benchmark: startup, memory, per-frame main-thread and render work, and jank, against both A and Jetpack Compose. The open question is whether removing the replay outweighs the startup and memory costs.
- **For platform engineers:** C is where direct HWUI access makes sense. The most valuable outcome would be a supported native recording API, which would help any non-Java UI toolkit on Android, not just this one.
- **B is not a path.** Undocumented loader workarounds plus build-specific C++ ABI are not even a sound basis for a benchmark.

## See also

- [07-platforms.md](07-platforms.md), "Android": the glue as built.
- [12-android-performance.md](12-android-performance.md): the measurements against Jetpack Compose.
- Decisions D152 (serialized display list and its rejected alternatives: a JNI call per operation, shipping Skia), D157 (groups as `RenderNode`s by version), D190 (icon masks), D195 (render node reuse), D270 (the display list as a tree).
