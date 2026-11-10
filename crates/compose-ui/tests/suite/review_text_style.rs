//! Adversarial review of `w2-text-style` (`D2300`, `D2425`): `AnnotatedString::runs` shift and
//! transform accumulation across nested `sp`/`em` font sizes, cancellation, inapplicable shifts,
//! non-overlapping run isolation, and `resolve_letter_spacing_px` with zero and non-finite scales.

use compose_ui::{
    AnnotatedString, BaselineShift, Density, SpanStyle, TextGeometricTransform, TextStyle, UnitExt,
    platform::{font_features, resolve_baseline_shift_px, resolve_letter_spacing_px},
};

#[test]
fn nested_em_and_sp_font_sizes_shift_at_each_span_s_prior_size_as_android_spans() {
    // Base is 20sp.
    // Outer span sets fontSize = 2.0.em() (40sp) AND baselineShift = Superscript (+0.5):
    // Android attaches `BaselineShiftSpan` BEFORE `RelativeSizeSpan`, so outer shift uses 20sp
    // (+10sp).
    // Inner span sets fontSize = 0.5.em() (20sp) AND baselineShift = Subscript (-0.5):
    // inner shift sees 40sp (-20sp).
    // Net shift is -10sp over final 20sp => BaselineShift(-0.5), NOT cancelled to 0!
    let text = AnnotatedString::build(|builder| {
        builder.with_style(
            SpanStyle {
                font_size: 2.0.em(),
                baseline_shift: Some(BaselineShift::SUPERSCRIPT),
                ..SpanStyle::DEFAULT
            },
            |builder| {
                builder.with_style(
                    SpanStyle {
                        font_size: 0.5.em(),
                        baseline_shift: Some(BaselineShift::SUBSCRIPT),
                        ..SpanStyle::DEFAULT
                    },
                    |builder| {
                        builder.append("ab");
                    },
                );
                builder.append("cd");
            },
        );
        builder.append("ef");
    });

    let base = TextStyle { font_size: 20.0.sp(), ..TextStyle::DEFAULT };
    let runs = text.runs(&base);
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].0, 0..2);
    assert_eq!(runs[0].1.font_size, 20.0.sp());
    assert_eq!(runs[0].1.baseline_shift, Some(BaselineShift(-0.5)));

    // "cd" has 40sp final size and +10sp shift => multiplier +0.25.
    assert_eq!(runs[1].0, 2..4);
    assert_eq!(runs[1].1.font_size, 40.0.sp());
    assert_eq!(runs[1].1.baseline_shift, Some(BaselineShift(0.25)));

    // "ef" is unshifted and untransformed (adjacent runs never leak state).
    assert_eq!(runs[2].0, 4..6);
    assert_eq!(runs[2].1.font_size, 20.0.sp());
    assert_eq!(runs[2].1.baseline_shift, None);
}

#[test]
fn base_text_style_shift_and_transform_combine_and_cancel_with_spans_and_ignore_non_applicable() {
    let base = TextStyle {
        font_size: 20.0.sp(),
        baseline_shift: Some(BaselineShift::SUPERSCRIPT),
        text_geometric_transform: Some(TextGeometricTransform { scale_x: 2.0, skew_x: -0.25 }),
        ..TextStyle::DEFAULT
    };
    let text = AnnotatedString::build(|builder| {
        // Cancels base Superscript (+0.5 + -0.5 = 0 -> None), combines transform (2*0.5=1, -0.25+0.25=0).
        builder.with_style(
            SpanStyle {
                baseline_shift: Some(BaselineShift::SUBSCRIPT),
                text_geometric_transform: Some(TextGeometricTransform {
                    scale_x: 0.5,
                    skew_x: 0.25,
                }),
                ..SpanStyle::DEFAULT
            },
            |builder| {
                builder.append("ab");
            },
        );
        // Inapplicable shifts (NONE, UNSPECIFIED, ±Infinity) do not reset base Superscript.
        for (chunk, shift) in [
            ("cd", BaselineShift::NONE),
            ("ef", BaselineShift::UNSPECIFIED),
            ("gh", BaselineShift(f32::INFINITY)),
            ("ij", BaselineShift(f32::NEG_INFINITY)),
        ] {
            builder.with_style(
                SpanStyle { baseline_shift: Some(shift), ..SpanStyle::DEFAULT },
                |builder| {
                    builder.append(chunk);
                },
            );
        }
    });

    let runs = text.runs(&base);
    assert_eq!(runs.len(), 5);
    assert_eq!(runs[0].1.baseline_shift, None);
    assert_eq!(runs[0].1.text_geometric_transform, Some(TextGeometricTransform::NONE));
    for run in &runs[1..] {
        assert_eq!(run.1.baseline_shift, Some(BaselineShift::SUPERSCRIPT));
        assert_eq!(
            run.1.text_geometric_transform,
            Some(TextGeometricTransform { scale_x: 2.0, skew_x: -0.25 })
        );
    }
}

#[test]
#[ignore = "review: non-finite scale_x poisons em letter spacing with NaN/Infinity"]
fn letter_spacing_baseline_shift_and_font_features_handle_zero_and_non_finite_values_safely() {
    let d = Density::new(2.0);
    for bad_scale in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let style = TextStyle {
            letter_spacing: 2.0.em(),
            text_geometric_transform: Some(TextGeometricTransform {
                scale_x: bad_scale,
                skew_x: 0.0,
            }),
            ..TextStyle::DEFAULT
        };
        let resolved = resolve_letter_spacing_px(&style, 20.0, &d).unwrap();
        assert!(
            resolved.is_finite(),
            "non-finite scale_x must not poison letter spacing: {resolved}"
        );
        assert_eq!(resolved, 40.0);
    }

    let zero_scale = TextStyle {
        letter_spacing: 2.0.em(),
        text_geometric_transform: Some(TextGeometricTransform { scale_x: 0.0, skew_x: 0.5 }),
        ..TextStyle::DEFAULT
    };
    assert_eq!(resolve_letter_spacing_px(&zero_scale, 20.0, &d), Some(0.0));
    let zero_scale_sp = TextStyle { letter_spacing: 3.0.sp(), ..zero_scale };
    assert_eq!(resolve_letter_spacing_px(&zero_scale_sp, 20.0, &d), Some(6.0));

    assert_eq!(
        resolve_baseline_shift_px(
            &TextStyle { baseline_shift: Some(BaselineShift(f32::NAN)), ..TextStyle::DEFAULT },
            -20.0
        ),
        0.0
    );
    // Multibyte UTF-8 and malformed quotes in font_features never panic.
    assert!(font_features("\"ä\" 1, 'unclosed, normal, \"\" 1, \"kern\" 9999999999").is_empty());
}
