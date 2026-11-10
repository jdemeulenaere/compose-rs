//! Adversarial review tests for `w2-graphics` (`D2400`–`D2404`, `D2430`–`D2433`):
//! `Interpolatable`, `Brush` / `ShaderBrush` caching, transforms, equality, placement,
//! `Brush::composite`, `BrushPainter`, `Shadow::lerp`, and opaque `ImageBitmap` initialization.

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;

use compose_ui::colorspace::ColorSpaces;
use compose_ui::interpolatable::{self, Interpolant, Interpolatable, Lerped};
use compose_ui::shadow::Shadow as BoxShadow;
use compose_ui::{
    BlendMode, Brush, BrushPainter, Color, CustomPainter, CustomShaderBrush, Dp, ImageBitmap,
    ImageBitmapConfig, Matrix, Offset, Painter, Shader, ShaderBrush, Size, TileMode,
    create_font_family_resolver,
};

#[derive(Clone, Copy, PartialEq, Debug)]
struct ZstA;

impl Interpolatable for ZstA {
    fn lerp(&self, other: Option<&dyn Any>, t: f32) -> Option<Box<dyn Any>> {
        if other.is_some_and(|o| o.is::<ZstB>()) { Some(Box::new(100.0 * t)) } else { None }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct ZstB;

impl Interpolatable for ZstB {
    fn lerp(&self, other: Option<&dyn Any>, t: f32) -> Option<Box<dyn Any>> {
        if other.is_some_and(|o| o.is::<ZstA>()) { Some(Box::new(100.0 * (1.0 - t))) } else { None }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct PlainNumber(f32);

#[derive(Clone, Copy, PartialEq, Debug)]
struct Valued(f32);

impl Interpolatable for Valued {
    fn lerp(&self, other: Option<&dyn Any>, t: f32) -> Option<Box<dyn Any>> {
        if let Some(other) = other.and_then(|o| o.downcast_ref::<PlainNumber>()) {
            return Some(Box::new(Valued(self.0 + (other.0 - self.0) * t)));
        }
        if other.is_none() {
            return Some(Box::new(Valued(self.0 * (1.0 - t))));
        }
        None
    }

    fn equals(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Valued>().is_some_and(|o| o.0 == self.0)
    }
}

#[test]
fn interpolatable_lerp_distinct_zsts_and_asymmetric_pairs() {
    // Distinct zero-sized types in a tuple (or `Box::new`) share the exact same address; `lerp`
    // must NOT short-circuit them as pointer-equal and skip `Interpolatable::lerp`.
    let pair = (ZstA, ZstB);
    match interpolatable::lerp(
        Some(Interpolant::Interpolatable(&pair.0)),
        Some(Interpolant::Interpolatable(&pair.1)),
        0.25,
    ) {
        Lerped::Value(boxed) => {
            let v = *boxed.downcast::<f32>().expect("f32 from ZstA::lerp");
            assert!((v - 25.0).abs() < 1e-5, "expected 25.0, got {v}");
        }
        other => panic!("expected Lerped::Value from ZstA -> ZstB, got {other:?}"),
    }

    // Asymmetric `Interpolatable` left vs plain `Other` right, and vice versa (`1.0 - t`).
    let left = Valued(10.0);
    let right = PlainNumber(30.0);
    match interpolatable::lerp(
        Some(Interpolant::Interpolatable(&left)),
        Some(Interpolant::Other(&right)),
        0.25,
    ) {
        Lerped::Value(boxed) => {
            let v = boxed.downcast::<Valued>().expect("Valued");
            assert!((v.0 - 15.0).abs() < 1e-5);
        }
        other => panic!("expected Value(15.0), got {other:?}"),
    }
    match interpolatable::lerp(
        Some(Interpolant::Other(&right)),
        Some(Interpolant::Interpolatable(&left)),
        0.25,
    ) {
        Lerped::Value(boxed) => {
            let v = boxed.downcast::<Valued>().expect("Valued");
            // `left.lerp(right, 1.0 - 0.25 = 0.75)` => 10 + 20 * 0.75 = 25.
            assert!((v.0 - 25.0).abs() < 1e-5);
        }
        other => panic!("expected Value(25.0), got {other:?}"),
    }

    // Extrapolation beyond [0, 1] with `None` on right (`10 * (1 - (-0.5)) = 15`, `10 * (1 - 1.5) = -5`).
    for (t, expected) in [(-0.5f32, 15.0f32), (1.5f32, -5.0f32)] {
        match interpolatable::lerp(Some(Interpolant::Interpolatable(&left)), None, t) {
            Lerped::Value(boxed) => {
                let v = boxed.downcast::<Valued>().expect("Valued");
                assert!((v.0 - expected).abs() < 1e-5, "at t={t}: got {}", v.0);
            }
            other => panic!("at t={t}: expected Value({expected}), got {other:?}"),
        }
    }

    // Equal values by `Interpolatable::equals` switch halfway without allocating `Value`.
    let copy = Valued(10.0);
    assert!(matches!(
        interpolatable::lerp(
            Some(Interpolant::Interpolatable(&left)),
            Some(Interpolant::Interpolatable(&copy)),
            0.49,
        ),
        Lerped::Start
    ));
    assert!(matches!(
        interpolatable::lerp(
            Some(Interpolant::Interpolatable(&left)),
            Some(Interpolant::Interpolatable(&copy)),
            0.5,
        ),
        Lerped::Stop
    ));
}

#[derive(Clone)]
struct CountingCustomBrush {
    id: u32,
    intrinsic: Size,
    calls: Rc<Cell<usize>>,
}

impl CustomShaderBrush for CountingCustomBrush {
    fn create_shader(&self, size: Size) -> Shader {
        assert!(
            size.width > 0.0 && size.height > 0.0,
            "create_shader must never be called on empty or NaN size: {size:?}"
        );
        self.calls.set(self.calls.get() + 1);
        Shader::linear_gradient(
            Offset::ZERO,
            Offset::new(size.width, size.height),
            [Color::RED, Color::BLUE],
            TileMode::Clamp,
        )
    }

    fn intrinsic_size(&self) -> Size {
        self.intrinsic
    }

    fn same(&self, other: &dyn CustomShaderBrush) -> bool {
        other.downcast_ref::<CountingCustomBrush>().is_some_and(|o| o.id == self.id)
    }
}

#[test]
fn shader_brush_custom_equality_checks_transform_and_empty_or_nan_sizes() {
    let calls_a = Rc::new(Cell::new(0));
    let calls_b = Rc::new(Cell::new(0));
    let brush_a = ShaderBrush::custom(CountingCustomBrush {
        id: 7,
        intrinsic: Size::new(40.0, 20.0),
        calls: calls_a.clone(),
    });
    let brush_b = ShaderBrush::custom(CountingCustomBrush {
        id: 7,
        intrinsic: Size::new(40.0, 20.0),
        calls: calls_b,
    });
    assert_eq!(brush_a, brush_b);

    // Distinct `transform` matrices on two custom brushes with `same() == true` must NOT be equal.
    brush_a.set_transform(Some(Matrix::translation(12.0, 4.0)));
    assert_ne!(brush_a, brush_b);
    brush_b.set_transform(Some(Matrix::translation(12.0, 4.0)));
    assert_eq!(brush_a, brush_b);

    // Clones share `Rc` state (`transform` and `created` cache).
    let clone_a = brush_a.clone();
    let wrapped = Brush::Shader(brush_a.clone());
    assert!(wrapped.shader(Size::new(50.0, 30.0), Offset::ZERO, 1.0).is_some());
    assert_eq!(calls_a.get(), 1);
    assert!(
        Brush::Shader(clone_a).shader(Size::new(50.0, 30.0), Offset::new(5.0, 5.0), 1.0).is_some()
    );
    assert_eq!(calls_a.get(), 1, "clone must reuse cached shader at same size");

    // Empty, negative, unspecified, AND partial-NaN sizes must return `None`, clear `created`,
    // and never call `create_shader` (including `resolved_at`).
    for bad_size in [
        Size::ZERO,
        Size::new(0.0, 30.0),
        Size::new(50.0, 0.0),
        Size::new(-10.0, 30.0),
        Size::UNSPECIFIED,
        Size::new(f32::NAN, 30.0),
        Size::new(50.0, f32::NAN),
    ] {
        assert_eq!(wrapped.shader(bad_size, Offset::ZERO, 1.0), None, "for {bad_size:?}");
        let resolved = wrapped.resolved_at(bad_size);
        assert_eq!(resolved.shader(bad_size, Offset::ZERO, 1.0), None, "resolved {bad_size:?}");
        let grad = Brush::radial_gradient([Color::RED, Color::BLUE]);
        assert_eq!(grad.shader(bad_size, Offset::ZERO, 1.0), None, "gradient {bad_size:?}");
    }
    assert_eq!(calls_a.get(), 1, "no bad size may call create_shader");

    // Returning to `(50, 30)` after an empty/NaN size creates the shader anew (`created` was cleared).
    assert!(wrapped.shader(Size::new(50.0, 30.0), Offset::ZERO, 1.0).is_some());
    assert_eq!(calls_a.get(), 2);
}

#[test]
fn brush_placed_preserves_shader_alpha_and_signed_infinity() {
    let fixed = Brush::from(Shader::linear_gradient(
        Offset::ZERO,
        Offset::new(10.0, 0.0),
        [Color::RED, Color::BLUE],
        TileMode::Clamp,
    ))
    .with_alpha(0.4);
    assert!((fixed.color(1.0).alpha() - 0.4).abs() < 1e-5);

    // Multi-paragraph text uses `MultiParagraph` -> `Brush::placed` via `draw_multi_paragraph`;
    // check both `color(alpha)` and `shader(...)` preserve `with_alpha(0.4)` without double-scaling
    // or dropping alpha when `placed`!
    {
        use compose_ui::platform::{
            FixedTextMeasurer, ParagraphHandle, ParagraphMetrics, SharedTextMeasurer,
            TextLayoutOptions, TextMeasurer as PlatformTextMeasurer, TextPosition, UiHost,
        };
        use compose_ui::{
            AnnotatedRange, AnnotatedString, Annotation, Constraints, Density, IntSize, LocaleList,
            Modifier, MultiParagraph, MultiParagraphIntrinsics, ParagraphStyle, Rect, TextStyle,
        };

        struct StyleCapture {
            inner: FixedTextMeasurer,
            alphas: Rc<Cell<Vec<f32>>>,
        }
        impl PlatformTextMeasurer for StyleCapture {
            fn layout(
                &mut self,
                text: &str,
                style: &TextStyle,
                options: TextLayoutOptions,
                constraints: Constraints,
                density: &Density,
            ) -> ParagraphHandle {
                if let Some(brush) = style.foreground().brush() {
                    let mut v = self.alphas.take();
                    v.push(brush.color(1.0).alpha());
                    self.alphas.set(v);
                }
                self.inner.layout(text, style, options, constraints, density)
            }
            fn metrics(&self, p: ParagraphHandle) -> ParagraphMetrics {
                self.inner.metrics(p)
            }
            fn hit_test(&self, p: ParagraphHandle, pos: Offset) -> TextPosition {
                self.inner.hit_test(p, pos)
            }
            fn cursor_rect(&self, p: ParagraphHandle, off: usize) -> Rect {
                self.inner.cursor_rect(p, off)
            }
            fn text(&self, p: ParagraphHandle) -> &str {
                self.inner.text(p)
            }
            fn release(&mut self, p: ParagraphHandle) {
                self.inner.release(p);
            }
        }

        let alphas = Rc::new(Cell::new(Vec::new()));
        let measurer_for_host =
            Box::new(StyleCapture { inner: FixedTextMeasurer::default(), alphas: alphas.clone() });
        let shared = SharedTextMeasurer::new(Box::new(FixedTextMeasurer::default()));
        let resolver = create_font_family_resolver(&shared);
        let text = AnnotatedString::with_annotations(
            "A\nB",
            vec![
                AnnotatedRange::new(Annotation::ParagraphStyle(ParagraphStyle::default()), 0, 2),
                AnnotatedRange::new(Annotation::ParagraphStyle(ParagraphStyle::default()), 2, 3),
            ],
        );
        let style = TextStyle { brush: Some(fixed.clone()), ..TextStyle::DEFAULT };
        let intrinsics = Rc::new(MultiParagraphIntrinsics::new(
            text,
            &style,
            &[],
            Density::new(1.0),
            resolver,
            true,
            LocaleList::empty(),
        ));
        let mp = Rc::new(MultiParagraph::new_in_paint_of(
            intrinsics,
            Constraints::new(0, 100, 0, 100),
            10,
            compose_ui::TextOverflow::Clip,
            &style,
        ));
        let mut host = UiHost::new(IntSize::new(100, 100), Density::new(1.0), measurer_for_host);
        let style_for_draw = style.clone();
        host.set_content(move || {
            let mp = mp.clone();
            let style_for_draw = style_for_draw.clone();
            compose_ui::Layout(
                Modifier.draw_behind(move |scope| {
                    scope.draw_multi_paragraph(&mp, &style_for_draw);
                }),
                Rc::new(compose_ui::EmptyMeasurePolicy),
                compose::callback(|| {}),
            );
        });
        host.frame();
        let recorded = alphas.take();
        assert!(!recorded.is_empty());
        for alpha in recorded {
            assert!(
                (alpha - 0.4).abs() < 1e-5,
                "Brush::placed on ShaderBrush must preserve color(1.0).alpha() == 0.4, got {alpha}"
            );
        }
    }
    let shader = fixed.shader(Size::new(20.0, 20.0), Offset::new(0.0, -10.0), 0.5).unwrap();
    assert!((shader.alpha() - 0.2).abs() < 1e-5);

    // Negative infinity on a `LinearGradient` endpoint stays `-f32::INFINITY` (only `+INFINITY`
    // resolves to `size.width`/`size.height`, matching Compose's `LinearGradient.createShader`).
    let neg_inf = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::new(f32::NEG_INFINITY, 0.0),
        Offset::new(f32::INFINITY, 10.0),
        TileMode::Clamp,
    );
    let ps = neg_inf.shader(Size::new(80.0, 40.0), Offset::new(5.0, 7.0), 1.0).unwrap();
    match ps.shader {
        Shader::LinearGradient { from, to, .. } => {
            assert_eq!(from, Offset::new(f32::NEG_INFINITY, 7.0));
            assert_eq!(to, Offset::new(85.0, 17.0));
        }
        other => panic!("expected LinearGradient, got {other:?}"),
    }
}

#[test]
fn brush_composite_solid_normalization_and_painter_intrinsics() {
    let mask = Brush::horizontal_gradient([Color::BLACK, Color::TRANSPARENT]);
    // Compose's `Brush.composite` converts `SolidColor(c)` via `toShaderBrush()` to
    // `verticalGradient(listOf(c, c))`, so both forms compare equal.
    let from_solid = Brush::composite(Brush::solid(Color::RED), mask.clone(), BlendMode::SrcIn);
    let from_grad = Brush::composite(
        Brush::vertical_gradient([Color::RED, Color::RED]),
        mask,
        BlendMode::SrcIn,
    );
    assert_eq!(from_solid, from_grad);
    assert_eq!(
        Painter::new(BrushPainter::new(from_solid)),
        Painter::new(BrushPainter::new(from_grad))
    );

    let custom = Brush::Shader(ShaderBrush::custom(CountingCustomBrush {
        id: 1,
        intrinsic: Size::new(64.0, 32.0),
        calls: Rc::new(Cell::new(0)),
    }));
    let painter = BrushPainter::new(custom);
    assert_eq!(painter.intrinsic_size(), Size::new(64.0, 32.0));
}

#[test]
fn brush_and_shadow_lerp_edge_cases() {
    // `RadialGradient` and `SweepGradient` interpolate finite centers smoothly (not at 0.5),
    // while `Offset::UNSPECIFIED` yields `UNSPECIFIED` through `Offset::lerp` until both are finite.
    let r1 = Brush::radial_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::new(10.0, 20.0),
        40.0,
        TileMode::Clamp,
    );
    let r2 = Brush::radial_gradient_with(
        [Color::GREEN, Color::WHITE],
        Offset::new(30.0, 60.0),
        80.0,
        TileMode::Repeated,
    );
    let mid = Brush::lerp(Some(&r1), Some(&r2), 0.25).unwrap();
    let ps = mid.shader(Size::new(100.0, 100.0), Offset::ZERO, 1.0).unwrap();
    match ps.shader {
        Shader::RadialGradient { center, radius, gradient } => {
            assert_eq!(center, Offset::new(15.0, 30.0));
            assert!((radius - 50.0).abs() < 1e-5);
            assert_eq!(gradient.tile_mode, TileMode::Clamp, "t=0.25 < 0.5 keeps start tile_mode");
        }
        other => panic!("expected RadialGradient, got {other:?}"),
    }

    // `ShaderBrush`es switch halfway in `Brush::lerp` and in `shadow::lerp`.
    let sb1 = Brush::from(Shader::sweep_gradient(Offset::new(5.0, 5.0), [Color::RED, Color::BLUE]));
    let sb2 =
        Brush::from(Shader::sweep_gradient(Offset::new(5.0, 5.0), [Color::GREEN, Color::WHITE]));
    assert_eq!(Brush::lerp(Some(&sb1), Some(&sb2), 0.49).as_ref(), Some(&sb1));
    assert_eq!(Brush::lerp(Some(&sb1), Some(&sb2), 0.5).as_ref(), Some(&sb2));

    let s1 = BoxShadow::brushed(Dp(10.0), sb1.clone());
    let s2 = BoxShadow::brushed(Dp(20.0), sb2.clone());
    let shadow_before = compose_ui::shadow::lerp(Some(&s1), Some(&s2), 0.25).unwrap();
    let shadow_after = compose_ui::shadow::lerp(Some(&s1), Some(&s2), 0.75).unwrap();
    assert_eq!(shadow_before.radius, Dp(12.5));
    assert_eq!(shadow_before.brush.as_ref(), Some(&sb1));
    assert_eq!(shadow_after.radius, Dp(17.5));
    assert_eq!(shadow_after.brush.as_ref(), Some(&sb2));

    // `None` <-> brushed `Shadow` uses `transparentCopy()` (keeps `radius = 16.dp` and
    // interpolates `brush` against transparent solid color, matching Compose's `Shadow.kt`).
    let from_none = compose_ui::shadow::lerp(
        None,
        Some(&BoxShadow::brushed(Dp(16.0), Brush::horizontal_gradient([Color::RED, Color::BLUE]))),
        0.5,
    )
    .unwrap();
    assert_eq!(from_none.radius, Dp(16.0));
    assert_eq!(
        from_none.brush,
        Brush::lerp(None, Some(&Brush::horizontal_gradient([Color::RED, Color::BLUE])), 0.5)
    );
}

#[test]
fn opaque_image_bitmap_initializes_alpha_bytes() {
    let argb = ImageBitmap::new_with(2, 2, ImageBitmapConfig::Argb8888, false, &ColorSpaces::SRGB);
    let raw_argb = argb.pixels();
    for chunk in raw_argb.chunks_exact(4) {
        assert_eq!(chunk, &[0, 0, 0, 255], "opaque Argb8888 starts with alpha 255");
    }
    assert_eq!(argb.color(0, 0), Color::BLACK);
    assert_eq!(argb.to_pixel_map().get(1, 1), Color::BLACK);

    let f16 = ImageBitmap::new_with(2, 2, ImageBitmapConfig::F16, false, &ColorSpaces::DISPLAY_P3);
    let raw_f16 = f16.pixels();
    for chunk in raw_f16.chunks_exact(8) {
        assert_eq!(&chunk[0..6], &[0, 0, 0, 0, 0, 0]);
        assert_eq!(u16::from_le_bytes([chunk[6], chunk[7]]), 0x3c00, "1.0 in IEEE 754 binary16");
    }
    let c = f16.color(1, 1);
    assert_eq!(*c.color_space(), ColorSpaces::DISPLAY_P3);
    assert_eq!((c.red(), c.green(), c.blue(), c.alpha()), (0.0, 0.0, 0.0, 1.0));
}
