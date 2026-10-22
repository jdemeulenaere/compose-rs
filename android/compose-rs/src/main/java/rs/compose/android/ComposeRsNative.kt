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
    ): Long

    @JvmStatic external fun destroy(host: Long)

    @JvmStatic external fun resize(host: Long, width: Int, height: Int)

    @JvmStatic external fun setDensity(host: Long, density: Float, fontScale: Float)

    /** Recomposes, lays out and draws what changed: [Frame] flags, and the next timer. */
    @JvmStatic external fun frame(host: Long): Long

    /**
     * Between frames: lazy lists that scrolled compose and measure the items they'll reach next,
     * for at most [availableNanos]. Returns whether work is left, for the next idle time.
     */
    @JvmStatic external fun prefetch(host: Long, availableNanos: Long): Boolean

    /** What `onDraw` replays: resource changes, then the last frame's display list. */
    @JvmStatic external fun takeDrawing(host: Long): ByteBuffer?

    /** Action 0 down, 1 move, 2 up, 3 cancel; [age]: how long ago it happened (ns). */
    @JvmStatic external fun touch(host: Long, action: Int, pointer: Int, x: Float, y: Float, age: Long)

    @JvmStatic
    external fun key(host: Long, key: Int, character: Int, down: Boolean, meta: Int): Boolean

    @JvmStatic external fun text(host: Long, text: String)

    /** A wheel's scroll at ([x], [y]), by ([dx], [dy]) pixels. */
    @JvmStatic external fun scroll(host: Long, x: Float, y: Float, dx: Float, dy: Float)

    @JvmStatic external fun setRtl(host: Long, rtl: Boolean)

    /** The focused text field's kind: 1 a password. */
    @JvmStatic external fun textInputKind(host: Long): Int

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
     * The window's insets in pixels, 4 per kind (left, top, right, bottom): status bars,
     * navigation bars, caption bar, keyboard, display cutout.
     */
    @JvmStatic external fun setWindowInsets(host: Long, insets: IntArray)

    /** A property for the content: [kind] 0 removes it, 1 is [text], 2 [number], 3 a boolean. */
    @JvmStatic external fun setProp(host: Long, name: String, kind: Int, text: String?, number: Double)

    /** Runs the timers due and the tasks woken: the next timer, and whether frames are wanted. */
    @JvmStatic external fun tick(): Long

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

    /** The milliseconds to the next timer, or -1. */
    fun nextTimer(result: Long): Long = (result shr 8) - 1
}
