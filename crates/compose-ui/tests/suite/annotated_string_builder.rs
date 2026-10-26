//! `AnnotatedStringBuilder`, adapted from Compose's `AnnotatedStringBuilderTest`. Ranges are
//! bytes (D1100); the texts here are ASCII.

use compose_ui::{
    AnnotatedRange, AnnotatedString, AnnotatedStringBuilder, Annotation, Bullet, Color, FontStyle,
    FontWeight, LinkAnnotation, LinkInteractionListener, ParagraphStyle, SharedString, SpanStyle,
    StringAnnotation, TextAlign, TextIndent, TextUnit, TtsAnnotation, UnitExt,
};

fn range<T>(item: T, start: usize, end: usize) -> AnnotatedRange<T> {
    AnnotatedRange::new(item, start, end)
}

fn color(color: Color) -> SpanStyle {
    SpanStyle { color, ..SpanStyle::DEFAULT }
}

fn line_height(height: f32) -> ParagraphStyle {
    ParagraphStyle { line_height: height.sp(), ..ParagraphStyle::DEFAULT }
}

fn indent(by: TextUnit) -> ParagraphStyle {
    ParagraphStyle {
        text_indent: Some(TextIndent { first_line: by, rest_line: by }),
        ..ParagraphStyle::DEFAULT
    }
}

/// `text` in `color` over a paragraph `line_height` high, as the Compose test's helper.
fn styled(text: &str, color: Color, line_height_sp: f32) -> AnnotatedString {
    AnnotatedString::with_styles(
        text.to_string(),
        [range(self::color(color), 0, text.len())],
        [range(line_height(line_height_sp), 0, text.len())],
    )
}

/// The range from `start` to `end` in `text`, both included, as the Compose test's helper.
fn inclusive<T>(text: &str, start: char, end: char, item: T, tag: &str) -> AnnotatedRange<T> {
    AnnotatedRange::tagged(
        item,
        text.find(start).unwrap(),
        text.find(end).unwrap() + 1,
        tag.to_string(),
    )
}

fn string_annotation(value: &str) -> Annotation {
    Annotation::StringAnnotation(StringAnnotation(value.to_string().into()))
}

fn strings(values: &[(&str, usize, usize, &str)]) -> Vec<AnnotatedRange<SharedString>> {
    values
        .iter()
        .map(|&(value, start, end, tag)| {
            AnnotatedRange::tagged(
                SharedString::from(value.to_string()),
                start,
                end,
                tag.to_string(),
            )
        })
        .collect()
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, defaultConstructor
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, constructorWithString
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, constructorWithAnnotatedString_hasSameAnnotatedStringAttributes
#[test]
fn builders_start_empty_or_from_a_text() {
    let empty = AnnotatedStringBuilder::new().to_annotated_string();
    assert!(
        empty.is_empty() && empty.span_styles().is_empty() && empty.paragraph_styles().is_empty()
    );
    let text = AnnotatedStringBuilder::from("a").to_annotated_string();
    assert_eq!(text.as_str(), "a");
    assert!(text.span_styles().is_empty() && text.paragraph_styles().is_empty());
    let original = styled("a", Color::RED, 20.0);
    let copy = AnnotatedStringBuilder::from(original.clone()).to_annotated_string();
    assert_eq!(copy.as_str(), original.as_str());
    assert_eq!(copy.span_styles(), original.span_styles());
    assert_eq!(copy.paragraph_styles(), original.paragraph_styles());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, addStyle_withSpanStyle_addsStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, addStyle_withParagraphStyle_addsStyle
#[test]
fn added_styles_cover_their_range() {
    let text =
        AnnotatedStringBuilder::from("ab").add_style(color(Color::RED), 0, 1).to_annotated_string();
    assert!(text.paragraph_styles().is_empty());
    assert_eq!(text.span_styles(), vec![range(color(Color::RED), 0, 1)]);
    let text = AnnotatedStringBuilder::from("ab")
        .add_paragraph_style(line_height(30.0), 0, 1)
        .to_annotated_string();
    assert!(text.span_styles().is_empty());
    assert_eq!(text.paragraph_styles(), vec![range(line_height(30.0), 0, 1)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, append_withString_appendsTheText
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, append_withString_andMultipleCalls_appendsAllOfTheText
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, append_withCharSequence_appendsTheText_whenNotAnnotatedString
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, append_withCharSequenceAndRange_appendsTheText_whenNotAnnotatedString
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, append_char_appends
#[test]
fn appended_text_follows() {
    let text = AnnotatedStringBuilder::from("a").append("b").to_annotated_string();
    assert_eq!(text.as_str(), "ab");
    assert!(text.span_styles().is_empty() && text.paragraph_styles().is_empty());
    assert_eq!(
        AnnotatedStringBuilder::from("a").append("b").append("c").to_annotated_string().as_str(),
        "abc"
    );
    // Compose's `CharSequence` overloads: a plain string, a part of one.
    assert_eq!(
        AnnotatedStringBuilder::from("a").append("bc").to_annotated_string().as_str(),
        "abc"
    );
    assert_eq!(
        AnnotatedStringBuilder::from("a").append(&"bcde"[1..3]).to_annotated_string().as_str(),
        "acd"
    );
    let text =
        AnnotatedStringBuilder::from("a").append_char('b').append_char('c').to_annotated_string();
    assert_eq!(text, AnnotatedString::new("abc"));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, append_withAnnotatedString_appendsTheText
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, append_withCharSequence_appendsTheText_whenAnnotatedString
#[test]
fn an_appended_annotated_string_brings_its_styles() {
    let result = AnnotatedStringBuilder::from(styled("a", Color::RED, 20.0))
        .append_annotated(&styled("b", Color::BLUE, 30.0))
        .to_annotated_string();
    assert_eq!(result.as_str(), "ab");
    assert_eq!(
        result.span_styles(),
        vec![range(color(Color::RED), 0, 1), range(color(Color::BLUE), 1, 2)]
    );
    assert_eq!(
        result.paragraph_styles(),
        vec![range(line_height(20.0), 0, 1), range(line_height(30.0), 1, 2)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, append_withAnnotatedStringAndRange_appendsTheText
#[test]
fn a_part_of_an_annotated_string_brings_the_annotations_over_it() {
    let text = AnnotatedString::with_annotations(
        "a",
        [
            inclusive("a", 'a', 'a', Annotation::SpanStyle(color(Color::RED)), ""),
            inclusive("a", 'a', 'a', Annotation::ParagraphStyle(line_height(20.0)), ""),
            inclusive("a", 'a', 'a', string_annotation("prefix"), "prefixTag"),
        ],
    );
    let appended = "bcdef";
    let mut annotations = vec![
        inclusive(appended, 'b', 'f', Annotation::SpanStyle(color(Color::BLUE)), ""),
        inclusive(appended, 'c', 'f', Annotation::SpanStyle(color(Color::GREEN)), ""),
        inclusive(appended, 'b', 'e', Annotation::SpanStyle(color(Color::YELLOW)), ""),
        inclusive(appended, 'c', 'e', Annotation::SpanStyle(color(Color::MAGENTA)), ""),
    ];
    for (letter, height) in [('b', 30.0), ('c', 40.0), ('d', 50.0), ('e', 60.0), ('f', 70.0)] {
        annotations.push(inclusive(
            appended,
            letter,
            letter,
            Annotation::ParagraphStyle(line_height(height)),
            "",
        ));
    }
    for (start, value, tag) in [('b', "1", "tag1"), ('c', "2", "tag2")] {
        annotations.push(inclusive(appended, start, 'f', string_annotation(value), tag));
    }
    for (start, value, tag) in [('b', "3", "tag3"), ('c', "4", "tag4")] {
        annotations.push(inclusive(appended, start, 'e', string_annotation(value), tag));
    }
    let appended_text = AnnotatedString::with_annotations(appended, annotations);
    let result = AnnotatedStringBuilder::from(text)
        .append_annotated_range(&appended_text, 1, 4)
        .to_annotated_string();
    let expected = "acde";
    assert_eq!(result.as_str(), expected);
    let mut spans = vec![inclusive(expected, 'a', 'a', color(Color::RED), "")];
    for each in [Color::BLUE, Color::GREEN, Color::YELLOW, Color::MAGENTA] {
        spans.push(inclusive(expected, 'c', 'e', color(each), ""));
    }
    assert_eq!(result.span_styles(), spans);
    let paragraphs: Vec<_> = [('a', 20.0), ('c', 40.0), ('d', 50.0), ('e', 60.0)]
        .into_iter()
        .map(|(letter, height)| inclusive(expected, letter, letter, line_height(height), ""))
        .collect();
    assert_eq!(result.paragraph_styles(), paragraphs);
    let others: Vec<_> = result
        .annotations()
        .iter()
        .filter(|range| {
            !matches!(range.item, Annotation::SpanStyle(_) | Annotation::ParagraphStyle(_))
        })
        .cloned()
        .collect();
    let mut expected_others =
        vec![inclusive(expected, 'a', 'a', string_annotation("prefix"), "prefixTag")];
    for (value, tag) in [("1", "tag1"), ("2", "tag2"), ("3", "tag3"), ("4", "tag4")] {
        expected_others.push(inclusive(expected, 'c', 'e', string_annotation(value), tag));
    }
    assert_eq!(others, expected_others);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, append_withCharSequenceAndRange_appendsTheText_whenAnnotatedString
#[test]
fn a_part_of_a_built_string_brings_its_nested_annotations() {
    let appended = AnnotatedString::build(|b| {
        b.with_style(color(Color::BLUE), |b| {
            b.append("b(");
            b.with_paragraph_style(line_height(30.0), |b| {
                b.append("c(");
                b.with_annotation("tag", "annotation", |b| {
                    b.append("de");
                });
                b.append(")f");
            });
            b.append(")g");
        });
    });
    let text = "b(c(de)f)g";
    let result = AnnotatedStringBuilder::from(styled("a", Color::RED, 20.0))
        .append_annotated_range(&appended, text.find('c').unwrap(), text.find('f').unwrap() + 1)
        .to_annotated_string();
    let expected = "ac(de)f";
    assert_eq!(result.as_str(), expected);
    assert_eq!(
        result.span_styles(),
        vec![range(color(Color::RED), 0, 1), range(color(Color::BLUE), 1, 7)]
    );
    assert_eq!(
        result.paragraph_styles(),
        vec![range(line_height(20.0), 0, 1), range(line_height(30.0), 1, 7)]
    );
    assert_eq!(
        result.annotations().to_vec(),
        vec![
            range(Annotation::SpanStyle(color(Color::RED)), 0, 1),
            range(Annotation::ParagraphStyle(line_height(20.0)), 0, 1),
            range(Annotation::SpanStyle(color(Color::BLUE)), 1, 7),
            range(Annotation::ParagraphStyle(line_height(30.0)), 1, 7),
            AnnotatedRange::tagged(string_annotation("annotation"), 3, 5, "tag"),
        ]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushStyle_without_pop
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushStyle_with_multiple_styles
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushStyle_with_multiple_styles_on_top_of_each_other
#[test]
fn pushed_styles_cover_what_is_appended_until_popped() {
    let mut builder = AnnotatedStringBuilder::new();
    builder.push_style(color(Color::RED));
    builder.append("Test").pop();
    let result = builder.to_annotated_string();
    assert_eq!(result.span_styles(), vec![range(color(Color::RED), 0, 4)]);

    let styles = [
        color(Color::RED),
        SpanStyle { font_style: Some(FontStyle::Italic), ..SpanStyle::DEFAULT },
        SpanStyle { font_weight: Some(FontWeight::BOLD), ..SpanStyle::DEFAULT },
    ];
    let mut builder = AnnotatedStringBuilder::new();
    for (index, style) in styles.iter().enumerate() {
        builder.push_style(style.clone());
        builder.append(&format!("Style{index}"));
    }
    let result = builder.to_annotated_string();
    assert_eq!(result.as_str(), "Style0Style1Style2");
    let starts: Vec<_> =
        result.span_styles().iter().map(|range| (range.start, range.end)).collect();
    assert_eq!(starts, vec![(0, 18), (6, 18), (12, 18)]);

    let mut builder = AnnotatedStringBuilder::new();
    builder.push_style(styles[0].clone());
    builder.append("Test");
    builder.push_style(styles[1].clone());
    builder.append(" me").pop().pop();
    let result = builder.to_annotated_string();
    assert_eq!(
        result.span_styles(),
        vec![range(styles[0].clone(), 0, 7), range(styles[1].clone(), 4, 7)]
    );

    let mut builder = AnnotatedStringBuilder::new();
    for style in &styles {
        builder.push_style(style.clone());
    }
    let result = builder.to_annotated_string();
    assert!(result.is_empty());
    assert_eq!(
        result.span_styles(),
        styles.iter().map(|style| range(style.clone(), 0, 0)).collect::<Vec<_>>()
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushStyle_with_multiple_stacks_should_construct_styles_in_the_same_order
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushStyle_with_multiple_nested_styles_should_return_styles_in_same_order
#[test]
fn pushed_styles_keep_their_push_order() {
    let styles = [
        color(Color::RED),
        SpanStyle { font_style: Some(FontStyle::Italic), ..SpanStyle::DEFAULT },
        SpanStyle { font_weight: Some(FontWeight::BOLD), ..SpanStyle::DEFAULT },
        SpanStyle { letter_spacing: 1.2.em(), ..SpanStyle::DEFAULT },
    ];
    let mut builder = AnnotatedStringBuilder::new();
    builder.push_style(styles[0].clone());
    builder.append("layer1-1");
    builder.push_style(styles[1].clone());
    builder.append("layer2-1");
    builder.push_style(styles[2].clone());
    builder.append("layer3-1").pop();
    builder.push_style(styles[3].clone());
    builder.append("layer3-2").pop().append("layer2-2").pop().append("layer1-2");
    let order: Vec<_> =
        builder.to_annotated_string().span_styles().into_iter().map(|range| range.item).collect();
    assert_eq!(order, styles.to_vec());

    let mut builder = AnnotatedStringBuilder::new();
    builder.push_style(styles[0].clone());
    builder.append("layer1-1");
    builder.push_style(styles[1].clone());
    builder.append("layer2-1").pop();
    builder.push_style(styles[2].clone());
    builder.append("layer2-2");
    builder.push_style(styles[3].clone());
    builder.append("layer3-1").pop().append("layer2-3").pop().append("layer1-2").pop();
    let order: Vec<_> =
        builder.to_annotated_string().span_styles().into_iter().map(|range| range.item).collect();
    assert_eq!(order, styles.to_vec());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pop_when_empty_does_not_throw_exception
#[test]
#[should_panic(expected = "Nothing to pop.")]
fn popping_nothing_panics() {
    AnnotatedStringBuilder::new().pop();
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pop_in_the_middle
#[test]
fn a_pop_ends_the_style_where_the_text_is() {
    let italic = SpanStyle { font_style: Some(FontStyle::Italic), ..SpanStyle::DEFAULT };
    let mut builder = AnnotatedStringBuilder::new();
    builder.append("Style0");
    builder.push_style(color(Color::RED));
    builder.append("Style1").pop();
    builder.push_style(italic.clone());
    builder.append("Style2").pop().append("Style3");
    let result = builder.to_annotated_string();
    assert_eq!(result.as_str(), "Style0Style1Style2Style3");
    assert_eq!(result.span_styles(), vec![range(color(Color::RED), 6, 12), range(italic, 12, 18)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, push_increments_the_style_index
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, push_reduces_the_style_index_after_pop
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pop_until_index_pops_correctly
#[test]
fn pushes_return_their_depth() {
    let mut builder = AnnotatedStringBuilder::new();
    let indices: Vec<_> = (0..3).map(|_| builder.push_style(color(Color::RED))).collect();
    assert_eq!(indices, vec![0, 1, 2]);
    let mut builder = AnnotatedStringBuilder::new();
    assert_eq!(
        (builder.push_style(color(Color::RED)), builder.push_style(color(Color::RED))),
        (0, 1)
    );
    builder.pop();
    assert_eq!(builder.push_paragraph_style(line_height(18.0)), 1);
    let mut builder = AnnotatedStringBuilder::new();
    builder.push_style(color(Color::RED));
    let second = builder.push_style(color(Color::RED));
    builder.push_style(color(Color::RED));
    builder.pop_to(second);
    assert_eq!(builder.push_style(color(Color::RED)), second);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pop_until_throws_exception_for_invalid_index
#[test]
#[should_panic(expected = "should be less than")]
fn popping_to_an_index_not_pushed_panics() {
    let mut builder = AnnotatedStringBuilder::new();
    let index = builder.push_style(color(Color::RED));
    builder.pop_to(index + 1);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, withStyle_applies_style_to_block
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, withStyle_with_paragraphStyle_applies_style_to_block
#[test]
fn with_style_covers_what_the_block_appends() {
    let result = AnnotatedString::build(|b| {
        b.with_style(color(Color::RED), |b| {
            b.append("Style");
        });
    });
    assert!(result.paragraph_styles().is_empty());
    assert_eq!(result.span_styles(), vec![range(color(Color::RED), 0, 5)]);
    let result = AnnotatedString::build(|b| {
        b.with_paragraph_style(line_height(18.0), |b| {
            b.append("Style");
        });
    });
    assert!(result.span_styles().is_empty());
    assert_eq!(result.paragraph_styles(), vec![range(line_height(18.0), 0, 5)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, builderLambda
#[test]
fn the_build_function_builds() {
    let right = ParagraphStyle { text_align: Some(TextAlign::Right), ..ParagraphStyle::DEFAULT };
    let result = AnnotatedString::build(|b| {
        b.with_paragraph_style(right, |b| {
            b.with_style(color(Color::RED), |b| {
                b.append("Hello");
            });
        });
        b.append(" ");
        b.push_paragraph_style(right);
        b.push_style(color(Color::BLUE));
        b.append("World").pop();
    });
    assert_eq!(result.as_str(), "Hello World");
    assert_eq!(
        result.span_styles(),
        vec![range(color(Color::RED), 0, 5), range(color(Color::BLUE), 6, 11)]
    );
    assert_eq!(result.paragraph_styles(), vec![range(right, 0, 5), range(right, 6, 11)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, toAnnotatedString_calling_twice_creates_equal_annotated_strings
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, can_call_other_functions_after_toAnnotatedString
#[test]
fn building_leaves_the_builder_usable() {
    let mut builder = AnnotatedStringBuilder::new();
    builder.push_style(color(Color::RED));
    builder.append("Hello");
    builder.push_style(color(Color::BLUE));
    builder.append("World");
    assert_eq!(builder.to_annotated_string(), builder.to_annotated_string());

    let size = |size: f32| SpanStyle { font_size: size.sp(), ..SpanStyle::DEFAULT };
    let mut builder = AnnotatedStringBuilder::new();
    builder.push_style(size(12.0));
    builder.append("Hello");
    builder.push_style(size(16.0));
    builder.append("World");
    let first = builder.to_annotated_string();
    builder.pop().pop();
    builder.push_style(size(18.0));
    let second = builder.append("!").to_annotated_string();
    let mut expected = AnnotatedStringBuilder::from(first);
    expected.with_style(size(18.0), |b| {
        b.append("!");
    });
    assert_eq!(second, expected.to_annotated_string());
}

fn hello_world_annotations(
    first: impl FnOnce(&mut AnnotatedStringBuilder) -> usize,
    second: impl FnOnce(&mut AnnotatedStringBuilder) -> usize,
) -> AnnotatedString {
    let mut builder = AnnotatedStringBuilder::new();
    first(&mut builder);
    builder.append("Hello");
    second(&mut builder);
    builder.append("world").pop().append("!").pop();
    builder.to_annotated_string()
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushAnnotation
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, hasStringAnnotationTrue
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, hasStringAnnotationFalse
#[test]
fn pushed_string_annotations_are_found_by_tag() {
    let mut builder = AnnotatedStringBuilder::new();
    builder.push_string_annotation("tag", "Annotation");
    builder.append("Test").pop();
    let result = builder.to_annotated_string();
    assert_eq!(
        result.get_string_annotations_with_tag("tag", 0, 4),
        strings(&[("Annotation", 0, 4, "tag")])
    );
    assert!(result.has_string_annotations("tag", 0, 4));
    let result = AnnotatedStringBuilder::from(result).append("Test").to_annotated_string();
    assert!(!result.has_string_annotations("tag", 4, 8));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushAnnotation_multiple_nested
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushAnnotation_multiple_differentTag
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, getAnnotation_withoutTag_multipleAnnotations
#[test]
fn nested_string_annotations_are_found_where_they_intersect() {
    let result = hello_world_annotations(
        |b| b.push_string_annotation("tag", "Annotation1"),
        |b| b.push_string_annotation("tag", "Annotation2"),
    );
    assert_eq!(result.as_str(), "Helloworld!");
    let count = |start, end| result.get_string_annotations_with_tag("tag", start, end).len();
    assert_eq!((count(0, 11), count(0, 5), count(5, 10), count(10, 11)), (2, 1, 2, 1));
    assert_eq!(
        result.get_string_annotations_with_tag("tag", 0, 11),
        strings(&[("Annotation1", 0, 11, "tag"), ("Annotation2", 5, 10, "tag")])
    );
    let result = hello_world_annotations(
        |b| b.push_string_annotation("tag1", "Annotation1"),
        |b| b.push_string_annotation("tag2", "Annotation2"),
    );
    let with = |tag, start, end| result.get_string_annotations_with_tag(tag, start, end);
    assert_eq!((with("tag1", 0, 11).len(), with("tag1", 0, 5).len()), (1, 1));
    assert_eq!(with("tag1", 5, 10), strings(&[("Annotation1", 0, 11, "tag1")]));
    assert_eq!(with("tag2", 5, 10), strings(&[("Annotation2", 5, 10, "tag2")]));
    assert!(with("tag2", 10, 11).is_empty());
    assert_eq!(result.get_string_annotations(0, 5), strings(&[("Annotation1", 0, 11, "tag1")]));
    assert_eq!(
        result.get_string_annotations(5, 6),
        strings(&[("Annotation1", 0, 11, "tag1"), ("Annotation2", 5, 10, "tag2")])
    );
    assert_eq!(result.get_string_annotations(10, 11), strings(&[("Annotation1", 0, 11, "tag1")]));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, getAnnotation
#[test]
fn string_annotations_are_found_where_they_intersect_or_start() {
    let mut builder = AnnotatedStringBuilder::new();
    builder.append("Hello");
    builder.push_string_annotation("tag", "Annotation");
    builder.append("World").pop().append("Hello");
    builder.push_string_annotation("tag", "Annotation");
    builder.pop().append("World");
    let result = builder.to_annotated_string();
    let count = |start, end| result.get_string_annotations_with_tag("tag", start, end).len();
    assert_eq!(
        [
            count(0, 5),
            count(0, 6),
            count(6, 6),
            count(10, 10),
            count(8, 13),
            count(15, 15),
            count(10, 15),
            count(15, 20)
        ],
        [0, 1, 1, 0, 1, 1, 0, 1]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, getAnnotation_separates_ttsAnnotation_and_stringAnnotation
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, getAnnotation_withTag_withTtsAnnotation_withStringAnnotation
#[test]
fn tts_and_string_annotations_are_apart() {
    let verbatim = TtsAnnotation::Verbatim("abc".into());
    let result = hello_world_annotations(
        |b| b.push_tts_annotation(verbatim.clone()),
        |b| b.push_string_annotation("tag", "annotation"),
    );
    assert_eq!(result.get_tts_annotations(0, 5), vec![range(verbatim.clone(), 0, 11)]);
    assert_eq!(result.get_tts_annotations(5, 6), vec![range(verbatim, 0, 11)]);
    assert!(result.get_string_annotations(0, 5).is_empty());
    assert_eq!(result.get_string_annotations(5, 6), strings(&[("annotation", 5, 10, "tag")]));
    assert!(result.get_string_annotations(10, 11).is_empty());
    assert!(result.get_string_annotations_with_tag("tag", 0, 5).is_empty());
    assert_eq!(
        result.get_string_annotations_with_tag("tag", 5, 6),
        strings(&[("annotation", 5, 10, "tag")])
    );
    assert!(result.get_string_annotations_with_tag("tag1", 5, 6).is_empty());
    assert!(result.get_string_annotations_with_tag("tag", 10, 11).is_empty());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, getAnnotation_separates_linkAnnotation_and_stringAnnotation
// (getAnnotation_separates_urlAnnotation_and_stringAnnotation and
// getAnnotation_withTag_withUrlAnnotation_withStringAnnotation test the deprecated `UrlAnnotation`)
#[test]
fn links_and_string_annotations_are_apart() {
    let url = LinkAnnotation::url("url");
    let clickable =
        LinkAnnotation::clickable("clickable tag", Some(LinkInteractionListener::new(|_| {})));
    let mut builder = AnnotatedStringBuilder::new();
    builder.push_link(url.clone());
    builder.append("ab");
    builder.push_link(clickable.clone());
    builder.append("cd");
    builder.push_string_annotation("tag", "annotation");
    builder.append("ef").pop();
    let result = builder.to_annotated_string();
    assert_eq!(result.get_link_annotations(0, 2), vec![range(url.clone(), 0, 6)]);
    assert_eq!(result.get_link_annotations(3, 5), vec![range(url, 0, 6), range(clickable, 2, 6)]);
    assert!(result.has_link_annotations(0, 2));
    assert!(result.get_string_annotations(0, 4).is_empty());
    assert_eq!(result.get_string_annotations(4, 6), strings(&[("annotation", 4, 6, "tag")]));
    assert!(result.get_string_annotations_with_tag("another tag", 4, 6).is_empty());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, pushBullet
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, addBullet
#[test]
fn bullets_are_annotations() {
    let mut builder = AnnotatedStringBuilder::new();
    builder.push_bullet(Bullet::default_bullet());
    let result = builder.append("text").pop().to_annotated_string();
    assert_eq!(result.as_str(), "text");
    assert_eq!(
        result.annotations().to_vec(),
        vec![range(Annotation::Bullet(Bullet::default_bullet()), 0, 4)]
    );
    let result = AnnotatedStringBuilder::from("text text")
        .add_bullet(Bullet::default_bullet(), 0, 4)
        .to_annotated_string();
    assert_eq!(
        result.annotations().to_vec(),
        vec![range(Annotation::Bullet(Bullet::default_bullet()), 0, 4)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, withBulletList_nestedIndentation
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, withBulletListItem_getsSettingsFromList
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringBuilderTest.kt, withBulletListItem_nested_getsSettingsFromList
#[test]
fn bullet_lists_indent_their_items() {
    let result = AnnotatedString::build(|b| {
        b.with_bullet_list(10.sp(), Bullet::default_bullet(), |b| {
            b.append("text");
            b.with_bullet_list(5.sp(), Bullet::default_bullet(), |b| {
                b.append("text");
            });
        });
    });
    assert_eq!(
        result.paragraph_styles(),
        vec![range(indent(10.sp()), 0, 8), range(indent(15.sp()), 4, 8)]
    );
    let result = AnnotatedString::build(|b| {
        b.with_bullet_list(10.sp(), Bullet::default_bullet(), |b| {
            b.with_bullet_list_item(None, |b| {
                b.append("text");
            });
        });
    });
    let bullet = || Annotation::Bullet(Bullet::default_bullet());
    let paragraph = |by: f32| Annotation::ParagraphStyle(indent(by.sp()));
    assert_eq!(
        result.annotations().to_vec(),
        vec![range(paragraph(10.0), 0, 4), range(paragraph(10.0), 0, 4), range(bullet(), 0, 4)]
    );
    let result = AnnotatedString::build(|b| {
        b.with_bullet_list(10.sp(), Bullet::default_bullet(), |b| {
            b.with_bullet_list_item(None, |b| {
                b.append("text");
            });
            b.with_bullet_list(5.sp(), Bullet::default_bullet(), |b| {
                b.with_bullet_list_item(None, |b| {
                    b.append("text");
                });
            });
        });
    });
    assert_eq!(
        result.annotations().to_vec(),
        vec![
            range(paragraph(10.0), 0, 8),
            range(paragraph(10.0), 0, 4),
            range(bullet(), 0, 4),
            range(paragraph(15.0), 4, 8),
            range(paragraph(15.0), 4, 8),
            range(bullet(), 4, 8),
        ]
    );
}
