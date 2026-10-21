package rs.compose.android

import android.graphics.Bitmap
import android.graphics.BlurMaskFilter
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.RenderNode
import android.util.SparseArray
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Replays compose-rs display lists (`crates/compose-android/src/serialize.rs`) on a `Canvas`:
 * on a hardware-accelerated view, the system records the calls and draws them on the GPU.
 * Paths and images are kept by id across frames, as the buffer defines and drops them.
 *
 * A group (a repaint boundary: a graphics layer, a lazy item) is recorded into a `RenderNode`
 * of its own, kept while its bytes stay the same: a frame where it's unchanged (it scrolled, or
 * something beside it animates) draws the node instead of replaying its operations. The bytes
 * name everything they draw (paragraph handles, path and image ids aren't reused), so equal
 * bytes draw the same.
 */
internal class Replayer(private val text: TextMeasurer) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val shadowPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val bitmapPaint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val rect = RectF()
    private val source = Rect()
    private val clip = Path()
    private val radii = FloatArray(8)
    private val paths = SparseArray<Path>()
    private val images = SparseArray<Bitmap>()

    private fun ByteBuffer.readRect(into: RectF): RectF {
        into.set(getFloat(), getFloat(), getFloat(), getFloat())
        return into
    }

    /** Reads 4 corner radii (top left, top right, bottom right, bottom left) as 8 floats. */
    private fun ByteBuffer.readRadii(): FloatArray {
        for (corner in 0 until 4) {
            val radius = getFloat()
            radii[corner * 2] = radius
            radii[corner * 2 + 1] = radius
        }
        return radii
    }

    // The paint's last settings: a setter (a native call) runs only for a change.
    private var paintColor = paint.color
    private var paintStroke = false
    private var paintWidth = 0f
    private var paintCap = 0

    private fun ByteBuffer.readPaint(): Paint {
        val color = getInt()
        val stroke = get().toInt() != 0
        val width = getFloat()
        val cap = get().toInt()
        if (color != paintColor) {
            paintColor = color
            paint.color = color
        }
        if (stroke != paintStroke) {
            paintStroke = stroke
            paint.style = if (stroke) Paint.Style.STROKE else Paint.Style.FILL
        }
        if (width != paintWidth) {
            paintWidth = width
            paint.strokeWidth = width
        }
        if (cap != paintCap) {
            paintCap = cap
            paint.strokeCap = when (cap) {
                1 -> Paint.Cap.ROUND
                2 -> Paint.Cap.SQUARE
                else -> Paint.Cap.BUTT
            }
        }
        return paint
    }

    /** A group's recording, and the bytes it was recorded from. */
    private class Group(val node: RenderNode) {
        var bytes: ByteBuffer = ByteBuffer.allocateDirect(0).order(ByteOrder.LITTLE_ENDIAN)
        var length = 0
        var frame = 0
        /** The groups drawn inside it: kept as long as it is. */
        var children = ArrayList<Long>()
    }

    private val groups = HashMap<Long, Group>()
    private var frame = 0
    private var width = 1
    private var height = 1
    /** The groups met in the group being recorded (null outside any). */
    private var children: ArrayList<Long>? = null
    /** An unknown operation was met: the rest can't be read. */
    private var unreadable = false

    fun replay(canvas: Canvas, buffer: ByteBuffer) {
        buffer.order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(0)
        frame++
        width = canvas.width.coerceAtLeast(1)
        height = canvas.height.coerceAtLeast(1)
        children = null
        unreadable = false
        replay(canvas, buffer, buffer.limit())
        // The groups this frame didn't draw are gone.
        val iterator = groups.values.iterator()
        while (iterator.hasNext()) {
            val group = iterator.next()
            if (group.frame != frame) {
                group.node.discardDisplayList()
                iterator.remove()
            }
        }
    }

    /** Replays the operations up to [end] on [canvas], restoring what they saved. */
    private fun replay(canvas: Canvas, buffer: ByteBuffer, end: Int) {
        val start = canvas.saveCount
        while (buffer.position() < end && !unreadable) {
            when (buffer.get().toInt()) {
                SAVE -> canvas.save()
                RESTORE -> if (canvas.saveCount > start) canvas.restore()
                TRANSLATE -> canvas.translate(buffer.getFloat(), buffer.getFloat())
                CLIP_RECT -> canvas.clipRect(buffer.readRect(rect))
                CLIP_ROUND_RECT -> {
                    buffer.readRect(rect)
                    clip.reset()
                    clip.addRoundRect(rect, buffer.readRadii(), Path.Direction.CW)
                    canvas.clipPath(clip)
                }
                DRAW_RECT -> {
                    buffer.readRect(rect)
                    canvas.drawRect(rect, buffer.readPaint())
                }
                DRAW_ROUND_RECT -> {
                    buffer.readRect(rect)
                    val corners = buffer.readRadii()
                    val paint = buffer.readPaint()
                    drawRoundRect(canvas, corners, paint)
                }
                DRAW_LINE -> {
                    val x1 = buffer.getFloat()
                    val y1 = buffer.getFloat()
                    val x2 = buffer.getFloat()
                    val y2 = buffer.getFloat()
                    canvas.drawLine(x1, y1, x2, y2, buffer.readPaint())
                }
                DRAW_PARAGRAPH -> {
                    val handle = buffer.getInt()
                    text.draw(canvas, handle, buffer.getFloat(), buffer.getFloat())
                }
                SAVE_LAYER_ALPHA -> {
                    val alpha = (buffer.getFloat().coerceIn(0f, 1f) * 255f + 0.5f).toInt()
                    canvas.saveLayerAlpha(null, alpha)
                }
                TRANSFORM -> {
                    val pivotX = buffer.getFloat()
                    val pivotY = buffer.getFloat()
                    val scaleX = buffer.getFloat()
                    val scaleY = buffer.getFloat()
                    val rotation = buffer.getFloat()
                    val translateX = buffer.getFloat()
                    val translateY = buffer.getFloat()
                    canvas.translate(pivotX + translateX, pivotY + translateY)
                    canvas.rotate(rotation)
                    canvas.scale(scaleX, scaleY)
                    canvas.translate(-pivotX, -pivotY)
                }
                DRAW_IMAGE -> {
                    val image = images.get(buffer.getInt())
                    val src = buffer.readRect(RectF())
                    buffer.readRect(rect)
                    bitmapPaint.alpha = (buffer.getFloat().coerceIn(0f, 1f) * 255f + 0.5f).toInt()
                    if (image != null) {
                        source.set(src.left.toInt(), src.top.toInt(), src.right.toInt(), src.bottom.toInt())
                        canvas.drawBitmap(image, source, rect, bitmapPaint)
                    }
                }
                DRAW_SHADOW -> drawShadow(canvas, buffer)
                DRAW_ARC -> {
                    buffer.readRect(rect)
                    val startAngle = buffer.getFloat()
                    val sweepAngle = buffer.getFloat()
                    val useCenter = buffer.get().toInt() != 0
                    canvas.drawArc(rect, startAngle, sweepAngle, useCenter, buffer.readPaint())
                }
                DRAW_PATH -> {
                    val path = paths.get(buffer.getInt())
                    val x = buffer.getFloat()
                    val y = buffer.getFloat()
                    val scaleX = buffer.getFloat()
                    val scaleY = buffer.getFloat()
                    val paint = buffer.readPaint()
                    if (path != null) {
                        canvas.save()
                        canvas.translate(x, y)
                        canvas.scale(scaleX, scaleY)
                        canvas.drawPath(path, paint)
                        canvas.restore()
                    }
                }
                GROUP -> {
                    val key = buffer.getLong()
                    val length = buffer.getInt()
                    val from = buffer.position()
                    group(canvas, buffer, key, from, from + length)
                    buffer.position(from + length)
                }
                DEFINE_PATH -> definePath(buffer)
                DROP_PATH -> paths.remove(buffer.getInt())
                DEFINE_IMAGE -> defineImage(buffer)
                DROP_IMAGE -> {
                    val id = buffer.getInt()
                    images.get(id)?.recycle()
                    images.remove(id)
                }
                else -> unreadable = true
            }
        }
        canvas.restoreToCount(start)
    }

    /** Draws group [key], bytes [from] to [end]: its kept node if they're the same. */
    private fun group(canvas: Canvas, buffer: ByteBuffer, key: Long, from: Int, end: Int) {
        if (!canvas.isHardwareAccelerated) {
            replay(canvas, buffer, end)
            return
        }
        children?.add(key)
        val kept = groups[key]
        if (kept != null && kept.length == end - from && recorded(kept) &&
            same(kept.bytes, buffer, from, end - from)
        ) {
            keep(kept)
            canvas.drawRenderNode(kept.node)
            return
        }
        val group = kept ?: Group(RenderNode("compose-rs group")).also { groups[key] = it }
        group.frame = frame
        // Its bounds: the view's from the group's origin, what it draws past them not clipped.
        group.node.setPosition(0, 0, width, height)
        group.node.setClipToBounds(false)
        val outer = children
        val inner = ArrayList<Long>()
        children = inner
        val recording = group.node.beginRecording(width, height)
        try {
            replay(recording, buffer, end)
        } finally {
            group.node.endRecording()
            children = outer
        }
        group.children = inner
        val length = end - from
        if (group.bytes.capacity() < length) {
            group.bytes = ByteBuffer.allocateDirect(length + length / 4).order(ByteOrder.LITTLE_ENDIAN)
        }
        val source = buffer.duplicate()
        source.limit(end).position(from)
        group.bytes.clear()
        group.bytes.put(source)
        group.length = length
        canvas.drawRenderNode(group.node)
    }

    /**
     * Whether [group] and the groups inside it still have their recordings: the system drops
     * them all when the window hides (`destroyHardwareResources`).
     */
    private fun recorded(group: Group): Boolean =
        group.node.hasDisplayList() && group.children.all { child -> groups[child]?.let(::recorded) ?: false }

    /** Marks [group] and the groups inside it drawn this frame. */
    private fun keep(group: Group) {
        group.frame = frame
        for (child in group.children) groups[child]?.let(::keep)
    }

    /** Whether [length] bytes of [buffer] from [from] are [kept]'s. */
    private fun same(kept: ByteBuffer, buffer: ByteBuffer, from: Int, length: Int): Boolean {
        var at = 0
        while (at + 8 <= length) {
            if (kept.getLong(at) != buffer.getLong(from + at)) return false
            at += 8
        }
        while (at < length) {
            if (kept.get(at) != buffer.get(from + at)) return false
            at++
        }
        return true
    }

    private fun drawRoundRect(canvas: Canvas, corners: FloatArray, paint: Paint) {
        val first = corners[0]
        if (corners.all { it == first }) {
            canvas.drawRoundRect(rect, first, first, paint)
        } else {
            clip.reset()
            clip.addRoundRect(rect, corners, Path.Direction.CW)
            canvas.drawPath(clip, paint)
        }
    }

    /**
     * A Material shadow, approximated with a blurred copy of the outline below it (the outline
     * itself is drawn over by the surface): Android's own elevation shadows need render nodes.
     */
    private fun drawShadow(canvas: Canvas, buffer: ByteBuffer) {
        val rounded = buffer.get().toInt() != 0
        buffer.readRect(rect)
        val corners = buffer.readRadii()
        val elevation = buffer.getFloat()
        buffer.getFloat()
        buffer.getFloat()
        buffer.getFloat()
        buffer.getFloat()
        val ambient = buffer.getInt()
        val spot = buffer.getInt()
        if (elevation <= 0f) return
        // The spot shadow: offset down, blurred by the elevation.
        val alpha = ((ambient ushr 24) + (spot ushr 24)).coerceAtMost(255)
        shadowPaint.color = (alpha shl 24) or (spot and 0xFFFFFF)
        shadowPaint.maskFilter = BlurMaskFilter(elevation, BlurMaskFilter.Blur.NORMAL)
        canvas.save()
        canvas.translate(0f, elevation / 2f)
        if (rounded) {
            clip.reset()
            clip.addRoundRect(rect, corners, Path.Direction.CW)
            canvas.drawPath(clip, shadowPaint)
        } else {
            canvas.drawRect(rect, shadowPaint)
        }
        canvas.restore()
    }

    private fun definePath(buffer: ByteBuffer) {
        val id = buffer.getInt()
        val path = Path()
        path.fillType = if (buffer.get().toInt() == 1) Path.FillType.EVEN_ODD else Path.FillType.WINDING
        repeat(buffer.getInt()) {
            when (buffer.get().toInt()) {
                0 -> path.moveTo(buffer.getFloat(), buffer.getFloat())
                1 -> path.lineTo(buffer.getFloat(), buffer.getFloat())
                2 -> path.quadTo(buffer.getFloat(), buffer.getFloat(), buffer.getFloat(), buffer.getFloat())
                3 -> path.cubicTo(
                    buffer.getFloat(),
                    buffer.getFloat(),
                    buffer.getFloat(),
                    buffer.getFloat(),
                    buffer.getFloat(),
                    buffer.getFloat(),
                )
                else -> path.close()
            }
        }
        paths.put(id, path)
    }

    private fun defineImage(buffer: ByteBuffer) {
        val id = buffer.getInt()
        val width = buffer.getInt()
        val height = buffer.getInt()
        val length = buffer.getInt()
        val pixels = buffer.slice()
        pixels.limit(length)
        buffer.position(buffer.position() + length)
        // Premultiplied RGBA bytes: ARGB_8888's memory layout.
        val bitmap = Bitmap.createBitmap(width.coerceAtLeast(1), height.coerceAtLeast(1), Bitmap.Config.ARGB_8888)
        bitmap.copyPixelsFromBuffer(pixels)
        images.get(id)?.recycle()
        images.put(id, bitmap)
    }

    fun clear() {
        for (group in groups.values) group.node.discardDisplayList()
        groups.clear()
        paths.clear()
        for (index in 0 until images.size()) images.valueAt(index).recycle()
        images.clear()
    }

    private companion object {
        const val SAVE = 0
        const val RESTORE = 1
        const val TRANSLATE = 2
        const val CLIP_RECT = 3
        const val CLIP_ROUND_RECT = 4
        const val DRAW_RECT = 5
        const val DRAW_ROUND_RECT = 6
        const val DRAW_LINE = 7
        const val DRAW_PARAGRAPH = 8
        const val SAVE_LAYER_ALPHA = 9
        const val TRANSFORM = 10
        const val DRAW_IMAGE = 11
        const val DRAW_SHADOW = 12
        const val DRAW_ARC = 13
        const val DRAW_PATH = 14
        const val GROUP = 15
        const val DEFINE_PATH = 20
        const val DROP_PATH = 21
        const val DEFINE_IMAGE = 22
        const val DROP_IMAGE = 23
    }
}
