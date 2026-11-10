package rs.compose.android

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.ColorSpace
import java.nio.ByteBuffer

/**
 * The platform's image decoder for compose-rs (`compose_ui::decode_image_bytes`,
 * `image_resource`): `BitmapFactory`, to a software ARGB_8888 bitmap whose premultiplied pixels
 * Rust copies (`crates/compose-android/src/image.rs`), then recycles.
 */
object ComposeRsImages {
    /** The image [bytes] encode, as a premultiplied ARGB_8888 bitmap, or null. Any thread. */
    @JvmStatic
    fun decode(bytes: ByteArray): Bitmap? {
        val options = BitmapFactory.Options().apply {
            inPreferredConfig = Bitmap.Config.ARGB_8888
            inPremultiplied = true
            // Colors as compose-rs draws them (sRGB), whatever profile the image carries.
            inPreferredColorSpace = ColorSpace.get(ColorSpace.Named.SRGB)
        }
        val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options) ?: return null
        if (bitmap.config == Bitmap.Config.ARGB_8888) return bitmap
        // A grayscale, 565 or wide-gamut image: converted to what Rust reads.
        val converted = bitmap.copy(Bitmap.Config.ARGB_8888, false)
        bitmap.recycle()
        return converted
    }

    /**
     * The platform's bitmap rasterizer (`compose_ui::platform::BitmapRasterizer`,
     * `crates/compose-android/src/image.rs`): draws [drawing] (a display list in the replayer's
     * format, its paths and images defined first) over [pixels] ([width] × [height], encoded per
     * [config], [hasAlpha], [colorSpaceId], Rust's), in place, with a software `Canvas` over a
     * bitmap, as Compose's `Canvas(imageBitmap)`. Any thread; one drawing at a time.
     */
    @JvmStatic
    @Synchronized
    fun rasterize(
        pixels: ByteBuffer,
        width: Int,
        height: Int,
        config: Int,
        hasAlpha: Boolean,
        colorSpaceId: Int,
        drawing: ByteBuffer,
    ) {
        val bitmap = createPlatformBitmap(width, height, config, hasAlpha, colorSpaceId)
        try {
            bitmap.copyPixelsFromBuffer(pixels)
            // A replayer of its own: the paths and images it defines are freed after.
            val replayer = Replayer(TextMeasurer(), null)
            try {
                replayer.replay(Canvas(bitmap), drawing)
            } finally {
                replayer.clear()
            }
            pixels.position(0)
            bitmap.copyPixelsToBuffer(pixels)
        } finally {
            bitmap.recycle()
        }
    }

    internal fun createPlatformBitmap(
        width: Int,
        height: Int,
        config: Int,
        hasAlpha: Boolean,
        colorSpaceId: Int,
    ): Bitmap {
        val w = width.coerceAtLeast(1)
        val h = height.coerceAtLeast(1)
        return when (config) {
            1 -> Bitmap.createBitmap(w, h, Bitmap.Config.ALPHA_8)
            2 -> Bitmap.createBitmap(w, h, Bitmap.Config.RGB_565)
            3 -> Bitmap.createBitmap(
                null,
                w,
                h,
                Bitmap.Config.RGBA_F16,
                hasAlpha,
                colorSpaceOf(colorSpaceId),
            )
            else -> Bitmap.createBitmap(
                null,
                w,
                h,
                Bitmap.Config.ARGB_8888,
                hasAlpha,
                colorSpaceOf(colorSpaceId),
            )
        }
    }

    private fun colorSpaceOf(id: Int): ColorSpace = ColorSpace.get(
        when (id) {
            1 -> ColorSpace.Named.LINEAR_SRGB
            2 -> ColorSpace.Named.EXTENDED_SRGB
            3 -> ColorSpace.Named.LINEAR_EXTENDED_SRGB
            4 -> ColorSpace.Named.BT709
            5 -> ColorSpace.Named.BT2020
            6 -> ColorSpace.Named.DCI_P3
            7 -> ColorSpace.Named.DISPLAY_P3
            8 -> ColorSpace.Named.NTSC_1953
            9 -> ColorSpace.Named.SMPTE_C
            10 -> ColorSpace.Named.ADOBE_RGB
            11 -> ColorSpace.Named.PRO_PHOTO_RGB
            12 -> ColorSpace.Named.ACES
            13 -> ColorSpace.Named.ACESCG
            else -> ColorSpace.Named.SRGB
        }
    )
}
