package rs.compose.android

import android.graphics.Canvas
import android.graphics.RenderNode
import android.graphics.Typeface
import android.graphics.text.LineBreaker
import android.text.Layout
import android.text.SpannableString
import android.text.Spanned
import android.text.StaticLayout
import android.text.TextDirectionHeuristics
import android.text.TextPaint
import android.text.TextUtils
import android.text.style.BackgroundColorSpan
import android.text.style.LineHeightSpan
import android.text.style.MetricAffectingSpan
import android.util.SparseArray
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.ceil

/**
 * Paragraphs laid out with `StaticLayout` for compose-rs (`crates/compose-android/src/text.rs`),
 * kept by handle to draw them. Rust sends a layout request (style runs and line options, see
 * `encode_request`) and reads the metrics back from [results].
 */
internal class TextMeasurer {
    /** Floats Rust reads after a call: a layout's metrics, or a cursor's rectangle. */
    val results: ByteBuffer = ByteBuffer.allocateDirect(32).order(ByteOrder.nativeOrder())

    private val layouts = SparseArray<Layout>()
    /** Each paragraph's drawing, recorded once: a frame draws the node, not the glyphs. */
    private val nodes = SparseArray<RenderNode>()
    private val typefaces = HashMap<Long, Typeface>()

    /** A text style, as the request encodes it. */
    private class Style(
        val color: Int,
        val fontPx: Float,
        val weight: Int,
        val italic: Boolean,
        val family: Int,
        val familyName: String?,
        val letterSpacingPx: Float,
        val decoration: Int,
        val lineHeightPx: Float,
        val background: Int,
    )

    private fun readStyle(request: ByteBuffer): Style {
        val color = request.getInt()
        val fontPx = request.getFloat()
        val weight = request.getInt()
        val italic = request.get().toInt() != 0
        val family = request.get().toInt()
        var name: String? = null
        if (family == 4) {
            val bytes = ByteArray(request.getInt())
            request.get(bytes)
            name = String(bytes, Charsets.UTF_8)
        }
        return Style(
            color = color,
            fontPx = fontPx,
            weight = weight,
            italic = italic,
            family = family,
            familyName = name,
            letterSpacingPx = request.getFloat(),
            decoration = request.get().toInt(),
            lineHeightPx = request.getFloat(),
            background = request.getInt(),
        )
    }

    private fun typeface(style: Style): Typeface {
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

    private fun apply(paint: TextPaint, style: Style) {
        paint.color = style.color
        paint.textSize = style.fontPx
        paint.typeface = typeface(style)
        paint.letterSpacing = if (style.fontPx > 0f) style.letterSpacingPx / style.fontPx else 0f
        paint.isUnderlineText = style.decoration and 1 != 0
        paint.isStrikeThruText = style.decoration and 2 != 0
    }

    /** A run's style over the paragraph's. */
    private inner class RunSpan(private val style: Style) : MetricAffectingSpan() {
        override fun updateDrawState(paint: TextPaint) = apply(paint, style)

        override fun updateMeasureState(paint: TextPaint) = apply(paint, style)
    }

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
        val paint = TextPaint(TextPaint.ANTI_ALIAS_FLAG)
        apply(paint, base)
        val runCount = request.getInt()
        val content: CharSequence = if (runCount == 0) {
            text
        } else {
            val spannable = SpannableString(text)
            repeat(runCount) {
                val start = request.getInt()
                val end = request.getInt()
                val style = readStyle(request)
                if (start < end) {
                    spannable.setSpan(RunSpan(style), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
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
        val text = if (base.lineHeightPx > 0f && content.isNotEmpty()) {
            val spannable = content as? SpannableString ?: SpannableString(content)
            spannable.setSpan(
                LineHeightSpan.Standard(base.lineHeightPx.toInt().coerceAtLeast(1)),
                0,
                spannable.length,
                Spanned.SPAN_INCLUSIVE_INCLUSIVE,
            )
            spannable
        } else {
            content
        }
        // The width to break lines at, as the desktop measurer: the constraint when wrapping
        // (or ellipsizing), else the text's own; then the text's width, kept in the constraints.
        val bounded = maxWidth != Int.MAX_VALUE
        val available = if ((softWrap || ellipsis) && bounded) maxWidth else Int.MAX_VALUE
        // The text's own width is a second measure of all of it: only when the constraints
        // leave a choice.
        val width = if (minWidth == available) {
            available
        } else {
            ceil(Layout.getDesiredWidth(text, paint)).toInt().coerceIn(minWidth, available)
        }
        val alignment = when (align) {
            1 -> Layout.Alignment.ALIGN_OPPOSITE
            2 -> Layout.Alignment.ALIGN_CENTER
            else -> Layout.Alignment.ALIGN_NORMAL
        }
        val builder = StaticLayout.Builder.obtain(text, 0, text.length, paint, width.coerceAtLeast(0))
            .setAlignment(alignment)
            // The direction Rust decided (its first strong character's), for the whole text, as
            // the desktop measurer: alignment and the cursor's movement agree with what's drawn.
            .setTextDirection(if (rtl) TextDirectionHeuristics.RTL else TextDirectionHeuristics.LTR)
            .setIncludePad(false)
            .setUseLineSpacingFromFallbacks(true)
            .setBreakStrategy(LineBreaker.BREAK_STRATEGY_SIMPLE)
            .setHyphenationFrequency(Layout.HYPHENATION_FREQUENCY_NONE)
            .setMaxLines(maxLines)
        if (ellipsis) builder.setEllipsize(TextUtils.TruncateAt.END)
        if (align == 3) builder.setJustificationMode(LineBreaker.JUSTIFICATION_MODE_INTER_WORD)
        val layout = builder.build()
        layouts.put(handle, layout)
        nodes.get(handle)?.discardDisplayList()
        nodes.remove(handle)
        // StaticLayout limits the lines only when it ellipsizes: the rest are cut here, as
        // Compose's `TextLayout` does (the text is clipped to its height).
        val lines = layout.lineCount.coerceIn(1, maxLines.coerceAtLeast(1))
        results.putFloat(0, width.toFloat())
        results.putFloat(4, layout.getLineBottom(lines - 1).toFloat())
        results.putFloat(8, layout.getLineBaseline(0).toFloat())
        results.putFloat(12, layout.getLineBaseline(lines - 1).toFloat())
        results.putFloat(16, lines.toFloat())
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
        results.putFloat(0, x)
        results.putFloat(4, layout.getLineTop(line).toFloat())
        results.putFloat(8, x)
        results.putFloat(12, layout.getLineBottom(line).toFloat())
    }

    fun release(handle: Int) {
        layouts.remove(handle)
        nodes.get(handle)?.discardDisplayList()
        nodes.remove(handle)
    }

    fun clear() {
        layouts.clear()
        for (index in 0 until nodes.size()) nodes.valueAt(index).discardDisplayList()
        nodes.clear()
    }

    /**
     * Draws paragraph [handle] with its top left at ([x], [y]): on a hardware canvas, its
     * render node, recorded the first time (a paragraph never changes once laid out).
     */
    fun draw(canvas: Canvas, handle: Int, x: Float, y: Float) {
        val layout = layouts.get(handle) ?: return
        if (!canvas.isHardwareAccelerated) {
            canvas.save()
            canvas.translate(x, y)
            layout.draw(canvas)
            canvas.restore()
            return
        }
        var node = nodes.get(handle)
        // Recorded again if the system dropped its recording (the window hid).
        if (node == null || !node.hasDisplayList()) {
            node = node ?: RenderNode("paragraph")
            // Its bounds: the layout's width and height, glyphs past them (an italic's
            // overhang, a line wider than an ellipsized width) not clipped.
            node.setPosition(0, 0, layout.width.coerceAtLeast(1), layout.height.coerceAtLeast(1))
            node.setClipToBounds(false)
            val recording = node.beginRecording()
            layout.draw(recording)
            node.endRecording()
            nodes.put(handle, node)
        }
        canvas.save()
        canvas.translate(x, y)
        canvas.drawRenderNode(node)
        canvas.restore()
    }
}
