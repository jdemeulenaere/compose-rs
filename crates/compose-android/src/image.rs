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
    let Ok(rasterize) = env.get_static_method_id(
        &class,
        jni_str!("rasterize"),
        jni_sig!("(Ljava/nio/ByteBuffer;IIIZILjava/nio/ByteBuffer;)V"),
    ) else {
        return;
    };
    let Ok(rasterizer_class) = env.new_global_ref(&class) else { return };
    let Ok(class) = env.new_global_ref(class) else { return };
    let rasterizer = AndroidBitmapRasterizer { class: rasterizer_class, rasterize };
    compose_ui::platform::set_image_decoder(Arc::new(AndroidImageDecoder { class, decode }));
    compose_ui::platform::set_bitmap_rasterizer(Arc::new(rasterizer));
}

/// The platform's bitmap rasterizer on Android (`compose_ui::platform::set_bitmap_rasterizer`):
/// a bitmap's drawings written as a frame is (`serialize.rs`, its paths and images defined
/// first) and replayed by `ComposeRsImages.rasterize` with a software `Canvas` over an
/// `android.graphics.Bitmap` of its pixels, copied in and back (D1915).
struct AndroidBitmapRasterizer {
    class: Global<JClass<'static>>,
    rasterize: JStaticMethodID,
}

impl AndroidBitmapRasterizer {
    #[allow(clippy::too_many_arguments)]
    fn call_rasterize(
        &self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        config: u8,
        has_alpha: bool,
        color_space_id: u8,
        drawing: &[u8],
    ) {
        // Any thread: attached for good if it isn't, in a local frame of its own.
        let _ = crate::bridge::vm().attach_current_thread(|env| -> jni::errors::Result<()> {
            let drawn = (|| -> jni::errors::Result<()> {
                // SAFETY: Kotlin reads and writes the buffers during the call only; both outlive
                // it, and nothing else touches them meanwhile (`pixels` is borrowed mutably).
                let pixels =
                    unsafe { env.new_direct_byte_buffer(pixels.as_mut_ptr(), pixels.len()) }?;
                // SAFETY: as above; Kotlin only reads it.
                let drawing = unsafe {
                    env.new_direct_byte_buffer(drawing.as_ptr().cast_mut(), drawing.len())
                }?;
                // SAFETY: `rasterize` is `(Ljava/nio/ByteBuffer;IIIZILjava/nio/ByteBuffer;)V`,
                // static, of `class`.
                unsafe {
                    env.call_static_method_unchecked(
                        &self.class,
                        self.rasterize,
                        ReturnType::Primitive(jni::signature::Primitive::Void),
                        &[
                            JValue::Object(&pixels).as_jni(),
                            JValue::Int(width as i32).as_jni(),
                            JValue::Int(height as i32).as_jni(),
                            JValue::Int(i32::from(config)).as_jni(),
                            JValue::Bool(has_alpha).as_jni(),
                            JValue::Int(i32::from(color_space_id)).as_jni(),
                            JValue::Object(&drawing).as_jni(),
                        ],
                    )
                }?;
                Ok(())
            })();
            crate::bridge::check(env, "ComposeRsImages.rasterize");
            drawn
        });
    }
}

impl compose_ui::platform::BitmapRasterizer for AndroidBitmapRasterizer {
    fn draw(
        &self,
        list: &compose_ui::platform::DisplayList,
        width: u32,
        height: u32,
        config: compose_ui::ImageBitmapConfig,
        has_alpha: bool,
        color_space: &compose_ui::colorspace::ColorSpace,
        pixels: &mut [u8],
    ) {
        if width == 0 || height == 0 {
            return;
        }
        let drawing = bitmap_drawing(list);
        if let Some((config_byte, cs_id)) =
            crate::serialize::android_bitmap_format(config, color_space)
        {
            self.call_rasterize(pixels, width, height, config_byte, has_alpha, cs_id, &drawing);
        } else {
            let mut f16 = compose_ui::platform::image_to_extended_srgb_f16(
                pixels,
                config,
                has_alpha,
                color_space,
            );
            self.call_rasterize(&mut f16, width, height, 3, has_alpha, 2, &drawing);
            compose_ui::platform::extended_srgb_f16_to_image(
                &f16,
                pixels,
                config,
                has_alpha,
                color_space,
            );
        }
    }
}

/// `list` in the replayer's format: its paths and images defined, then its operations, as a
/// frame's (`takeDrawing`). Its paragraphs are left out: a bitmap's canvas has no text measurer.
pub(crate) fn bitmap_drawing(list: &compose_ui::platform::DisplayList) -> Vec<u8> {
    use compose_ui::platform::{DrawOp, Optimizations};

    use crate::serialize::{GroupCache, Resources, Writer, write_display_list};
    let paragraphs = list.ops.iter().any(|op| matches!(op, DrawOp::DrawParagraph(..)));
    let without;
    let list = if paragraphs {
        let mut copy = compose_ui::platform::DisplayList::default();
        copy.ops = list
            .ops
            .iter()
            .filter(|op| !matches!(op, DrawOp::DrawParagraph(..)))
            .cloned()
            .collect();
        copy.shaders = list.shaders.clone();
        copy.color_filters = list.color_filters.clone();
        without = copy;
        &without
    } else {
        list
    };
    let (mut resources, mut ops, mut drawing) =
        (Resources::default(), Writer::default(), Writer::default());
    write_display_list(list, &mut resources, &mut GroupCache::new(Optimizations::NONE), &mut ops);
    resources.write_pending(&mut drawing);
    drawing.bytes.extend_from_slice(&ops.bytes);
    drawing.bytes
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

#[cfg(test)]
mod tests {
    use compose_ui::platform::{DisplayList, DrawOp};
    use compose_ui::{Color, Offset, Paint, Path, Rect};

    use super::bitmap_drawing;
    use crate::serialize::{operand_bytes, tag};

    // Compose: compose/ui/ui-graphics/src/androidMain/kotlin/androidx/compose/ui/graphics/AndroidCanvas.android.kt, Canvas(imageBitmap) (no Compose test)
    /// A bitmap's drawing defines its paths and images before the operations naming them, as a
    /// frame's: the replayer of `ComposeRsImages.rasterize` starts with none.
    #[test]
    fn a_bitmaps_drawing_defines_its_resources_first() {
        let image = compose_ui::ImageBitmap::from_rgba_premultiplied(1, 1, vec![255; 4]);
        let mut list = DisplayList::default();
        let rect = Rect::new(0.0, 0.0, 1.0, 1.0);
        let draw = DrawOp::DrawImage {
            image,
            src: rect,
            dst: rect,
            alpha: 1.0,
            color_filter: None,
            filter_quality: compose_ui::FilterQuality::Low,
        };
        list.push(draw);
        let bytes = bitmap_drawing(&list);
        assert_eq!(bytes[0], tag::DEFINE_IMAGE);
        // Its id, size and length, its four bytes of pixels, then the drawing.
        let ops_at = 1 + 4 * 4 + 4;
        assert_eq!(&bytes[17..ops_at], &[255; 4]);
        assert_eq!(bytes[ops_at], tag::DRAW_IMAGE);
        assert_eq!(ops_at + 1 + operand_bytes(&bytes, ops_at), bytes.len());

        let path = Path::new();
        path.add_rect(Rect::new(0.0, 0.0, 2.0, 2.0));
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawPath {
            path: path.data(),
            offset: Offset::ZERO,
            scale: (1.0, 1.0),
            paint: Paint::fill(Color::RED),
        });
        assert_eq!(bitmap_drawing(&list)[0], tag::DEFINE_PATH);
    }
}
