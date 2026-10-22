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
import kotlin.math.ceil
import kotlin.math.floor

/**
 * Replays compose-rs display lists (`crates/compose-android/src/serialize.rs`) on a `Canvas`:
 * on a hardware-accelerated view, the system records the calls and draws them on the GPU.
 * Paths and images are kept by id across frames, as the buffer defines and drops them. A small
 * filled path is drawn from an alpha mask rasterized once per scale (D190), as Compose's
 * `VectorPainter` draws an icon from a cached bitmap: the GPU draws a textured quad instead of
 * rendering the path (through a scratch atlas, an extra render pass, every frame).
 *
 * A group (a repaint boundary: a graphics layer, a lazy item) is recorded into a `RenderNode`
 * of its own, kept by its key and version (D157). Rust gives a group a new version whenever
 * what it draws changes, a nested group's change included, so a key and a version always come
 * with the same bytes: a frame where a group is unchanged (it scrolled, or something beside it
 * animates) draws its node, its bytes skipped unread, nested groups' records included.
 *
 * A graphics layer's properties (alpha, transform, clip) are in no group's bytes: the frame's
 * `LAYERS` section lists them for every group that has a layer, nested ones in kept parents
 * included, and they're set on the groups' nodes before the replay. So a layer-only animation
 * records nothing again: it sets properties, and the view's own recording draws the nodes.
 *
 * Every frame's buffer holds every group's bytes (Rust copies unchanged ones from a cache), so
 * a group whose recording the system dropped (the window hid) is recorded again from them.
 */
internal class Replayer(private val text: TextMeasurer) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val shadowPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val bitmapPaint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val rect = RectF()
    private val source = Rect()
    private val clip = Path()
    private val radii = FloatArray(8)
    private val paths = SparseArray<KeptPath>()
    /**
     * The alpha masks of small filled paths, least recently recorded first (D190): a kept
     * group's node draws its masks without replaying, so it doesn't touch them.
     */
    private val masks = LinkedHashMap<Mask, Unit>(16, 0.75f, true)
    private var maskBytes = 0
    private val maskCanvas = Canvas()
    private val maskFill = Paint(Paint.ANTI_ALIAS_FLAG)
    /** Draws a mask in the path's color (an `ALPHA_8` bitmap takes the paint's). */
    private val maskPaint = Paint(Paint.FILTER_BITMAP_FLAG)
    private var maskPaintColor = maskPaint.color
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

    /**
     * A group's render node. Its origin is the group's; what it draws outside its bounds isn't
     * clipped (`setClipToBounds(false)`), except by an alpha's offscreen layer, which they bound.
     * A layer's node has the layer's bounds (from the group's origin to the layer's far
     * corner: the layer's own when it's at the group's origin, as Compose's), others the
     * view's size (their bounds only limit what a change of theirs repaints, as D154's).
     */
    private class Group {
        val node = RenderNode("compose-rs group").apply {
            setClipToBounds(false)
            // Until a layer says otherwise (setLayer): an alpha below 1 goes offscreen.
            setHasOverlappingRendering(true)
        }
        /** The version [node] holds the recording of (-1: none). */
        var version = -1L
        /** The last frame that drew it: a group not drawn by a frame is gone. */
        var frame = 0
        /** The last frame whose `LAYERS` section listed it. */
        var layerFrame = 0
        /** Its node has a layer's properties, [applied] (else they're the identity). */
        var layered = false
        /** The `LAYERS` entry (after its key) last set on [node], while [layered]. */
        val applied = ByteArray(LAYER_ENTRY_BYTES - 8)
        /** The groups drawn inside it: kept as long as it is. */
        var children = ArrayList<Long>()
    }

    private val groups = HashMap<Long, Group>()
    private var frame = 0
    private var width = 1
    private var height = 1
    /** The groups met in the group being recorded (null outside any). */
    private var children: ArrayList<Long>? = null
    /** In a software draw: where each group's `LAYERS` entry is in the buffer. */
    private val softwareLayers = HashMap<Long, Int>()
    private val clipBounds = Rect()
    /** An unknown operation was met: the rest can't be read. */
    private var unreadable = false

    fun replay(canvas: Canvas, buffer: ByteBuffer) {
        buffer.order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(0)
        children = null
        unreadable = false
        if (!canvas.isHardwareAccelerated) {
            // A software draw (a snapshot) replays everything, layers applied as operations,
            // and leaves the groups alone: the window's last frame still draws their nodes.
            softwareLayers.clear()
            replay(canvas, buffer, buffer.limit())
            return
        }
        frame++
        width = canvas.width.coerceAtLeast(1)
        height = canvas.height.coerceAtLeast(1)
        replay(canvas, buffer, buffer.limit())
        val iterator = groups.values.iterator()
        while (iterator.hasNext()) {
            val group = iterator.next()
            if (group.frame != frame) {
                // Gone: this frame didn't draw it.
                group.node.discardDisplayList()
                iterator.remove()
            } else if (group.layered && group.layerFrame != frame) {
                // No longer a layer. (Properties set after drawing the node still apply: the
                // render thread reads them when the frame is done.)
                resetLayer(group.node)
                group.node.setPosition(0, 0, width, height)
                group.layered = false
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
                    val id = buffer.getInt()
                    val kept = paths.get(id)
                    val x = buffer.getFloat()
                    val y = buffer.getFloat()
                    val scaleX = buffer.getFloat()
                    val scaleY = buffer.getFloat()
                    val paint = buffer.readPaint()
                    if (kept != null && (paintStroke || !drawMask(canvas, kept, x, y, scaleX, scaleY))) {
                        canvas.save()
                        canvas.translate(x, y)
                        canvas.scale(scaleX, scaleY)
                        canvas.drawPath(kept.path, paint)
                        canvas.restore()
                    }
                }
                GROUP -> {
                    val key = buffer.getLong()
                    val version = buffer.getLong()
                    val length = buffer.getInt()
                    val from = buffer.position()
                    group(canvas, buffer, key, version, from + length)
                    buffer.position(from + length)
                }
                LAYERS -> layers(canvas.isHardwareAccelerated, buffer)
                DEFINE_PATH -> definePath(buffer)
                DROP_PATH -> dropPath(buffer.getInt())
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

    /**
     * Draws group [key] at [version], its content the bytes up to [end]: its kept node if it
     * holds that version's recording (whole: the system drops recordings when the window
     * hides), else recorded from them first.
     */
    private fun group(canvas: Canvas, buffer: ByteBuffer, key: Long, version: Long, end: Int) {
        if (!canvas.isHardwareAccelerated) {
            // Its layer as the operations Rust left out: transform, clip, alpha layer.
            val save = canvas.save()
            softwareLayers[key]?.let { at -> applyLayer(canvas, buffer, at) }
            replay(canvas, buffer, end)
            canvas.restoreToCount(save)
            return
        }
        children?.add(key)
        val group = groups.getOrPut(key) { Group() }
        if (group.version == version && recorded(group)) {
            // The same bytes as recorded, nested groups' included: their versions are too.
            keep(group)
            canvas.drawRenderNode(group.node)
            return
        }
        group.frame = frame
        // Matches nothing until recorded whole.
        group.version = -1L
        // A layer's node has its bounds already (setLayer).
        if (!group.layered) group.node.setPosition(0, 0, width, height)
        val outer = children
        val inner = ArrayList<Long>()
        children = inner
        // Its size is a hint: what's recorded past it is kept, as a view's children drawn past
        // its bounds are.
        val recording =
            group.node.beginRecording(group.node.width.coerceAtLeast(1), group.node.height.coerceAtLeast(1))
        try {
            replay(recording, buffer, end)
        } finally {
            group.node.endRecording()
            children = outer
        }
        group.children = inner
        // A recording cut short by an unknown operation isn't that version's.
        if (!unreadable) group.version = version
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

    /**
     * The `LAYERS` section: each group's layer, set on its node (a new group's node made now,
     * recorded when its record comes), whether its record is read this frame or skipped inside
     * a kept parent's. In a software draw, where each entry is, for [group].
     */
    private fun layers(hardware: Boolean, buffer: ByteBuffer) {
        val count = buffer.getInt()
        var at = buffer.position()
        repeat(count) {
            val key = buffer.getLong(at)
            if (hardware) {
                val group = groups.getOrPut(key) { Group() }
                // Set only when it changed: a frame where nothing moves sets nothing.
                if (!group.layered || !applied(group.applied, buffer, at + 8)) {
                    for (index in group.applied.indices) group.applied[index] = buffer.get(at + 8 + index)
                    setLayer(group.node, buffer, at + 8)
                    group.layered = true
                }
                group.layerFrame = frame
            } else {
                softwareLayers[key] = at + 8
            }
            at += LAYER_ENTRY_BYTES
        }
        buffer.position(at)
    }

    /** Whether [applied] holds the `LAYERS` entry at [at] (after its key). */
    private fun applied(applied: ByteArray, buffer: ByteBuffer, at: Int): Boolean {
        for (index in applied.indices) {
            if (applied[index] != buffer.get(at + index)) return false
        }
        return true
    }

    /**
     * Sets the layer at [at] (a `LAYERS` entry after its key: alpha, pivot, scale, rotation,
     * translation, clip, overlapping, bounds) on [node]. A node scales and rotates around its
     * pivot then translates, clips in its own coordinates and applies its alpha last, as the
     * operations Rust left out (`Transform`, `ClipRect`, `SaveLayerAlpha`). Its position is the
     * group's origin (a recording doesn't depend on the layer's offset, which can change
     * without the group's version) to the layer's far corner.
     */
    private fun setLayer(node: RenderNode, buffer: ByteBuffer, at: Int) {
        node.setPosition(0, 0, ceil(buffer.getFloat(at + 58)).toInt(), ceil(buffer.getFloat(at + 62)).toInt())
        node.setAlpha(buffer.getFloat(at))
        node.setPivotX(buffer.getFloat(at + 4))
        node.setPivotY(buffer.getFloat(at + 8))
        node.setScaleX(buffer.getFloat(at + 12))
        node.setScaleY(buffer.getFloat(at + 16))
        node.setRotationZ(buffer.getFloat(at + 20))
        node.setTranslationX(buffer.getFloat(at + 24))
        node.setTranslationY(buffer.getFloat(at + 28))
        if (buffer.get(at + 32).toInt() != 0) {
            // Whole pixels: a layer's bounds are its node's (whole) size at its (whole) offset.
            clipBounds.set(
                floor(buffer.getFloat(at + 33)).toInt(),
                floor(buffer.getFloat(at + 37)).toInt(),
                ceil(buffer.getFloat(at + 41)).toInt(),
                ceil(buffer.getFloat(at + 45)).toInt(),
            )
            node.setClipRect(clipBounds)
        } else {
            node.setClipRect(null)
        }
        // An alpha below 1 draws the content offscreen then composites it, as Compose's layers
        // do (`CompositingStrategy.Auto`), unless it's one drawing: then each drawing's alpha
        // is multiplied instead, as D153's folded layers (the same pixels, no offscreen pass).
        node.setHasOverlappingRendering(buffer.get(at + 49).toInt() != 0)
    }

    /** No layer: [node]'s properties back to the identity. */
    private fun resetLayer(node: RenderNode) {
        node.setAlpha(1f)
        node.setScaleX(1f)
        node.setScaleY(1f)
        node.setRotationZ(0f)
        node.setTranslationX(0f)
        node.setTranslationY(0f)
        node.setClipRect(null)
        node.setHasOverlappingRendering(true)
    }

    /** The layer at [at] (as [setLayer] reads it) applied on a software [canvas]. */
    private fun applyLayer(canvas: Canvas, buffer: ByteBuffer, at: Int) {
        val alpha = buffer.getFloat(at)
        val pivotX = buffer.getFloat(at + 4)
        val pivotY = buffer.getFloat(at + 8)
        canvas.translate(pivotX + buffer.getFloat(at + 24), pivotY + buffer.getFloat(at + 28))
        canvas.rotate(buffer.getFloat(at + 20))
        canvas.scale(buffer.getFloat(at + 12), buffer.getFloat(at + 16))
        canvas.translate(-pivotX, -pivotY)
        if (buffer.get(at + 32).toInt() != 0) {
            canvas.clipRect(
                buffer.getFloat(at + 33),
                buffer.getFloat(at + 37),
                buffer.getFloat(at + 41),
                buffer.getFloat(at + 45),
            )
        }
        if (alpha < 1f) {
            // Bounded as the node's offscreen layer is.
            canvas.saveLayerAlpha(
                0f,
                0f,
                ceil(buffer.getFloat(at + 58)),
                ceil(buffer.getFloat(at + 62)),
                (alpha.coerceIn(0f, 1f) * 255f + 0.5f).toInt(),
            )
        }
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

    /** A path, its bounds (its control points', in its units) and its masks (D190). */
    private class KeptPath(val path: Path) {
        private var computedBounds: RectF? = null

        /** Computed on first use: a stroked path (a text field's outline) never needs them. */
        val bounds: RectF
            get() = computedBounds ?: RectF().also {
                // `exact` is ignored (and deprecated for that) on API 31+: the control points'.
                @Suppress("DEPRECATION")
                path.computeBounds(it, true)
                computedBounds = it
            }

        /** Its masks, least recently recorded first: at most [MAX_MASKS_PER_PATH]. */
        val masks = ArrayList<Mask>(1)

        /** Its mask at ([scaleX], [scaleY]), moved last, or null. */
        fun mask(scaleX: Float, scaleY: Float): Mask? {
            for (index in masks.indices) {
                val mask = masks[index]
                if (mask.scaleX == scaleX && mask.scaleY == scaleY) {
                    if (index != masks.lastIndex) masks.add(masks.removeAt(index))
                    return mask
                }
            }
            return null
        }
    }

    /** A path's mask at a scale, its top left corner from the path's drawn origin, in pixels. */
    private class Mask(
        val path: KeptPath,
        val scaleX: Float,
        val scaleY: Float,
        val bitmap: Bitmap,
        val left: Float,
        val top: Float,
    )

    /**
     * Draws filled path [kept] scaled by [scaleX] and [scaleY] at ([x], [y]) from its mask,
     * made on first use; false (nothing drawn) when the scaled path is larger than
     * [MAX_MASK_SIZE]. A path keeps [MAX_MASKS_PER_PATH] masks: a size animating makes one per
     * frame (as Compose's `DrawCache` draws its bitmap again), but they don't fill the cache.
     *
     * The mask is the path rasterized (anti-aliased, by the CPU) at its drawn size, its corners
     * on whole pixels from ([x], [y]): drawn where Rust placed the path (on whole pixels, as
     * layout and an icon's centering round), it covers the pixels `drawPath` would, edges
     * anti-aliased by Skia's raster scan converter instead of the GPU's. Drawn on fractional
     * pixels or under a scaling layer, it's filtered (bilinear), as Compose's `VectorPainter`
     * bitmap is: keyed by scale only, a path has one mask per drawn size, not one per
     * subpixel position.
     */
    private fun drawMask(
        canvas: Canvas,
        kept: KeptPath,
        x: Float,
        y: Float,
        scaleX: Float,
        scaleY: Float,
    ): Boolean {
        val mask = kept.mask(scaleX, scaleY)?.also { masks[it] }
            ?: makeMask(kept, scaleX, scaleY)
            ?: return false
        if (paintColor != maskPaintColor) {
            maskPaintColor = paintColor
            maskPaint.color = paintColor
        }
        canvas.drawBitmap(mask.bitmap, x + mask.left, y + mask.top, maskPaint)
        return true
    }

    private fun makeMask(kept: KeptPath, scaleX: Float, scaleY: Float): Mask? {
        val bounds = kept.bounds
        val x1 = bounds.left * scaleX
        val x2 = bounds.right * scaleX
        val y1 = bounds.top * scaleY
        val y2 = bounds.bottom * scaleY
        // A pixel of margin each side, for anti-aliasing.
        val left = floor(minOf(x1, x2)) - 1f
        val top = floor(minOf(y1, y2)) - 1f
        val width = (ceil(maxOf(x1, x2)) + 1f - left).toInt()
        val height = (ceil(maxOf(y1, y2)) + 1f - top).toInt()
        if (bounds.isEmpty || width !in 1..MAX_MASK_SIZE || height !in 1..MAX_MASK_SIZE) return null
        val drawn = Bitmap.createBitmap(width, height, Bitmap.Config.ALPHA_8)
        maskCanvas.setBitmap(drawn)
        maskCanvas.save()
        maskCanvas.translate(-left, -top)
        maskCanvas.scale(scaleX, scaleY)
        maskCanvas.drawPath(kept.path, maskFill)
        maskCanvas.restore()
        maskCanvas.setBitmap(null)
        // An immutable copy, so the render thread uploads it once and never checks it for
        // changes (`setImmutable` isn't public API). The drawn one was never recorded.
        val bitmap = drawn.copy(Bitmap.Config.ALPHA_8, false)
        drawn.recycle()
        // Drawn pixel for pixel, whatever the canvas's density.
        bitmap.density = Bitmap.DENSITY_NONE
        val mask = Mask(kept, scaleX, scaleY, bitmap, left, top)
        if (kept.masks.size == MAX_MASKS_PER_PATH) forget(kept.masks.removeAt(0))
        kept.masks.add(mask)
        masks[mask] = Unit
        maskBytes += bitmap.allocationByteCount
        // The least recently recorded masks go past the budget. Not recycled: a kept group's
        // recording may still draw them (the garbage collector frees them after it).
        val iterator = masks.keys.iterator()
        while (maskBytes > MAX_MASK_BYTES && iterator.hasNext()) {
            val oldest = iterator.next()
            if (oldest === mask) break
            iterator.remove()
            oldest.path.masks.remove(oldest)
            maskBytes -= oldest.bitmap.allocationByteCount
        }
        return mask
    }

    /** Forgets [mask], taken from its path's. */
    private fun forget(mask: Mask) {
        masks.remove(mask)
        maskBytes -= mask.bitmap.allocationByteCount
    }

    private fun dropPath(id: Int) {
        val kept = paths.get(id) ?: return
        paths.remove(id)
        for (mask in kept.masks) forget(mask)
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
        // An id is defined once, but a stale mask must never outlive its path.
        dropPath(id)
        paths.put(id, KeptPath(path))
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
        masks.clear()
        maskBytes = 0
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
        const val LAYERS = 16
        const val DEFINE_PATH = 20
        const val DROP_PATH = 21
        const val DEFINE_IMAGE = 22
        const val DROP_IMAGE = 23

        /** The largest path (scaled, in pixels each way) drawn from a mask (D190). */
        const val MAX_MASK_SIZE = 256
        /** The masks' memory budget: 4 MB, 64 masks of 256 by 256 pixels, ~500 icons of 24 dp at 3.5x. */
        const val MAX_MASK_BYTES = 4 shl 20
        /** The masks a path keeps, one per drawn size (D190). */
        const val MAX_MASKS_PER_PATH = 4

        /**
         * The bytes of a `LAYERS` entry: its key, 8 floats, whether it clips, its clip, whether
         * it overlaps, its bounds.
         */
        const val LAYER_ENTRY_BYTES = 8 + 8 * 4 + 1 + 4 * 4 + 1 + 4 * 4
    }
}
