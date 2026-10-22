//! The platform's image decoder on Android (`compose_ui::set_image_decoder`): `BitmapFactory`,
//! through `ComposeRsImages.kt`. The bitmap's premultiplied RGBA pixels are copied into an
//! `ImageBitmap` (one copy, by `copyPixelsToBuffer` into a buffer Rust owns), which the display
//! list's image resources then send to the replayer as any other (D224).

use std::sync::Arc;

use compose_ui::ImageBitmap;
use jni::JNIEnv;
use jni::objects::{GlobalRef, JClass, JObject, JStaticMethodID, JValue};
use jni::signature::ReturnType;

/// `ComposeRsImages`' class and its `decode([B)Landroid/graphics/Bitmap;`.
struct AndroidImageDecoder {
    class: GlobalRef,
    decode: JStaticMethodID,
}

/// Sets the platform's decoder: called from `JNI_OnLoad`, whose class loader finds the app's
/// classes.
pub(crate) fn install(env: &mut JNIEnv) {
    let Ok(class) = env.find_class("rs/compose/android/ComposeRsImages") else { return };
    let Ok(decode) = env.get_static_method_id(&class, "decode", "([B)Landroid/graphics/Bitmap;")
    else {
        return;
    };
    let Ok(class) = env.new_global_ref(class) else { return };
    compose_ui::set_image_decoder(Arc::new(AndroidImageDecoder { class, decode }));
}

/// The largest image decoded, in pixels: a gigabyte of pixels, as the desktop decoder's.
const MAX_PIXELS: u64 = 1 << 28;

impl compose_ui::ImageDecoder for AndroidImageDecoder {
    fn decode(&self, bytes: &[u8]) -> Option<ImageBitmap> {
        // Any thread: attached (as a daemon, so it never blocks the VM's exit) if it isn't.
        let mut env = crate::bridge::vm().attach_current_thread_as_daemon().ok()?;
        let result = env.with_local_frame(8, |env| -> jni::errors::Result<Option<ImageBitmap>> {
            let array = env.byte_array_from_slice(bytes)?;
            // SAFETY: `decode` is `([B)Landroid/graphics/Bitmap;`, static, of `class`.
            let bitmap = unsafe {
                env.call_static_method_unchecked(
                    <&JClass>::from(self.class.as_obj()),
                    self.decode,
                    ReturnType::Object,
                    &[JValue::Object(&array).as_jni()],
                )
            }?
            .l()?;
            if bitmap.is_null() {
                return Ok(None);
            }
            Ok(copy_pixels(env, &bitmap))
        });
        crate::bridge::check(&mut env, "ComposeRsImages.decode");
        result.ok().flatten()
    }
}

/// The ARGB_8888 `bitmap`'s pixels (premultiplied, R, G, B, A in memory), then recycled.
fn copy_pixels(env: &mut JNIEnv, bitmap: &JObject) -> Option<ImageBitmap> {
    let width = env.call_method(bitmap, "getWidth", "()I", &[]).and_then(|value| value.i()).ok()?;
    let height =
        env.call_method(bitmap, "getHeight", "()I", &[]).and_then(|value| value.i()).ok()?;
    let image = (|| {
        if width <= 0 || height <= 0 || width as u64 * height as u64 > MAX_PIXELS {
            return None;
        }
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        // SAFETY: Kotlin writes the pixels during the call only; the vector outlives it.
        let buffer =
            unsafe { env.new_direct_byte_buffer(pixels.as_mut_ptr(), pixels.len()) }.ok()?;
        env.call_method(
            bitmap,
            "copyPixelsToBuffer",
            "(Ljava/nio/Buffer;)V",
            &[JValue::Object(&buffer)],
        )
        .ok()?;
        Some(ImageBitmap::from_rgba_premultiplied(width as u32, height as u32, pixels))
    })();
    // A failed copy's exception is logged and cleared before calling again.
    crate::bridge::check(env, "Bitmap.copyPixelsToBuffer");
    // Its pixels freed now rather than at a garbage collection.
    let _ = env.call_method(bitmap, "recycle", "()V", &[]);
    image
}
