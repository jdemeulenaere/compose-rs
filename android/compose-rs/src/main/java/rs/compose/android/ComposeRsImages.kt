package rs.compose.android

import android.graphics.Bitmap
import android.graphics.BitmapFactory

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
        }
        val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options) ?: return null
        if (bitmap.config == Bitmap.Config.ARGB_8888) return bitmap
        // A grayscale, 565 or wide-gamut image: converted to what Rust reads.
        val converted = bitmap.copy(Bitmap.Config.ARGB_8888, false)
        bitmap.recycle()
        return converted
    }
}
