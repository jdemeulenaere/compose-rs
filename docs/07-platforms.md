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

- **Desktop:** SkParagraph with the system font manager, as Compose Desktop uses through Skiko; tests measure with a fixed-advance measurer by default, so headless measurements are identical on every machine (no fonts are bundled: D94).
- **Android:** a Kotlin-side cache of `StaticLayout`s keyed by handle. Rust asks for a layout once per changed text, through one JNI call that returns the metrics. Drawing calls `layout.draw(canvas)` in the replayer. This is the text stack Android Compose itself draws with, so line breaks, hyphenation, fallback fonts and emoji match exactly.
- `BasicText` keeps its `ParagraphHandle` on the node, keyed on text, style and width constraint, so recomposing with the same text measures nothing.

## Desktop (`compose-desktop`)

- **One thread.** `application` creates the winit event loop on the main thread, owns the compositions, and runs everything there: no frame mailbox or input channel.
- **Windows** are Compose Desktop's: `application(App)`, where `#[composable] fn App()` holds `Window!(on_close_request = …, title = …, state = remember_window_state(size), { … })`s (a composable function, so the call macros memoize the windows' content lambdas). The application's content is a composition of its own whose nodes are windows (`WindowApplier`); each `Window` in it is opened, its content composed in the window's own `UiHost` and reading the application's state like any content, and closed (its composition disposed) when it leaves the composition. `visible = false` hides it, keeping its composition. The application recomposes before any window's frame, so a window never shows values from both sides of a change. The close button, Cmd+W and Ctrl+W call the window's `on_close_request`, which removes it or not. Cmd+Q (the macOS app menu's Quit) ends the application, as Compose Desktop's default quit. `exit_application()` ends it, as it ends once no window is left. A `WindowState` holds the window's size: the window follows it (applied at the loop's next turn), and it follows the user's resizing. Animations step on one window's frames; the others draw when what they show changes. `run_app(options, content)` is one window whose close request exits.
- **Skia surfaces** on the GPU: Metal on macOS, Vulkan or OpenGL on Linux, Direct3D on Windows. The binaries are prebuilt by `skia-safe`, so building the app doesn't compile Skia. This is the same size trade-off Compose Desktop makes by shipping Skiko.
- **The frame loop**, driven by winit's `about_to_wait` and `RedrawRequested`:
  1. **Events:** pointer, key and text events are dispatched; handlers run and write state, which invalidates scopes and schedules a frame.
  2. **Recompose** invalid scopes.
  3. **Layout and draw** from dirty nodes, recording the display list.
  4. **Render and present** through Skia, only when something was drawn (or the window resized): the display list is rendered in place on Skia's CPU raster surface and its pixels copied straight into the window's buffer (softbuffer), so a warm frame allocates nothing.
  5. **Prefetch** while waiting (`about_to_wait`), when no frame has work: `UiHost::prefetch` until the next frame is due (the last one plus a refresh interval; a whole interval when nothing animates), so lazy lists that scrolled compose and measure their next items; work left wakes the loop at the deadline for another try.

  The loop sleeps (`ControlFlow::Wait`) until there's a reason to work: an idle window costs 0% CPU. Tasks ([04](04-runtime.md)) wake it through an event-loop proxy from any thread; a pending `delay` makes it wake at its end (`WaitUntil`); and frames only animations wait for come one display refresh interval apart (the monitor's rate, 60 Hz otherwise), since the CPU raster surface has no vsync.
- **HiDPI.** Layout is in logical pixels (`Dp`), and rendering happens at the window's scale factor.
- **The platform's values.** `LOCAL_CONFIGURATION` is the window's size in dp and density, and the system's light or dark appearance as winit reports it (`ThemeChanged` updates it); window insets are zero; Escape that nothing focused takes goes back (dialogs close on it); images decode with Skia's codecs.
- **Input mapping:** pointer move, press, release and wheel (a cancelled touch cancels its gesture); keys with modifiers; committed text (sent whether or not the key was consumed: a focused text field consumes its typed keys, unless Ctrl or Cmd is held) and IME preedit (shown by the focused field; the candidate window is placed at its cursor with `set_ime_cursor_area`); focus, occlusion, resize, scale changes and close. Closing via the close button, Cmd+W or Cmd+Q (Ctrl+W elsewhere) ends `run_app`. A drag goes on outside the window. When a field commits a composition itself, the host resets the input method (`set_ime_allowed` off and on).
- **Accessibility** through AccessKit (`accesskit_winit`): the window is shown once the adapter is attached; while a screen reader is on, a frame that changed something rebuilds the AccessKit tree from the merged semantics tree (roles, labels from content descriptions or text, values, toggled, selected, disabled, bounds in physical pixels, the focus on the semantics node holding the focused node). A selected radio button is checked (`toggled`), a dialog is a modal `Dialog` node, and a slider a `Slider` with its numeric value, range and step. A screen reader's Click, Focus, Increment, Decrement and SetValue come back as semantics actions (`SemanticsAction::OnClick`, `RequestFocus`, `SetProgress`: a step, or a twentieth of a continuous range), performed by the node's semantics modifier nodes, as Compose's `SemanticsActions`. With no screen reader on, nothing is built.
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

A thin Kotlin layer (`android/compose-rs`, an AAR) and `crates/compose-android` (the JNI functions, the display list's serializer, the text measurer):

| Concern | Kotlin side | Crossings |
|---|---|---|
| Frames | a Choreographer callback calls `frame` (recompose, lay out, record); `onDraw` takes the bytes and replays them; the first frame is composed during the view's first layout, so the window's first frame shows the content | one per frame, one per draw |
| Prefetch | when `frame` says lazy lists wait for idle time, the view posts a message (run once the frame is done, as Compose's `AndroidPrefetchScheduler`) calling `prefetch` with the time to the next frame (the Choreographer frame's start plus the display's refresh period); work left, or no time, tries again after the next Choreographer frame; a hidden window prefetches nothing | one per idle time |
| Drawing | `Replayer.kt`: each operation a `Canvas` call; paths and images kept by id (defined once, dropped with their owner); a small filled path (at most 256 px each way, as icons) drawn from an `ALPHA_8` mask rasterized once per scale (at most 4 per path) and tinted by the paint, as Compose's `VectorPainter` caches an icon's bitmap (D190), the GPU rendering no path; each paragraph recorded once into a `RenderNode`, then drawn by reference; each repaint boundary's group (graphics layers, lazy items) recorded into a `RenderNode` kept by its key and version (D154, D157); a released paragraph's or gone group's node reused for the next one rather than left to the collector (D195); recorded again from the frame's bytes if the system dropped the recording (the window hid); a graphics layer's alpha, transform and clip set on its group's node each frame from the buffer's `LAYERS` section (nested groups in kept parents too), so a layer-only animation records nothing again; Rust copies an unchanged group's bytes from a cache by key and version | in the frame's buffer |
| Touch and keys | `onTouchEvent` forwards each pointer's samples with their event times (historical ones too: a fling's velocity comes from them); `onKeyDown`/`onKeyUp` the key and its character (without Ctrl, Alt or Meta, so shortcuts are known); a wheel's `ACTION_SCROLL` | one per sample, per key |
| Text measurement | `TextMeasurer.kt`: a `StaticLayout` per paragraph (a `TextPaint` shared by the paragraphs of a style, D195), its style runs and line options encoded in a request; metrics back through a shared buffer; offsets converted between UTF-8 and UTF-16 on the Rust side | one per laid-out paragraph |
| Soft keyboard | `onCreateInputConnection`: committed text, the composing text (shown in place), deletions as keys; a password field's input type keeps the keyboard from learning it | per edit |
| Accessibility | `Accessibility.kt`, an `AccessibilityNodeProvider`: the merged semantics tree as virtual views (texts, roles, states, ranges, click, focus and progress actions, explore-by-touch) | when a service asks after a change |
| Tasks | woken tasks and `delay`s become main-thread `Handler` posts (a new earliest timer wakes the host) | per tick |
| Lifecycle and configuration | density, font scale and layout direction passed to the composition, and Android's `Configuration` (screen size in dp, orientation, font scale, density, `uiMode`'s night mode) as `LOCAL_CONFIGURATION`; detaching from the window disposes the composition, keeping what its `remember_saveable`s hold for the next one | on change |
| Saved state | `onSaveInstanceState` saves the `remember_saveable` values (`saveState`: bytes in a `Bundle`), and the next composition (`create`) gets them back: an activity recreated for a configuration change or after the process was killed, and a view attached again. The view takes the id `R.id.compose_rs_view` unless given one, as views save state only with an id (Kotlin Compose's `AndroidView` saves it in its own registry) (D223) | on save, on create |
| Back | while the content has an enabled `BackHandler` (a frame's flag), the view intercepts back: through an AndroidX activity's `OnBackPressedDispatcher` when the app has one (AndroidX is a compile-only dependency of the library, loaded only if present), else the window's `OnBackInvokedDispatcher` (API 33+) and the back key reaching the view; otherwise the activity gets back and finishes (D221) | per back |
| Window insets | `setOnApplyWindowInsetsListener`: status bars, navigation bars, caption bar, keyboard and cutout insets (by type, API 30+; the system windows' on API 29), for the insets paddings | on change |
| Images | `ComposeRsImages.decode`: `BitmapFactory` to a premultiplied ARGB_8888 bitmap, whose pixels Rust copies into an `ImageBitmap` (`decode_image_bytes`, `image_resource`), sent to the replayer as any image (D224) | one per decode |
| Properties and events | `setProp(name, value)` (strings, numbers, booleans; set again on each composition) reach `LOCAL_HOST_PROPS`, recomposing their readers; `LOCAL_HOST_EVENTS`' events call the `setOnEventListener` listener with a name and a payload (D226) | per change, per event |
| Tracing | the frame's phases as `ATrace` sections (`compose-rs: recompose`, `layout`, `draw`, `text layout`, `serialize`, `replay`, `prefetch`), beside the system's in Perfetto | none |

The **frame and composition thread** is Android's main thread. Choreographer drives frames only while scopes are invalid or animations run, so an idle view costs nothing. Rendering is the system's: a hardware-accelerated view's `Canvas` is recorded by the UI thread and drawn by the render thread on the GPU (Vulkan on recent devices), as Compose's is.

**Packaging:** the app's Rust library (its root composables named with `compose_android::export_apps!`) builds as a `cdylib` for arm64 with `android/build-rust.sh` (the NDK's linker, no `cargo-ndk` needed; segments aligned for 16 KB pages, as Android 15+ requires), run by Gradle before each build; `ComposeRsView(context, "name")` shows one of its apps. `android/demo` hosts the examples (its counter tab reads the Kotlin tab as a property and sends Kotlin an event shown in a toast); `android/bench-compose` and `android/bench-rs` are the same app in Kotlin Compose and compose-rs, measured by `android/bench/measure.py` ([12](12-android-performance.md)).

### Later option: native Skia on Android

If profiling ever shows the replayer as a bottleneck (very heavy custom drawing), a `SkiaRenderer` can render natively into an `AHardwareBuffer`. The buffer is wrapped once as a `Bitmap` (`Bitmap.wrapHardwareBuffer`, API 29+) that `onDraw` draws, so the view is still composited like Compose content. The costs are shipping Skia (roughly 10 MB per ABI), pixels and text that may differ slightly from the system's, and managing a GPU context of its own. The `Renderer` interface keeps this a local change.

## Later platforms

- **iOS:** Skia on Metal inside a `UIView`, with UIKit glue for input, keyboard and accessibility.
- **Web:** CanvasKit (Skia compiled to WebAssembly), or a Canvas2D replayer like Android's.
