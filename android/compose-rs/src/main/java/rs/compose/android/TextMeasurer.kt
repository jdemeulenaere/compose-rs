package rs.compose.android

import android.annotation.TargetApi
import android.content.res.AssetManager
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.RenderNode
import android.graphics.Shader
import android.graphics.SweepGradient
import android.graphics.Typeface
import android.graphics.text.LineBreaker
import android.os.Build
import android.os.Trace
import android.text.BoringLayout
import android.text.Layout
import android.text.SpannableString
import android.text.Spanned
import android.text.StaticLayout
import android.text.TextDirectionHeuristic
import android.text.TextDirectionHeuristics
import android.text.TextPaint
import android.text.TextUtils
import android.text.style.BackgroundColorSpan
import android.text.style.LineHeightSpan
import android.text.style.MetricAffectingSpan
import android.text.style.ReplacementSpan
import android.util.Log
import android.util.SparseArray
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.text.BreakIterator
import java.util.PriorityQueue
import kotlin.math.ceil
import kotlin.math.roundToInt

/**
 * Paragraphs laid out for compose-rs (`crates/compose-android/src/text.rs`), as Compose's
 * `TextLayout`: a `BoringLayout` for a single left-to-right line that fits, else a
 * `StaticLayout` (D330); kept by handle to draw them. Rust sends a layout request (style runs and line options, see
 * `encode_request`) and reads the metrics back from [results]. A paragraph laid out again at other widths
 * ([relayout]) reuses what was measured of its text, as Compose's `ParagraphIntrinsics` (D550).
 */
internal class TextMeasurer(
    /**
     * Whether each `BoringLayout` is checked against the `StaticLayout` it stands for (debuggable
     * apps, D330): a debug build lays out each single line twice, and its hit tests and cursor
     * positions too (quadratic in the line's length).
     */
    private val checkBoring: Boolean = false,
    /** Where [loadFontAsset] looks. */
    private val assets: AssetManager? = null,
) {
    /** Floats Rust reads after a call: a layout's metrics and the text's own width, or a cursor's rectangle. */
    val results: ByteBuffer = ByteBuffer.allocateDirect(32).order(ByteOrder.nativeOrder())

    private val layouts = SparseArray<Layout>()
    /** The empty last line's metrics of the paragraphs that have one ([lastLineMetrics]). */
    private val lastLines = SparseArray<Paint.FontMetricsInt>()
    /** Each paragraph's drawing, recorded once: a frame draws the node, not the glyphs. */
    private val nodes = SparseArray<RenderNode>()
    /** Released paragraphs' nodes, for the next ones (D195). */
    private val pool = RenderNodePool("paragraph", NODE_POOL_CAPACITY)
    private val typefaces = HashMap<Long, Typeface>()
    /**
     * A paint per paragraph style, shared by the layouts of that style: a paint's native side
     * otherwise waits for the collector. A layout only reads its paint, but [drawInPaint] swaps
     * the color and decoration of a layout's paint while it draws one given another paint
     * ([restyle]), and puts them back.
     */
    private val paints = HashMap<PaintStyle, TextPaint>()
    /**
     * The canvas layouts draw on ([drawInPaint]), around the one they're drawn into, as
     * Compose's shared `TextAndroidCanvas`: its clip bounds reach past the bottom, so a layout
     * draws all its lines, not only those within a recording's size when it was recorded (D646).
     */
    private val textCanvas = TextAndroidCanvas()
    /** The size of the layout being drawn, which a run's gradient is over ([RunSpan]). */
    private var drawingWidth = 0f
    private var drawingHeight = 0f

    /** What of a text style its paint holds: the key of [paints]. */
    private data class PaintStyle(
        val color: Int,
        val fontPx: Float,
        val weight: Int,
        val italic: Boolean,
        val family: Int,
        val familyName: String?,
        val typeface: Int,
        val letterSpacingPx: Float,
        val decoration: Int,
    )

    /**
     * How a line height is spread around each line, as Compose's `LineHeightStyle` (its `Fixed`
     * mode): the share above the text ([topRatio], -1 for proportional) and which ends are
     * trimmed. Rust sends `LineHeightStyle.Default` (proportional, both trimmed) for a style
     * without one, as Compose's `createCharSequence` applies it (D571).
     */
    private data class LineHeightStyle(val topRatio: Float, val trimFirstLineTop: Boolean, val trimLastLineBottom: Boolean)

    /** A text style, as the request encodes it: its paint's part, and the spans'. */
    private class Style(
        val paint: PaintStyle,
        val lineHeightPx: Float,
        val lineHeightStyle: LineHeightStyle,
        val background: Int,
        val extras: PaintExtras?,
    )

    /**
     * A gradient over a paragraph, as Compose's `ShaderBrush` over its `Size`: its positions are
     * resolved at the size it's drawn at, its shader kept while that stays.
     */
    private class TextBrush(
        private val kind: Int,
        private val tile: Shader.TileMode,
        val alpha: Float,
        colors: IntArray,
        stops: FloatArray?,
        private val first: Float,
        private val second: Float,
        private val third: Float,
        private val fourth: Float,
    ) {
        // A gradient needs two colors: one is a solid color.
        private val colors = if (colors.size == 1) intArrayOf(colors[0], colors[0]) else colors
        private val stops = stops?.takeIf { it.size == this.colors.size }
        private var shader: Shader? = null
        private var resolvedWidth = -1f
        private var resolvedHeight = -1f

        fun shaderAt(width: Float, height: Float): Shader? {
            if (width != resolvedWidth || height != resolvedHeight) {
                shader = make(width, height)
                resolvedWidth = width
                resolvedHeight = height
            }
            return shader
        }

        private fun make(width: Float, height: Float): Shader? {
            if (colors.isEmpty()) return null
            val x = { value: Float -> if (value.isInfinite()) width else value }
            val y = { value: Float -> if (value.isInfinite()) height else value }
            val centerX = if (first.isNaN()) width / 2f else x(first)
            val centerY = if (second.isNaN()) height / 2f else y(second)
            return when (kind) {
                0 -> LinearGradient(x(first), y(second), x(third), y(fourth), colors, stops, tile)
                1 -> {
                    val radius = if (third.isInfinite()) minOf(width, height) / 2f else third
                    // Android refuses it, and there's nothing to spread the colors over.
                    if (radius > 0f) RadialGradient(centerX, centerY, radius, colors, stops, tile) else null
                }
                else -> SweepGradient(centerX, centerY, colors, stops)
            }
        }
    }

    /**
     * What a paint has beyond its color and decoration, set on it only while it draws, as
     * Compose's `AndroidTextPaint.setBrush`, `setShadow` and `setDrawStyle`.
     */
    private class PaintExtras(
        val brush: TextBrush?,
        val shadowColor: Int,
        val shadowX: Float,
        val shadowY: Float,
        val shadowBlur: Float,
        val hasShadow: Boolean,
        val strokeWidth: Float,
        val strokeMiter: Float,
        val strokeCap: Paint.Cap,
        val strokeJoin: Paint.Join,
        val hasStroke: Boolean,
    ) {
        fun applyTo(paint: TextPaint, width: Float, height: Float) {
            brush?.shaderAt(width, height)?.let { shader ->
                paint.color = 0xFF000000.toInt()
                paint.alpha = (brush.alpha * 255f).roundToInt()
                paint.shader = shader
            }
            if (hasShadow) {
                // As Compose's `correctBlurRadius`: a radius of 0 would remove the shadow.
                val blur = if (shadowBlur == 0f) Float.MIN_VALUE else shadowBlur
                paint.setShadowLayer(blur, shadowX, shadowY, shadowColor)
            }
            if (hasStroke) {
                paint.style = Paint.Style.STROKE
                paint.strokeWidth = strokeWidth
                paint.strokeMiter = strokeMiter
                paint.strokeCap = strokeCap
                paint.strokeJoin = strokeJoin
            }
        }

        fun clearFrom(paint: TextPaint) {
            if (brush != null) paint.shader = null
            if (hasShadow) paint.clearShadowLayer()
            if (hasStroke) paint.style = Paint.Style.FILL
        }
    }

    /**
     * The room for a placeholder, as Compose's `PlaceholderSpan`: nothing drawn, its size in the
     * font around it, and the line's ascent and descent as it needs them.
     */
    private class PlaceholderSpan(
        private val width: Float,
        private val widthInEm: Boolean,
        private val height: Float,
        private val heightInEm: Boolean,
        val align: Int,
    ) : ReplacementSpan() {
        var widthPx = 0
        var heightPx = 0
        var fontAscent = 0
        var fontDescent = 0

        override fun getSize(paint: Paint, text: CharSequence?, start: Int, end: Int, fm: Paint.FontMetricsInt?): Int {
            val font = paint.fontMetricsInt
            fontAscent = font.ascent
            fontDescent = font.descent
            widthPx = ceil(if (widthInEm) width * paint.textSize else width).toInt()
            heightPx = ceil(if (heightInEm) height * paint.textSize else height).toInt()
            if (fm != null) {
                fm.ascent = font.ascent
                fm.descent = font.descent
                fm.leading = font.leading
                when (align) {
                    // Top and the text's: too tall, it goes down.
                    1, 4 -> if (fm.ascent + heightPx > fm.descent) fm.descent = fm.ascent + heightPx
                    // Bottom and the text's: too tall, it goes up.
                    2, 5 -> if (fm.ascent > fm.descent - heightPx) fm.ascent = fm.descent - heightPx
                    // Center and the text's: too tall, both ways.
                    3, 6 -> if (fm.descent - fm.ascent < heightPx) {
                        fm.ascent -= (heightPx - (fm.descent - fm.ascent)) / 2
                        fm.descent = fm.ascent + heightPx
                    }
                    else -> if (fm.ascent > -heightPx) fm.ascent = -heightPx
                }
                fm.top = minOf(font.top, fm.ascent)
                fm.bottom = maxOf(font.bottom, fm.descent)
            }
            return widthPx
        }

        override fun draw(
            canvas: Canvas,
            text: CharSequence?,
            start: Int,
            end: Int,
            x: Float,
            top: Int,
            y: Int,
            bottom: Int,
            paint: Paint,
        ) {}
    }

    /** What [flags] (of a style's decoration byte) say follow, as Rust's `encode_paint_extras`. */
    private fun readExtras(request: ByteBuffer, flags: Int): PaintExtras? {
        if (flags and 0x1C == 0) return null
        var brush: TextBrush? = null
        if (flags and 4 != 0) {
            val kind = request.get().toInt()
            val tile = when (request.get().toInt()) {
                1 -> Shader.TileMode.REPEAT
                2 -> Shader.TileMode.MIRROR
                // Transparent past the ends: API 31+, clamped below.
                3 -> if (Build.VERSION.SDK_INT >= 31) Shader.TileMode.DECAL else Shader.TileMode.CLAMP
                else -> Shader.TileMode.CLAMP
            }
            val alpha = request.float()
            val colors = IntArray(request.getInt()) { request.getInt() }
            val stopCount = request.getInt()
            val stops = if (stopCount == 0) null else FloatArray(stopCount) { request.float() }
            brush = TextBrush(
                kind, tile, alpha, colors, stops, request.float(), request.float(), request.float(), request.float(),
            )
        }
        val hasShadow = flags and 8 != 0
        val shadowColor = if (hasShadow) request.getInt() else 0
        val shadowX = if (hasShadow) request.float() else 0f
        val shadowY = if (hasShadow) request.float() else 0f
        val shadowBlur = if (hasShadow) request.float() else 0f
        val hasStroke = flags and 16 != 0
        val strokeWidth = if (hasStroke) request.float() else 0f
        val strokeMiter = if (hasStroke) request.float() else 0f
        val strokeCap = if (!hasStroke) Paint.Cap.BUTT else when (request.get().toInt()) {
            1 -> Paint.Cap.ROUND
            2 -> Paint.Cap.SQUARE
            else -> Paint.Cap.BUTT
        }
        val strokeJoin = if (!hasStroke) Paint.Join.MITER else when (request.get().toInt()) {
            1 -> Paint.Join.ROUND
            2 -> Paint.Join.BEVEL
            else -> Paint.Join.MITER
        }
        return PaintExtras(
            brush, shadowColor, shadowX, shadowY, shadowBlur, hasShadow, strokeWidth, strokeMiter, strokeCap, strokeJoin,
            hasStroke,
        )
    }

    private fun readStyle(request: ByteBuffer): Style {
        val color = request.getInt()
        val fontPx = request.float()
        val weight = request.getInt()
        val italic = request.get().toInt() != 0
        val family = request.get().toInt()
        var name: String? = null
        if (family == 4) {
            val bytes = ByteArray(request.getInt())
            request.get(bytes)
            name = String(bytes, Charsets.UTF_8)
        }
        val typeface = if (family == 5) request.getInt() else -1
        val letterSpacingPx = request.float()
        val decoration = request.get().toInt()
        val paint = PaintStyle(
            color = color,
            fontPx = fontPx,
            weight = weight,
            italic = italic,
            family = family,
            familyName = name,
            typeface = typeface,
            letterSpacingPx = letterSpacingPx,
            decoration = decoration and 3,
        )
        val lineHeightPx = request.float()
        val topRatio = request.float()
        val trim = request.get().toInt()
        val lineHeightStyle = LineHeightStyle(topRatio, trimFirstLineTop = trim and 1 != 0, trimLastLineBottom = trim and 2 != 0)
        val background = request.getInt()
        return Style(paint, lineHeightPx, lineHeightStyle, background, readExtras(request, decoration))
    }

    /** Applies a font's variation settings, as Compose's `setFontVariationSettings`. */
    private fun varied(typeface: Typeface?, settings: String): Typeface? {
        if (typeface == null || settings.isEmpty()) return typeface
        val paint = Paint()
        paint.typeface = typeface
        paint.fontVariationSettings = settings
        return paint.typeface
    }

    /** A direct buffer of [bytes], which a font of them and its variations share. */
    fun directBuffer(bytes: ByteArray): ByteBuffer {
        val buffer = ByteBuffer.allocateDirect(bytes.size).order(ByteOrder.nativeOrder())
        buffer.put(bytes)
        buffer.rewind()
        return buffer
    }

    /**
     * A font of a direct buffer, as Compose's fonts of a file descriptor: -1 if it isn't one. Its
     * weight and slant are its table's, as Compose's `TypefaceBuilderCompat` lets them be, so a
     * bold font isn't taken for a regular one to fake.
     */
    fun loadFontBuffer(buffer: ByteBuffer, settings: String): Int =
        try {
            val font = android.graphics.fonts.Font.Builder(buffer)
                .setFontVariationSettings(settings)
                .build()
            val family = android.graphics.fonts.FontFamily.Builder(font).build()
            register(Typeface.CustomFallbackBuilder(family).setStyle(font.style).build())
        } catch (_: Exception) {
            -1
        }

    /** A font of a file, as Compose's `Font(File)`: -1 if it can't be read. */
    fun loadFontFile(path: String, settings: String): Int =
        try {
            register(
                Typeface.Builder(java.io.File(path)).setFontVariationSettings(settings).build()
            )
        } catch (_: Exception) {
            -1
        }

    /** A font of the package's assets, as Compose's `Font(path, assetManager)`. */
    fun loadFontAsset(path: String, settings: String): Int =
        try {
            register(Typeface.Builder(assets!!, path).setFontVariationSettings(settings).build())
        } catch (_: Exception) {
            -1
        }

    private fun platformTypeface(name: String?, weight: Int, italic: Boolean): Typeface {
        if (!italic && name.isNullOrEmpty()) {
            if (weight == 400) return Typeface.DEFAULT
            if (weight == 700) return Typeface.DEFAULT_BOLD
        }
        val family = if (name == null) Typeface.DEFAULT else Typeface.create(name, Typeface.NORMAL)
        return Typeface.create(family, weight.coerceIn(1, 1000), italic)
    }

    /** The platform's default or a generic family, as Compose's `PlatformTypefaces`. */
    fun createTypeface(name: String?, weight: Int, italic: Boolean): Int =
        register(platformTypeface(name, weight, italic))

    /**
     * A family on the device, as Compose's `optionalOnDeviceFontFamilyByName`: -1 if it isn't
     * there (what a missing family gives is the default's), or if it can't take its settings.
     */
    fun loadDeviceFont(name: String, weight: Int, italic: Boolean, settings: String): Int =
        try {
            val typeface = platformTypeface(name, weight, italic)
            val generic =
                name == "sans-serif" || name == "serif" || name == "monospace" || name == "cursive"
            val missing =
                !generic && (
                    typeface == Typeface.create(Typeface.DEFAULT, weight.coerceIn(1, 1000), italic) ||
                        typeface == platformTypeface(null, weight, italic)
                )
            if (missing) -1 else register(varied(typeface, settings))
        } catch (_: Exception) {
            -1
        }

    /** A typeface faked to [weight] and [italic], as Compose's `synthesizeTypeface`. */
    fun synthesizeTypeface(number: Int, weight: Int, italic: Boolean): Int {
        val typeface = synchronized(loaded) { loaded[number] } ?: return -1
        return register(Typeface.create(typeface, weight.coerceIn(1, 1000), italic))
    }

    /** Rust let go of typeface [number]: it goes with the last paragraph's paint holding it. */
    fun releaseTypeface(number: Int) {
        synchronized(loaded) { loaded.remove(number) }
    }

    private fun typeface(style: PaintStyle): Typeface {
        if (style.family == 5) {
            return synchronized(loaded) { loaded[style.typeface] } ?: Typeface.DEFAULT
        }
        val key = (style.family.toLong() shl 40) or (style.weight.toLong() shl 8) or
            (if (style.italic) 1L else 0L) or ((style.familyName?.hashCode()?.toLong() ?: 0L) shl 48)
        return typefaces.getOrPut(key) {
            val base = when (style.family) {
                1 -> Typeface.SERIF
                2 -> Typeface.MONOSPACE
                3 -> Typeface.create("cursive", Typeface.NORMAL)
                4 -> Typeface.create(style.familyName, Typeface.NORMAL)
                else -> Typeface.DEFAULT
            }
            Typeface.create(base, style.weight.coerceIn(1, 1000), style.italic)
        }
    }

    private fun apply(paint: TextPaint, style: PaintStyle) {
        paint.color = style.color
        paint.textSize = style.fontPx
        paint.typeface = typeface(style)
        paint.letterSpacing = if (style.fontPx > 0f) style.letterSpacingPx / style.fontPx else 0f
        paint.isUnderlineText = style.decoration and 1 != 0
        paint.isStrikeThruText = style.decoration and 2 != 0
    }

    /**
     * A run's style over the paragraph's. Its color and decoration are the paragraph's paint's
     * unless the run sets them ([ownColor], [ownDecoration]), so a paragraph drawn in a new paint
     * ([restyle]) gives them to the runs that don't, as Compose's spans over its `TextPaint`.
     */
    private inner class RunSpan(
        private val style: PaintStyle,
        private val ownColor: Boolean,
        private val ownDecoration: Boolean,
        private val extras: PaintExtras?,
    ) : MetricAffectingSpan() {
        override fun updateDrawState(paint: TextPaint) {
            applyRun(paint)
            // Over the paragraph's size, as Compose's `ShaderBrushSpan`.
            extras?.applyTo(paint, drawingWidth, drawingHeight)
        }

        override fun updateMeasureState(paint: TextPaint) = applyRun(paint)

        private fun applyRun(paint: TextPaint) {
            val color = paint.color
            val underline = paint.isUnderlineText
            val strike = paint.isStrikeThruText
            apply(paint, style)
            if (!ownColor) paint.color = color
            if (!ownDecoration) {
                paint.isUnderlineText = underline
                paint.isStrikeThruText = strike
            }
        }
    }

    /**
     * What a paragraph keeps of its text to lay it out again at other widths without measuring it
     * again, as Compose's `ParagraphIntrinsics` (`LayoutIntrinsics`: the text with its spans, the
     * paint, the boring metrics and the text's own width, D550).
     */
    private inner class Intrinsics(
        val content: CharSequence,
        /** The paragraph's paint, and what of its style it holds. */
        val paintStyle: PaintStyle,
        val paint: TextPaint,
        val direction: TextDirectionHeuristic,
        /** One line of left-to-right text's metrics (D330), or null. */
        val boring: BoringLayout.Metrics?,
        /** A line height in px over every line, or 0. */
        val lineHeight: Float,
        /** How [lineHeight] is spread around each line. */
        val lineHeightStyle: LineHeightStyle,
        val letterSpaced: Boolean,
        val softWrap: Boolean,
        val maxLines: Int,
        /** Where it ellipsizes, as Compose's `TextOverflow`: a start or a middle on one line only. */
        val ellipsis: TextUtils.TruncateAt?,
        /** 0 normal, 1 opposite, 2 center, 3 justify. */
        val align: Int,
        /** The boring line's width, as Compose's `maxIntrinsicWidth` takes it. */
        val boringWidth: Float,
        /**
         * The text's own width, as Compose's `LayoutIntrinsics.maxIntrinsicWidth`: the boring
         * line's, else a measure of all of it, half a pixel wider with letter spacing
         * (b/173574230: else a StaticLayout can break the line it measured, or ellipsize it).
         * Measured once, at the first layout, as Compose's `finalConstraints` reads it.
         */
        val maxIntrinsicWidth: Float,
    ) {
        /** What the paint has beyond its color and decoration, for the drawing. */
        var extras: PaintExtras? = null

        /** The placeholders in [content], in the order they were given. */
        var placeholders: Array<PlaceholderSpan>? = null

        /**
         * The width of the widest segment of [content] between line break opportunities, as
         * Compose's `LayoutIntrinsics.minIntrinsicWidth`: measured when first asked.
         */
        val minIntrinsicWidth: Float by lazy(LazyThreadSafetyMode.NONE) { minIntrinsicWidth(content, paint) }

        /** [content] with its line height span, for a `StaticLayout` (made when one needs it). */
        val staticText: CharSequence by lazy(LazyThreadSafetyMode.NONE) { withLineHeight(content, lineHeight, lineHeightStyle) }

        /**
         * These in another paint, which changes no metric (its color and decoration): nothing is
         * measured again, as Compose draws a kept paragraph in a new color.
         */
        fun inPaint(color: Int, decoration: Int): Intrinsics {
            val style = paintStyle.copy(color = color, decoration = decoration)
            val painted = Intrinsics(
                content, style, this@TextMeasurer.paint(style), direction, boring, lineHeight, lineHeightStyle, letterSpaced,
                softWrap, maxLines, ellipsis, align, boringWidth, maxIntrinsicWidth,
            )
            painted.placeholders = placeholders
            return painted
        }

        val alignment = when (align) {
            1 -> Layout.Alignment.ALIGN_OPPOSITE
            2 -> Layout.Alignment.ALIGN_CENTER
            else -> Layout.Alignment.ALIGN_NORMAL
        }
    }

    /** Each paragraph's [Intrinsics], by handle, as [layouts]. */
    private val intrinsics = SparseArray<Intrinsics>()

    fun layout(handle: Int, text: String, request: ByteBuffer) {
        request.order(ByteOrder.LITTLE_ENDIAN)
        val flags = request.get().toInt()
        val softWrap = flags and 1 != 0
        val hasPlaceholders = flags and 4 != 0
        // For an intrinsic probe only: measured, never drawn, so not recorded ahead (D556).
        val probe = flags and 2 != 0
        val maxLines = request.getInt()
        val ellipsis = when (request.get().toInt()) {
            1 -> TextUtils.TruncateAt.END
            2 -> TextUtils.TruncateAt.START
            3 -> TextUtils.TruncateAt.MIDDLE
            else -> null
        }
        val align = request.get().toInt()
        val rtl = request.get().toInt() != 0
        val minWidth = request.getInt()
        val maxWidth = request.getInt()
        val base = readStyle(request)
        val paint = paint(base.paint)
        val runCount = request.getInt()
        var letterSpaced = paint.letterSpacing != 0f
        val content: CharSequence = if (runCount == 0 && !hasPlaceholders) {
            text
        } else {
            val spannable = SpannableString(text)
            repeat(runCount) {
                val start = request.getInt()
                val end = request.getInt()
                val inherits = request.get().toInt()
                val style = readStyle(request)
                if (start < end) {
                    val span = RunSpan(
                        style.paint, ownColor = inherits and 1 == 0, ownDecoration = inherits and 2 == 0, style.extras,
                    )
                    spannable.setSpan(span, start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                    if (style.paint.fontPx > 0f && style.paint.letterSpacingPx != 0f) letterSpaced = true
                    if (style.background != 0) {
                        spannable.setSpan(
                            BackgroundColorSpan(style.background),
                            start,
                            end,
                            Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
                        )
                    }
                }
            }
            spannable
        }
        val maxHeight = request.getInt()
        var placeholders: Array<PlaceholderSpan>? = null
        if (hasPlaceholders) {
            val spannable = content as SpannableString
            placeholders = Array(request.getInt()) {
                val start = request.getInt()
                val end = request.getInt()
                val width = request.float()
                val widthInEm = request.get().toInt() != 0
                val height = request.float()
                val heightInEm = request.get().toInt() != 0
                val span = PlaceholderSpan(width, widthInEm, height, heightInEm, request.get().toInt())
                spannable.setSpan(span, start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                span
            }
        }
        // A line height over every line (Compose's span, [StyledLineHeightSpan]), an empty text's
        // included (its line takes it from [lastLineMetrics], as Compose's).
        val lineHeight = if (base.lineHeightPx > 0f) base.lineHeightPx else 0f
        // The direction Rust decided (its first strong character's), for the whole text, as the
        // desktop measurer: alignment and the cursor's movement agree with what's drawn.
        val direction = if (rtl) TextDirectionHeuristics.RTL else TextDirectionHeuristics.LTR
        // One line of left-to-right text has boring metrics (D330): its width, in one measure.
        val boring = boringMetrics(content, paint, direction, lineHeight, base.lineHeightStyle)
        val boringWidth = if (boring == null) 0f else intrinsicWidth(boring.width.toFloat(), letterSpaced)
        val maxIntrinsicWidth = if (boring != null) {
            boringWidth
        } else {
            intrinsicWidth(ceil(Layout.getDesiredWidth(content, paint)), letterSpaced)
        }
        val kept = Intrinsics(
            content, base.paint, paint, direction, boring, lineHeight, base.lineHeightStyle, letterSpaced, softWrap,
            maxLines, ellipsis, align, boringWidth, maxIntrinsicWidth,
        )
        kept.extras = base.extras
        kept.placeholders = placeholders
        layoutAt(handle, kept, minWidth, maxWidth, maxHeight, drawn = !probe)
    }

    /**
     * Lays out paragraph [previous]'s text again as paragraph [handle] for an intrinsic probe, as
     * [relayout] does but keeping [previous]: measured from what [previous] kept, never drawn, so
     * not recorded ahead (D556). False (nothing changed) when [previous] kept nothing.
     */
    fun probeLayout(previous: Int, handle: Int, minWidth: Int, maxWidth: Int, maxHeight: Int): Boolean {
        val kept = intrinsics.get(previous) ?: return false
        layoutAt(handle, kept, minWidth, maxWidth, maxHeight, drawn = false)
        return true
    }

    /**
     * Gives paragraph [previous]'s layout, as paragraph [handle], the paint of [color] (ARGB) and
     * [decoration] (1 underline, 2 line through), and releases [previous]: the same `Layout`,
     * drawn in the new paint ([record]), and laid out in it later ([relayout]). As Compose keeps a
     * paragraph whose style changed only in what it draws (`hasSameLayoutAffectingAttributes`)
     * and paints it in the new color. False (nothing changed) without [previous].
     */
    fun restyle(previous: Int, handle: Int, color: Int, decoration: Int): Boolean {
        val layout = layouts.get(previous) ?: return false
        val kept = intrinsics.get(previous) ?: return false
        val lastLine = lastLines.get(previous)
        release(previous)
        layouts.put(handle, layout)
        if (lastLine != null) lastLines.put(handle, lastLine)
        intrinsics.put(handle, kept.inPaint(color, decoration))
        releaseNode(handle)
        if (collectingLaidOut) laidOut.add(handle)
        return true
    }

    /**
     * Gives paragraph [previous]'s layout, as paragraph [handle] too, the paint of [color] (ARGB)
     * and [decoration], [previous] staying as it is: the one `Layout` drawn in each paint
     * ([drawInPaint]), as Compose paints a layout it keeps in another color (`Paragraph.paint`).
     * False (nothing changed) without [previous].
     */
    fun painted(previous: Int, handle: Int, color: Int, decoration: Int): Boolean {
        val layout = layouts.get(previous) ?: return false
        val kept = intrinsics.get(previous) ?: return false
        layouts.put(handle, layout)
        lastLines.get(previous)?.let { lastLines.put(handle, it) }
        intrinsics.put(handle, kept.inPaint(color, decoration))
        if (collectingLaidOut) laidOut.add(handle)
        return true
    }

    /**
     * Gives paragraph [handle], just [restyle]d or [painted], the gradient, shadow and stroke in
     * [extras] (a flags byte, then what they say), as Rust's `encode_paint_extras`.
     */
    fun paintExtras(handle: Int, extras: ByteBuffer) {
        extras.order(ByteOrder.LITTLE_ENDIAN)
        val flags = extras.get().toInt()
        intrinsics.get(handle)?.extras = readExtras(extras, flags)
    }

    /**
     * Lays out paragraph [previous]'s text again as paragraph [handle], between [minWidth] and
     * [maxWidth] (`Int.MAX_VALUE`: unbounded) and at most [maxHeight] high if it's ellipsized, from
     * what [previous] kept of it ([Intrinsics]: no
     * measure of the text), as Compose's new `Paragraph` from the `ParagraphIntrinsics` its cache
     * keeps when only the constraints change; then releases [previous]. False (nothing changed,
     * [results] untouched) when [previous] kept nothing.
     */
    fun relayout(previous: Int, handle: Int, minWidth: Int, maxWidth: Int, maxHeight: Int): Boolean {
        // Rust lays the text out anew (nothing kept: a paragraph cleared or never laid out).
        val kept = intrinsics.get(previous) ?: return false
        release(previous)
        layoutAt(handle, kept, minWidth, maxWidth, maxHeight)
        return true
    }

    /** Lays out [kept]'s text as paragraph [handle] at the constraints' width, writing [results]. */
    private fun layoutAt(
        handle: Int,
        kept: Intrinsics,
        minWidth: Int,
        maxWidth: Int,
        maxHeight: Int,
        drawn: Boolean = true,
    ) {
        // The width to break lines at, as Compose's `finalMaxWidth`: the constraint when wrapping
        // (or ellipsizing), else the text's own; then the text's width, kept in the constraints.
        val bounded = maxWidth != Int.MAX_VALUE
        val available = if ((kept.softWrap || kept.ellipsis != null) && bounded) maxWidth else Int.MAX_VALUE
        val width = if (minWidth == available) {
            available
        } else {
            ceil(kept.maxIntrinsicWidth).toInt().coerceIn(minWidth, available)
        }
        var maxLines = kept.maxLines
        var layout = textLayout(kept, width, maxLines)
        // Below API 35 the characters hidden by a start or middle ellipsis keep their letter
        // spacing, as they do for Compose's `AndroidParagraph`, which lays out without them.
        if (Build.VERSION.SDK_INT < 35 && kept.letterSpaced && layout.lineCount > 0 && layout.getEllipsisCount(0) > 0 &&
            kept.ellipsis != TextUtils.TruncateAt.END
        ) {
            val before = layout.getEllipsisStart(0)
            val after = before + layout.getEllipsisCount(0)
            val text = kept.staticText
            val shown = TextUtils.concat(text.subSequence(0, before), "\u2026", text.subSequence(after, text.length))
            layout = staticLayout(
                shown, kept.paint, width.coerceAtLeast(0), kept.alignment, kept.direction, maxLines, kept.ellipsis, kept.align,
            )
        }
        // As Compose's: an ellipsis ends the last line the height has room for (one, if it has
        // room for none).
        if (kept.ellipsis == TextUtils.TruncateAt.END && maxLines > 1 && layout.height > maxHeight) {
            var fitting = 0
            while (fitting < layout.lineCount && layout.getLineBottom(fitting) <= maxHeight) fitting++
            if (fitting != maxLines) {
                maxLines = fitting.coerceAtLeast(1)
                layout = textLayout(kept, width, maxLines)
            }
        }
        layouts.put(handle, layout)
        intrinsics.put(handle, kept)
        releaseNode(handle)
        if (collectingLaidOut && drawn) laidOut.add(handle)
        // StaticLayout limits the lines only when it ellipsizes: the rest are cut here, as
        // Compose's `TextLayout` does (the text is clipped to its height).
        val lines = layout.lineCount.coerceIn(1, maxLines.coerceAtLeast(1))
        val lastLine = lastLineMetrics(layout, lines - 1, kept)
        if (lastLine == null) lastLines.remove(handle) else lastLines.put(handle, lastLine)
        results.putInt(0, width.toFloat().toRawBits())
        results.putInt(4, lineBottom(layout, lines - 1, lines, lastLine).toRawBits())
        results.putInt(8, lineBaseline(layout, 0, lines, lastLine).toRawBits())
        results.putInt(12, lineBaseline(layout, lines - 1, lines, lastLine).toRawBits())
        results.putInt(16, lines.toFloat().toRawBits())
        // Lines past max lines, cut or ellipsized, as Compose's `TextLayout.didExceedMaxLines`
        // (API 26+: the line count passes max lines unless it ellipsizes, which marks the last).
        val lastEllipsized = layout.lineCount > 0 && layout.getEllipsisCount(layout.lineCount - 1) > 0
        val exceeded = layout.lineCount > maxLines || lastEllipsized
        results.putInt(20, (if (exceeded) 1f else 0f).toRawBits())
        // The text's own width, from which Rust can tell another width would lay it out the same.
        results.putInt(24, kept.maxIntrinsicWidth.toRawBits())
    }

    /** As Compose's `TextLayout`: a `BoringLayout` when the boring line fits the width, else a `StaticLayout`. */
    private fun textLayout(kept: Intrinsics, width: Int, maxLines: Int): Layout {
        val boring = kept.boring
        if (Build.VERSION.SDK_INT < 33 || boring == null || kept.boringWidth > width) {
            return staticLayout(
                kept.staticText, kept.paint, width.coerceAtLeast(0), kept.alignment, kept.direction, maxLines,
                kept.ellipsis, kept.align,
            )
        }
        val layout = boringLayout(kept.content, kept.paint, width, kept.alignment, boring)
        if (!checkBoring) return layout
        val static = staticLayout(
            kept.staticText, kept.paint, width, kept.alignment, kept.direction, maxLines, kept.ellipsis, kept.align,
        )
        // A difference is logged, and the StaticLayout kept (as before D330).
        return if (sameLayout(layout, static, kept.paint)) layout else static
    }

    /**
     * The metrics of the empty last line [line] of [layout] (an empty text's, or after a last new
     * line), as Compose's `TextLayout.getLastLineMetrics` (ui-text 1.12.1, read from its class):
     * a zero-width space laid out alone with a copy of the line height span, so the empty line
     * is as high as a line of text would be. Null when the line isn't empty or there's no line
     * height (a `StaticLayout` gives the empty line the font's metrics then, as Compose's).
     */
    private fun lastLineMetrics(layout: Layout, line: Int, kept: Intrinsics): Paint.FontMetricsInt? {
        if (kept.lineHeight <= 0f || layout.getLineStart(line) != layout.getLineEnd(line)) return null
        val style = kept.lineHeightStyle
        // Compose's copy: its first line's top trimmed as the original's last line's bottom,
        // except after other lines when that one is trimmed.
        val trimFirstLineTop = if (line != 0 && style.trimLastLineBottom) false else style.trimLastLineBottom
        val space = SpannableString("\u200B")
        val span = StyledLineHeightSpan(kept.lineHeight, space.length, style.copy(trimFirstLineTop = trimFirstLineTop))
        space.setSpan(span, 0, space.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        val single = StaticLayout.Builder.obtain(space, 0, space.length, kept.paint, Int.MAX_VALUE)
            .setTextDirection(kept.direction)
            .setIncludePad(false)
            .setUseLineSpacingFromFallbacks(true)
            .build()
        val metrics = Paint.FontMetricsInt()
        metrics.ascent = single.getLineAscent(0)
        metrics.descent = single.getLineDescent(0)
        metrics.top = single.getLineTop(0)
        metrics.bottom = single.getLineBottom(0)
        return metrics
    }

    /**
     * Line [line]'s bottom of [lines], as Compose's `TextLayout.getLineBottom`: an empty last
     * line's from its [lastLine] metrics, under the line before it.
     */
    private fun lineBottom(layout: Layout, line: Int, lines: Int, lastLine: Paint.FontMetricsInt?): Float =
        if (line == lines - 1 && lastLine != null) {
            (if (line == 0) 0 else layout.getLineBottom(line - 1)).toFloat() + lastLine.bottom
        } else {
            layout.getLineBottom(line).toFloat()
        }

    /** Line [line]'s baseline of [lines], as Compose's `TextLayout.getLineBaseline`. */
    private fun lineBaseline(layout: Layout, line: Int, lines: Int, lastLine: Paint.FontMetricsInt?): Float =
        if (line == lines - 1 && lastLine != null) {
            layout.getLineTop(line).toFloat() - lastLine.ascent
        } else {
            layout.getLineBaseline(line).toFloat()
        }

    /**
     * [text]'s metrics as a single boring line, or null: as Compose's `BoringLayoutFactory33`
     * (`BoringLayout.isBoring` with fallback line spacing, which says no to a text with a new
     * line, a tab, a surrogate pair (emoji), a character that could be right to left, a
     * paragraph span, or that [direction] calls right to left). API 33+ only: below it, a
     * `BoringLayout` can't use fallback fonts' line spacing as the `StaticLayout` does, so a line
     * with a taller fallback font (CJK) would be shorter (Compose accepts that difference).
     *
     * A [lineHeight] (px, 0 for none) is applied to the metrics as the `StaticLayout`'s span
     * ([lineHeightSpan]) applies it to its single line (the first and the last), rather than
     * ruling the text out as Compose's line height spans do (D330, D571).
     */
    private fun boringMetrics(
        text: CharSequence,
        paint: TextPaint,
        direction: TextDirectionHeuristic,
        lineHeight: Float,
        lineHeightStyle: LineHeightStyle,
    ): BoringLayout.Metrics? {
        if (Build.VERSION.SDK_INT < 33) return null
        // An empty text with a line height: its line comes from [lastLineMetrics], after a
        // `StaticLayout` (Compose's span makes no text boring).
        if (text.isEmpty() && lineHeight > 0f) return null
        val metrics = BoringLayout.isBoring(text, paint, direction, true, null) ?: return null
        if (lineHeight > 0f) {
            lineHeightSpan(lineHeight, text.length, lineHeightStyle).chooseHeight(text, 0, text.length, 0, 0, metrics)
        }
        return metrics
    }

    /** [width] as Compose's `maxIntrinsicWidth`: half a pixel more for [letterSpaced] text. */
    private fun intrinsicWidth(width: Float, letterSpaced: Boolean): Float =
        if (letterSpaced && width != 0f) width + 0.5f else width

    /**
     * [content] with a [lineHeight] span over it (px; none when 0), as a `StaticLayout` takes it:
     * a copy, so that [content], which a `BoringLayout` of the same text may draw, has none.
     */
    private fun withLineHeight(content: CharSequence, lineHeight: Float, lineHeightStyle: LineHeightStyle): CharSequence {
        if (lineHeight == 0f) return content
        val spannable = SpannableString(content)
        // Its end past an empty text or a last new line, as Compose's `setLineHeight`.
        val end = if (content.isEmpty() || content[content.length - 1] == '\n') content.length + 1 else content.length
        spannable.setSpan(lineHeightSpan(lineHeight, end, lineHeightStyle), 0, spannable.length, Spanned.SPAN_INCLUSIVE_INCLUSIVE)
        return spannable
    }

    /**
     * The span spreading [lineHeight] (px) around each line of a text ending at [end], as
     * Compose's `setLineHeight` (ui-text 1.12.1): its `LineHeightStyleSpan` (D571; its plain
     * `LineHeightSpan` is for `includeFontPadding` only). A new one per text: it keeps the
     * metrics of the first line it sees.
     */
    private fun lineHeightSpan(lineHeight: Float, end: Int, style: LineHeightStyle): LineHeightSpan =
        StyledLineHeightSpan(lineHeight, end, style)

    /**
     * Compose's `LineHeightStyleSpan` in its `Fixed` mode, the default (ui-text 1.12.1, read
     * from its class): every line [lineHeight] (ceiled) high, the difference from the font's
     * height split by the alignment's top ratio (the font's ascent's share for proportional),
     * the first line's top and the last line's bottom back to the font's when trimmed. The
     * target metrics are the first line's, as Compose's.
     */
    private class StyledLineHeightSpan(
        private val lineHeight: Float,
        private val endIndex: Int,
        private val style: LineHeightStyle,
    ) : LineHeightSpan {
        private var firstAscent = Int.MIN_VALUE
        private var ascent = 0
        private var descent = 0
        private var lastDescent = 0

        override fun chooseHeight(
            text: CharSequence,
            start: Int,
            end: Int,
            spanstartv: Int,
            lineHeight: Int,
            metrics: Paint.FontMetricsInt,
        ) {
            if (metrics.descent - metrics.ascent <= 0) return
            val first = start == 0
            val last = end == endIndex
            // A single line trimmed at both ends keeps its font's height.
            if (first && last && style.trimFirstLineTop && style.trimLastLineBottom) return
            if (firstAscent == Int.MIN_VALUE) target(metrics)
            metrics.ascent = if (first) firstAscent else ascent
            metrics.descent = if (last) lastDescent else descent
        }

        private fun target(metrics: Paint.FontMetricsInt) {
            val height = metrics.descent - metrics.ascent
            val target = ceil(lineHeight).toInt()
            val difference = target - height
            val topRatio = if (style.topRatio == -1f) Math.abs(metrics.ascent.toFloat()) / height else style.topRatio
            // In floats, then ceiled as doubles, as Compose's.
            val descentDifference = if (difference <= 0) {
                ceil((difference * topRatio).toDouble()).toInt()
            } else {
                ceil((difference * (1f - topRatio)).toDouble()).toInt()
            }
            descent = metrics.descent + descentDifference
            ascent = descent - target
            firstAscent = if (style.trimFirstLineTop) metrics.ascent else ascent
            lastDescent = if (style.trimLastLineBottom) metrics.descent else descent
        }
    }

    /**
     * A `BoringLayout` of [text], as Compose's `BoringLayoutFactory33.create` (no line spacing,
     * no padding, fallback line spacing). Without ellipsis: its line fits [width], so there is
     * nothing to cut (Compose passes it, and `TextUtils.ellipsize` measures the text again to find
     * that out).
     */
    @TargetApi(33)
    private fun boringLayout(
        text: CharSequence,
        paint: TextPaint,
        width: Int,
        alignment: Layout.Alignment,
        metrics: BoringLayout.Metrics,
    ): Layout = BoringLayout(text, paint, width, alignment, 1f, 0f, metrics, false, null, width, true)

    private fun staticLayout(
        text: CharSequence,
        paint: TextPaint,
        width: Int,
        alignment: Layout.Alignment,
        direction: TextDirectionHeuristic,
        maxLines: Int,
        ellipsis: TextUtils.TruncateAt?,
        align: Int,
    ): Layout {
        val builder = StaticLayout.Builder.obtain(text, 0, text.length, paint, width)
            .setAlignment(alignment)
            .setTextDirection(direction)
            .setIncludePad(false)
            .setUseLineSpacingFromFallbacks(true)
            .setBreakStrategy(LineBreaker.BREAK_STRATEGY_SIMPLE)
            .setHyphenationFrequency(Layout.HYPHENATION_FREQUENCY_NONE)
            .setMaxLines(maxLines)
        if (ellipsis != null) builder.setEllipsize(ellipsis)
        if (align == 3) builder.setJustificationMode(LineBreaker.JUSTIFICATION_MODE_INTER_WORD)
        return builder.build()
    }

    /**
     * In debuggable apps ([checkBoring]): whether the `BoringLayout` [boring] answers as the
     * `StaticLayout` [static] of the same text (with its line height span) and width does, in
     * everything Rust reads or the drawing uses: size, lines, baselines, ellipsis, direction,
     * each offset's position, the offset at each of those positions and between them, and the
     * boring line's width against `getDesiredWidth`'s. Each difference is logged.
     */
    private fun sameLayout(boring: Layout, static: Layout, paint: TextPaint): Boolean {
        val text = boring.text
        var same = true
        fun differs(what: String, a: Any, b: Any) {
            same = false
            Log.e(TAG, "BoringLayout differs from StaticLayout in $what ($a, $b) for \"$text\"")
        }
        fun compare(what: String, a: Int, b: Int) {
            if (a != b) differs(what, a, b)
        }
        compare("width", boring.width, static.width)
        compare("height", boring.height, static.height)
        compare("line count", boring.lineCount, static.lineCount)
        compare("top", boring.getLineTop(0), static.getLineTop(0))
        compare("bottom", boring.getLineBottom(0), static.getLineBottom(0))
        compare("baseline", boring.getLineBaseline(0), static.getLineBaseline(0))
        compare("ellipsis", boring.getEllipsisCount(0), static.getEllipsisCount(0))
        compare("line end", boring.getLineEnd(0), static.getLineEnd(0))
        compare("direction", boring.getParagraphDirection(0), static.getParagraphDirection(0))
        compare(
            "desired width",
            ceil(boring.getLineWidth(0)).toInt(),
            ceil(Layout.getDesiredWidth(text, paint)).toInt(),
        )
        var previous = -2f
        for (offset in 0..text.length) {
            val x = boring.getPrimaryHorizontal(offset)
            val expected = static.getPrimaryHorizontal(offset)
            // Compared as primitives: 0f and -0f are the same position.
            if (x != expected) differs("offset $offset's position", x, expected)
            for (at in floatArrayOf(x, (previous + x) / 2f)) {
                compare("the offset at $at", boring.getOffsetForHorizontal(0, at), static.getOffsetForHorizontal(0, at))
            }
            previous = x
        }
        val end = boring.width + 2f
        compare("the offset at $end", boring.getOffsetForHorizontal(0, end), static.getOffsetForHorizontal(0, end))
        return same
    }

    /** Paragraph [handle]'s minimum intrinsic width ([Intrinsics.minIntrinsicWidth]), or NaN. */
    fun minIntrinsicWidth(handle: Int): Float = intrinsics.get(handle)?.minIntrinsicWidth ?: Float.NaN

    fun hitTest(handle: Int, x: Float, y: Float): Int {
        val layout = layouts.get(handle) ?: return 0
        val line = layout.getLineForVertical(y.toInt())
        return layout.getOffsetForHorizontal(line, x)
    }

    fun cursor(handle: Int, offset: Int) {
        val layout = layouts.get(handle) ?: return
        val at = offset.coerceIn(0, layout.text.length)
        val line = layout.getLineForOffset(at)
        val x = layout.getPrimaryHorizontal(at)
        // As Compose's `getCursorRect`: the line's top and bottom, an empty last line's from its
        // metrics ([lastLineMetrics]).
        val lines = (intrinsics.get(handle)?.maxLines ?: layout.lineCount).coerceIn(1, layout.lineCount.coerceAtLeast(1))
        results.putInt(0, x.toRawBits())
        results.putInt(4, (layout.getLineTop(line).toFloat()).toRawBits())
        results.putInt(8, x.toRawBits())
        results.putInt(12, lineBottom(layout, line, lines, lastLines.get(handle)).toRawBits())
    }

    /**
     * Paragraph [handle]'s layout for Rust's `Paragraph` queries, read in one crossing (D1105;
     * Rust's `paragraph_from_layout` reads it): its line count, UTF-16 length, width, height,
     * min and max intrinsic widths, whether it exceeded its max lines and whether its last line
     * is ellipsized; per line its start, end (the text's end for an ellipsized last line, as
     * Compose's `TextLayout.getLineEnd`), visible end, top, bottom, baseline, left, right, width
     * and whether its paragraph runs right to left; per UTF-16 offset, the text's end included,
     * its primary and secondary horizontal positions and whether the character there is right
     * to left. Null without a layout. Rust asks only for a text with an `onTextLayout`.
     */
    fun paragraphLayout(handle: Int): FloatArray? {
        val layout = layouts.get(handle) ?: return null
        val kept = intrinsics.get(handle)
        val text = layout.text
        val length = text.length
        val lines = (kept?.maxLines ?: layout.lineCount).coerceIn(1, layout.lineCount.coerceAtLeast(1))
        val lastLine = lastLines.get(handle)
        val lastEllipsized = layout.getEllipsisCount(lines - 1) > 0
        val placeholders = kept?.placeholders ?: emptyArray()
        val out = FloatArray(
            PARAGRAPH_HEADER + lines * PARAGRAPH_LINE + (length + 1) * PARAGRAPH_OFFSET + placeholders.size * PARAGRAPH_PLACEHOLDER,
        )
        out[0] = lines.toFloat()
        out[1] = length.toFloat()
        out[2] = layout.width.toFloat()
        out[3] = lineBottom(layout, lines - 1, lines, lastLine)
        // Compose's `minIntrinsicWidth` (its line breaker's segments), as the intrinsics measure it.
        out[4] = kept?.minIntrinsicWidth ?: layout.width.toFloat()
        out[5] = kept?.maxIntrinsicWidth ?: layout.width.toFloat()
        out[6] = if (layout.lineCount > lines || lastEllipsized) 1f else 0f
        out[7] = if (lastEllipsized) 1f else 0f
        for (line in 0 until lines) {
            val at = PARAGRAPH_HEADER + line * PARAGRAPH_LINE
            // Compose's `getLineEnd` and `getLineVisibleEnd`: a start or a middle one leaves them.
            val ellipsized = line == lines - 1 && lastEllipsized && kept?.ellipsis == TextUtils.TruncateAt.END
            out[at] = layout.getLineStart(line).toFloat()
            out[at + 1] = (if (ellipsized) length else layout.getLineEnd(line)).toFloat()
            out[at + 2] = (if (ellipsized) layout.getLineStart(line) + layout.getEllipsisStart(line) else lineVisibleEnd(layout, line)).toFloat()
            out[at + 3] = layout.getLineTop(line).toFloat()
            out[at + 4] = lineBottom(layout, line, lines, lastLine)
            out[at + 5] = lineBaseline(layout, line, lines, lastLine)
            out[at + 6] = layout.getLineLeft(line)
            out[at + 7] = layout.getLineRight(line)
            out[at + 8] = layout.getLineWidth(line)
            out[at + 9] = if (layout.getParagraphDirection(line) == Layout.DIR_RIGHT_TO_LEFT) 1f else 0f
        }
        val offsets = PARAGRAPH_HEADER + lines * PARAGRAPH_LINE
        for (offset in 0..length) {
            val at = offsets + offset * PARAGRAPH_OFFSET
            out[at] = layout.getPrimaryHorizontal(offset)
            out[at + 1] = layout.getSecondaryHorizontal(offset)
            out[at + 2] = if (offset < length && layout.isRtlCharAt(offset)) 1f else 0f
        }
        val boxes = offsets + (length + 1) * PARAGRAPH_OFFSET
        for ((index, span) in placeholders.withIndex()) {
            placeholderBox(layout, span, lines, lastLine, out, boxes + index * PARAGRAPH_PLACEHOLDER)
        }
        return out
    }

    /**
     * Where placeholder [span] went in [layout], at [at] in [out], as Compose's `placeholderRects`:
     * nothing written for one cut by the maximum lines or an ellipsis.
     */
    private fun placeholderBox(
        layout: Layout,
        span: PlaceholderSpan,
        lines: Int,
        lastLine: Paint.FontMetricsInt?,
        out: FloatArray,
        at: Int,
    ) {
        val text = layout.text as? Spanned ?: return
        val start = text.getSpanStart(span)
        val end = text.getSpanEnd(span)
        if (start < 0) return
        val line = layout.getLineForOffset(start)
        val ellipsized = layout.getEllipsisCount(line) > 0 && end > layout.getLineStart(line) + layout.getEllipsisStart(line)
        if (line >= lines || ellipsized || end > layout.getLineEnd(line)) return
        val leftToRightLine = layout.getParagraphDirection(line) == Layout.DIR_LEFT_TO_RIGHT
        val rightToLeftCharacter = layout.isRtlCharAt(start)
        val edge = if (leftToRightLine != rightToLeftCharacter) layout.getPrimaryHorizontal(start) else layout.getSecondaryHorizontal(start)
        val left = if (rightToLeftCharacter) edge - span.widthPx else edge
        val baseline = lineBaseline(layout, line, lines, lastLine)
        val lineTop = layout.getLineTop(line).toFloat()
        val bottom = lineBottom(layout, line, lines, lastLine)
        val height = span.heightPx.toFloat()
        val top = when (span.align) {
            1 -> lineTop
            2 -> bottom - height
            3 -> (lineTop + bottom - height) / 2f
            4 -> span.fontAscent + baseline
            5 -> span.fontDescent + baseline - height
            6 -> (span.fontAscent + span.fontDescent - height) / 2f + baseline
            else -> baseline - height
        }
        out[at] = 1f
        out[at + 1] = left
        out[at + 2] = top
        out[at + 3] = left + span.widthPx
        out[at + 4] = top + height
    }

    /**
     * Line [line]'s end without its trailing spaces and line feed, as Compose's
     * `LayoutHelper.lineEndToVisibleEnd` (not `Layout.getLineVisibleEnd`, which keeps some).
     */
    private fun lineVisibleEnd(layout: Layout, line: Int): Int {
        val start = layout.getLineStart(line)
        var end = layout.getLineEnd(line)
        while (end > start && isLineEndSpace(layout.text[end - 1])) end--
        return end
    }

    fun release(handle: Int) {
        layouts.remove(handle)
        lastLines.remove(handle)
        intrinsics.remove(handle)
        releaseNode(handle)
    }

    /** Paragraph [handle]'s node back to the pool: no recording draws the paragraph again. */
    private fun releaseNode(handle: Int) {
        val node = nodes.get(handle) ?: return
        nodes.remove(handle)
        pool.recycle(node)
    }

    /**
     * Its host is gone: nothing draws its paragraphs again. Their layouts stay, for a measurer or
     * a result the host's content kept (D1118): they go with this object when those do.
     */
    fun clearDrawing() {
        laidOut.clear()
        for (index in 0 until nodes.size()) nodes.valueAt(index).discardDisplayList()
        nodes.clear()
        pool.clear()
        paints.clear()
    }

    /** The paint of paragraphs of style [style]. */
    private fun paint(style: PaintStyle): TextPaint {
        paints[style]?.let { return it }
        // Bounded: an animated style (a font size, a color) makes a new one each frame.
        if (paints.size >= PAINT_CACHE_CAPACITY) paints.clear()
        val paint = TextPaint(TextPaint.ANTI_ALIAS_FLAG)
        apply(paint, style)
        paints[style] = paint
        return paint
    }

    /**
     * Draws paragraph [handle] with its top left at ([x], [y]): on a hardware canvas, its
     * render node, recorded the first time (a paragraph never changes once laid out).
     */
    fun draw(canvas: Canvas, hardware: Boolean, handle: Int, x: Float, y: Float) {
        val layout = layouts.get(handle) ?: return
        // Without render nodes of their own, drawn into the recording around them, as Compose's.
        if (!hardware || !paragraphNodes) {
            canvas.save()
            canvas.translate(x, y)
            drawInPaint(handle, layout, canvas)
            canvas.restore()
            return
        }
        var node = nodes.get(handle)
        // Recorded again if the system dropped its recording (the window hid).
        if (node == null || !node.hasDisplayList()) node = record(handle, layout, node)
        canvas.save()
        canvas.translate(x, y)
        canvas.drawRenderNode(node)
        canvas.restore()
    }

    /**
     * Draws [layout] in paragraph [handle]'s paint: a layout given a new paint ([restyle]) still
     * holds the old one, whose color and decoration are swapped for the drawing (a recording
     * copies them), then put back (other layouts of that style share it). Every line is drawn,
     * whatever [target]'s clip, through [textCanvas] (D646).
     */
    private fun drawInPaint(handle: Int, layout: Layout, target: Canvas) {
        textCanvas.setCanvas(target)
        val canvas: Canvas = textCanvas
        drawingWidth = layout.width.toFloat()
        drawingHeight = layout.height.toFloat()
        val kept = intrinsics.get(handle)
        val wanted = kept?.paint ?: layout.paint
        val extras = kept?.extras
        val held = layout.paint
        if (wanted === held && extras == null) {
            layout.draw(canvas)
            return
        }
        val color = held.color
        val underline = held.isUnderlineText
        val strike = held.isStrikeThruText
        held.color = wanted.color
        held.isUnderlineText = wanted.isUnderlineText
        held.isStrikeThruText = wanted.isStrikeThruText
        extras?.applyTo(held, drawingWidth, drawingHeight)
        try {
            layout.draw(canvas)
        } finally {
            extras?.clearFrom(held)
            held.color = color
            held.isUnderlineText = underline
            held.isStrikeThruText = strike
        }
    }

    /** Records paragraph [handle]'s [layout] into its node ([node], else one from the pool). */
    private fun record(handle: Int, layout: Layout, node: RenderNode?): RenderNode {
        val recorded = node ?: pool.obtain()
        // Its bounds: the layout's width and height, glyphs past them (an italic's overhang, a
        // line wider than an ellipsized width) not clipped.
        recorded.setPosition(0, 0, layout.width.coerceAtLeast(1), layout.height.coerceAtLeast(1))
        recorded.setClipToBounds(false)
        val recording = recorded.beginRecording()
        drawInPaint(handle, layout, recording)
        recorded.endRecording()
        nodes.put(handle, recorded)
        return recorded
    }

    /**
     * Each paragraph recorded once into a render node of its own, which recordings draw by
     * reference ([ComposeRsOptimizations.PARAGRAPH_RENDER_NODES], D153); else drawn into the
     * recording drawing it, as Compose's. Set by the view.
     */
    var paragraphNodes = false

    /**
     * While set (by the view, around a prefetch), the paragraphs laid out are noted in
     * [laidOut], for [recordLaidOut] to record before a frame shows them.
     */
    var collectingLaidOut = false

    /** Paragraphs laid out while prefetching, oldest first, not recorded yet. */
    private val laidOut = IntList()

    /** How many paragraphs the last [recordLaidOut] recorded. */
    var recordedLast = 0
        private set

    /** About how long recording a paragraph takes (ns), from the last ones. */
    private var recordNanos = INITIAL_RECORD_NANOS

    /**
     * Records the paragraphs laid out while prefetching ([collectingLaidOut]), oldest first,
     * while one is expected to end before [deadlineNanos] (`System.nanoTime`; one at least, if
     * it's not past, so a slow recording can't stop them for good): the frame that
     * scrolls their item in then draws their nodes rather than recording them (on average 1
     * ms of the frames revealing an item). Skipped: the ones released or laid out again since
     * (a later layout of the same text is a new paragraph, noted itself), those recorded
     * already (a frame drew them first), and empty ones. Returns whether some are left, for
     * the next idle time.
     */
    fun recordLaidOut(deadlineNanos: Long): Boolean {
        recordedLast = 0
        // Without render nodes of their own (`PARAGRAPH_RENDER_NODES`), nothing to record ahead.
        if (!paragraphNodes) laidOut.clear()
        if (laidOut.size == 0) return false
        Trace.beginSection("compose-rs: record paragraphs")
        var index = 0
        var recorded = 0
        while (index < laidOut.size) {
            val handle = laidOut[index]
            val layout = layouts.get(handle)
            if (layout == null || nodes.get(handle) != null || layout.text.isEmpty()) {
                index++
                continue
            }
            val start = System.nanoTime()
            if (start >= deadlineNanos || recorded > 0 && start + recordNanos > deadlineNanos) break
            record(handle, layout, null)
            recorded++
            // A running average, as Rust's prefetch estimates, an outlier (a collection, the
            // thread descheduled) counting for at most a millisecond.
            val took = (System.nanoTime() - start).coerceAtMost(MAX_RECORD_NANOS)
            recordNanos = (recordNanos * 3 + took) / 4
            index++
        }
        laidOut.removeFirst(index)
        recordedLast = recorded
        Trace.endSection()
        return laidOut.size > 0
    }

    private companion object {
        const val TAG = "compose-rs"
        /**
         * The typefaces of fonts Rust resolved (`family` 5 in a request), by a number never used
         * again: process-wide, as a typeface kept past a host or shared between views is one
         * (D1121). Workers register too (D1122).
         */
        val loaded = HashMap<Int, Typeface>()
        var nextTypeface = 0

        /** [typeface]'s number, or -1 without one. */
        fun register(typeface: Typeface?): Int {
            if (typeface == null) return -1
            return synchronized(loaded) {
                val number = nextTypeface++
                loaded[number] = typeface
                number
            }
        }

        /** About ten list items' paragraphs: what a fling releases before composing more. */
        const val NODE_POOL_CAPACITY = 64
        const val PAINT_CACHE_CAPACITY = 64
        /** A paragraph's recording before any was timed: a short one's, on a big core. */
        const val INITIAL_RECORD_NANOS = 30_000L
        /** The most one recording counts for in [recordNanos]. */
        const val MAX_RECORD_NANOS = 1_000_000L
        /** [paragraphLayout]'s floats: its header's, a line's and an offset's (Rust's `PARAGRAPH_*`). */
        const val PARAGRAPH_HEADER = 8
        const val PARAGRAPH_LINE = 10
        const val PARAGRAPH_OFFSET = 3
        const val PARAGRAPH_PLACEHOLDER = 5
    }
}

/**
 * The width of [text]'s widest segment between line break opportunities, as Compose's
 * `LayoutIntrinsics.computeMinIntrinsicWidth`: of the ten longest segments, the widest.
 */
private fun minIntrinsicWidth(text: CharSequence, paint: TextPaint): Float {
    val iterator = BreakIterator.getLineInstance(paint.textLocale)
    iterator.setText(text.toString())
    val candidates = PriorityQueue<IntArray>(MIN_WIDTH_CANDIDATES, compareBy { it[1] - it[0] })
    var start = 0
    var end = iterator.next()
    while (end != BreakIterator.DONE) {
        if (candidates.size < MIN_WIDTH_CANDIDATES) {
            candidates.add(intArrayOf(start, end))
        } else {
            val shortest = candidates.peek()
            if (shortest != null && shortest[1] - shortest[0] < end - start) {
                candidates.poll()
                candidates.add(intArrayOf(start, end))
            }
        }
        start = end
        end = iterator.next()
    }
    return candidates.maxOfOrNull { Layout.getDesiredWidth(text, it[0], it[1], paint) } ?: 0f
}

/** A space a line's visible end leaves out, as Compose's `LayoutHelper.isLineEndSpace`. */
internal fun isLineEndSpace(c: Char): Boolean =
    c == ' ' || c == '\n' || c == '\u1680' || (c in '\u2000'..'\u200A' && c != '\u2007') ||
        c == '\u205F' || c == '\u3000'

/** How many of the longest segments [minIntrinsicWidth] measures, as Compose's heap. */
private const val MIN_WIDTH_CANDIDATES = 10

/** A growable list of ints, without boxing them. */
internal class IntList {
    private var values = IntArray(16)
    var size = 0
        private set

    operator fun get(index: Int): Int = values[index]

    fun add(value: Int) {
        if (size == values.size) values = values.copyOf(size * 2)
        values[size++] = value
    }

    /** Removes the first [count] values. */
    fun removeFirst(count: Int) {
        if (count <= 0) return
        System.arraycopy(values, count, values, 0, size - count)
        size -= count
    }

    fun clear() {
        size = 0
    }
}
