//! The platform's image decoder on Android (`compose_ui::platform::set_image_decoder`): `BitmapFactory`,
//! through `ComposeRsImages.kt`. The bitmap's premultiplied RGBA pixels are copied into an
//! `ImageBitmap` (one copy, by `copyPixelsToBuffer` into a buffer Rust owns), which the display
//! list's image resources then send to the replayer as any other (D224).

use std::sync::Arc;

use compose_ui::ImageBitmap;
use jni::ids::JStaticMethodID;
use jni::objects::{JClass, JObject};
use jni::refs::Global;
use jni::signature::ReturnType;
use jni::{Env, JValue, jni_sig, jni_str};

/// `ComposeRsImages`' class and its `decode([B)Landroid/graphics/Bitmap;`.
struct AndroidImageDecoder {
    class: Global<JClass<'static>>,
    decode: JStaticMethodID,
}

/// Sets the platform's decoder: called from `JNI_OnLoad`, whose class loader finds the app's
/// classes.
pub(crate) fn install(env: &mut Env) {
    let Ok(class) = env.find_class(jni_str!("rs/compose/android/ComposeRsImages")) else { return };
    let Ok(decode) = env.get_static_method_id(
        &class,
        jni_str!("decode"),
        jni_sig!("([B)Landroid/graphics/Bitmap;"),
    ) else {
        return;
    };
    let Ok(class) = env.new_global_ref(class) else { return };
    compose_ui::platform::set_image_decoder(Arc::new(AndroidImageDecoder { class, decode }));
}

/// The largest image decoded, in pixels: a gigabyte of pixels, as the desktop decoder's.
const MAX_PIXELS: u64 = 1 << 28;

impl compose_ui::platform::ImageDecoder for AndroidImageDecoder {
    fn decode(&self, bytes: &[u8]) -> Option<ImageBitmap> {
        // Any thread: attached for good if it isn't (a pool's thread, detached when it ends);
        // in a local frame of its own.
        let result = crate::bridge::vm().attach_current_thread(
            |env| -> jni::errors::Result<Option<ImageBitmap>> {
                let decoded = (|| -> jni::errors::Result<Option<ImageBitmap>> {
                    let array = env.byte_array_from_slice(bytes)?;
                    // SAFETY: `decode` is `([B)Landroid/graphics/Bitmap;`, static, of `class`.
                    let bitmap = unsafe {
                        env.call_static_method_unchecked(
                            &self.class,
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
                })();
                crate::bridge::check(env, "ComposeRsImages.decode");
                decoded
            },
        );
        result.ok().flatten()
    }
}

/// The ARGB_8888 `bitmap`'s pixels (premultiplied, R, G, B, A in memory), then recycled.
fn copy_pixels(env: &mut Env, bitmap: &JObject) -> Option<ImageBitmap> {
    let mut int =
        |name| env.call_method(bitmap, name, jni_sig!("()I"), &[]).and_then(|value| value.i()).ok();
    let width = int(jni_str!("getWidth"))?;
    let height = int(jni_str!("getHeight"))?;
    let row_bytes = int(jni_str!("getRowBytes"))?;
    let image = (|| {
        if width <= 0 || height <= 0 || width as u64 * height as u64 > MAX_PIXELS {
            return None;
        }
        let (width, height, row_bytes) = (width as usize, height as usize, row_bytes as usize);
        // Rows may be padded: copied with their stride, then packed.
        if row_bytes < width * 4 {
            return None;
        }
        let mut pixels = vec![0u8; row_bytes * height];
        // SAFETY: Kotlin writes the pixels during the call only; the vector outlives it.
        let buffer =
            unsafe { env.new_direct_byte_buffer(pixels.as_mut_ptr(), pixels.len()) }.ok()?;
        env.call_method(
            bitmap,
            jni_str!("copyPixelsToBuffer"),
            jni_sig!("(Ljava/nio/Buffer;)V"),
            &[JValue::Object(&buffer)],
        )
        .ok()?;
        if row_bytes != width * 4 {
            for row in 1..height {
                pixels.copy_within(row * row_bytes..row * row_bytes + width * 4, row * width * 4);
            }
            pixels.truncate(width * height * 4);
        }
        Some(ImageBitmap::from_rgba_premultiplied(width as u32, height as u32, pixels))
    })();
    // A failed copy's exception is logged and cleared before calling again.
    crate::bridge::check(env, "Bitmap.copyPixelsToBuffer");
    // Its pixels freed now rather than at a garbage collection.
    let _ = env.call_method(bitmap, jni_str!("recycle"), jni_sig!("()V"), &[]);
    image
}
