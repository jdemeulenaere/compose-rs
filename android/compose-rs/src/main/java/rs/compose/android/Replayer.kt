package rs.compose.android

import android.graphics.Bitmap
import android.graphics.BlurMaskFilter
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PorterDuff
import android.graphics.RadialGradient
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.RenderNode
import android.graphics.Shader
import android.graphics.SweepGradient
import android.os.Build
import android.util.SparseArray
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.ceil
import kotlin.math.floor

/**
 * A float read as its bits: `ByteBuffer.getFloat` isn't compiled in the boot image (it runs in
 * the interpreter, and the JIT doesn't compile it either), `getInt` is, and `fromBits` is an
 * intrinsic.
 */
@Suppress("NOTHING_TO_INLINE")
internal inline fun ByteBuffer.float(): Float = Float.fromBits(getInt())

@Suppress("NOTHING_TO_INLINE")
internal inline fun ByteBuffer.floatAt(index: Int): Float = Float.fromBits(getInt(index))

/**
 * Replays compose-rs display lists (`crates/compose-android/src/serialize.rs`) on a `Canvas`:
 * on a hardware-accelerated view, the system records the calls and draws them on the GPU.
 * Paths and images are kept by id across frames, as the buffer defines and drops them. A small
 * filled path is drawn from an alpha mask rasterized once per scale (D190), as Compose's
 * `VectorPainter` draws an icon from a cached bitmap: the GPU draws a textured quad instead of
 * rendering the path (through a scratch atlas, an extra render pass, every frame).
 *
 * A dropped path's `Path` is rewound at once and kept for the next definition (D410), as
 * Compose's apps reset one `Path` to draw a shape that changes each frame: its native memory,
 * and the mask HWUI's GPU cache keeps for its contents, go when the recordings drawing it do,
 * not when the collector finalizes it. A recording that draws paths new each frame (an
 * animated drawing) draws them without masks (D411). A frame dropping a path drawn as one
 * flushes HWUI's layers first ([flushNode], D510), so the GPU cache reuses the dropped paths'
 * masks' textures for this frame's.
 *
 * A gradient comes whole in each paint drawing it (D210): its `Shader` is kept by its bytes, so
 * a frame drawing the same gradients builds none.
 *
 * A group (a repaint boundary: a graphics layer, a lazy item) is recorded into a `RenderNode`
 * of its own, kept by its key and version (D157). Rust gives a group's record a new version
 * whenever its bytes change, so a key and a version always come with the same bytes: a frame
 * where a group is unchanged (it scrolled, or something beside it animates) draws its node,
 * its bytes skipped unread. A record names the groups drawn in it (`GROUP_REF`): their nodes
 * are drawn by reference, recorded from their own records (D323), so a group whose nested
 * group changed keeps its recording.
 *
 * A graphics layer's properties (alpha, transform, clip) are in no group's bytes: the frame's
 * `LAYERS` section lists them for every group that has a layer, nested ones in kept parents
 * included, and they're set on the groups' nodes before the replay. So a layer-only animation
 * records nothing again: it sets properties, and the view's own recording draws the nodes.
 *
 * Every frame's buffer holds every group's bytes (Rust copies unchanged ones from a cache), so
 * a group whose recording the system dropped (the window hid) is recorded again from them.
 *
 * Between frames, the groups of an item a prefetch measured are recorded ahead ([recordAhead],
 * D350) from records Rust wrote ahead, as a frame would record them: kept apart (a frame not
 * naming them doesn't drop them) until a frame names one, which then finds its version
 * recorded, and draws it by reference.
 *
 * A group composed and not placed (a lazy list's item kept beside the wanted ones) keeps its
 * render node: every frame names it in a `KEPT` section, without its record (Rust keeps that
 * in its cache), rather than drawing it (D380).
 * HWUI drops the recording of a node no recording draws any more (when the render thread
 * syncs a frame that no longer draws it), so the kept groups' nodes are drawn by [keptNode], a
 * node at alpha 0, which the render thread skips: the frame placing one again finds its version
 * recorded, and records nothing.
 */
internal class Replayer(private val text: TextMeasurer) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val shadowPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    /**
     * Blur filters by elevation (a filter is a native object the collector frees): a few
     * elevations, kept in arrays so a lookup boxes nothing, the oldest replaced when full.
     */
    private val blurElevations = FloatArray(BLUR_CACHE_CAPACITY)
    private val blurFilters = arrayOfNulls<BlurMaskFilter>(BLUR_CACHE_CAPACITY)
    private var blurCount = 0
    private var blurNext = 0
    /**
     * Gradients by their bytes (kind to stops) and local transform (x, y, scale x, scale y
     * each), kept as the blur filters are: a frame's gradients are built once, not each frame.
     */
    private val shaderKeys = arrayOfNulls<ByteArray>(SHADER_CACHE_CAPACITY)
    private val shaderLocals = FloatArray(SHADER_CACHE_CAPACITY * 4)
    private val shaders = arrayOfNulls<Shader>(SHADER_CACHE_CAPACITY)
    private var shaderCount = 0
    private var shaderNext = 0
    private val bitmapPaint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val rect = RectF()
    private val source = Rect()
    private val clip = Path()
    private val radii = FloatArray(8)
    private val paths = SparseArray<KeptPath>()
    /** Dropped paths, rewound, for the next definitions (D410). */
    private val pathPool = ArrayList<KeptPath>(PATH_POOL_CAPACITY)
    /**
     * The group being recorded (null: the frame's own canvas), and whether it draws the paths
     * defined this frame without masks: it drew new ones in the last frames as well, so they
     * change every frame (D411).
     */
    private var recordingGroup: Group? = null
    private var skipNewMasks = false
    /** The last frame the frame's own canvas drew a path new that frame that a mask could draw. */
    private var newPathFrame = NEVER
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
        into.set(float(), float(), float(), float())
        return into
    }

    /** Reads 4 corner radii (top left, top right, bottom right, bottom left) as 8 floats. */
    private fun ByteBuffer.readRadii(): FloatArray {
        for (corner in 0 until 4) {
            val radius = float()
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
    private var paintShader: Shader? = null

    /**
     * Reads a paint: color, style, width, cap, then its shader (a kind; for a gradient, its
     * definition and alpha). A drawing scaled by ([scaleX], [scaleY]) then moved by ([x], [y])
     * (a path's) has its gradient moved back, as compose-skia's: a gradient is in the list's
     * coordinates.
     */
    private fun ByteBuffer.readPaint(x: Float = 0f, y: Float = 0f, scaleX: Float = 1f, scaleY: Float = 1f): Paint {
        var color = getInt()
        val stroke = get().toInt() != 0
        val width = float()
        val cap = get().toInt()
        val kind = get().toInt()
        val shader = if (kind == SHADER_NONE) null else readShader(kind, x, y, scaleX, scaleY)
        if (shader != null) {
            // The shader paints; the paint's alpha is the gradient's, as compose-skia's.
            color = (float().coerceIn(0f, 1f) * 255f + 0.5f).toInt() shl 24
        }
        if (shader !== paintShader) {
            paintShader = shader
            paint.shader = shader
        }
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
    private class Group(val node: RenderNode) {
        init {
            node.setClipToBounds(false)
            // Until a layer says otherwise (setLayer): an alpha below 1 goes offscreen.
            node.setHasOverlappingRendering(true)
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
        /**
         * The size last given to [node], read here rather than with `RenderNode.getWidth` and
         * `getHeight` (not compiled in the boot image: they'd run in the interpreter). -1
         * until set: a node from the pool keeps its last group's size.
         */
        var width = -1
        var height = -1
        /** The last frame it was recorded drawing a path new that frame that a mask could draw. */
        var newPathFrame = NEVER
        /**
         * Its recording drew such a path without its mask (D411): recorded again, though its
         * version is the same, the first time its record is read more than [NEW_PATH_FRAMES]
         * frames after it last drew a new path.
         */
        var unmasked = false

        /** Sets [node]'s bounds to [width] × [height] at the origin, unless they're those already. */
        fun setPosition(width: Int, height: Int) {
            if (width == this.width && height == this.height) return
            this.width = width
            this.height = height
            node.setPosition(0, 0, width, height)
        }

    }

    private val groups = HashMap<Long, Group>()
    /**
     * The groups recorded ahead ([recordAhead]) that no frame named yet, by key: a frame naming
     * one takes it into [groups]. Dropped all at once when [AHEAD_ITEMS] items wait (or
     * [AHEAD_GROUPS] groups): a kept recording draws its nested groups' nodes by reference, so a
     * nested group's node goes only with every recording that may draw it (a frame's recordings
     * draw no node of these: a frame takes into [groups] each node it draws).
     */
    private val ahead = HashMap<Long, Group>()
    /** The items recorded ahead, by their outermost group's key (pruned of the taken ones). */
    private val aheadItems = HashSet<Long>()
    /** Recording ahead ([recordAhead]): groups are [ahead]'s, not the frame's. */
    private var recordingAhead = false
    /** The group recorded ahead draws a node of the last frame's: not kept as its version. */
    private var drawsFrameGroup = false
    /**
     * Draws the kept groups' nodes (D380), at alpha 0: the render thread draws nothing of it,
     * but the nodes stay in its tree, their recordings with them.
     */
    private val keptNode = RenderNode("compose-rs kept").apply {
        setAlpha(0f)
        setClipToBounds(true)
        setPosition(0, 0, 0, 0)
    }
    /**
     * A 1 × 1 layer, cleared, that a frame dropping a path drawn as a path records and draws
     * (D510). HWUI draws a frame's changed layers and flushes them before drawing the frame,
     * and Skia's GPU cache takes in, at a flush, the paths whose contents died: those of the
     * recordings this frame's sync freed. So the masks it cached for them (a stroked path's,
     * rasterized on the CPU) go back to its scratch textures before this frame's masks take
     * theirs. Without a layer, the frame has one flush, which takes them in only at its end:
     * each frame's masks took new textures, and a frame's worth more stayed allocated (as in
     * Compose's apps without a layer to draw, D510). It draws nothing: transparent, copied
     * over the view's top left pixel.
     */
    private val flushNode = RenderNode("compose-rs flush").apply {
        setPosition(0, 0, 1, 1)
        setUseCompositingLayer(true, null)
    }
    /** A path drawn as a path was dropped: the frame draws [flushNode]. */
    private var flushLayers = false
    /** The nodes [keptNode] draws, the first [keptCount]. */
    private var keptNodes = arrayOfNulls<RenderNode>(8)
    private var keptCount = 0
    /** The last frame with a `KEPT` section. */
    private var keptFrame = 0
    /** That section's keys and groups (null: unknown), the first [keptEntryCount]. */
    private var keptEntryKeys = LongArray(16)
    private var keptEntryGroups = arrayOfNulls<Group>(16)
    private var keptEntryCount = 0
    /** Gone groups' nodes, for new groups (D195). */
    private val pool = RenderNodePool("compose-rs group", NODE_POOL_CAPACITY)
    private var frame = 0
    private var width = 1
    private var height = 1
    /** In a software draw: where each group's `LAYERS` entry is in the buffer. */
    private val softwareLayers = HashMap<Long, Int>()
    /** In a software draw: where each group's record is in the buffer (its content's start). */
    private val softwareGroups = HashMap<Long, Int>()
    private val clipBounds = Rect()
    /** An unknown operation was met: the rest can't be read. */
    private var unreadable = false
    /** The frame's canvas is hardware accelerated (groups are recorded into render nodes). */
    private var hardware = false

    fun replay(canvas: Canvas, buffer: ByteBuffer) {
        buffer.order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(0)
        unreadable = false
        // Read once a frame: `isHardwareAccelerated` isn't compiled in the boot image.
        hardware = canvas.isHardwareAccelerated
        if (!hardware) {
            // A software draw (a snapshot) replays everything, layers applied as operations,
            // and leaves the groups alone: the window's last frame still draws their nodes.
            softwareLayers.clear()
            softwareGroups.clear()
            replay(canvas, buffer, buffer.limit())
            return
        }
        frame++
        width = canvas.width.coerceAtLeast(1)
        height = canvas.height.coerceAtLeast(1)
        recordingGroup = null
        skipNewMasks = recent(newPathFrame)
        replay(canvas, buffer, buffer.limit())
        if (flushLayers) {
            flushLayers = false
            // Its recording changes, so its layer is drawn again, and flushed before the frame.
            val recording = flushNode.beginRecording(1, 1)
            try {
                recording.drawColor(0, PorterDuff.Mode.CLEAR)
            } finally {
                flushNode.endRecording()
            }
            canvas.drawRenderNode(flushNode)
        }
        if (keptFrame != frame && keptCount > 0) {
            // None kept: the frame doesn't draw keptNode, nor keptNode its old nodes.
            keptNode.discardDisplayList()
            keptNodes.fill(null)
            keptCount = 0
            keptEntryGroups.fill(null)
            keptEntryCount = 0
        }
        val iterator = groups.values.iterator()
        while (iterator.hasNext()) {
            val group = iterator.next()
            if (group.frame != frame) {
                // Gone: this frame has no record of it, so no recording drawn from now on draws
                // it (one that did changed, and was recorded again).
                iterator.remove()
                if (group.layered) resetLayer(group.node)
                pool.recycle(group.node)
            } else if (group.layered && group.layerFrame != frame) {
                // No longer a layer. (Properties set after drawing the node still apply: the
                // render thread reads them when the frame is done.)
                resetLayer(group.node)
                group.setPosition(width, height)
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
                TRANSLATE -> canvas.translate(buffer.float(), buffer.float())
                CLIP_RECT -> canvas.clipRect(buffer.readRect(rect))
                CLIP_ROUND_RECT -> {
                    buffer.readRect(rect)
                    clip.reset()
                    clip.addRoundRect(rect, buffer.readRadii(), Path.Direction.CW)
                    canvas.clipPath(clip)
                }
                CLIP_PATH -> {
                    val kept = paths.get(buffer.getInt())
                    val x = buffer.float()
                    val y = buffer.float()
                    if (kept == null) {
                        // An unknown path clips everything out rather than nothing.
                        canvas.clipRect(0f, 0f, 0f, 0f)
                    } else {
                        kept.drawnAsPath = true
                        if (x == 0f && y == 0f) {
                            canvas.clipPath(kept.path)
                        } else {
                            // Moved by a copy, not the canvas: translating back could leave a
                            // rounding error in the matrix.
                            kept.path.offset(x, y, clip)
                            canvas.clipPath(clip)
                        }
                    }
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
                    val x1 = buffer.float()
                    val y1 = buffer.float()
                    val x2 = buffer.float()
                    val y2 = buffer.float()
                    canvas.drawLine(x1, y1, x2, y2, buffer.readPaint())
                }
                DRAW_PARAGRAPH -> {
                    val handle = buffer.getInt()
                    text.draw(canvas, hardware, handle, buffer.float(), buffer.float())
                }
                SAVE_LAYER_ALPHA -> {
                    val alpha = (buffer.float().coerceIn(0f, 1f) * 255f + 0.5f).toInt()
                    canvas.saveLayerAlpha(null, alpha)
                }
                TRANSFORM -> {
                    val pivotX = buffer.float()
                    val pivotY = buffer.float()
                    val scaleX = buffer.float()
                    val scaleY = buffer.float()
                    val rotation = buffer.float()
                    val translateX = buffer.float()
                    val translateY = buffer.float()
                    canvas.translate(pivotX + translateX, pivotY + translateY)
                    canvas.rotate(rotation)
                    canvas.scale(scaleX, scaleY)
                    canvas.translate(-pivotX, -pivotY)
                }
                DRAW_IMAGE -> {
                    val image = images.get(buffer.getInt())
                    val src = buffer.readRect(RectF())
                    buffer.readRect(rect)
                    bitmapPaint.alpha = (buffer.float().coerceIn(0f, 1f) * 255f + 0.5f).toInt()
                    if (image != null) {
                        source.set(src.left.toInt(), src.top.toInt(), src.right.toInt(), src.bottom.toInt())
                        canvas.drawBitmap(image, source, rect, bitmapPaint)
                    }
                }
                DRAW_SHADOW -> drawShadow(canvas, buffer)
                DRAW_ARC -> {
                    buffer.readRect(rect)
                    val startAngle = buffer.float()
                    val sweepAngle = buffer.float()
                    val useCenter = buffer.get().toInt() != 0
                    canvas.drawArc(rect, startAngle, sweepAngle, useCenter, buffer.readPaint())
                }
                DRAW_PATH -> {
                    val id = buffer.getInt()
                    val kept = paths.get(id)
                    val x = buffer.float()
                    val y = buffer.float()
                    val scaleX = buffer.float()
                    val scaleY = buffer.float()
                    val paint = buffer.readPaint(x, y, scaleX, scaleY)
                    // A mask is tinted by a color: a gradient draws the path.
                    val masked = !paintStroke && paintShader == null && maskNew(kept, scaleX, scaleY)
                    if (kept != null && (!masked || !drawMask(canvas, kept, x, y, scaleX, scaleY))) {
                        kept.drawnAsPath = true
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
                    group(buffer, key, version, from + length)
                    buffer.position(from + length)
                    // A record cut short (not kept as its version) leaves the rest readable.
                    unreadable = false
                }
                GROUP_REF -> drawGroup(canvas, buffer, buffer.getLong())
                KEPT -> kept(canvas, buffer)
                LAYERS -> layers(hardware, buffer)
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
     * The record of group [key] at [version], its content the bytes up to [end]: its node is
     * recorded from them, unless it holds that version's recording already (whole: the system
     * drops recordings when the window hides). Drawn where a `GROUP_REF` names it. In a
     * software draw, where it is, for [drawGroup].
     */
    private fun group(buffer: ByteBuffer, key: Long, version: Long, end: Int) {
        if (!hardware) {
            softwareGroups[key] = buffer.position()
            return
        }
        // Ahead, a group of the last frame's is left to the frames.
        if (recordingAhead && groups.containsKey(key)) return
        val group = groupOf(key)
        group.frame = frame
        drawsFrameGroup = false
        // A group recorded without a new path's mask is recorded again once its paths stopped
        // changing (D411): not on the next frame, which a path changing every other frame would
        // make a recording and a mask each time.
        val keep = !group.unmasked || recent(group.newPathFrame)
        if (group.version == version && group.node.hasDisplayList() && keep) return
        // Matches nothing until recorded whole.
        group.version = -1L
        // A layer's node has its bounds already (setLayer).
        if (!group.layered) group.setPosition(width, height)
        // Its size is a hint: what's recorded past it is kept, as a view's children drawn past
        // its bounds are.
        val recording =
            group.node.beginRecording(group.width.coerceAtLeast(1), group.height.coerceAtLeast(1))
        val outerGroup = recordingGroup
        val outerSkip = skipNewMasks
        recordingGroup = group
        skipNewMasks = recent(group.newPathFrame)
        group.unmasked = false
        try {
            replay(recording, buffer, end)
        } finally {
            group.node.endRecording()
            recordingGroup = outerGroup
            skipNewMasks = outerSkip
        }
        // A recording cut short by an unknown operation isn't that version's (nor one ahead
        // drawing a frame's node, which the frames may drop).
        if (!unreadable && !drawsFrameGroup) group.version = version
    }

    /**
     * The `KEPT` section: the groups composed and not placed (D380), outer before inner, each
     * with whether it is outermost. Their nodes are kept as recorded, layers included, though
     * the frame neither draws them nor (unless changed) holds their records; the outermost are
     * drawn by [keptNode] (recorded again when they change, or when its recording was dropped),
     * which the frame's canvas draws. Skipped in a software draw.
     */
    private fun kept(canvas: Canvas, buffer: ByteBuffer) {
        val count = buffer.getInt()
        if (!hardware) {
            buffer.position(buffer.position() + count * KEPT_ENTRY_BYTES)
            return
        }
        // The last frame's entries are this one's, by index, while they are the same keys: a
        // group named by every frame since stayed in [groups].
        val known = if (keptFrame == frame - 1) keptEntryCount else 0
        keptFrame = frame
        if (keptNodes.size < count) keptNodes = keptNodes.copyOf(count)
        if (keptEntryKeys.size < count) {
            keptEntryKeys = keptEntryKeys.copyOf(count)
            keptEntryGroups = keptEntryGroups.copyOf(count)
        }
        var outermost = 0
        var changed = !keptNode.hasDisplayList()
        for (index in 0 until count) {
            val key = buffer.getLong()
            val outer = buffer.get().toInt() != 0
            val same = index < known && keptEntryKeys[index] == key
            val group = if (same && keptEntryGroups[index] != null) keptEntryGroups[index] else groups[key]
            keptEntryKeys[index] = key
            keptEntryGroups[index] = group
            // Unknown (its recording was lost, or never made): the frame placing it records it.
            if (group == null) continue
            group.frame = frame
            group.layerFrame = frame
            if (outer) {
                if (keptNodes[outermost] !== group.node) {
                    keptNodes[outermost] = group.node
                    changed = true
                }
                outermost++
            }
        }
        for (index in count until keptEntryCount) keptEntryGroups[index] = null
        keptEntryCount = count
        if (outermost != keptCount) changed = true
        for (index in outermost until keptCount) keptNodes[index] = null
        keptCount = outermost
        if (changed) {
            val recording = keptNode.beginRecording(1, 1)
            try {
                for (index in 0 until outermost) recording.drawRenderNode(keptNodes[index]!!)
            } finally {
                keptNode.endRecording()
            }
        }
        canvas.drawRenderNode(keptNode)
    }

    /**
     * Group [key]'s: a frame's, or, recording ahead, [ahead]'s (new if none). In a frame, one
     * recorded ahead is taken into [groups]: its node is the one recordings ahead drew.
     */
    private fun groupOf(key: Long): Group {
        groups[key]?.let { return it }
        if (recordingAhead) return ahead.getOrPut(key) { Group(pool.obtain()) }
        val group = ahead.remove(key) ?: Group(pool.obtain())
        groups[key] = group
        return group
    }

    /**
     * Records ahead, between frames, the groups in [buffer] (`ComposeRsNative.drawAhead`):
     * resource changes, then an item's `LAYERS` entries and `GROUP` records, as a frame lists
     * and records them (D350). The frame placing the item writes the same records, versions
     * unchanged, and finds them recorded. Without a hardware frame yet, only the resource changes
     * are taken. Returns whether groups were recorded.
     */
    fun recordAhead(buffer: ByteBuffer): Boolean {
        buffer.order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(0)
        unreadable = false
        val recordsGroups = frame > 0
        if (recordsGroups) {
            aheadItems.retainAll { ahead.containsKey(it) }
            if (aheadItems.size >= AHEAD_ITEMS || ahead.size >= AHEAD_GROUPS) dropAhead()
        }
        var recorded = false
        hardware = true
        recordingAhead = true
        try {
            while (buffer.position() < buffer.limit() && !unreadable) {
                when (buffer.get().toInt()) {
                    GROUP -> {
                        val key = buffer.getLong()
                        val version = buffer.getLong()
                        val length = buffer.getInt()
                        val from = buffer.position()
                        if (recordsGroups) {
                            // The first is the item's.
                            if (!recorded) aheadItems.add(key)
                            recorded = true
                            group(buffer, key, version, from + length)
                        }
                        buffer.position(from + length)
                        unreadable = false
                    }
                    LAYERS -> if (recordsGroups) layers(true, buffer) else skipLayers(buffer)
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
        } finally {
            recordingAhead = false
            unreadable = false
        }
        return recorded
    }

    private fun skipLayers(buffer: ByteBuffer) {
        buffer.position(buffer.position() + 4 + buffer.getInt(buffer.position()) * LAYER_ENTRY_BYTES)
    }

    /** Drops every group recorded ahead that no frame named. */
    private fun dropAhead() {
        for (group in ahead.values) {
            if (group.layered) resetLayer(group.node)
            pool.recycle(group.node)
        }
        ahead.clear()
        aheadItems.clear()
    }

    /**
     * Draws group [key] on [canvas]: its node (recorded from its record, which comes first), or
     * in a software draw its record's operations, under its layer's.
     */
    private fun drawGroup(canvas: Canvas, buffer: ByteBuffer, key: Long) {
        if (hardware) {
            // Made here if new: a parent's record comes before those of the groups in it, which
            // are recorded later in this frame (its node is drawn by reference).
            if (recordingAhead && groups.containsKey(key)) drawsFrameGroup = true
            canvas.drawRenderNode(groupOf(key).node)
            return
        }
        val from = softwareGroups[key] ?: return
        val position = buffer.position()
        // Its layer as the operations Rust left out: transform, clip, alpha layer.
        val save = canvas.save()
        softwareLayers[key]?.let { at -> applyLayer(canvas, buffer, at) }
        buffer.position(from)
        replay(canvas, buffer, from + buffer.getInt(from - 4))
        canvas.restoreToCount(save)
        buffer.position(position)
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
                // Ahead, a group of the last frame's is left to the frames.
                val group = if (recordingAhead && groups.containsKey(key)) null else groupOf(key)
                // Read in one copy, compared with what was set: a frame where nothing moves
                // sets nothing, and a change sets only the properties that changed.
                buffer.position(at + 8)
                buffer.get(entry)
                if (group != null && (!group.layered || !entry.contentEquals(group.applied))) {
                    setLayer(group, buffer, at + 8, if (group.layered) group.applied else null)
                    System.arraycopy(entry, 0, group.applied, 0, entry.size)
                    group.layered = true
                }
                if (group != null) group.layerFrame = frame
            } else {
                softwareLayers[key] = at + 8
            }
            at += LAYER_ENTRY_BYTES
        }
        buffer.position(at)
    }

    /** The `LAYERS` entry being read (after its key). */
    private val entry = ByteArray(LAYER_ENTRY_BYTES - 8)

    /** Whether [entry]'s bytes [from] until [to] differ from [old]'s (or there's no [old]). */
    private fun changed(old: ByteArray?, from: Int, to: Int): Boolean {
        if (old == null) return true
        for (index in from until to) {
            if (old[index] != entry[index]) return true
        }
        return false
    }

    /**
     * Sets the layer at [at] (a `LAYERS` entry after its key: alpha, pivot, scale, rotation,
     * translation, clip, overlapping, bounds) on [group]'s node: the properties whose bytes
     * differ from [old] (the entry last set), all of them without one. A node scales and
     * rotates around its pivot then translates, clips in its own coordinates and applies its
     * alpha last, as the operations Rust left out (`Transform`, `ClipRect`, `SaveLayerAlpha`).
     * Its position is the group's origin (a recording doesn't depend on the layer's offset,
     * which can change without the group's version) to the layer's far corner.
     */
    private fun setLayer(group: Group, buffer: ByteBuffer, at: Int, old: ByteArray?) {
        val node = group.node
        if (changed(old, 58, 66)) {
            group.setPosition(ceil(buffer.floatAt(at + 58)).toInt(), ceil(buffer.floatAt(at + 62)).toInt())
        }
        if (changed(old, 0, 4)) node.setAlpha(buffer.floatAt(at))
        if (changed(old, 4, 8)) node.setPivotX(buffer.floatAt(at + 4))
        if (changed(old, 8, 12)) node.setPivotY(buffer.floatAt(at + 8))
        if (changed(old, 12, 16)) node.setScaleX(buffer.floatAt(at + 12))
        if (changed(old, 16, 20)) node.setScaleY(buffer.floatAt(at + 16))
        if (changed(old, 20, 24)) node.setRotationZ(buffer.floatAt(at + 20))
        if (changed(old, 24, 28)) node.setTranslationX(buffer.floatAt(at + 24))
        if (changed(old, 28, 32)) node.setTranslationY(buffer.floatAt(at + 28))
        if (changed(old, 32, 49)) {
            if (buffer.get(at + 32).toInt() != 0) {
                // Whole pixels: a layer's bounds are its node's (whole) size at its (whole) offset.
                clipBounds.set(
                    floor(buffer.floatAt(at + 33)).toInt(),
                    floor(buffer.floatAt(at + 37)).toInt(),
                    ceil(buffer.floatAt(at + 41)).toInt(),
                    ceil(buffer.floatAt(at + 45)).toInt(),
                )
                node.setClipRect(clipBounds)
            } else {
                node.setClipRect(null)
            }
        }
        // An alpha below 1 draws the content offscreen then composites it, as Compose's layers
        // do (`CompositingStrategy.Auto`), unless it's one drawing: then each drawing's alpha
        // is multiplied instead, as D153's folded layers (the same pixels, no offscreen pass).
        if (changed(old, 49, 50)) node.setHasOverlappingRendering(buffer.get(at + 49).toInt() != 0)
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
        val alpha = buffer.floatAt(at)
        val pivotX = buffer.floatAt(at + 4)
        val pivotY = buffer.floatAt(at + 8)
        canvas.translate(pivotX + buffer.floatAt(at + 24), pivotY + buffer.floatAt(at + 28))
        canvas.rotate(buffer.floatAt(at + 20))
        canvas.scale(buffer.floatAt(at + 12), buffer.floatAt(at + 16))
        canvas.translate(-pivotX, -pivotY)
        if (buffer.get(at + 32).toInt() != 0) {
            canvas.clipRect(
                buffer.floatAt(at + 33),
                buffer.floatAt(at + 37),
                buffer.floatAt(at + 41),
                buffer.floatAt(at + 45),
            )
        }
        if (alpha < 1f) {
            // Bounded as the node's offscreen layer is.
            canvas.saveLayerAlpha(
                0f,
                0f,
                ceil(buffer.floatAt(at + 58)),
                ceil(buffer.floatAt(at + 62)),
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
        val elevation = buffer.float()
        buffer.float()
        buffer.float()
        buffer.float()
        buffer.float()
        val ambient = buffer.getInt()
        val spot = buffer.getInt()
        if (elevation <= 0f) return
        // The spot shadow: offset down, blurred by the elevation.
        val alpha = ((ambient ushr 24) + (spot ushr 24)).coerceAtMost(255)
        shadowPaint.color = (alpha shl 24) or (spot and 0xFFFFFF)
        val blur = blur(elevation)
        if (shadowPaint.maskFilter !== blur) shadowPaint.maskFilter = blur
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

    /**
     * The gradient whose definition starts at the [kind] byte just read, at the buffer's
     * position after it (left after it, before its alpha): kept by its bytes and its local
     * transform (a path's scale and offset, see [readPaint]), built if new. An unknown kind
     * makes the rest unreadable.
     */
    private fun ByteBuffer.readShader(kind: Int, x: Float, y: Float, scaleX: Float, scaleY: Float): Shader? {
        val start = position() - 1
        val geometry = when (kind) {
            SHADER_LINEAR -> 16
            SHADER_RADIAL -> 12
            SHADER_SWEEP -> 8
            else -> {
                unreadable = true
                return null
            }
        }
        val colorsAt = start + 2 + geometry
        val colorCount = getInt(colorsAt)
        val stopsAt = colorsAt + 4 + 4 * colorCount
        val stopCount = getInt(stopsAt)
        val end = stopsAt + 4 + 4 * stopCount
        position(end)
        // A NaN transform matches no key (NaN isn't equal to itself): built, never kept.
        val cacheable = !x.isNaN() && !y.isNaN() && !scaleX.isNaN() && !scaleY.isNaN()
        if (cacheable) {
            for (index in 0 until shaderCount) {
                if (shaderMatches(index, this, start, end, x, y, scaleX, scaleY)) return shaders[index]
            }
        }
        val colors = IntArray(colorCount) { getInt(colorsAt + 4 + it * 4) }
        val stops = if (stopCount == 0) null else FloatArray(stopCount) { floatAt(stopsAt + 4 + it * 4) }
        val tileMode = when (get(start + 1).toInt()) {
            1 -> Shader.TileMode.REPEAT
            2 -> Shader.TileMode.MIRROR
            // Transparent past the ends: API 31+, clamped below.
            3 -> if (Build.VERSION.SDK_INT >= 31) Shader.TileMode.DECAL else Shader.TileMode.CLAMP
            else -> Shader.TileMode.CLAMP
        }
        val shader = when (kind) {
            SHADER_LINEAR -> LinearGradient(
                floatAt(start + 2),
                floatAt(start + 6),
                floatAt(start + 10),
                floatAt(start + 14),
                colors,
                stops,
                tileMode,
            )
            SHADER_RADIAL ->
                RadialGradient(floatAt(start + 2), floatAt(start + 6), floatAt(start + 10), colors, stops, tileMode)
            else -> SweepGradient(floatAt(start + 2), floatAt(start + 6), colors, stops)
        }
        if ((x != 0f || y != 0f || scaleX != 1f || scaleY != 1f) && scaleX != 0f && scaleY != 0f) {
            val local = Matrix()
            local.setScale(1f / scaleX, 1f / scaleY)
            local.preTranslate(-x, -y)
            shader.setLocalMatrix(local)
        }
        if (!cacheable) return shader
        val at = if (shaderCount < SHADER_CACHE_CAPACITY) shaderCount++ else shaderNext.also {
            shaderNext = (shaderNext + 1) % SHADER_CACHE_CAPACITY
        }
        shaderKeys[at] = ByteArray(end - start) { get(start + it) }
        shaderLocals[at * 4] = x
        shaderLocals[at * 4 + 1] = y
        shaderLocals[at * 4 + 2] = scaleX
        shaderLocals[at * 4 + 3] = scaleY
        shaders[at] = shader
        return shader
    }

    /** Whether kept shader [index] is the definition from [start] to [end] with that transform. */
    private fun shaderMatches(
        index: Int,
        buffer: ByteBuffer,
        start: Int,
        end: Int,
        x: Float,
        y: Float,
        scaleX: Float,
        scaleY: Float,
    ): Boolean {
        val key = shaderKeys[index]!!
        if (key.size != end - start) return false
        val local = index * 4
        if (shaderLocals[local] != x || shaderLocals[local + 1] != y) return false
        if (shaderLocals[local + 2] != scaleX || shaderLocals[local + 3] != scaleY) return false
        for (offset in key.indices) {
            if (key[offset] != buffer.get(start + offset)) return false
        }
        return true
    }

    /** The blur filter for [elevation], made once while it's among the last few used. */
    private fun blur(elevation: Float): BlurMaskFilter {
        for (index in 0 until blurCount) {
            if (blurElevations[index] == elevation) return blurFilters[index]!!
        }
        val filter = BlurMaskFilter(elevation, BlurMaskFilter.Blur.NORMAL)
        val at = if (blurCount < BLUR_CACHE_CAPACITY) blurCount++ else blurNext.also {
            blurNext = (blurNext + 1) % BLUR_CACHE_CAPACITY
        }
        blurElevations[at] = elevation
        blurFilters[at] = filter
        return filter
    }

    /**
     * A path, its bounds (its control points', in its units) and its masks (D190). Reused once
     * dropped ([rewind], D410).
     */
    private class KeptPath {
        val path = Path()
        private val computedBounds = RectF()
        private var boundsComputed = false
        /** The frame that defined it (D411). */
        var definedFrame = 0
        /**
         * A recording drew it as a path (`drawPath`, `clipPath`), so HWUI's GPU cache may hold
         * a mask of its contents: dropping it flushes the layers first ([flushNode], D510).
         */
        var drawnAsPath = false

        /** Computed on first use: a stroked path (a text field's outline) never needs them. */
        val bounds: RectF
            get() {
                if (!boundsComputed) {
                    // `exact` is ignored (and deprecated for that) on API 31+: the control points'.
                    @Suppress("DEPRECATION")
                    path.computeBounds(computedBounds, true)
                    boundsComputed = true
                }
                return computedBounds
            }

        /**
         * Empties it for another definition. `rewind` lets go of the native contents now: a
         * recording drawing them keeps its own reference, and once the recordings go, HWUI's
         * GPU cache drops their mask (keyed by the contents' generation). A dropped `Path` left
         * to the collector kept them, and their cache entry, until it was finalized.
         */
        fun rewind() {
            path.rewind()
            boundsComputed = false
            drawnAsPath = false
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
        if (!fitsMask(kept, scaleX, scaleY)) return null
        // A pixel of margin each side, for anti-aliasing.
        val left = floor(minOf(x1, x2)) - 1f
        val top = floor(minOf(y1, y2)) - 1f
        val width = maskSpan(x1, x2)
        val height = maskSpan(y1, y2)
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
        // The least recently recorded masks go past the budget, recycled (D411): a kept group's
        // recording that draws one keeps its own reference to its pixels.
        val iterator = masks.keys.iterator()
        while (maskBytes > MAX_MASK_BYTES && iterator.hasNext()) {
            val oldest = iterator.next()
            if (oldest === mask) break
            iterator.remove()
            oldest.path.masks.remove(oldest)
            maskBytes -= oldest.bitmap.allocationByteCount
            oldest.bitmap.recycle()
        }
        return mask
    }

    /**
     * Forgets [mask], taken from its path's, and recycles it (D411): a recording that drew it
     * keeps its own reference to its pixels.
     */
    private fun forget(mask: Mask) {
        masks.remove(mask)
        maskBytes -= mask.bitmap.allocationByteCount
        mask.bitmap.recycle()
    }

    /** The pixels a mask spans from [a] to [b] (scaled), with a pixel of margin each side. */
    private fun maskSpan(a: Float, b: Float): Int =
        (ceil(maxOf(a, b)) + 1f - (floor(minOf(a, b)) - 1f)).toInt()

    /** Whether [kept] scaled by ([scaleX], [scaleY]) is small enough for a mask ([MAX_MASK_SIZE]). */
    private fun fitsMask(kept: KeptPath, scaleX: Float, scaleY: Float): Boolean {
        val bounds = kept.bounds
        if (bounds.isEmpty) return false
        val width = maskSpan(bounds.left * scaleX, bounds.right * scaleX)
        val height = maskSpan(bounds.top * scaleY, bounds.bottom * scaleY)
        return width in 1..MAX_MASK_SIZE && height in 1..MAX_MASK_SIZE
    }

    /** Whether [at] (a frame a recording drew a new path in) is one of the last few (D411). */
    private fun recent(at: Int): Boolean = at != NEVER && frame - at in 0..NEW_PATH_FRAMES

    /**
     * Whether a mask may draw [kept] here (D411): unless it was defined this frame and the
     * recording drawing it drew new paths a mask could draw in the last frames as well. Those
     * change every frame (an animated drawing defines new ones each frame): a mask for each
     * would be rasterized, uploaded and dropped a frame later. A group recorded so is recorded
     * again the first time its record is read after its paths stop changing, so it doesn't
     * keep `drawPath` (until then, it does: say, a kept group, or no frame after it).
     */
    private fun maskNew(kept: KeptPath?, scaleX: Float, scaleY: Float): Boolean {
        if (kept == null || !hardware || kept.definedFrame != frame) return true
        // Too large for a mask: drawn as a path whatever it does.
        if (!fitsMask(kept, scaleX, scaleY)) return true
        val group = recordingGroup
        if (group != null) group.newPathFrame = frame else newPathFrame = frame
        if (!skipNewMasks) return true
        group?.unmasked = true
        return false
    }

    /**
     * Drops path [id] (D410, D411): its masks are recycled, and it's rewound for the next definition,
     * or reset (its memory freed) when the pool is full. A recording that drew them (the last
     * frame's, which the render thread may still be drawing, or a kept node's) keeps its own
     * reference to their native contents, so recycling and rewinding change nothing drawn.
     */
    private fun dropPath(id: Int) {
        val kept = paths.get(id) ?: return
        paths.remove(id)
        if (kept.drawnAsPath) flushLayers = true
        for (mask in kept.masks) forget(mask)
        kept.masks.clear()
        if (pathPool.size < PATH_POOL_CAPACITY) {
            kept.rewind()
            pathPool.add(kept)
        } else {
            kept.path.reset()
        }
    }

    private fun definePath(buffer: ByteBuffer) {
        val id = buffer.getInt()
        // An id is defined once, but a stale mask must never outlive its path.
        dropPath(id)
        val kept = if (pathPool.isEmpty()) KeptPath() else pathPool.removeAt(pathPool.size - 1)
        kept.definedFrame = frame
        val path = kept.path
        path.fillType = if (buffer.get().toInt() == 1) Path.FillType.EVEN_ODD else Path.FillType.WINDING
        repeat(buffer.getInt()) {
            when (buffer.get().toInt()) {
                0 -> path.moveTo(buffer.float(), buffer.float())
                1 -> path.lineTo(buffer.float(), buffer.float())
                2 -> path.quadTo(buffer.float(), buffer.float(), buffer.float(), buffer.float())
                3 -> path.cubicTo(
                    buffer.float(),
                    buffer.float(),
                    buffer.float(),
                    buffer.float(),
                    buffer.float(),
                    buffer.float(),
                )
                else -> path.close()
            }
        }
        paths.put(id, kept)
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
        keptNode.discardDisplayList()
        flushNode.discardDisplayList()
        flushLayers = false
        keptNodes.fill(null)
        keptCount = 0
        keptEntryGroups.fill(null)
        keptEntryCount = 0
        for (group in groups.values) group.node.discardDisplayList()
        groups.clear()
        for (group in ahead.values) group.node.discardDisplayList()
        ahead.clear()
        aheadItems.clear()
        pool.clear()
        // Reset, not rewound: their memory goes now, not when they're collected.
        for (index in 0 until paths.size()) paths.valueAt(index).path.reset()
        paths.clear()
        for (kept in pathPool) kept.path.reset()
        pathPool.clear()
        masks.clear()
        maskBytes = 0
        for (index in 0 until images.size()) images.valueAt(index).recycle()
        images.clear()
        shaderKeys.fill(null)
        shaders.fill(null)
        shaderCount = 0
        shaderNext = 0
        paintShader = null
        paint.shader = null
    }

    private companion object {
        /** About ten list items' groups: what a fling drops before drawing new ones. */
        const val NODE_POOL_CAPACITY = 32
        /**
         * The most items recorded ahead waiting for a frame, as Rust keeps their records
         * (`AHEAD_CAPACITY`): a list's two prefetched items, with room for items scrolled away
         * unseen.
         */
        const val AHEAD_ITEMS = 8
        /** The most groups recorded ahead waiting for a frame, however few items hold them. */
        const val AHEAD_GROUPS = 128
        /** Elevations whose blur filters are kept: Material's levels, and an animated one. */
        const val BLUR_CACHE_CAPACITY = 8
        /** Gradients kept: a screen's few, and an animated one. */
        const val SHADER_CACHE_CAPACITY = 16

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
        const val CLIP_PATH = 17
        const val GROUP_REF = 18
        const val KEPT = 19
        /** The bytes of a `KEPT` entry: its key, whether it is outermost. */
        const val KEPT_ENTRY_BYTES = 8 + 1
        const val DEFINE_PATH = 20
        const val DROP_PATH = 21
        const val DEFINE_IMAGE = 22
        const val DROP_IMAGE = 23

        const val SHADER_NONE = 0
        const val SHADER_LINEAR = 1
        const val SHADER_RADIAL = 2
        const val SHADER_SWEEP = 3

        /** The largest path (scaled, in pixels each way) drawn from a mask (D190). */
        const val MAX_MASK_SIZE = 256
        /** The masks' memory budget: 4 MB, 64 masks of 256 by 256 pixels, ~500 icons of 24 dp at 3.5x. */
        const val MAX_MASK_BYTES = 4 shl 20
        /** The masks a path keeps, one per drawn size (D190). */
        const val MAX_MASKS_PER_PATH = 4
        /** Dropped paths kept for new definitions: a canvas's paths replaced each frame (D410). */
        const val PATH_POOL_CAPACITY = 64
        /**
         * A recording that drew paths new in one of the last this many frames draws this
         * frame's new paths without masks (D411): they change every frame.
         */
        const val NEW_PATH_FRAMES = 2
        /** A frame long past: no new path drawn yet. */
        const val NEVER = Int.MIN_VALUE / 2

        /**
         * The bytes of a `LAYERS` entry: its key, 8 floats, whether it clips, its clip, whether
         * it overlaps, its bounds.
         */
        const val LAYER_ENTRY_BYTES = 8 + 8 * 4 + 1 + 4 * 4 + 1 + 4 * 4
    }
}
