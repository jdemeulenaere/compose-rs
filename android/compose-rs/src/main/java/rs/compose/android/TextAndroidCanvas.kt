@file:Suppress("DEPRECATION", "OVERRIDE_DEPRECATION")

package rs.compose.android

import android.annotation.TargetApi
import android.graphics.BlendMode
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.DrawFilter
import android.graphics.Matrix
import android.graphics.Matrix44
import android.graphics.Mesh
import android.graphics.NinePatch
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Picture
import android.graphics.PorterDuff
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Region
import android.graphics.RenderNode
import android.graphics.Shader
import android.graphics.fonts.Font
import android.graphics.text.MeasuredText

/**
 * The canvas a paragraph's layout draws on, as Compose's `TextAndroidCanvas` (ui-text): every
 * call goes to [nativeCanvas], but its clip bounds reach down without end. `Layout.draw` draws
 * only the lines within the canvas's clip bounds, and a render node's recording canvas reports
 * the node's size when it was recorded: a group recorded while its layer was smaller than the
 * text (a card growing with `animate_content_size`), whose record stays the same bytes as it
 * grows (D323), kept the lines that fit then (D646). Its own canvas state is never used.
 */
internal class TextAndroidCanvas : Canvas() {
    private lateinit var nativeCanvas: Canvas

    /** Draws on [canvas] from now on. */
    fun setCanvas(canvas: Canvas) {
        nativeCanvas = canvas
    }

    /** [nativeCanvas]'s clip bounds from the top, as tall as can be, as Compose's. */
    override fun getClipBounds(bounds: Rect): Boolean {
        val result = nativeCanvas.getClipBounds(bounds)
        if (result) bounds.set(0, 0, bounds.width(), Int.MAX_VALUE)
        return result
    }

    // `Canvas()` asks before any canvas is set: its own canvas is then a raster one, never used.
    override fun isHardwareAccelerated(): Boolean = ::nativeCanvas.isInitialized && nativeCanvas.isHardwareAccelerated

    override fun setBitmap(bitmap: Bitmap?) = nativeCanvas.setBitmap(bitmap)

    override fun enableZ() = nativeCanvas.enableZ()

    override fun disableZ() = nativeCanvas.disableZ()

    override fun isOpaque(): Boolean = nativeCanvas.isOpaque

    override fun getWidth(): Int = nativeCanvas.width

    override fun getHeight(): Int = nativeCanvas.height

    override fun getDensity(): Int = nativeCanvas.density

    override fun setDensity(density: Int) {
        nativeCanvas.density = density
    }

    override fun getMaximumBitmapWidth(): Int = nativeCanvas.maximumBitmapWidth

    override fun getMaximumBitmapHeight(): Int = nativeCanvas.maximumBitmapHeight

    override fun save(): Int = nativeCanvas.save()

    override fun saveLayer(bounds: RectF?, paint: Paint?, saveFlags: Int): Int =
        nativeCanvas.saveLayer(bounds, paint, saveFlags)

    override fun saveLayer(bounds: RectF?, paint: Paint?): Int = nativeCanvas.saveLayer(bounds, paint)

    override fun saveLayer(left: Float, top: Float, right: Float, bottom: Float, paint: Paint?, saveFlags: Int): Int =
        nativeCanvas.saveLayer(left, top, right, bottom, paint, saveFlags)

    override fun saveLayer(left: Float, top: Float, right: Float, bottom: Float, paint: Paint?): Int =
        nativeCanvas.saveLayer(left, top, right, bottom, paint)

    override fun saveLayerAlpha(bounds: RectF?, alpha: Int, saveFlags: Int): Int =
        nativeCanvas.saveLayerAlpha(bounds, alpha, saveFlags)

    override fun saveLayerAlpha(bounds: RectF?, alpha: Int): Int = nativeCanvas.saveLayerAlpha(bounds, alpha)

    override fun saveLayerAlpha(left: Float, top: Float, right: Float, bottom: Float, alpha: Int, saveFlags: Int): Int =
        nativeCanvas.saveLayerAlpha(left, top, right, bottom, alpha, saveFlags)

    override fun saveLayerAlpha(left: Float, top: Float, right: Float, bottom: Float, alpha: Int): Int =
        nativeCanvas.saveLayerAlpha(left, top, right, bottom, alpha)

    override fun restore() = nativeCanvas.restore()

    override fun getSaveCount(): Int = nativeCanvas.saveCount

    override fun restoreToCount(saveCount: Int) = nativeCanvas.restoreToCount(saveCount)

    override fun translate(dx: Float, dy: Float) = nativeCanvas.translate(dx, dy)

    override fun scale(sx: Float, sy: Float) = nativeCanvas.scale(sx, sy)

    override fun rotate(degrees: Float) = nativeCanvas.rotate(degrees)

    override fun skew(sx: Float, sy: Float) = nativeCanvas.skew(sx, sy)

    override fun concat(matrix: Matrix?) = nativeCanvas.concat(matrix)

    @TargetApi(33)
    override fun concat(m: Matrix44?) = nativeCanvas.concat(m)

    override fun setMatrix(matrix: Matrix?) = nativeCanvas.setMatrix(matrix)

    override fun getMatrix(ctm: Matrix) = nativeCanvas.getMatrix(ctm)

    override fun clipRect(rect: RectF, op: Region.Op): Boolean = nativeCanvas.clipRect(rect, op)

    override fun clipRect(rect: Rect, op: Region.Op): Boolean = nativeCanvas.clipRect(rect, op)

    override fun clipRect(rect: RectF): Boolean = nativeCanvas.clipRect(rect)

    override fun clipRect(rect: Rect): Boolean = nativeCanvas.clipRect(rect)

    override fun clipRect(left: Float, top: Float, right: Float, bottom: Float, op: Region.Op): Boolean =
        nativeCanvas.clipRect(left, top, right, bottom, op)

    override fun clipRect(left: Float, top: Float, right: Float, bottom: Float): Boolean =
        nativeCanvas.clipRect(left, top, right, bottom)

    override fun clipRect(left: Int, top: Int, right: Int, bottom: Int): Boolean =
        nativeCanvas.clipRect(left, top, right, bottom)

    override fun clipOutRect(rect: RectF): Boolean = nativeCanvas.clipOutRect(rect)

    override fun clipOutRect(rect: Rect): Boolean = nativeCanvas.clipOutRect(rect)

    override fun clipOutRect(left: Float, top: Float, right: Float, bottom: Float): Boolean =
        nativeCanvas.clipOutRect(left, top, right, bottom)

    override fun clipOutRect(left: Int, top: Int, right: Int, bottom: Int): Boolean =
        nativeCanvas.clipOutRect(left, top, right, bottom)

    override fun clipPath(path: Path, op: Region.Op): Boolean = nativeCanvas.clipPath(path, op)

    override fun clipPath(path: Path): Boolean = nativeCanvas.clipPath(path)

    override fun clipOutPath(path: Path): Boolean = nativeCanvas.clipOutPath(path)

    @TargetApi(33)
    override fun clipShader(shader: Shader) = nativeCanvas.clipShader(shader)

    @TargetApi(33)
    override fun clipOutShader(shader: Shader) = nativeCanvas.clipOutShader(shader)

    override fun getDrawFilter(): DrawFilter? = nativeCanvas.drawFilter

    override fun setDrawFilter(filter: DrawFilter?) {
        nativeCanvas.drawFilter = filter
    }

    @TargetApi(30)
    override fun quickReject(rect: RectF): Boolean = nativeCanvas.quickReject(rect)

    override fun quickReject(rect: RectF, type: EdgeType): Boolean = nativeCanvas.quickReject(rect, type)

    @TargetApi(30)
    override fun quickReject(path: Path): Boolean = nativeCanvas.quickReject(path)

    override fun quickReject(path: Path, type: EdgeType): Boolean = nativeCanvas.quickReject(path, type)

    @TargetApi(30)
    override fun quickReject(left: Float, top: Float, right: Float, bottom: Float): Boolean =
        nativeCanvas.quickReject(left, top, right, bottom)

    override fun quickReject(left: Float, top: Float, right: Float, bottom: Float, type: EdgeType): Boolean =
        nativeCanvas.quickReject(left, top, right, bottom, type)

    override fun drawPicture(picture: Picture) = nativeCanvas.drawPicture(picture)

    override fun drawPicture(picture: Picture, dst: RectF) = nativeCanvas.drawPicture(picture, dst)

    override fun drawPicture(picture: Picture, dst: Rect) = nativeCanvas.drawPicture(picture, dst)

    override fun drawArc(
        oval: RectF,
        startAngle: Float,
        sweepAngle: Float,
        useCenter: Boolean,
        paint: Paint,
    ) = nativeCanvas.drawArc(oval, startAngle, sweepAngle, useCenter, paint)

    override fun drawArc(
        left: Float,
        top: Float,
        right: Float,
        bottom: Float,
        startAngle: Float,
        sweepAngle: Float,
        useCenter: Boolean,
        paint: Paint,
    ) = nativeCanvas.drawArc(left, top, right, bottom, startAngle, sweepAngle, useCenter, paint)

    override fun drawARGB(a: Int, r: Int, g: Int, b: Int) = nativeCanvas.drawARGB(a, r, g, b)

    override fun drawBitmap(bitmap: Bitmap, left: Float, top: Float, paint: Paint?) =
        nativeCanvas.drawBitmap(bitmap, left, top, paint)

    override fun drawBitmap(bitmap: Bitmap, src: Rect?, dst: RectF, paint: Paint?) =
        nativeCanvas.drawBitmap(bitmap, src, dst, paint)

    override fun drawBitmap(bitmap: Bitmap, src: Rect?, dst: Rect, paint: Paint?) =
        nativeCanvas.drawBitmap(bitmap, src, dst, paint)

    override fun drawBitmap(
        colors: IntArray,
        offset: Int,
        stride: Int,
        x: Float,
        y: Float,
        width: Int,
        height: Int,
        hasAlpha: Boolean,
        paint: Paint?,
    ) = nativeCanvas.drawBitmap(colors, offset, stride, x, y, width, height, hasAlpha, paint)

    override fun drawBitmap(
        colors: IntArray,
        offset: Int,
        stride: Int,
        x: Int,
        y: Int,
        width: Int,
        height: Int,
        hasAlpha: Boolean,
        paint: Paint?,
    ) = nativeCanvas.drawBitmap(colors, offset, stride, x, y, width, height, hasAlpha, paint)

    override fun drawBitmap(bitmap: Bitmap, matrix: Matrix, paint: Paint?) =
        nativeCanvas.drawBitmap(bitmap, matrix, paint)

    override fun drawBitmapMesh(
        bitmap: Bitmap,
        meshWidth: Int,
        meshHeight: Int,
        verts: FloatArray,
        vertOffset: Int,
        colors: IntArray?,
        colorOffset: Int,
        paint: Paint?,
    ) = nativeCanvas.drawBitmapMesh(bitmap, meshWidth, meshHeight, verts, vertOffset, colors, colorOffset, paint)

    override fun drawCircle(cx: Float, cy: Float, radius: Float, paint: Paint) =
        nativeCanvas.drawCircle(cx, cy, radius, paint)

    override fun drawColor(color: Int) = nativeCanvas.drawColor(color)

    override fun drawColor(color: Long) = nativeCanvas.drawColor(color)

    override fun drawColor(color: Int, mode: PorterDuff.Mode) = nativeCanvas.drawColor(color, mode)

    override fun drawColor(color: Int, mode: BlendMode) = nativeCanvas.drawColor(color, mode)

    override fun drawColor(color: Long, mode: BlendMode) = nativeCanvas.drawColor(color, mode)

    override fun drawLine(startX: Float, startY: Float, stopX: Float, stopY: Float, paint: Paint) =
        nativeCanvas.drawLine(startX, startY, stopX, stopY, paint)

    override fun drawLines(pts: FloatArray, offset: Int, count: Int, paint: Paint) =
        nativeCanvas.drawLines(pts, offset, count, paint)

    override fun drawLines(pts: FloatArray, paint: Paint) = nativeCanvas.drawLines(pts, paint)

    override fun drawOval(oval: RectF, paint: Paint) = nativeCanvas.drawOval(oval, paint)

    override fun drawOval(left: Float, top: Float, right: Float, bottom: Float, paint: Paint) =
        nativeCanvas.drawOval(left, top, right, bottom, paint)

    override fun drawPaint(paint: Paint) = nativeCanvas.drawPaint(paint)

    override fun drawPatch(patch: NinePatch, dst: Rect, paint: Paint?) = nativeCanvas.drawPatch(patch, dst, paint)

    override fun drawPatch(patch: NinePatch, dst: RectF, paint: Paint?) = nativeCanvas.drawPatch(patch, dst, paint)

    override fun drawPath(path: Path, paint: Paint) = nativeCanvas.drawPath(path, paint)

    override fun drawPoint(x: Float, y: Float, paint: Paint) = nativeCanvas.drawPoint(x, y, paint)

    override fun drawPoints(pts: FloatArray?, offset: Int, count: Int, paint: Paint) =
        nativeCanvas.drawPoints(pts, offset, count, paint)

    override fun drawPoints(pts: FloatArray, paint: Paint) = nativeCanvas.drawPoints(pts, paint)

    override fun drawPosText(text: CharArray, index: Int, count: Int, pos: FloatArray, paint: Paint) =
        nativeCanvas.drawPosText(text, index, count, pos, paint)

    override fun drawPosText(text: String, pos: FloatArray, paint: Paint) = nativeCanvas.drawPosText(text, pos, paint)

    override fun drawRect(rect: RectF, paint: Paint) = nativeCanvas.drawRect(rect, paint)

    override fun drawRect(r: Rect, paint: Paint) = nativeCanvas.drawRect(r, paint)

    override fun drawRect(left: Float, top: Float, right: Float, bottom: Float, paint: Paint) =
        nativeCanvas.drawRect(left, top, right, bottom, paint)

    override fun drawRegion(region: Region, paint: Paint) = nativeCanvas.drawRegion(region, paint)

    override fun drawRGB(r: Int, g: Int, b: Int) = nativeCanvas.drawRGB(r, g, b)

    override fun drawRoundRect(rect: RectF, rx: Float, ry: Float, paint: Paint) =
        nativeCanvas.drawRoundRect(rect, rx, ry, paint)

    override fun drawRoundRect(
        left: Float,
        top: Float,
        right: Float,
        bottom: Float,
        rx: Float,
        ry: Float,
        paint: Paint,
    ) = nativeCanvas.drawRoundRect(left, top, right, bottom, rx, ry, paint)

    override fun drawDoubleRoundRect(
        outer: RectF,
        outerRx: Float,
        outerRy: Float,
        inner: RectF,
        innerRx: Float,
        innerRy: Float,
        paint: Paint,
    ) = nativeCanvas.drawDoubleRoundRect(outer, outerRx, outerRy, inner, innerRx, innerRy, paint)

    override fun drawDoubleRoundRect(
        outer: RectF,
        outerRadii: FloatArray,
        inner: RectF,
        innerRadii: FloatArray,
        paint: Paint,
    ) = nativeCanvas.drawDoubleRoundRect(outer, outerRadii, inner, innerRadii, paint)

    @TargetApi(31)
    override fun drawGlyphs(
        glyphIds: IntArray,
        glyphIdOffset: Int,
        positions: FloatArray,
        positionOffset: Int,
        glyphCount: Int,
        font: Font,
        paint: Paint,
    ) = nativeCanvas.drawGlyphs(glyphIds, glyphIdOffset, positions, positionOffset, glyphCount, font, paint)

    override fun drawText(text: CharArray, index: Int, count: Int, x: Float, y: Float, paint: Paint) =
        nativeCanvas.drawText(text, index, count, x, y, paint)

    override fun drawText(text: String, x: Float, y: Float, paint: Paint) = nativeCanvas.drawText(text, x, y, paint)

    override fun drawText(text: String, start: Int, end: Int, x: Float, y: Float, paint: Paint) =
        nativeCanvas.drawText(text, start, end, x, y, paint)

    override fun drawText(text: CharSequence, start: Int, end: Int, x: Float, y: Float, paint: Paint) =
        nativeCanvas.drawText(text, start, end, x, y, paint)

    override fun drawTextRun(
        text: CharArray,
        index: Int,
        count: Int,
        contextIndex: Int,
        contextCount: Int,
        x: Float,
        y: Float,
        isRtl: Boolean,
        paint: Paint,
    ) = nativeCanvas.drawTextRun(text, index, count, contextIndex, contextCount, x, y, isRtl, paint)

    override fun drawTextRun(
        text: CharSequence,
        start: Int,
        end: Int,
        contextStart: Int,
        contextEnd: Int,
        x: Float,
        y: Float,
        isRtl: Boolean,
        paint: Paint,
    ) = nativeCanvas.drawTextRun(text, start, end, contextStart, contextEnd, x, y, isRtl, paint)

    override fun drawTextRun(
        text: MeasuredText,
        start: Int,
        end: Int,
        contextStart: Int,
        contextEnd: Int,
        x: Float,
        y: Float,
        isRtl: Boolean,
        paint: Paint,
    ) = nativeCanvas.drawTextRun(text, start, end, contextStart, contextEnd, x, y, isRtl, paint)

    override fun drawTextOnPath(
        text: CharArray,
        index: Int,
        count: Int,
        path: Path,
        hOffset: Float,
        vOffset: Float,
        paint: Paint,
    ) = nativeCanvas.drawTextOnPath(text, index, count, path, hOffset, vOffset, paint)

    override fun drawTextOnPath(text: String, path: Path, hOffset: Float, vOffset: Float, paint: Paint) =
        nativeCanvas.drawTextOnPath(text, path, hOffset, vOffset, paint)

    override fun drawVertices(
        mode: VertexMode,
        vertexCount: Int,
        verts: FloatArray,
        vertOffset: Int,
        texs: FloatArray?,
        texOffset: Int,
        colors: IntArray?,
        colorOffset: Int,
        indices: ShortArray?,
        indexOffset: Int,
        indexCount: Int,
        paint: Paint,
    ) = nativeCanvas.drawVertices(
        mode, vertexCount, verts, vertOffset, texs, texOffset, colors, colorOffset, indices, indexOffset, indexCount,
        paint,
    )

    @TargetApi(34)
    override fun drawMesh(mesh: Mesh, blendMode: BlendMode?, paint: Paint) =
        nativeCanvas.drawMesh(mesh, blendMode, paint)

    override fun drawRenderNode(renderNode: RenderNode) = nativeCanvas.drawRenderNode(renderNode)
}
