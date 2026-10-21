package rs.compose.android

import android.graphics.Bitmap
import android.graphics.BlurMaskFilter
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Rect
import android.graphics.RectF
import android.util.SparseArray
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Replays compose-rs display lists (`crates/compose-android/src/serialize.rs`) on a `Canvas`:
 * on a hardware-accelerated view, the system records the calls and draws them on the GPU.
 * Paths and images are kept by id across frames, as the buffer defines and drops them.
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

    fun replay(canvas: Canvas, buffer: ByteBuffer) {
        buffer.order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(0)
        val start = canvas.saveCount
        while (buffer.hasRemaining()) {
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
                DEFINE_PATH -> definePath(buffer)
                DROP_PATH -> paths.remove(buffer.getInt())
                DEFINE_IMAGE -> defineImage(buffer)
                DROP_IMAGE -> {
                    val id = buffer.getInt()
                    images.get(id)?.recycle()
                    images.remove(id)
                }
                else -> {
                    // An unknown operation: the rest can't be read.
                    break
                }
            }
        }
        canvas.restoreToCount(start)
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
        const val DEFINE_PATH = 20
        const val DROP_PATH = 21
        const val DEFINE_IMAGE = 22
        const val DROP_IMAGE = 23
    }
}
