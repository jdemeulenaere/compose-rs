# 7. Platforms and rendering

The two first targets are **desktop** (macOS, Windows, Linux) and **Android**, with the goal of looking and behaving as Kotlin Compose does on each. Compose draws with Skia on both: through Skiko on desktop, and through the system renderer (HWUI, built on Skia) on Android. So this design uses Skia too, in the way each platform makes possible:

| | Desktop | Android |
|---|---|---|
| Drawing | Skia through `skia-safe` (prebuilt binaries), on the GPU | the system `android.graphics.Canvas`, through a display-list replayer: no Skia shipped |
| Text | Skia's paragraph module (SkParagraph: HarfBuzz shaping, ICU line breaking and bidi) | Android's own text stack (`TextPaint`, `StaticLayout`) through JNI |
| Host | a winit window (`run_app`) | a custom `View`, usable inside a Kotlin Compose UI through `AndroidView` |
| Matches | Compose Desktop's drawing and text | Android Compose's drawing and text, pixel for pixel, since both use the system renderer |

## The rendering interface

`compose-ui` never draws directly. Its draw pass records a **display list**, and a `Renderer` backend replays it.

- **Display-list operations** are a subset of the `Canvas` API that both Skia and Android's `Canvas` support directly:
  - `save` and `restore`;
  - clips: rectangle, rounded rectangle, path;
  - transforms: translate, scale, rotate, matrix;
  - shapes: `draw_rect`, `draw_round_rect`, `draw_path`, `draw_image`;
  - text: `draw_paragraph`;
  - a `Paint` with colour, stroke, shader (gradients) and blend mode.

  Keeping to that subset makes both backends a straightforward translation.
- **Paragraphs** are created and measured by the `TextMeasurer` backend, and drawn by handle. The same platform object that measured a text draws it, so measurement and drawing can never disagree.
- **Backends:**
  - `SkiaRenderer` on desktop, with a GPU surface for windows and a CPU raster surface for tests and screenshots;
  - `AndroidCanvasRenderer` on Android;
  - later, if wanted, a native Skia renderer on Android (see below).

## Text

```rust
pub trait TextMeasurer {
    fn layout(&mut self, text: &AnnotatedString, style: &TextStyle, constraints: Constraints) -> ParagraphHandle;
    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics; // size, baselines, line metrics
    fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition; // for selection and the cursor
    fn release(&mut self, paragraph: ParagraphHandle);
}
```

- **Desktop:** SkParagraph with the system font manager, as Compose Desktop uses through Skiko, plus bundled fonts for tests, so headless measurements are identical on every machine.
- **Android:** a Kotlin-side cache of `StaticLayout`s keyed by handle. Rust asks for a layout once per changed text, through one JNI call that returns the metrics. Drawing calls `layout.draw(canvas)` in the replayer. This is the text stack Android Compose itself draws with, so line breaks, hyphenation, fallback fonts and emoji match exactly.
- `BasicText` keeps its `ParagraphHandle` on the node, keyed on text, style and width constraint, so recomposing with the same text measures nothing.

## Desktop (`compose-desktop`)

- **One thread.** `run_app` creates the winit event loop on the main thread, opens the window, owns the `Composition`, and runs everything there: no frame mailbox or input channel.
- **Skia surfaces** on the GPU: Metal on macOS, Vulkan or OpenGL on Linux, Direct3D on Windows. The binaries are prebuilt by `skia-safe`, so building the app doesn't compile Skia. This is the same size trade-off Compose Desktop makes by shipping Skiko.
- **The frame loop**, driven by winit's `about_to_wait` and `RedrawRequested`:
  1. **Events:** pointer, key and text events are dispatched; handlers run and write state, which invalidates scopes and schedules a frame.
  2. **Recompose** invalid scopes.
  3. **Layout and draw** from dirty nodes, recording the display list.
  4. **Render and present** through Skia, only when something was drawn (or the window resized): the display list is rendered in place on Skia's CPU raster surface and its pixels copied straight into the window's buffer (softbuffer), so a warm frame allocates nothing.

  The loop sleeps (`ControlFlow::Wait`) until there's a reason to work: an idle window costs 0% CPU.
- **HiDPI.** Layout is in logical pixels (`Dp`), and rendering happens at the window's scale factor.
- **Input mapping:** pointer move, press, release and wheel (a cancelled touch cancels its gesture); keys with modifiers; committed text (sent whether or not the key was consumed: a focused text field consumes its typed keys, unless Ctrl or Cmd is held) and IME preedit; focus, occlusion, resize, scale changes and close. Closing via the close button, Cmd+W or Cmd+Q ends `run_app`.
- **Screenshots and statistics.** `COMPOSE_SCREENSHOT=path.png` renders the last frame through Skia's CPU raster backend. `COMPOSE_FRAME_STATS=1` reports events, recomposition, layout, draw and render times, input-to-present, and allocations per frame (with the `compose-test` counting allocator).

## Android (`compose-android`)

### Embedding

The main use is a compose-rs UI **inside an ordinary Kotlin Compose UI**:

```kotlin
@Composable
fun Screen() {
    Column {
        Text("Kotlin Compose above")
        AndroidView(factory = { context -> ComposeRsView(context, ComposeRs.entry("todo_app")) })
    }
}
```

`ComposeRsView` is a plain `View` that draws in `onDraw(canvas)`. So it's composited exactly as Compose content is: Compose's transforms, clipping, alpha, scrolling and z-order apply to it. A `ComposeRsActivity` wraps the same view for full-screen apps.

**Rejected:** a `SurfaceView` with its own renderer. It's composited as a separate layer that Compose's transforms, clipping and scrolling don't reach reliably, which makes it unfit for embedding.

### Drawing without shipping Skia

The NDK offers no C API for `Canvas`: drawing with the system's Skia means the `android.graphics` API. The system's own Skia library is private to the platform, and apps can't load it. To keep crossings into Java (JNI) to a constant number per frame:

1. **On a Choreographer frame callback**, Kotlin calls Rust **once**. Rust recomposes, lays out and draws, writing the display list into a direct `ByteBuffer` that Rust and Kotlin share without copying. It returns whether anything changed, and Kotlin calls `invalidate()` if so.
2. **In `onDraw(canvas)`**, a small Kotlin replayer reads the buffer and issues the `Canvas` calls: the same calls Android Compose's own drawing makes, with only one extra crossing.

### The rest of the platform glue

This is a thin Kotlin layer, part of an AAR shipped with the Rust library:

| Concern | Kotlin side | Crossings |
|---|---|---|
| Touch and keys | `onTouchEvent`, `onKeyEvent` forward batched events | one per event batch |
| Text measurement | `StaticLayout` cache keyed by paragraph handle | one per changed text |
| Soft keyboard | `onCreateInputConnection` returns an `InputConnection` bridged to the focused `TextFieldState` | per edit |
| Accessibility | an `AccessibilityNodeProvider` built from the semantics tree | when the system asks |
| Lifecycle and configuration | density, font scale and dark mode passed to the composition; detaching from the window disposes it | on change |

The **frame and composition thread** is Android's main thread. Choreographer drives frames only while scopes are invalid, so an idle view costs nothing.

**Packaging:** the Rust code builds as a `cdylib` per ABI (arm64-v8a, x86_64) with `cargo-ndk`, and an AAR bundles it with the Kotlin shim.

### Later option: native Skia on Android

If profiling ever shows the replayer as a bottleneck (very heavy custom drawing), a `SkiaRenderer` can render natively into an `AHardwareBuffer`. The buffer is wrapped once as a `Bitmap` (`Bitmap.wrapHardwareBuffer`, API 29+) that `onDraw` draws, so the view is still composited like Compose content. The costs are shipping Skia (roughly 10 MB per ABI), pixels and text that may differ slightly from the system's, and managing a GPU context of its own. The `Renderer` interface keeps this a local change.

## Later platforms

- **iOS:** Skia on Metal inside a `UIView`, with UIKit glue for input, keyboard and accessibility.
- **Web:** CanvasKit (Skia compiled to WebAssembly), or a Canvas2D replayer like Android's.
