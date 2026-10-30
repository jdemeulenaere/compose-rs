package rs.compose.android

import java.nio.ByteBuffer

/** The Rust side (`crates/compose-android/src/bridge.rs`), in the app's compose-rs library. */
internal object ComposeRsNative {
    @JvmStatic
    external fun create(
        view: ComposeRsView,
        measurer: TextMeasurer,
        results: ByteBuffer,
        app: String,
        width: Int,
        height: Int,
        density: Float,
        fontScale: Float,
        rtl: Boolean,
        saved: ByteArray?,
        optimizations: Long,
        lifecycleState: Int,
        viewModelStore: Long,
    ): Long

    @JvmStatic external fun destroy(host: Long)

    /**
     * Whether the view shows with performance hints enabled: its frames then report the
     * RenderThread's work to the process's hint session (`performance_hint.rs`, D390).
     */
    @JvmStatic external fun setPerformanceHint(host: Long, shown: Boolean)

    /** compose-rs's optimizations ([ComposeRsOptimizations] bits), from the next frame on. */
    @JvmStatic external fun setOptimizations(host: Long, optimizations: Long)

    /** The bits a list of optimizations names (Rust's names), or -1 for an unknown one. */
    @JvmStatic external fun parseOptimizations(list: String): Long

    /** The view asks the content for the focus: whether the content has it. */
    @JvmStatic
    external fun ownerRequestedFocus(
        host: Long,
        direction: Int,
        hasRect: Boolean,
        left: Float,
        top: Float,
        right: Float,
        bottom: Float,
    ): Boolean

    /** The view lost the focus. */
    @JvmStatic external fun ownerLostFocus(host: Long)

    /** The view system's touch mode: the input mode (Keyboard when it's false). */
    @JvmStatic external fun setInputMode(host: Long, touch: Boolean)

    @JvmStatic external fun resize(host: Long, width: Int, height: Int)

    @JvmStatic external fun setDensity(host: Long, density: Float, fontScale: Float)

    /** Where the view's lifecycle is, by the ordinal of AndroidX's `Lifecycle.State`. */
    @JvmStatic external fun setLifecycleState(host: Long, state: Int)

    /** A view model store surviving hosts, by its token, for [AndroidXViewModels]. */
    @JvmStatic external fun newViewModelStore(): Long

    /** Clears that store for good. */
    @JvmStatic external fun clearViewModelStore(token: Long)

    /** `Settings.Global.ANIMATOR_DURATION_SCALE`: the main thread's animations follow it. */
    @JvmStatic external fun setAnimatorDurationScale(scale: Float)

    /** The setting's not watched any more: the next animation calls [readAnimatorScale]. */
    @JvmStatic external fun readAnimatorScaleAgain()

    /** Recomposes, lays out and draws what changed: [Frame] flags, and the next timer. */
    @JvmStatic external fun frame(host: Long): Long

    /**
     * Between frames: lazy lists that scrolled compose and measure the items they'll reach next,
     * for at most [availableNanos] ([unboundedFirst]: the first request without a deadline, as
     * Compose's scheduler when no frame was drawn for two intervals). Returns whether work is
     * left, for the next idle time.
     */
    @JvmStatic external fun prefetch(host: Long, availableNanos: Long, unboundedFirst: Boolean): Boolean

    /**
     * When the frame after the last Choreographer callback's should start (`System.nanoTime`):
     * the vsync after the last one at or before [startedNanos], the callback's start, on
     * [frameTimeNanos]' phase, so a frame time shifted by buffer stuffing recovery still gives
     * the next vsync (`frame_time.rs`); 0 without a frame time.
     */
    @JvmStatic external fun nextFrameNanos(frameTimeNanos: Long, intervalNanos: Long, startedNanos: Long): Long

    /**
     * What `onDraw` replays (resource changes, then the last frame's display list) is ready in
     * [drawingBuffer]: its length, with [NEW_DRAWING_BUFFER] set when that buffer moved.
     */
    @JvmStatic external fun takeDrawing(host: Long): Long

    /** A buffer over the drawing's memory, until [takeDrawing] says it moved. */
    @JvmStatic external fun drawingBuffer(host: Long): ByteBuffer?

    const val NEW_DRAWING_BUFFER = 1L shl 32

    /** Whether an item prefetched waits to be drawn ahead ([drawAhead]). */
    @JvmStatic external fun hasDrawAhead(host: Long): Boolean

    /**
     * Between frames: draws ahead the next item prefetched and not placed yet, its groups'
     * records ready in [drawingBuffer] (after resource changes) for `Replayer.recordAhead`: their
     * length as [takeDrawing] returns it (0: nothing drawn), with [MORE_AHEAD] if another waits.
     */
    @JvmStatic external fun drawAhead(host: Long): Long

    const val MORE_AHEAD = 1L shl 33

    /** Gives up drawing ahead the items waiting: the frames placing them draw them. */
    @JvmStatic external fun cancelDrawAhead(host: Long)

    /** Action 0 down, 1 move, 2 up, 3 cancel; [age]: how long ago it happened (ns). */
    @JvmStatic external fun touch(host: Long, action: Int, pointer: Int, x: Float, y: Float, age: Long)

    @JvmStatic
    external fun key(host: Long, key: Int, character: Int, down: Boolean, meta: Int): Boolean

    @JvmStatic external fun text(host: Long, text: String)

    /** A wheel's scroll at ([x], [y]), by ([dx], [dy]) pixels. */
    @JvmStatic external fun scroll(host: Long, x: Float, y: Float, dx: Float, dy: Float)

    @JvmStatic external fun setRtl(host: Long, rtl: Boolean)

    /**
     * The focused text field's `EditorInfo` (`input_method.rs`): `imeOptions` in the high 32 bits,
     * `inputType` in the low ones.
     */
    @JvmStatic external fun textInputOptions(host: Long): Long

    /** The soft keyboard's action key (`EditorInfo.IME_ACTION_*`): whether the field took it. */
    @JvmStatic external fun performEditorAction(host: Long, action: Int): Boolean

    /** The milliseconds to the next timer's end, or -1. */
    @JvmStatic external fun nextTimer(): Long

    @JvmStatic external fun preedit(host: Long, text: String, cursor: Int)

    /** The merged semantics tree, for accessibility (`accessibility.rs`). */
    @JvmStatic external fun semantics(host: Long): ByteBuffer?

    /** An accessibility action on virtual view [id]: 0 click, 1 focus, 2 set progress. */
    @JvmStatic external fun performAction(host: Long, id: Int, action: Int, value: Float): Boolean

    /** What the content's `remember_saveable`s hold, for the saved instance state, or null. */
    @JvmStatic external fun saveState(host: Long): ByteArray?

    /** The user went back: whether a `BackHandler` took it. */
    @JvmStatic external fun onBackPressed(host: Long): Boolean

    /** A back gesture starts (API 34+): its end is [onBackPressed] or [onBackCancelled]. */
    @JvmStatic
    external fun onBackStarted(
        host: Long,
        swipeEdge: Int,
        progress: Float,
        touchX: Float,
        touchY: Float,
        frameTimeMillis: Long,
    )

    /** It goes on. */
    @JvmStatic
    external fun onBackProgressed(
        host: Long,
        swipeEdge: Int,
        progress: Float,
        touchX: Float,
        touchY: Float,
        frameTimeMillis: Long,
    )

    /** It's given up. */
    @JvmStatic external fun onBackCancelled(host: Long)

    /** Android's `Configuration`, as the content's `LOCAL_CONFIGURATION`. */
    @JvmStatic
    external fun setConfiguration(
        host: Long,
        widthDp: Int,
        heightDp: Int,
        landscape: Boolean,
        fontScale: Float,
        density: Float,
        night: Boolean,
    )

    /**
     * The window's insets in pixels, 4 per insets (left, top, right, bottom), and the types
     * hidden: [ComposeRsInsets.values].
     */
    @JvmStatic external fun setWindowInsets(host: Long, insets: IntArray)

    /**
     * The display cutout's outline, as [android.graphics.Path.approximate] gives it, null for
     * none: [ComposeRsInsets.cutoutPath].
     */
    @JvmStatic external fun setWindowCutoutPath(host: Long, points: FloatArray?)

    /** A property for the content: [kind] 0 removes it, 1 is [text], 2 [number], 3 a boolean. */
    @JvmStatic external fun setProp(host: Long, name: String, kind: Int, text: String?, number: Double)

    /** Runs the timers due and the tasks woken: the next timer, and whether frames are wanted. */
    @JvmStatic external fun tick(): Long

    /** An animation's first read of the setting (`bridge.rs`): it's watched from then on. */
    @JvmStatic
    fun readAnimatorScale(): Float = ComposeRsRuntime.readAnimatorScale()

    /** A task woke, on any thread: a tick on the main thread. */
    @JvmStatic
    fun wake() {
        ComposeRsRuntime.wake()
    }
}

/** What [ComposeRsNative.frame] returns. */
internal object Frame {
    const val DREW = 1L
    const val AGAIN = 2L
    const val TEXT_INPUT = 4L
    const val RESET_INPUT = 8L
    const val SEMANTICS = 16L
    const val PREFETCH = 32L
    const val BACK = 64L
    const val SHOW_KEYBOARD = 128L
    const val HIDE_KEYBOARD = 256L
    const val OWNER_FOCUS = 512L
    const val OWNER_CLEAR = 1024L

    /** The milliseconds to the next timer, or -1. */
    fun nextTimer(result: Long): Long = (result shr 16) - 1
}
