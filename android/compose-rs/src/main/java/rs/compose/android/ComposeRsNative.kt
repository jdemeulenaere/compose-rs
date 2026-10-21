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
    ): Long

    @JvmStatic external fun destroy(host: Long)

    @JvmStatic external fun resize(host: Long, width: Int, height: Int)

    @JvmStatic external fun setDensity(host: Long, density: Float, fontScale: Float)

    /** Recomposes, lays out and draws what changed: [Frame] flags, and the next timer. */
    @JvmStatic external fun frame(host: Long): Long

    /** What `onDraw` replays: resource changes, then the last frame's display list. */
    @JvmStatic external fun takeDrawing(host: Long): ByteBuffer?

    /** Action 0 down, 1 move, 2 up, 3 cancel. */
    @JvmStatic external fun touch(host: Long, action: Int, pointer: Int, x: Float, y: Float)

    @JvmStatic
    external fun key(host: Long, key: Int, character: Int, down: Boolean, meta: Int): Boolean

    @JvmStatic external fun text(host: Long, text: String)

    @JvmStatic external fun preedit(host: Long, text: String, cursor: Int)

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

    /** The milliseconds to the next timer, or -1. */
    fun nextTimer(result: Long): Long = (result shr 8) - 1
}
