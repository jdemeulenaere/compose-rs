package rs.compose.android

import android.annotation.TargetApi
import android.graphics.Canvas
import android.graphics.RenderNode
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
import android.util.Log
import android.util.SparseArray
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.ceil

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
) {
    /** Floats Rust reads after a call: a layout's metrics and the text's own width, or a cursor's rectangle. */
    val results: ByteBuffer = ByteBuffer.allocateDirect(32).order(ByteOrder.nativeOrder())

    private val layouts = SparseArray<Layout>()
    /** Each paragraph's drawing, recorded once: a frame draws the node, not the glyphs. */
    private val nodes = SparseArray<RenderNode>()
    /** Released paragraphs' nodes, for the next ones (D195). */
    private val pool = RenderNodePool("paragraph", NODE_POOL_CAPACITY)
    private val typefaces = HashMap<Long, Typeface>()
    /**
     * A paint per paragraph style, shared by the layouts of that style (a layout reads its
     * paint and never changes it): a paint's native side otherwise waits for the collector.
     */
    private val paints = HashMap<PaintStyle, TextPaint>()

    /** What of a text style its paint holds: the key of [paints]. */
    private data class PaintStyle(
        val color: Int,
        val fontPx: Float,
        val weight: Int,
        val italic: Boolean,
        val family: Int,
        val familyName: String?,
        val letterSpacingPx: Float,
        val decoration: Int,
    )

    /** A text style, as the request encodes it: its paint's part, and the spans'. */
    private class Style(val paint: PaintStyle, val lineHeightPx: Float, val background: Int)

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
        val paint = PaintStyle(
            color = color,
            fontPx = fontPx,
            weight = weight,
            italic = italic,
            family = family,
            familyName = name,
            letterSpacingPx = request.float(),
            decoration = request.get().toInt(),
        )
        return Style(paint, lineHeightPx = request.float(), background = request.getInt())
    }

    private fun typeface(style: PaintStyle): Typeface {
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
    ) : MetricAffectingSpan() {
        override fun updateDrawState(paint: TextPaint) = applyRun(paint)

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
        val lineHeight: Int,
        val letterSpaced: Boolean,
        val softWrap: Boolean,
        val maxLines: Int,
        val ellipsis: Boolean,
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
        /** [content] with its line height span, for a `StaticLayout` (made when one needs it). */
        val staticText: CharSequence by lazy(LazyThreadSafetyMode.NONE) { withLineHeight(content, lineHeight) }

        /**
         * These in another paint, which changes no metric (its color and decoration): nothing is
         * measured again, as Compose draws a kept paragraph in a new color.
         */
        fun inPaint(color: Int, decoration: Int): Intrinsics {
            val style = paintStyle.copy(color = color, decoration = decoration)
            return Intrinsics(
                content, style, paint(style), direction, boring, lineHeight, letterSpaced, softWrap, maxLines,
                ellipsis, align, boringWidth, maxIntrinsicWidth,
            )
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
        val softWrap = request.get().toInt() != 0
        val maxLines = request.getInt()
        val ellipsis = request.get().toInt() != 0
        val align = request.get().toInt()
        val rtl = request.get().toInt() != 0
        val minWidth = request.getInt()
        val maxWidth = request.getInt()
        val base = readStyle(request)
        val paint = paint(base.paint)
        val runCount = request.getInt()
        var letterSpaced = paint.letterSpacing != 0f
        val content: CharSequence = if (runCount == 0) {
            text
        } else {
            val spannable = SpannableString(text)
            repeat(runCount) {
                val start = request.getInt()
                val end = request.getInt()
                val inherits = request.get().toInt()
                val style = readStyle(request)
                if (start < end) {
                    val span = RunSpan(style.paint, ownColor = inherits and 1 == 0, ownDecoration = inherits and 2 == 0)
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
        // A line height over every line (LineHeightSpan.Standard's), none on an empty text.
        val lineHeight = if (base.lineHeightPx > 0f && content.isNotEmpty()) {
            base.lineHeightPx.toInt().coerceAtLeast(1)
        } else {
            0
        }
        // The direction Rust decided (its first strong character's), for the whole text, as the
        // desktop measurer: alignment and the cursor's movement agree with what's drawn.
        val direction = if (rtl) TextDirectionHeuristics.RTL else TextDirectionHeuristics.LTR
        // One line of left-to-right text has boring metrics (D330): its width, in one measure.
        val boring = boringMetrics(content, paint, direction, lineHeight)
        val boringWidth = if (boring == null) 0f else intrinsicWidth(boring.width.toFloat(), letterSpaced)
        val maxIntrinsicWidth = if (boring != null) {
            boringWidth
        } else {
            intrinsicWidth(ceil(Layout.getDesiredWidth(content, paint)), letterSpaced)
        }
        val kept = Intrinsics(
            content, base.paint, paint, direction, boring, lineHeight, letterSpaced, softWrap, maxLines, ellipsis,
            align, boringWidth, maxIntrinsicWidth,
        )
        layoutAt(handle, kept, minWidth, maxWidth)
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
        release(previous)
        layouts.put(handle, layout)
        intrinsics.put(handle, kept.inPaint(color, decoration))
        releaseNode(handle)
        if (collectingLaidOut) laidOut.add(handle)
        return true
    }

    /**
     * Lays out paragraph [previous]'s text again as paragraph [handle], between [minWidth] and
     * [maxWidth] (`Int.MAX_VALUE`: unbounded), from what [previous] kept of it ([Intrinsics]: no
     * measure of the text), as Compose's new `Paragraph` from the `ParagraphIntrinsics` its cache
     * keeps when only the constraints change; then releases [previous]. False (nothing changed,
     * [results] untouched) when [previous] kept nothing.
     */
    fun relayout(previous: Int, handle: Int, minWidth: Int, maxWidth: Int): Boolean {
        // Rust lays the text out anew (nothing kept: a paragraph cleared or never laid out).
        val kept = intrinsics.get(previous) ?: return false
        release(previous)
        layoutAt(handle, kept, minWidth, maxWidth)
        return true
    }

    /** Lays out [kept]'s text as paragraph [handle] at the constraints' width, writing [results]. */
    private fun layoutAt(handle: Int, kept: Intrinsics, minWidth: Int, maxWidth: Int) {
        // The width to break lines at, as Compose's `finalMaxWidth`: the constraint when wrapping
        // (or ellipsizing), else the text's own; then the text's width, kept in the constraints.
        val bounded = maxWidth != Int.MAX_VALUE
        val available = if ((kept.softWrap || kept.ellipsis) && bounded) maxWidth else Int.MAX_VALUE
        val width = if (minWidth == available) {
            available
        } else {
            ceil(kept.maxIntrinsicWidth).toInt().coerceIn(minWidth, available)
        }
        val maxLines = kept.maxLines
        // As Compose's `TextLayout`: a `BoringLayout` when the boring line fits the width, else
        // a `StaticLayout`.
        val boring = kept.boring
        val layout = if (Build.VERSION.SDK_INT >= 33 && boring != null && kept.boringWidth <= width) {
            val layout = boringLayout(kept.content, kept.paint, width, kept.alignment, boring)
            if (checkBoring) {
                val static = staticLayout(
                    kept.staticText, kept.paint, width, kept.alignment, kept.direction, maxLines, kept.ellipsis,
                    kept.align,
                )
                // A difference is logged, and the StaticLayout kept (as before D330).
                if (sameLayout(layout, static, kept.paint)) layout else static
            } else {
                layout
            }
        } else {
            staticLayout(
                kept.staticText, kept.paint, width.coerceAtLeast(0), kept.alignment, kept.direction, maxLines,
                kept.ellipsis, kept.align,
            )
        }
        layouts.put(handle, layout)
        intrinsics.put(handle, kept)
        releaseNode(handle)
        if (collectingLaidOut) laidOut.add(handle)
        // StaticLayout limits the lines only when it ellipsizes: the rest are cut here, as
        // Compose's `TextLayout` does (the text is clipped to its height).
        val lines = layout.lineCount.coerceIn(1, maxLines.coerceAtLeast(1))
        results.putInt(0, width.toFloat().toRawBits())
        results.putInt(4, (layout.getLineBottom(lines - 1).toFloat()).toRawBits())
        results.putInt(8, (layout.getLineBaseline(0).toFloat()).toRawBits())
        results.putInt(12, (layout.getLineBaseline(lines - 1).toFloat()).toRawBits())
        results.putInt(16, lines.toFloat().toRawBits())
        // Lines past max lines, cut or ellipsized, as Compose's `TextLayout.didExceedMaxLines`
        // (API 26+: the line count passes max lines unless it ellipsizes, which marks the last).
        val lastEllipsized = layout.lineCount > 0 && layout.getEllipsisCount(layout.lineCount - 1) > 0
        val exceeded = layout.lineCount > maxLines || lastEllipsized
        results.putInt(20, (if (exceeded) 1f else 0f).toRawBits())
        // The text's own width, from which Rust can tell another width would lay it out the same.
        results.putInt(24, kept.maxIntrinsicWidth.toRawBits())
    }

    /**
     * [text]'s metrics as a single boring line, or null: as Compose's `BoringLayoutFactory33`
     * (`BoringLayout.isBoring` with fallback line spacing, which says no to a text with a new
     * line, a tab, a surrogate pair (emoji), a character that could be right to left, a
     * paragraph span, or that [direction] calls right to left). API 33+ only: below it, a
     * `BoringLayout` can't use fallback fonts' line spacing as the `StaticLayout` does, so a line
     * with a taller fallback font (CJK) would be shorter (Compose accepts that difference).
     *
     * A [lineHeight] (px, 0 for none) is applied to the metrics as the `StaticLayout`'s
     * `LineHeightSpan.Standard` would apply it to its single line, rather than ruling the text
     * out as Compose's `LineHeightStyleSpan` does (its trimming and alignment are not ours; D330).
     */
    private fun boringMetrics(
        text: CharSequence,
        paint: TextPaint,
        direction: TextDirectionHeuristic,
        lineHeight: Int,
    ): BoringLayout.Metrics? {
        if (Build.VERSION.SDK_INT < 33) return null
        val metrics = BoringLayout.isBoring(text, paint, direction, true, null) ?: return null
        // LineHeightSpan.Standard.chooseHeight, to the line's ascent and descent.
        val height = metrics.descent - metrics.ascent
        if (lineHeight > 0 && height > 0) {
            val ratio = lineHeight * 1.0f / height
            metrics.descent = Math.round(metrics.descent * ratio)
            metrics.ascent = metrics.descent - lineHeight
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
    private fun withLineHeight(content: CharSequence, lineHeight: Int): CharSequence {
        if (lineHeight == 0) return content
        val spannable = SpannableString(content)
        spannable.setSpan(LineHeightSpan.Standard(lineHeight), 0, spannable.length, Spanned.SPAN_INCLUSIVE_INCLUSIVE)
        return spannable
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
        ellipsis: Boolean,
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
        if (ellipsis) builder.setEllipsize(TextUtils.TruncateAt.END)
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
        results.putInt(0, x.toRawBits())
        results.putInt(4, (layout.getLineTop(line).toFloat()).toRawBits())
        results.putInt(8, x.toRawBits())
        results.putInt(12, (layout.getLineBottom(line).toFloat()).toRawBits())
    }

    fun release(handle: Int) {
        layouts.remove(handle)
        intrinsics.remove(handle)
        releaseNode(handle)
    }

    /** Paragraph [handle]'s node back to the pool: no recording draws the paragraph again. */
    private fun releaseNode(handle: Int) {
        val node = nodes.get(handle) ?: return
        nodes.remove(handle)
        pool.recycle(node)
    }

    fun clear() {
        laidOut.clear()
        layouts.clear()
        intrinsics.clear()
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
        if (!hardware) {
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
     * copies them), then put back (other layouts of that style share it).
     */
    private fun drawInPaint(handle: Int, layout: Layout, canvas: Canvas) {
        val wanted = intrinsics.get(handle)?.paint
        val held = layout.paint
        if (wanted == null || wanted === held) {
            layout.draw(canvas)
            return
        }
        val color = held.color
        val underline = held.isUnderlineText
        val strike = held.isStrikeThruText
        held.color = wanted.color
        held.isUnderlineText = wanted.isUnderlineText
        held.isStrikeThruText = wanted.isStrikeThruText
        try {
            layout.draw(canvas)
        } finally {
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
        /** About ten list items' paragraphs: what a fling releases before composing more. */
        const val NODE_POOL_CAPACITY = 64
        const val PAINT_CACHE_CAPACITY = 64
        /** A paragraph's recording before any was timed: a short one's, on a big core. */
        const val INITIAL_RECORD_NANOS = 30_000L
        /** The most one recording counts for in [recordNanos]. */
        const val MAX_RECORD_NANOS = 1_000_000L
    }
}

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
