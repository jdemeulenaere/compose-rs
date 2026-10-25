package rs.compose.android

/**
 * compose-rs's own optimizations, beyond what Jetpack Compose's rendering does: bits of a
 * [ComposeRsView.optimizations] set, the same as Rust's `compose_ui::platform::Optimization`s
 * (`optimizations.rs`, which a test checks these against; D520). The default ([DEFAULT]) is
 * [RECOMMENDED], the flags with a measured gain (D601, D610). [NONE] is the rendering Compose
 * does, which the benchmarks against Compose pass so they measure the same rendering, and each
 * optimization apart. Apps can also pick [ALL], or the ones they name.
 *
 * Adding one: its bit here, and in Rust's table.
 */
object ComposeRsOptimizations {
    /**
     * A graphics layer faded under `CompositingStrategy.Auto`, holding one drawing with a paint,
     * fades that paint (`setHasOverlappingRendering(false)`) instead of compositing offscreen,
     * as `ModulateAlpha` would: the same pixels (D153, D157, D521). On by default ([RECOMMENDED]).
     */
    const val SINGLE_DRAWING_FADE = 1L shl 0

    /**
     * The RenderThread and the main thread in a performance hint session of compose-rs's own
     * while the view shows (API 33+): [ComposeRsView.isPerformanceHintEnabled] (D390, D440, D500).
     * On by default ([RECOMMENDED], D610); parity runs leave it off, as Compose has no such
     * session (D568, D581).
     */
    const val PERFORMANCE_HINT = 1L shl 1

    /**
     * A text keeps its paragraph when new constraints change its width bounds but not the
     * width it's laid out at (its own, kept in the constraints), where Compose lays it out
     * again (D551). On by default ([RECOMMENDED]).
     */
    const val REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH = 1L shl 2

    /** Lazy lists prefetch two items ahead of the scroll, where Compose prefetches one (D156, D560). */
    const val PREFETCH_TWO_AHEAD = 1L shl 3

    /**
     * Prefetch steps start only with their recent maximum and a margin left, a step short of
     * time gives its request up until the next scroll, and the idle time runs to the vsync on
     * the frame time's phase (D340, D341, D381, D561). Compose starts a step with more than its
     * average left, keeps a request for the next frame, and runs to the last frame's time plus
     * an interval.
     */
    const val PREFETCH_BUDGET_RECENT_MAXIMUM = 1L shl 4

    /** Lazy lists prefetch after a layout at rest too, not only on scrolls (D470, D562). */
    const val PREFETCH_AT_REST = 1L shl 5

    /** Lazy lists keep two items behind the scroll composed, and their recordings (D375, D380, D563). */
    const val KEEP_ITEMS_BEHIND = 1L shl 6

    /**
     * Items a prefetch measured are drawn and recorded ahead, and the paragraphs it laid out
     * recorded, in the same idle time (D322, D350, D564).
     */
    const val DRAW_PREFETCHED_AHEAD = 1L shl 7

    /** Lazy grids compose and place two lines beyond each side (D456, D565). */
    const val GRID_LINES_BEYOND = 1L shl 8

    /** A frame dropping a path drawn as a path flushes HWUI's layers first (D510, D566). */
    const val FLUSH_LAYERS_BEFORE_DROPPED_PATHS = 1L shl 9

    /**
     * Each paragraph is recorded once into a render node of its own, drawn by reference (D153,
     * D567), where Compose draws a paragraph into its layer's recording each time it records.
     */
    const val PARAGRAPH_RENDER_NODES = 1L shl 10

    /** None: the rendering Compose does. */
    const val NONE = 0L

    /**
     * The recommended ones: those with a gain measured on the device (Rust's
     * `Optimization::RECOMMENDED`, D601): the hint session, drawing prefetched items ahead, the
     * single-drawing fade with its companion flush layer (the fade alone grows HWUI's GPU caches
     * by 31 %), and the paragraph kept at the same layout width. The default ([DEFAULT]).
     */
    const val RECOMMENDED = SINGLE_DRAWING_FADE or PERFORMANCE_HINT or REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH or DRAW_PREFETCHED_AHEAD or FLUSH_LAYERS_BEFORE_DROPPED_PATHS

    /** The ones on by default: [RECOMMENDED] (D610; it was [NONE], D581). */
    const val DEFAULT = RECOMMENDED

    /** Every one. */
    const val ALL = SINGLE_DRAWING_FADE or PERFORMANCE_HINT or REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH or PREFETCH_TWO_AHEAD or PREFETCH_BUDGET_RECENT_MAXIMUM or PREFETCH_AT_REST or KEEP_ITEMS_BEHIND or DRAW_PREFETCHED_AHEAD or GRID_LINES_BEYOND or FLUSH_LAYERS_BEFORE_DROPPED_PATHS or PARAGRAPH_RENDER_NODES

    /**
     * The set a list names: `none`, `all`, `recommended`, `default`, or names separated by commas
     * (as Rust's, `single_drawing_fade`), each `-name` taking one out of what's before it
     * (`all,-performance_hint`, `recommended,-performance_hint`). Needs the app's compose-rs library loaded
     * ([ComposeRsRuntime.loadLibrary]): the names are Rust's.
     *
     * @throws IllegalArgumentException for a name compose-rs doesn't know.
     */
    @JvmStatic
    fun parse(list: String): Long {
        val bits = ComposeRsNative.parseOptimizations(list)
        require(bits >= 0) { "unknown optimization in \"$list\"" }
        return bits
    }
}
