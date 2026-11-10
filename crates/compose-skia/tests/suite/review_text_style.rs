//! Adversarial review of `w2-text-style` (`D2300`–`D2306`): edge cases of `baseline_shift`,
//! `TextGeometricTransform` (`scale_x`, `skew_x`), `font_feature_settings`, `locale_list`,
//! `TextMotion`, and `include_font_padding` on `SkiaTextMeasurer` and `Paragraph::paint`.

use std::rc::Rc;

use compose::test::basic_measure_font;
use compose::ui::{
    AnnotatedRange, AnnotatedString, BaselineShift, BlendMode, Brush, Color, Constraints, Density,
    DrawScope, DrawStyle, FontFamily, LOCAL_FONT_FAMILY_RESOLVER, LocaleList, MultiParagraph,
    MultiParagraphIntrinsics, Offset, Paragraph, ParagraphStyle, Placeholder,
    PlaceholderVerticalAlign, Shadow, SpanStyle, Stroke, TextDecoration, TextDirection,
    TextGeometricTransform, TextMotion, TextOverflow, TextStyle, UnitExt, paragraph_of_text,
    platform::{TextLayoutOptions, TextMeasurer},
};
use compose_skia::SkiaTextMeasurer;

use super::text_painter::{Picture, draw_with};

fn sample_style(font_size: f32) -> TextStyle {
    TextStyle {
        font_size: font_size.sp(),
        font_family: Some(basic_measure_font().to_font_family()),
        text_direction: Some(TextDirection::Content),
        ..TextStyle::DEFAULT
    }
}

#[allow(clippy::too_many_arguments)]
fn laid_out(
    scope: &DrawScope<'_>,
    text: &str,
    style: &TextStyle,
    spans: &[AnnotatedRange<SpanStyle>],
    placeholders: &[AnnotatedRange<Placeholder>],
    width: i32,
    max_lines: u32,
    overflow: TextOverflow,
) -> Rc<dyn Paragraph> {
    paragraph_of_text(
        text,
        style,
        Constraints::new(0, width, 0, Constraints::INFINITY),
        Density::new(1.0),
        scope.current_value_of(&LOCAL_FONT_FAMILY_RESOLVER),
        LocaleList::from_language_tags("en"),
        spans,
        placeholders,
        max_lines,
        overflow,
    )
}

#[allow(clippy::too_many_arguments)]
fn painted_with(
    window: (i32, i32),
    text: &'static str,
    style: TextStyle,
    spans: Vec<AnnotatedRange<SpanStyle>>,
    placeholders: Vec<AnnotatedRange<Placeholder>>,
    width: i32,
    max_lines: u32,
    overflow: TextOverflow,
    paint_fn: impl Fn(&dyn Paragraph, &mut DrawScope<'_>) + 'static,
) -> Picture {
    draw_with(window, window, 1.0, 0, move |scope, _| {
        let paragraph =
            laid_out(scope, text, &style, &spans, &placeholders, width, max_lines, overflow);
        paint_fn(&*paragraph, scope);
    })
}

fn default_paint(
    window: (i32, i32),
    text: &'static str,
    style: TextStyle,
    spans: Vec<AnnotatedRange<SpanStyle>>,
    width: i32,
) -> Picture {
    painted_with(
        window,
        text,
        style,
        spans,
        vec![],
        width,
        u32::MAX,
        TextOverflow::Clip,
        |paragraph, scope| {
            paragraph.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
        },
    )
}

#[test]
#[ignore = "review: SkiaTextMeasurer clamps scale_x == 0 to 1.0 instead of collapsing glyph advances and painting nothing"]
fn zero_scale_x_collapses_glyph_widths_and_paints_no_glyphs_while_keeping_height() {
    let zero = TextGeometricTransform { scale_x: 0.0, skew_x: 0.0 };
    let style = TextStyle { text_geometric_transform: Some(zero), ..sample_style(40.0) };

    let mut skia = SkiaTextMeasurer::new();
    let handle = skia.layout(
        "abcde",
        &style,
        TextLayoutOptions::default(),
        Constraints::unbounded(),
        &Density::new(1.0),
    );
    let paragraph = skia.paragraph(handle);
    let width = paragraph.max_intrinsic_width();
    let height = paragraph.height();
    skia.release(handle);
    assert_eq!(width, 0.0, "scale_x == 0 should collapse glyph advances to 0");
    assert_eq!(height, 40.0, "scale_x == 0 should preserve unscaled line height");

    let picture = default_paint((200, 50), "abcde", style.clone(), vec![], 200);
    assert!(picture.is_blank(), "scale_x == 0 glyphs have zero width and paint no pixels");

    // With `em` letter spacing and `scale_x == 0`, spacing also collapses to 0 on both stacks;
    // with `sp` letter spacing, fixed spacing per character remains.
    let zero_em = TextStyle { letter_spacing: 2.0.em(), ..style.clone() };
    let mut fixed = compose::ui::platform::FixedTextMeasurer::with_metrics(
        compose::ui::platform::FixedFontMetrics::SAMPLE_FONT,
    );
    let h_fixed = fixed.layout(
        "abc",
        &zero_em,
        TextLayoutOptions::default(),
        Constraints::unbounded(),
        &Density::new(1.0),
    );
    let h_skia = skia.layout(
        "abc",
        &zero_em,
        TextLayoutOptions::default(),
        Constraints::unbounded(),
        &Density::new(1.0),
    );
    assert_eq!(fixed.paragraph(h_fixed).max_intrinsic_width(), 0.0);
    assert_eq!(skia.paragraph(h_skia).max_intrinsic_width(), 0.0);
    assert_eq!(skia.paragraph(h_skia).height(), 40.0);
    fixed.release(h_fixed);
    skia.release(h_skia);

    let zero_sp = TextStyle { letter_spacing: 5.0.sp(), ..style };
    let h_fixed_sp = fixed.layout(
        "abc",
        &zero_sp,
        TextLayoutOptions::default(),
        Constraints::unbounded(),
        &Density::new(1.0),
    );
    let h_skia_sp = skia.layout(
        "abc",
        &zero_sp,
        TextLayoutOptions::default(),
        Constraints::unbounded(),
        &Density::new(1.0),
    );
    assert_eq!(fixed.paragraph(h_fixed_sp).max_intrinsic_width(), 15.0);
    assert_eq!(skia.paragraph(h_skia_sp).max_intrinsic_width(), 15.0);
    fixed.release(h_fixed_sp);
    skia.release(h_skia_sp);
}

#[test]
#[ignore = "review: SkiaTextMeasurer clamps negative scale_x to 1.0 instead of horizontally mirroring glyphs"]
fn negative_scale_x_mirrors_glyph_horizontally_instead_of_drawing_unscaled() {
    // Sample font glyph 'a' at 40 px is a right-pointing triangle: left edge (x=2, y=20) is filled,
    // right edge (x=38, y=4) is empty. With scale_x = -1.0 around the glyph's origin, it flips!
    let neg = TextGeometricTransform { scale_x: -1.0, skew_x: 0.0 };
    let plain = default_paint((200, 50), "  a", sample_style(40.0), vec![], 200);
    let flipped = default_paint(
        (200, 50),
        "  a",
        TextStyle { text_geometric_transform: Some(neg), ..sample_style(40.0) },
        vec![],
        200,
    );
    assert_ne!(
        flipped, plain,
        "negative scale_x must mirror glyphs rather than falling back to scale_x = 1.0"
    );
}

#[test]
fn transformed_paragraph_with_end_ellipsis_still_paints_its_ellipsis() {
    // Use system sans-serif (sample font may not have U+2026 glyph or has empty glyph):
    let base = TextStyle {
        font_size: 30.0.sp(),
        font_family: Some(FontFamily::SansSerif),
        ..TextStyle::DEFAULT
    };
    let clipped = painted_with(
        (100, 50),
        "MMMMMMMMMM",
        TextStyle {
            text_geometric_transform: Some(TextGeometricTransform { scale_x: 1.0, skew_x: 0.3 }),
            ..base.clone()
        },
        vec![],
        vec![],
        90,
        1,
        TextOverflow::Clip,
        |p, s| p.paint(s, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver),
    );
    let ellipsized = painted_with(
        (100, 50),
        "MMMMMMMMMM",
        TextStyle {
            text_geometric_transform: Some(TextGeometricTransform { scale_x: 1.0, skew_x: 0.3 }),
            ..base.clone()
        },
        vec![],
        vec![],
        90,
        1,
        TextOverflow::Ellipsis,
        |p, s| p.paint(s, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver),
    );
    assert!(!ellipsized.is_blank());
    assert_ne!(clipped, ellipsized, "ellipsized transformed text should differ from clipped");

    // Compare single-M vs ellipsized "MMMMMMMMMM" at width matching "M…": the ellipsis after M
    // must actually paint pixels past M!
    let single_m = painted_with(
        (120, 50),
        "M",
        TextStyle {
            text_geometric_transform: Some(TextGeometricTransform { scale_x: 1.0, skew_x: 0.3 }),
            ..base.clone()
        },
        vec![],
        vec![],
        120,
        1,
        TextOverflow::Clip,
        |p, s| p.paint(s, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver),
    );
    let m_ellipsis = painted_with(
        (120, 50),
        "MMMMMM",
        TextStyle {
            text_geometric_transform: Some(TextGeometricTransform { scale_x: 1.0, skew_x: 0.3 }),
            ..base.clone()
        },
        vec![],
        vec![],
        60,
        1,
        TextOverflow::Ellipsis,
        |p, s| p.paint(s, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver),
    );
    assert_ne!(
        m_ellipsis, single_m,
        "the ellipsis glyph itself must be painted when the paragraph is transformed"
    );

    // Partial-span ellipsis: untransformed black "M" followed by transformed red "MMMMM" cut to "M…",
    // and start/middle ellipsis on transformed text.
    let span_ellipsis = painted_with(
        (120, 50),
        "MMMMMM",
        base.clone(),
        vec![AnnotatedRange::new(
            SpanStyle {
                color: Color::RED,
                text_geometric_transform: Some(TextGeometricTransform {
                    scale_x: 1.0,
                    skew_x: 0.3,
                }),
                ..SpanStyle::DEFAULT
            },
            1,
            6,
        )],
        vec![],
        60,
        1,
        TextOverflow::Ellipsis,
        |p, s| p.paint(s, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver),
    );
    let plain_m = painted_with(
        (120, 50),
        "M",
        base.clone(),
        vec![],
        vec![],
        120,
        1,
        TextOverflow::Clip,
        |p, s| p.paint(s, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver),
    );
    assert_ne!(span_ellipsis, plain_m, "ellipsis inside a transformed span must still paint");

    for overflow in [TextOverflow::StartEllipsis, TextOverflow::MiddleEllipsis] {
        let cut = painted_with(
            (120, 50),
            "MMMMMMMMMM",
            TextStyle {
                color: Color::BLUE,
                text_geometric_transform: Some(TextGeometricTransform {
                    scale_x: 1.2,
                    skew_x: 0.2,
                }),
                ..base.clone()
            },
            vec![],
            vec![],
            90,
            1,
            overflow,
            |p, s| p.paint(s, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver),
        );
        assert!(cut.contains(Color::BLUE), "{overflow:?} on transformed text must paint");
    }
}

#[test]
fn transformed_paragraph_honours_paint_overrides_for_color_stroke_shadow_and_brush() {
    let style = TextStyle {
        color: Color::RED,
        text_geometric_transform: Some(TextGeometricTransform { scale_x: 1.5, skew_x: 0.25 }),
        ..sample_style(40.0)
    };
    let repainted = painted_with(
        (200, 60),
        "abc",
        style.clone(),
        vec![],
        vec![],
        200,
        u32::MAX,
        TextOverflow::Clip,
        |p, scope| {
            let shadow =
                Shadow { color: Color::BLUE, offset: Offset::new(0.0, 12.0), blur_radius: 0.0 };
            let stroke = DrawStyle::Stroke(Stroke { width: 3.0, ..Stroke::default() });
            p.paint(
                scope,
                Color::GREEN,
                Some(shadow),
                Some(TextDecoration::UNDERLINE),
                Some(stroke),
                BlendMode::SrcOver,
            );
        },
    );
    assert!(
        repainted.contains(Color::GREEN),
        "paint color override should apply to transformed glyphs"
    );
    assert!(
        repainted.contains(Color::BLUE),
        "paint shadow override should apply to transformed glyphs"
    );
    assert!(!repainted.contains(Color::RED), "original red color should be overridden");

    let brushed = painted_with(
        (200, 60),
        "abc",
        style,
        vec![],
        vec![],
        200,
        u32::MAX,
        TextOverflow::Clip,
        |p, scope| {
            let gradient = Brush::horizontal_gradient([Color::GREEN, Color::BLUE]);
            p.paint_with_brush(scope, &gradient, 1.0, None, None, None, BlendMode::SrcOver);
        },
    );
    assert!(!brushed.contains(Color::RED), "paint_with_brush should override transformed glyphs");
    assert!(!brushed.is_blank());
}

#[test]
fn transformed_spans_in_bidi_multibyte_and_placeholder_text_paint_all_runs() {
    // Multi-char placeholder (2 UTF-16 units -> U+FFFC + U+FEFF = 6 UTF-8 bytes in builder),
    // followed by Hebrew RTL and transformed Latin.
    let placeholder = AnnotatedRange::new(
        Placeholder::new(20.0.sp(), 20.0.sp(), PlaceholderVerticalAlign::AboveBaseline),
        0,
        2,
    );
    let spans = vec![AnnotatedRange::new(
        SpanStyle {
            color: Color::BLUE,
            text_geometric_transform: Some(TextGeometricTransform { scale_x: 1.5, skew_x: 0.2 }),
            ..SpanStyle::DEFAULT
        },
        8,
        10,
    )];
    // "XX" (2 bytes, placeholder) + "\u{05D0}\u{05D1}\u{05D2}" (6 bytes, 2..8) + "ab" (2 bytes, 8..10)
    let text = "XX\u{05D0}\u{05D1}\u{05D2}ab";
    let base = TextStyle {
        font_size: 30.0.sp(),
        font_family: Some(FontFamily::SansSerif),
        ..TextStyle::DEFAULT
    };
    let pic = painted_with(
        (300, 60),
        text,
        base,
        spans,
        vec![placeholder],
        300,
        u32::MAX,
        TextOverflow::Clip,
        |p, scope| p.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver),
    );
    assert!(
        pic.contains(Color::BLUE),
        "transformed span after multi-unit placeholder and RTL text must paint"
    );
}

#[test]
fn alternating_static_and_animated_text_motions_in_one_frame_keep_distinct_metrics() {
    let static_style = TextStyle {
        font_size: 15.0.sp(),
        font_family: Some(FontFamily::SansSerif),
        text_motion: Some(TextMotion::Static),
        ..TextStyle::DEFAULT
    };
    let animated_style =
        TextStyle { text_motion: Some(TextMotion::Animated), ..static_style.clone() };
    let default_style = TextStyle { text_motion: None, ..static_style.clone() };

    let mut measurer = SkiaTextMeasurer::new();
    let opts = TextLayoutOptions::default();
    let c = Constraints::unbounded();
    let d = Density::new(1.0);

    // Alternate Static -> Animated -> Default (None) -> Static -> Animated -> Default on the same
    // measurer: each motion mode keeps its own ParagraphCache without stale hits or resets.
    let mut fresh_default = SkiaTextMeasurer::new();
    let h_d_ref = fresh_default.layout("Hello world 12345", &default_style, opts, c, &d);
    let w_d_ref = fresh_default.paragraph(h_d_ref).max_intrinsic_width();

    let h_s1 = measurer.layout("Hello world 12345", &static_style, opts, c, &d);
    let h_a1 = measurer.layout("Hello world 12345", &animated_style, opts, c, &d);
    let h_d1 = measurer.layout("Hello world 12345", &default_style, opts, c, &d);
    let h_s2 = measurer.layout("Hello world 12345", &static_style, opts, c, &d);
    let h_a2 = measurer.layout("Hello world 12345", &animated_style, opts, c, &d);
    let h_d2 = measurer.layout("Hello world 12345", &default_style, opts, c, &d);

    let w_s1 = measurer.paragraph(h_s1).max_intrinsic_width();
    let w_a1 = measurer.paragraph(h_a1).max_intrinsic_width();
    let w_d1 = measurer.paragraph(h_d1).max_intrinsic_width();
    let w_s2 = measurer.paragraph(h_s2).max_intrinsic_width();
    let w_a2 = measurer.paragraph(h_a2).max_intrinsic_width();
    let w_d2 = measurer.paragraph(h_d2).max_intrinsic_width();

    assert_eq!(w_s1, w_s2);
    assert_eq!(w_a1, w_a2);
    assert_eq!(w_d1, w_d_ref);
    assert_eq!(w_d2, w_d_ref);
    assert_ne!(w_s1, w_a1);
    // Verify ParagraphCache stays populated across interleaved Static/Animated layouts!
    for handle in [h_s1, h_a1, h_d1, h_s2, h_a2, h_d2] {
        measurer.release(handle);
    }

    // Also test MultiParagraph with mixed ParagraphStyle text_motion in one AnnotatedString!
    let mp_text = AnnotatedString::with_styles(
        "Hello world 12345Hello world 12345".to_string(),
        [],
        [
            AnnotatedRange::new(
                ParagraphStyle { text_motion: Some(TextMotion::Static), ..ParagraphStyle::DEFAULT },
                0,
                17,
            ),
            AnnotatedRange::new(
                ParagraphStyle {
                    text_motion: Some(TextMotion::Animated),
                    ..ParagraphStyle::DEFAULT
                },
                17,
                34,
            ),
        ],
    );
    let shared = compose::ui::platform::SharedTextMeasurer::new(Box::new(SkiaTextMeasurer::new()));
    let resolver = compose::ui::create_font_family_resolver(&shared);
    let intrinsics = MultiParagraphIntrinsics::new(
        mp_text,
        &TextStyle {
            font_size: 15.0.sp(),
            font_family: Some(FontFamily::SansSerif),
            ..TextStyle::DEFAULT
        },
        &[],
        Density::new(1.0),
        resolver,
        true,
        LocaleList::from_language_tags("en"),
    );
    let mp = MultiParagraph::new(
        Rc::new(intrinsics),
        Constraints::new(0, 500, 0, Constraints::INFINITY),
        u32::MAX,
        TextOverflow::Clip,
    );
    assert_eq!(mp.line_count(), 2);
    assert_ne!(mp.get_line_right(0), mp.get_line_right(1));
}

#[test]
fn stacked_baseline_shifts_and_geometric_transforms_paint_exact_cancelled_and_combined_pixels() {
    let base = sample_style(40.0);
    let plain = default_paint((200, 80), "abcde", base.clone(), vec![], 200);

    // Opposite baseline shifts on the same range cancel to exact unshifted pixels (catching a
    // last-span-wins regression where Superscript would still shift above Subscript).
    let cancelled = default_paint(
        (200, 80),
        "abcde",
        base.clone(),
        vec![
            AnnotatedRange::new(
                SpanStyle { baseline_shift: Some(BaselineShift::SUBSCRIPT), ..SpanStyle::DEFAULT },
                0,
                3,
            ),
            AnnotatedRange::new(
                SpanStyle {
                    baseline_shift: Some(BaselineShift::SUPERSCRIPT),
                    ..SpanStyle::DEFAULT
                },
                0,
                3,
            ),
        ],
        200,
    );
    assert_eq!(cancelled, plain, "Subscript + Superscript on the same range must paint unshifted");

    // Overlapping geometric transforms multiply scale_x (0.5 * 2.0 = 1.0) and add skew_x
    // (0.2 + 0.3 = 0.5), matching a single transform(1.0, 0.5) pixel-for-pixel.
    let combined_spans = default_paint(
        (200, 50),
        " aa",
        base.clone(),
        vec![
            AnnotatedRange::new(
                SpanStyle {
                    text_geometric_transform: Some(TextGeometricTransform {
                        scale_x: 0.5,
                        skew_x: 0.2,
                    }),
                    ..SpanStyle::DEFAULT
                },
                0,
                3,
            ),
            AnnotatedRange::new(
                SpanStyle {
                    text_geometric_transform: Some(TextGeometricTransform {
                        scale_x: 2.0,
                        skew_x: 0.3,
                    }),
                    ..SpanStyle::DEFAULT
                },
                0,
                3,
            ),
        ],
        200,
    );
    let single_transform = default_paint(
        (200, 50),
        " aa",
        TextStyle {
            text_geometric_transform: Some(TextGeometricTransform { scale_x: 1.0, skew_x: 0.5 }),
            ..base
        },
        vec![],
        200,
    );
    assert_eq!(combined_spans, single_transform);
}
