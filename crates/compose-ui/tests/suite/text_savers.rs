//! Saving text's values for `remember_saveable`, adapted from Compose's `SaversTest`.

use compose_runtime::saveable::Saveable;
use compose_ui::{
    AnnotatedString, AnnotatedStringBuilder, Annotation, LinkAnnotation, StringAnnotation,
    TextFieldValue, TextLinkStyles, TtsAnnotation,
};
use compose_ui::{
    BaselineShift, Color, EmojiSupportMatch, FontStyle, FontSynthesis, FontWeight, Hyphens,
    LineBreak, LineHeightAlignment, LineHeightMode, LineHeightStyle, LineHeightTrim, Locale,
    LocaleList, Offset, ParagraphStyle, PlatformParagraphStyle, Shadow, SpanStyle, TextAlign,
    TextDecoration, TextDirection, TextGeometricTransform, TextIndent, TextMotion, TextRange,
    TextUnit, UnitExt,
};

#[track_caller]
fn round_trips<T: Saveable + PartialEq + std::fmt::Debug>(original: T) {
    let saved = original.save();
    assert_eq!(T::restore(&saved), Some(original), "saved as {saved:?}");
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextUnit
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextUnit_unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_Offset
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_Offset_Unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_Offset_Infinite
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_Color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_Color_Unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_Shadow
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_Shadow_None
#[test]
fn units_offsets_colors_and_shadows_round_trip() {
    round_trips(2.sp());
    round_trips(1.5.em());
    round_trips(TextUnit::UNSPECIFIED);
    round_trips(Offset::new(10.0, 10.0));
    round_trips(Offset::UNSPECIFIED);
    round_trips(Offset::INFINITE);
    round_trips(Color::YELLOW);
    round_trips(Color::UNSPECIFIED);
    round_trips(Shadow { color: Color::BLUE, offset: Offset::new(5.0, 5.0), blur_radius: 2.0 });
    round_trips(Shadow::NONE);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_ParagraphStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_ParagraphStyle_with_a_nonnull_value
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_ParagraphStyle_with_no_null_value
#[test]
fn paragraph_styles_round_trip() {
    round_trips(ParagraphStyle::DEFAULT);
    round_trips(ParagraphStyle {
        text_direction: Some(TextDirection::Rtl),
        ..ParagraphStyle::DEFAULT
    });
    round_trips(ParagraphStyle {
        text_align: Some(TextAlign::Justify),
        text_direction: Some(TextDirection::Rtl),
        line_height: 10.sp(),
        text_indent: Some(TextIndent { first_line: 2.sp(), rest_line: 3.sp() }),
        platform_style: Some(PlatformParagraphStyle::DEFAULT),
        line_height_style: Some(LineHeightStyle::DEFAULT),
        line_break: Some(LineBreak::PARAGRAPH),
        hyphens: Some(Hyphens::Auto),
        text_motion: Some(TextMotion::Animated),
    });
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_SpanStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_SpanStyle_with_a_nonnull_value
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_SpanStyle_with_no_null_value
#[test]
fn span_styles_round_trip() {
    round_trips(SpanStyle::DEFAULT);
    round_trips(SpanStyle { baseline_shift: Some(BaselineShift::SUBSCRIPT), ..SpanStyle::DEFAULT });
    round_trips(SpanStyle {
        color: Color::RED,
        font_size: 10.sp(),
        font_weight: Some(FontWeight::BOLD),
        font_style: Some(FontStyle::Italic),
        font_synthesis: Some(FontSynthesis::All),
        font_feature_settings: Some("feature settings".into()),
        letter_spacing: 2.em(),
        baseline_shift: Some(BaselineShift::SUPERSCRIPT),
        text_geometric_transform: Some(TextGeometricTransform { scale_x: 2.0, skew_x: 3.0 }),
        locale_list: Some(LocaleList::new([
            Locale::new("sr-Latn-SR"),
            Locale::new("sr-Cyrl-SR"),
            Locale::new("en"),
        ])),
        background: Color::BLUE,
        text_decoration: Some(TextDecoration::LINE_THROUGH),
        shadow: Some(Shadow { color: Color::RED, offset: Offset::new(2.0, 2.0), blur_radius: 4.0 }),
        ..SpanStyle::DEFAULT
    });
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_FontWeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_FontWeight_w100
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_BaselineShift
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_BaselineShift_None
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextDecoration_None
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, testSaveRestore_lineThrough
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, testSaveRestore_underline
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextGeometricTransform
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextGeometricTransform_None
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextIndent
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextIndent_None
#[test]
fn style_values_round_trip() {
    round_trips(FontWeight::new(123));
    round_trips(FontWeight::W100);
    round_trips(BaselineShift(2.0));
    round_trips(BaselineShift::NONE);
    round_trips(TextDecoration::combine(&[
        TextDecoration::LINE_THROUGH,
        TextDecoration::UNDERLINE,
    ]));
    round_trips(TextDecoration::NONE);
    round_trips(TextDecoration::LINE_THROUGH);
    round_trips(TextDecoration::UNDERLINE);
    round_trips(TextGeometricTransform { scale_x: 1.0, skew_x: 2.0 });
    round_trips(TextGeometricTransform::NONE);
    round_trips(TextIndent { first_line: 1.sp(), rest_line: 2.sp() });
    round_trips(TextIndent::NONE);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_Locale
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_LocaleList
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_PlatformParagraphStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_PlatformParagraphStyle_with_no_null_args
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_LineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_LineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextMotion
#[test]
fn locales_platform_styles_and_line_styles_round_trip() {
    round_trips(Locale::new("sr-Latn-SR"));
    round_trips(LocaleList::new([
        Locale::new("sr-Latn-SR"),
        Locale::new("sr-Cyrl-SR"),
        Locale::new("en-US"),
    ]));
    round_trips(PlatformParagraphStyle::DEFAULT);
    round_trips(PlatformParagraphStyle {
        emoji_support_match: EmojiSupportMatch::All,
        include_font_padding: true,
    });
    round_trips(LineHeightStyle {
        alignment: LineHeightAlignment::PROPORTIONAL,
        trim: LineHeightTrim::Both,
        mode: LineHeightMode::Minimum,
    });
    round_trips(LineBreak::PARAGRAPH);
    round_trips(TextMotion::Animated);
}

#[test]
fn text_ranges_round_trip() {
    round_trips(TextRange::new(3, 1));
}

#[test]
fn a_span_styles_font_family_and_brush_are_not_saved_as_composes() {
    let style = SpanStyle {
        font_family: Some(compose_ui::FontFamily::Serif),
        brush: Some(compose_ui::Brush::linear_gradient([Color::RED, Color::BLUE])),
        ..SpanStyle::DEFAULT
    };
    assert_eq!(SpanStyle::restore(&style.save()), Some(SpanStyle::DEFAULT));
}

/// `value` saved, encoded as the platform keeps saved state, decoded and restored.
fn through_bytes<T: Saveable>(value: &T) -> Option<T> {
    use compose_runtime::saveable::SavedState;
    let mut state = SavedState::default();
    state.values.insert(1, vec![value.save()]);
    let decoded = SavedState::from_bytes(&state.to_bytes())?;
    T::restore(&decoded.values.get(&1)?[0])
}

type Set<T> = Box<dyn Fn(&mut T)>;

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/SaverRestorationTest.kt, spanStyle_restoration
#[test]
fn span_styles_survive_saved_state_one_field_after_another() {
    // Each field set in turn, the earlier ones kept, as Compose's test does through
    // `StateRestorationTester` (the family, brush, draw style and platform part aren't saved).
    let fields: Vec<Set<SpanStyle>> = vec![
        Box::new(|style| style.color = Color::GREEN),
        Box::new(|style| style.font_size = 16.sp()),
        Box::new(|style| style.font_weight = Some(FontWeight::new(400))),
        Box::new(|style| style.font_style = Some(FontStyle::Italic)),
        Box::new(|style| style.font_synthesis = Some(FontSynthesis::Weight)),
        Box::new(|style| style.font_feature_settings = Some("settings".into())),
        Box::new(|style| style.letter_spacing = 10.sp()),
        Box::new(|style| style.baseline_shift = Some(BaselineShift::SUBSCRIPT)),
        Box::new(|style| {
            style.text_geometric_transform =
                Some(TextGeometricTransform { scale_x: 2.0, ..TextGeometricTransform::NONE })
        }),
        Box::new(|style| style.locale_list = Some(LocaleList::from_language_tags("en"))),
        Box::new(|style| style.background = Color::YELLOW),
        Box::new(|style| style.text_decoration = Some(TextDecoration::UNDERLINE)),
        Box::new(|style| style.shadow = Some(Shadow { color: Color::RED, ..Shadow::NONE })),
    ];
    let mut style = SpanStyle::DEFAULT;
    for set in fields {
        set(&mut style);
        assert_eq!(through_bytes(&style), Some(style.clone()));
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/SaverRestorationTest.kt, paragraphStyle_restoration
#[test]
fn paragraph_styles_survive_saved_state_one_field_after_another() {
    let fields: Vec<Set<ParagraphStyle>> = vec![
        Box::new(|style| style.text_align = Some(TextAlign::Left)),
        Box::new(|style| style.text_direction = Some(TextDirection::Ltr)),
        Box::new(|style| style.line_height = 24.sp()),
        Box::new(|style| {
            style.text_indent = Some(TextIndent { first_line: 16.sp(), ..TextIndent::NONE })
        }),
        Box::new(|style| {
            style.platform_style = Some(PlatformParagraphStyle {
                emoji_support_match: EmojiSupportMatch::All,
                include_font_padding: false,
            })
        }),
        Box::new(|style| {
            style.line_height_style =
                Some(LineHeightStyle::new(LineHeightAlignment::BOTTOM, LineHeightTrim::Both))
        }),
        Box::new(|style| style.line_break = Some(LineBreak::SIMPLE)),
        Box::new(|style| style.hyphens = Some(Hyphens::Auto)),
        Box::new(|style| style.text_motion = Some(TextMotion::Static)),
    ];
    let mut style = ParagraphStyle::DEFAULT;
    for set in fields {
        set(&mut style);
        assert_eq!(through_bytes(&style), Some(style));
    }
}

fn link_styles(last: SpanStyle) -> TextLinkStyles {
    let colored = |color| Some(SpanStyle { color, ..SpanStyle::DEFAULT });
    TextLinkStyles {
        style: colored(Color::RED),
        focused_style: colored(Color::GREEN),
        hovered_style: colored(Color::BLUE),
        pressed_style: Some(last),
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextLinkStyles
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_TextLinkStyles_withNonNullValues
#[test]
fn link_styles_round_trip() {
    round_trips(TextLinkStyles::default());
    round_trips(link_styles(SpanStyle { color: Color::GRAY, ..SpanStyle::DEFAULT }));
}

fn paragraphs(b: &mut AnnotatedStringBuilder) {
    let justify =
        ParagraphStyle { text_align: Some(TextAlign::Justify), ..ParagraphStyle::DEFAULT };
    b.with_paragraph_style(justify, |b| {
        b.append("1");
    });
    let rtl =
        ParagraphStyle { text_direction: Some(TextDirection::Rtl), ..ParagraphStyle::DEFAULT };
    b.with_paragraph_style(rtl, |b| {
        b.append("2");
    });
}

fn spans(b: &mut AnnotatedStringBuilder) {
    b.with_style(SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT }, |b| {
        b.append("3");
    });
    b.with_style(SpanStyle { font_style: Some(FontStyle::Italic), ..SpanStyle::DEFAULT }, |b| {
        b.append("4");
    });
}

fn annotations(b: &mut AnnotatedStringBuilder) {
    b.with_annotation("Tag1", "Annotation1", |b| {
        b.append("5");
    });
    b.with_tts_annotation(TtsAnnotation::Verbatim("verbatim1".into()), |b| {
        b.append("6");
    });
    b.with_annotation("Tag2", "Annotation2", |b| {
        b.append("7");
    });
    b.with_tts_annotation(TtsAnnotation::Verbatim("verbatim2".into()), |b| {
        b.append("8");
    });
    let white = SpanStyle { color: Color::WHITE, ..SpanStyle::DEFAULT };
    b.with_link(LinkAnnotation::url("url3").with_styles(Some(link_styles(white))), |b| {
        b.append("11");
    });
    let gray = SpanStyle { background: Color::GRAY, ..SpanStyle::DEFAULT };
    let clickable = LinkAnnotation::clickable("tag3", None).with_styles(Some(link_styles(gray)));
    b.with_link(clickable, |b| {
        b.append("12");
    });
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotatedString
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotatedString_withSpanStyles
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotatedString_withParagraphStyles
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotatedString_withAnnotations
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotatedString_withSpanAndParagraphStylesAndAnnotations
// (the deprecated `UrlAnnotation`s of the last two have no counterpart)
#[test]
fn annotated_strings_round_trip() {
    let texts = [
        AnnotatedString::new("abc"),
        AnnotatedString::build(spans),
        AnnotatedString::build(paragraphs),
        AnnotatedString::build(annotations),
        AnnotatedString::build(|b| {
            paragraphs(b);
            spans(b);
            annotations(b);
        }),
    ];
    for text in texts {
        round_trips(text.clone());
        assert_eq!(through_bytes(&text), Some(text));
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotationSaver_ParagraphStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotationSaver_SpanStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotationSaver_VerbatimTtsAnnotation
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotationSaver_LinkAnnotationUrl
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotationSaver_LinkAnnotationClickable
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SaversTest.kt, test_AnnotationSaver_StringAnnotation
// (test_AnnotationSaver_UrlAnnotation tests the deprecated `UrlAnnotation`)
#[test]
fn annotations_round_trip() {
    let red = SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT };
    let styles = TextLinkStyles { style: Some(red.clone()), ..Default::default() };
    let centered =
        ParagraphStyle { text_align: Some(TextAlign::Center), ..ParagraphStyle::DEFAULT };
    for annotation in [
        Annotation::ParagraphStyle(centered),
        Annotation::SpanStyle(red),
        Annotation::Tts(TtsAnnotation::Verbatim("verbatim".into())),
        Annotation::Link(LinkAnnotation::url("url").with_styles(Some(styles.clone()))),
        Annotation::Link(LinkAnnotation::clickable("tag", None).with_styles(Some(styles))),
        Annotation::StringAnnotation(StringAnnotation("string".into())),
    ] {
        round_trips(annotation);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, test_Saver
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, test_Saver_defaultInstance
#[test]
fn text_field_values_save_their_text_and_selection() {
    let text = AnnotatedString::build(|b| {
        paragraphs(b);
        spans(b);
        annotations(b);
    });
    let original =
        TextFieldValue::new(text.clone(), TextRange::new(1, 2), Some(TextRange::new(3, 4)));
    let restored = TextFieldValue::restore(&original.save());
    assert_eq!(restored, Some(TextFieldValue::new(text, TextRange::new(1, 2), None)));
    round_trips(TextFieldValue::default());
}
