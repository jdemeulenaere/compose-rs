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
- **Android:** a Kotlin-side cache of layouts keyed by handle, chosen as Compose's `TextLayout` chooses (D330): a `BoringLayout` for a single left-to-right line that fits its width (API 33+; its metrics give the text's width, so no second measure), else a `StaticLayout`. Rust asks for a layout once per changed text, through one JNI call that returns the metrics. Drawing calls `layout.draw(canvas)` in the replayer. This is the text stack Android Compose itself draws with, so line breaks, hyphenation, fallback fonts and emoji match exactly. Whether lines were cut (`did_exceed_max_lines`) comes back with the metrics: more lines than the max, or the last one ellipsized, as Compose's `TextLayout`.
- `BasicText` keeps its `ParagraphHandle` on the node, keyed on text, style and width constraint, so recomposing with the same text measures nothing.

## Desktop (`compose-desktop`)

- **One thread.** `application` creates the winit event loop on the main thread, owns the compositions, and runs everything there: no frame mailbox or input channel.
- **Windows** are Compose Desktop's: `application(App)`, where `#[composable] fn App()` holds `Window!(on_close_request = …, title = …, state = remember_window_state(size), { … })`s (a composable function, so the call macros memoize the windows' content lambdas). The application's content is a composition of its own whose nodes are windows (`WindowApplier`); each `Window` in it is opened, its content composed in the window's own `UiHost` and reading the application's state like any content, and closed (its composition disposed) when it leaves the composition. `visible = false` hides it, keeping its composition. The application recomposes before any window's frame, so a window never shows values from both sides of a change. The close button, Cmd+W and Ctrl+W call the window's `on_close_request`, which removes it or not. Cmd+Q (the macOS app menu's Quit) ends the application, as Compose Desktop's default quit. `exit_application()` ends it, as it ends once no window is left. A `WindowState` holds the window's size: the window follows it (applied at the loop's next turn), and it follows the user's resizing. Animations step on one window's frames; the others draw when what they show changes. `run_app(options, content)` is one window whose close request exits.
- **Skia surfaces** on the GPU: Metal on macOS, Vulkan or OpenGL on Linux, Direct3D on Windows. The binaries are prebuilt by `skia-safe`, so building the app doesn't compile Skia. On Linux they link the system's FreeType and Fontconfig, so building needs their development packages and `pkg-config`; compose-skia's build script warns, with the install command, when pkg-config can't find them. This is the same size trade-off Compose Desktop makes by shipping Skiko.
- **The frame loop**, driven by winit's `about_to_wait` and `RedrawRequested`:
  1. **Events:** pointer, key and text events are dispatched; handlers run and write state, which invalidates scopes and schedules a frame.
  2. **Recompose** invalid scopes.
  3. **Layout and draw** from dirty nodes, recording the display list.
  4. **Render and present** through Skia, only when something was drawn (or the window resized): the display list is rendered in place on Skia's CPU raster surface and its pixels copied straight into the window's buffer (softbuffer), so a warm frame allocates nothing.
  5. **Prefetch** while waiting (`about_to_wait`), when no frame has work: `UiHost::prefetch` until the next frame is due (the last one plus a refresh interval; a whole interval when nothing animates), so lazy lists that scrolled compose and measure their next items; work left wakes the loop at the deadline for another try.

  The loop sleeps (`ControlFlow::Wait`) until there's a reason to work: an idle window costs 0% CPU. Tasks ([04](04-runtime.md)) wake it through an event-loop proxy from any thread; a pending `delay` makes it wake at its end (`WaitUntil`); and frames only animations wait for come one display refresh interval apart (the monitor's rate, 60 Hz otherwise), since the CPU raster surface has no vsync.
- **HiDPI.** Layout is in logical pixels (`Dp`), and rendering happens at the window's scale factor.
- **The platform's values.** `LOCAL_CONFIGURATION` is the window's size in dp and density, and the system's light or dark appearance as winit reports it (`ThemeChanged` updates it); window insets are zero; Escape that nothing focused takes goes back (dialogs close on it); images decode with Skia's codecs.
- **Platform services and logging.** `LOCAL_CLIPBOARD_MANAGER` is the system clipboard (`arboard`; one in memory if it can't be opened); `LOCAL_URI_HANDLER` opens a link `PlatformUriHandler` checked (allowed schemes only, D283) with `open` (macOS), `xdg-open` (Linux, BSDs) or `rundll32 url.dll,FileProtocolHandler` (Windows, the URI given verbatim with `raw_arg`), never through a shell, its input and output closed; `LOCAL_HAPTIC_FEEDBACK` does nothing. `application` installs a stderr logger for the `log` crate (`COMPOSE_LOG` filters it by level and target) unless the app set one (D280, D281).
- **Input mapping:** pointer move, press, release and wheel (a cancelled touch cancels its gesture); keys with modifiers; committed text (sent whether or not the key was consumed: a focused text field consumes its typed keys, unless Ctrl or Cmd is held) and IME preedit (shown by the focused field; the candidate window is placed at its cursor with `set_ime_cursor_area`); focus, occlusion, resize, scale changes and close. Closing via the close button, Cmd+W or Cmd+Q (Ctrl+W elsewhere) ends `run_app`. A drag goes on outside the window. When a field commits a composition itself, the host resets the input method (`set_ime_allowed` off and on).
- **Accessibility** through AccessKit (`accesskit_winit`): the window is shown once the adapter is attached; while a screen reader is on, a frame that changed something rebuilds the AccessKit tree from the merged semantics tree (roles, labels from content descriptions or text, values, toggled, selected, disabled, bounds in physical pixels, the focus on the semantics node holding the focused node). A selected radio button is checked (`toggled`), a dialog is a modal `Dialog` node, and a slider a `Slider` with its numeric value, range and step. A screen reader's Click, Focus, Increment, Decrement and SetValue come back as semantics actions (`SemanticsAction::OnClick`, `RequestFocus`, `SetProgress`: a step, or a twentieth of a continuous range), performed by the node's semantics modifier nodes, as Compose's `SemanticsActions`. With no screen reader on, nothing is built.
- **Screenshots and statistics.** `COMPOSE_SCREENSHOT=path.png` renders the last frame through Skia's CPU raster backend. `COMPOSE_FRAME_STATS=1` reports events, recomposition, layout, draw and render times, input-to-present, and allocations per frame (with the `compose-test` counting allocator).

## Android (`compose-android`)

compose-rs draws on Android through the public `Canvas` and `RenderNode` APIs, which HWUI then renders with Skia on the GPU; it doesn't call `libhwui` or Skia directly. [13](13-android-drawing-paths.md) explains why, what a device's `libhwui.so` exposes, and the alternatives.

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
| Frames | a Choreographer callback calls `frame` (recompose, lay out, record); `onDraw` takes the bytes (`takeDrawing` returns their length; one direct `ByteBuffer` over Rust's memory is kept, and made again only when that memory moves) and replays them; the first frame is composed during the view's first layout, so the window's first frame shows the content | one per frame, one per draw |
| Prefetch | when `frame` says lazy lists wait for idle time, the view posts a message (run once the frame is done, as Compose's `AndroidPrefetchScheduler`) calling `prefetch` with the time to the next frame (the Choreographer frame's start plus the display's refresh period); work left, or no time, tries again after the next Choreographer frame; a hidden window prefetches nothing. The paragraphs a prefetch lays out are recorded into their render nodes after it, while one fits before the next frame (left ones first in the next idle time), so the frame that scrolls their item in only draws them (D322) | one per idle time |
| Drawing | `Replayer.kt`: each operation a `Canvas` call; paths and images kept by id (defined once, dropped with their owner), a path drawn or clipped to (`clipPath`); a gradient written whole in each paint drawing it (not as a shader index, which names its recording's table, so a cached record stands on its own), built once into a `LinearGradient`, `RadialGradient` or `SweepGradient` kept by its bytes (D210); a small filled path (at most 256 px each way, as icons) drawn from an `ALPHA_8` mask rasterized once per scale (at most 4 per path) and tinted by the paint, as Compose's `VectorPainter` caches an icon's bitmap (D190), the GPU rendering no path; each paragraph recorded once into a `RenderNode`, then drawn by reference; each repaint boundary's group (graphics layers, lazy items) recorded into a `RenderNode` kept by its key and version (D154, D157); a released paragraph's or gone group's node reused for the next one rather than left to the collector (D195); recorded again from the frame's bytes if the system dropped the recording (the window hid); framework calls that the boot image doesn't compile (`ByteBuffer.getFloat`, `Canvas.isHardwareAccelerated`, `RenderNode`'s size getters and some setters) avoided, as they'd run in the interpreter: floats read as their int bits, a node's size tracked in Kotlin, a layer's properties set only where they changed (D321); a graphics layer's alpha, transform and clip set on its group's node each frame from the buffer's `LAYERS` section (nested groups in kept parents too), so a layer-only animation records nothing again; Rust copies an unchanged group's bytes from a cache by key and version, without reading its operations (the frame holds each group's recording by reference: D270); every group's record comes before the frame's operations and names the groups drawn in it by key (`GROUP_REF`), so a group whose nested group changed keeps its bytes, its version and its recording, the nested group's node drawn by reference (D323) | in the frame's buffer |
| Touch and keys | `onTouchEvent` forwards each pointer's samples with their event times (historical ones too: a fling's velocity comes from them); `onKeyDown`/`onKeyUp` the key and its character (without Ctrl, Alt or Meta, so shortcuts are known); a wheel's `ACTION_SCROLL` | one per sample, per key |
| Text measurement | `TextMeasurer.kt`: a `BoringLayout` per single-line paragraph that fits (Compose's choice, D330; in debuggable apps checked against the `StaticLayout`), else a `StaticLayout` (a `TextPaint` shared by the paragraphs of a style, D195), its style runs and line options encoded in a request; metrics back through a shared buffer; offsets converted between UTF-8 and UTF-16 on the Rust side | one per laid-out paragraph |
| Soft keyboard | `onCreateInputConnection`: committed text, the composing text (shown in place), deletions as keys. Its `EditorInfo` is the focused field's `ImeOptions` mapped as Compose's `EditorInfo.update` (in Rust, `input_method.rs`; Kotlin applies two ints): keyboard type to the text, number or phone class with the URI, email, password, number password or decimal variation (Ascii adds `IME_FLAG_FORCE_ASCII`); for text, a multi-line flag unless single line, the capitalization and auto-correct flags; the action to `IME_ACTION_*` (a single line's `Default` is `Done`; a multi-line `Default` adds `IME_FLAG_NO_ENTER_ACTION`, so Enter types a new line); a field masking a password with a `Text` keyboard gets a password's. Other options for the focused field (another field, or its own changed) restart the input. The keyboard's action key (`performEditorAction`) is the field's `perform_ime_action`; Enter in a single line performs it in the field. What the content asks of the keyboard (`SoftwareKeyboardController`, a field's `Done`) comes with the frame's flags: `showSoftInput` or `hideSoftInputFromWindow` (D250) | per edit; options per focus |
| Accessibility | `Accessibility.kt`, an `AccessibilityNodeProvider`: the merged semantics tree as virtual views (texts, roles, states, ranges, click, focus and progress actions, explore-by-touch) | when a service asks after a change |
| Tasks | woken tasks and `delay`s become main-thread `Handler` posts (a new earliest timer wakes the host); the tasks a frame resumes (`with_frame_nanos`) run in that frame, their resumption posting nothing (D321) | per tick |
| Lifecycle and configuration | density, font scale and layout direction passed to the composition, and Android's `Configuration` (screen size in dp, orientation, font scale, density, `uiMode`'s night mode) as `LOCAL_CONFIGURATION`; detaching from the window disposes the composition, keeping what its `remember_saveable`s hold for the next one | on change |
| Saved state | `onSaveInstanceState` saves the `remember_saveable` values (`saveState`: bytes in a `Bundle`), and the next composition (`create`) gets them back: an activity recreated for a configuration change or after the process was killed, and a view attached again. The view takes the id `R.id.compose_rs_view` unless given one, as views save state only with an id (Kotlin Compose's `AndroidView` saves it in its own registry) (D223) | on save, on create |
| Back | while the content has an enabled `BackHandler` (a frame's flag), the view intercepts back: through an AndroidX activity's `OnBackPressedDispatcher` when the app has one (AndroidX is a compile-only dependency of the library, loaded only if present), else the window's `OnBackInvokedDispatcher` (API 33+) and the back key reaching the view; otherwise the activity gets back and finishes (D221) | per back |
| Window insets | `setOnApplyWindowInsetsListener`: status bars, navigation bars, caption bar, keyboard and cutout insets (by type, API 30+; the system windows' on API 29), for the insets paddings | on change |
| Images | `ComposeRsImages.decode`: `BitmapFactory` to a premultiplied ARGB_8888 bitmap, whose pixels Rust copies into an `ImageBitmap` (`decode_image_bytes`, `remember_image_resource`), sent to the replayer as any image (D224) | one per decode |
| Properties and events | `setProp(name, value)` (strings, numbers, booleans; set again on each composition) reach `LOCAL_HOST_PROPS`, recomposing their readers; `LOCAL_HOST_EVENTS`' events reach the `setOnEventListener` listener with a name and a payload, posted to the main looper so it never runs inside a call into Rust (D226) | per change, per event |
| Platform services | the clipboard (`clipboardText`, `setClipboardText`: `LOCAL_CLIPBOARD_MANAGER` and text fields); `openUri` starts an `ACTION_VIEW` intent for a link Rust's `PlatformUriHandler` checked (`LOCAL_URI_HANDLER`, allowed schemes only, D283; a `RuntimeException` starting it, nothing to open it included, is logged; `FLAG_ACTIVITY_NEW_TASK` only when no activity is found through the context's wrappers), `performHapticFeedbackType` calls `View.performHapticFeedback` with the type's `HapticFeedbackConstants`, Compose's fallbacks below API 30 and 34 (`LOCAL_HAPTIC_FEEDBACK`); both posted to the main looper, as events are (D281) | per call |
| Logging | the `log` crate's records go to logcat (`__android_log_write`), tagged with the crate that logged (`compose_runtime`, the app's), split past 4000 bytes; installed by `JNI_OnLoad`, before the app's code, unless `export_apps!` starts with `logger = false;`; panics still go to `compose-rs` (D280) | none |
| Tracing | the frame's phases as `ATrace` sections (`compose-rs: recompose`, `layout`, `draw`, `text layout`, `serialize`, `replay`, `prefetch`), beside the system's in Perfetto | none |

The **frame and composition thread** is Android's main thread. Choreographer drives frames only while scopes are invalid or animations run, so an idle view costs nothing. Rendering is the system's: a hardware-accelerated view's `Canvas` is recorded by the UI thread and drawn by the render thread on the GPU (Vulkan on recent devices), as Compose's is.

**Packaging:** the app's Rust library (its root composables named with `compose_android::export_apps!`) builds as a `cdylib` for arm64 with `android/build-rust.sh` (the NDK's linker, no `cargo-ndk` needed; segments aligned for 16 KB pages, as Android 15+ requires), run by Gradle on each build as a task whose output directory is the variant's `jniLibs` (D300), and stripped by the Android Gradle plugin with the NDK the apps pin (`ndkVersion`, D301); `ComposeRsView(context, "name")` shows one of its apps. `android/demo` hosts the examples (its counter tab reads the Kotlin tab as a property and sends Kotlin an event shown in a toast); `android/bench-compose` and `android/bench-rs` are the same app in Kotlin Compose and compose-rs, measured by `android/bench/measure.py` ([12](12-android-performance.md)).

### Later option: native Skia on Android

If profiling ever shows the replayer as a bottleneck (very heavy custom drawing), a `SkiaRenderer` can render natively into an `AHardwareBuffer`. The buffer is wrapped once as a `Bitmap` (`Bitmap.wrapHardwareBuffer`, API 29+) that `onDraw` draws, so the view is still composited like Compose content. The costs are shipping Skia (roughly 10 MB per ABI), pixels and text that may differ slightly from the system's, and managing a GPU context of its own. The `Renderer` interface keeps this a local change.

## Later platforms

- **iOS:** Skia on Metal inside a `UIView`, with UIKit glue for input, keyboard and accessibility.
- **Web:** CanvasKit (Skia compiled to WebAssembly), or a Canvas2D replayer like Android's.
