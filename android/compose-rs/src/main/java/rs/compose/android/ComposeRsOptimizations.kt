package rs.compose.android

/**
 * compose-rs's own optimizations, beyond what Jetpack Compose's rendering does: bits of a
 * [ComposeRsView.optimizations] set, the same as Rust's `compose_ui::platform::Optimization`s
 * (`optimizations.rs`, which a test checks these against; D520). Each is off by default where
 * it changes what's drawn, or how, relative to Compose, so the benchmarks against Compose
 * measure the same rendering ([DEFAULT]) and each optimization apart.
 *
 * Adding one: its bit here, and in Rust's table.
 */
object ComposeRsOptimizations {
    /**
     * A graphics layer faded under `CompositingStrategy.Auto`, holding one drawing with a paint,
     * fades that paint (`setHasOverlappingRendering(false)`) instead of compositing offscreen,
     * as `ModulateAlpha` would: the same pixels. Off by default (D153, D157, D521).
     */
    const val SINGLE_DRAWING_FADE = 1L shl 0

    /**
     * The RenderThread and the main thread in a performance hint session of compose-rs's own
     * while the view shows (API 33+): [ComposeRsView.isPerformanceHintEnabled]. On by default
     * (D390, D440, D500).
     */
    const val PERFORMANCE_HINT = 1L shl 1

    /**
     * A text keeps its paragraph when new constraints change its width bounds but not the
     * width it's laid out at (its own, kept in the constraints), where Compose lays it out
     * again. Off by default (D551).
     */
    const val REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH = 1L shl 2

    /** None: the rendering Compose does. */
    const val NONE = 0L

    /** The ones on by default. */
    const val DEFAULT = PERFORMANCE_HINT

    /** Every one. */
    const val ALL = SINGLE_DRAWING_FADE or PERFORMANCE_HINT or REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH

    /**
     * The set a list names: `none`, `all`, `default`, or names separated by commas (as Rust's,
     * `single_drawing_fade`), each `-name` taking one out of what's before it
     * (`default,-performance_hint`). Needs the app's compose-rs library loaded
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
