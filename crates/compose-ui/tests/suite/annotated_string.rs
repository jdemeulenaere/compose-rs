//! `AnnotatedString`, adapted from Compose's `AnnotatedStringTest` and `LinkAnnotationTest`.
//! Ranges are bytes (D1100); the texts here are ASCII, so they're Compose's indices.

use compose_ui::{
    AnnotatedRange, AnnotatedString, AnnotatedStringBuilder, Annotation, Color, FontWeight,
    Hyphens, LineBreak, LinkAnnotation, LinkInteractionListener, ParagraphStyle, SpanStyle,
    StringAnnotation, TextAlign, TextDirection, TextIndent, TextLinkStyles, TextStyle, UnitExt,
};

fn par1() -> ParagraphStyle {
    ParagraphStyle {
        text_indent: Some(TextIndent { first_line: 10.sp(), ..TextIndent::NONE }),
        ..ParagraphStyle::DEFAULT
    }
}

fn par2() -> ParagraphStyle {
    ParagraphStyle { hyphens: Some(Hyphens::Auto), ..ParagraphStyle::DEFAULT }
}

fn par3() -> ParagraphStyle {
    ParagraphStyle { text_direction: Some(TextDirection::Rtl), ..ParagraphStyle::DEFAULT }
}

fn par4() -> ParagraphStyle {
    ParagraphStyle { line_break: Some(LineBreak::SIMPLE), ..ParagraphStyle::DEFAULT }
}

fn range<T>(item: T, start: usize, end: usize) -> AnnotatedRange<T> {
    AnnotatedRange::new(item, start, end)
}

fn default() -> ParagraphStyle {
    ParagraphStyle::DEFAULT
}

fn font_size(size: f32) -> SpanStyle {
    SpanStyle { font_size: size.sp(), ..SpanStyle::DEFAULT }
}

fn line_height(height: f32) -> ParagraphStyle {
    ParagraphStyle { line_height: height.sp(), ..ParagraphStyle::DEFAULT }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_nested
#[test]
fn nested_paragraphs_split_their_parent() {
    let text = AnnotatedString::build(|b| {
        b.with_paragraph_style(par1(), |b| {
            b.append("a");
            b.with_paragraph_style(par2(), |b| {
                b.append("b");
            });
            b.append("c");
        });
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(par1(), 0, 1), range(par1().merge(&par2()), 1, 2), range(par1(), 2, 3)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_overlapped
#[test]
fn a_paragraph_at_its_parents_start_splits_it() {
    let text = AnnotatedString::build(|b| {
        b.append("1234");
        b.add_paragraph_style(par1(), 0, 4);
        b.add_paragraph_style(par2(), 0, 2);
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(par1().merge(&par2()), 0, 2), range(par1(), 2, 4)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_stackCorrectlyCleared
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_stackCorrectlyCleared_whenEndsOverlap
#[test]
fn paragraphs_after_nested_ones_stand_alone() {
    let text = AnnotatedString::build(|b| {
        b.append("0");
        b.with_paragraph_style(par1(), |b| {
            b.append("a");
            b.with_paragraph_style(par2(), |b| {
                b.append("b");
            });
            b.append("c");
        });
        b.with_paragraph_style(par3(), |b| {
            b.append("f");
        });
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![
            range(default(), 0, 1),
            range(par1(), 1, 2),
            range(par1().merge(&par2()), 2, 3),
            range(par1(), 3, 4),
            range(par3(), 4, 5),
        ]
    );
    let text = AnnotatedString::build(|b| {
        b.with_paragraph_style(par1(), |b| {
            b.append("a");
            b.with_paragraph_style(par2(), |b| {
                b.append("b");
            });
        });
        b.with_paragraph_style(par3(), |b| {
            b.append("c");
        });
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(par1(), 0, 1), range(par1().merge(&par2()), 1, 2), range(par3(), 2, 3)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_fullyOverlapped
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_fullyOverlapped_stackCorrectlyCleared
#[test]
fn paragraphs_over_the_same_range_merge() {
    let both = |b: &mut AnnotatedStringBuilder| {
        b.with_paragraph_style(par1(), |b| {
            b.with_paragraph_style(par2(), |b| {
                b.append("a");
            });
        });
    };
    let text = AnnotatedString::build(both);
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(par1().merge(&par2()), 0, 1)]
    );
    let text = AnnotatedString::build(|b| {
        both(b);
        b.with_paragraph_style(par3(), |b| {
            b.append("b");
        });
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(par1().merge(&par2()), 0, 1), range(par3(), 1, 2)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_complex_withNoParagraphsInBetween
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_complex_nestedSiblingParagraphs
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_multiLevelNested
#[test]
fn complex_paragraphs_normalize_in_order() {
    let text = AnnotatedString::build(|b| {
        b.append("text1");
        b.with_paragraph_style(par1(), |b| {
            b.append("text2");
            b.with_paragraph_style(par2(), |b| {
                b.append("text3");
            });
        });
        b.append("text4");
        b.with_paragraph_style(par3(), |b| {
            b.append("text5");
        });
        b.append("text6");
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![
            range(default(), 0, 5),
            range(par1(), 5, 10),
            range(par1().merge(&par2()), 10, 15),
            range(default(), 15, 20),
            range(par3(), 20, 25),
            range(default(), 25, 30),
        ]
    );
    let text = AnnotatedString::build(|b| {
        b.append("text1");
        b.with_paragraph_style(par1(), |b| {
            b.append("text2");
            b.with_paragraph_style(par2(), |b| {
                b.append("text3");
            });
            b.with_paragraph_style(par3(), |b| {
                b.append("text4");
            });
            b.append("text5");
        });
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![
            range(default(), 0, 5),
            range(par1(), 5, 10),
            range(par1().merge(&par2()), 10, 15),
            range(par1().merge(&par3()), 15, 20),
            range(par1(), 20, 25),
        ]
    );
    let text = AnnotatedString::build(|b| {
        b.append("text1");
        b.with_paragraph_style(par1(), |b| {
            b.append("text2");
            b.with_paragraph_style(par2(), |b| {
                b.with_paragraph_style(par3(), |b| {
                    b.append("text3");
                    b.with_paragraph_style(par4(), |b| {
                        b.append("text4");
                    });
                });
            });
            b.append("text5");
        });
    });
    let three = par1().merge(&par2()).merge(&par3());
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![
            range(default(), 0, 5),
            range(par1(), 5, 10),
            range(three, 10, 15),
            range(three.merge(&par4()), 15, 20),
            range(par1(), 20, 25),
        ]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyle_withBlankLinesAround
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyle_withBlankLinesAtEnd
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyle_withBlankLines_correctlyClearsStack
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_zeroLengthParagraph_atStart
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_zeroLengthParagraph
#[test]
fn empty_paragraphs_stay_empty_paragraphs() {
    let text = AnnotatedString::build(|b| {
        b.push_paragraph_style(par1());
        b.pop();
        b.push_paragraph_style(par2());
        b.append("a");
        b.pop();
        b.push_paragraph_style(par3());
        b.pop();
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(par1(), 0, 0), range(par2(), 0, 1), range(par3(), 1, 1)]
    );
    let text = AnnotatedString::build(|b| {
        b.push_paragraph_style(par1());
        b.append("a");
        b.pop();
        b.push_paragraph_style(par2());
        b.pop();
        b.push_paragraph_style(par3());
        b.pop();
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(par1(), 0, 1), range(par2().merge(&par3()), 1, 1)]
    );
    let text = AnnotatedString::build(|b| {
        b.with_paragraph_style(par1(), |b| {
            b.with_paragraph_style(par2(), |_| {});
        });
        b.append("a");
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(par1().merge(&par2()), 0, 0), range(default(), 0, 1)]
    );
    let text = AnnotatedString::build(|b| {
        b.with_paragraph_style(par1(), |_| {});
        b.append("0");
        b.with_paragraph_style(par2(), |b| {
            b.append("a");
        });
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(par1(), 0, 0), range(default(), 0, 1), range(par2(), 1, 2)]
    );
    let text = AnnotatedString::build(|b| {
        b.append("0");
        b.with_paragraph_style(par1(), |_| {});
        b.with_paragraph_style(par2(), |b| {
            b.append("a");
        });
    });
    assert_eq!(
        text.normalized_paragraph_styles(&default()),
        vec![range(default(), 0, 1), range(par1(), 1, 1), range(par2(), 1, 2)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_only_string
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_empty_string
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_zeroLength_paragraphStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_with_newLine
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, normalizedParagraphStyles_with_only_lineFeed
#[test]
fn the_default_paragraph_style_fills_the_gaps() {
    let centered =
        ParagraphStyle { text_align: Some(TextAlign::Center), ..ParagraphStyle::DEFAULT };
    let base = line_height(20.0);
    let text = AnnotatedString::with_styles("Hello World", [], [range(centered, 0, 5)]);
    assert_eq!(
        text.normalized_paragraph_styles(&base),
        vec![range(base.merge(&centered), 0, 5), range(base, 5, 11)]
    );
    for plain in ["Hello World", "", "Hello\nWorld", "\n"] {
        assert_eq!(
            AnnotatedString::new(plain).normalized_paragraph_styles(&base),
            vec![range(base, 0, plain.len())]
        );
    }
    let rtl =
        ParagraphStyle { text_direction: Some(TextDirection::Rtl), ..ParagraphStyle::DEFAULT };
    let text = AnnotatedString::with_styles("a", [], [range(rtl, 0, 0)]);
    assert_eq!(
        text.normalized_paragraph_styles(&base),
        vec![range(base.merge(&rtl), 0, 0), range(base, 0, 1)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, length_returns_text_length
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, toString_returns_the_plain_string
#[test]
fn length_and_display_are_the_texts() {
    assert_eq!(AnnotatedString::new("abc").len(), 3);
    assert_eq!(AnnotatedString::new("abc").to_string(), "abc");
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, plus_operator_creates_a_new_annotated_string
#[test]
fn plus_appends_text_and_annotations() {
    let annotation =
        |value: &str| Annotation::StringAnnotation(StringAnnotation(value.to_string().into()));
    let red = SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT };
    let blue = SpanStyle { color: Color::BLUE, ..SpanStyle::DEFAULT };
    let annotations1 = vec![
        range(Annotation::SpanStyle(red.clone()), 0, 3),
        range(Annotation::SpanStyle(blue.clone()), 2, 4),
        range(Annotation::ParagraphStyle(line_height(20.0)), 0, 1),
        range(Annotation::ParagraphStyle(line_height(30.0)), 1, 5),
        AnnotatedRange::tagged(annotation("annotation1"), 0, 2, "scope1"),
        AnnotatedRange::tagged(annotation("annotation1"), 3, 5, "scope1"),
    ];
    let first = AnnotatedString::with_annotations("Hello", annotations1.clone());
    let cyan = SpanStyle { color: Color::CYAN, ..SpanStyle::DEFAULT };
    let second = AnnotatedString::with_annotations(
        "World",
        [
            range(Annotation::SpanStyle(cyan.clone()), 0, 5),
            range(Annotation::ParagraphStyle(line_height(10.0)), 0, 5),
            AnnotatedRange::tagged(annotation("annotation2"), 0, 5, "scope2"),
        ],
    );
    let mut expected = annotations1;
    expected.extend([
        range(Annotation::SpanStyle(cyan.clone()), 5, 10),
        range(Annotation::ParagraphStyle(line_height(10.0)), 5, 10),
        AnnotatedRange::tagged(annotation("annotation2"), 5, 10, "scope2"),
    ]);
    let sum = &first + &second;
    assert_eq!(sum, AnnotatedString::with_annotations("HelloWorld", expected));
    assert_eq!(sum.span_styles(), vec![range(red, 0, 3), range(blue, 2, 4), range(cyan, 5, 10)]);
    assert_eq!(
        sum.paragraph_styles(),
        vec![
            range(line_height(20.0), 0, 1),
            range(line_height(30.0), 1, 5),
            range(line_height(10.0), 5, 10)
        ]
    );
}

fn three_styled() -> AnnotatedString {
    AnnotatedString::build(|b| {
        b.with_style(font_size(12.0), |b| {
            b.append("a");
        });
        b.with_style(font_size(12.0), |b| {
            b.append("b");
        });
        b.with_paragraph_style(line_height(14.0), |b| {
            b.append("c");
        });
    })
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_returns_the_correct_string
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_returns_empty_text_for_start_equals_end
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_returns_original_text_for_text_range_is_full_range
#[test]
fn a_sub_sequence_has_its_part_of_the_text() {
    assert_eq!(
        AnnotatedStringBuilder::from("abcd").to_annotated_string().sub_sequence(1, 3).as_str(),
        "bc"
    );
    assert_eq!(
        three_styled().sub_sequence(1, 1),
        AnnotatedString::with_styles("", [range(font_size(12.0), 0, 0)], [])
    );
    let text = three_styled();
    let whole = text.sub_sequence(0, 3);
    assert_eq!(whole, text);
    assert!(compose_runtime::ComposeArg::same(&whole, &text), "the same string, shared");
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_doesNot_include_styles_before_the_start
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_doesNot_include_styles_after_the_end
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_collapsed_item_with_itemStart_equalTo_rangeStart
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_collapses_included_item
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_collapses_covering_item
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_with_collapsed_range_with_collapsed_item
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_includes_partial_matches
#[test]
fn a_sub_sequence_keeps_the_styles_over_it() {
    let text = AnnotatedString::build(|b| {
        b.with_style(font_size(12.0), |b| {
            b.append("a");
        });
        b.with_paragraph_style(line_height(14.0), |b| {
            b.append("b");
        });
        b.append("c");
    });
    assert_eq!(text.sub_sequence(2, 3), AnnotatedString::new("c"));
    let text = AnnotatedString::build(|b| {
        b.append("a");
        b.with_style(font_size(12.0), |b| {
            b.append("b");
        });
        b.with_paragraph_style(line_height(14.0), |b| {
            b.append("c");
        });
    });
    assert_eq!(text.sub_sequence(0, 1), AnnotatedString::new("a"));
    let collapsed = AnnotatedString::build(|b| {
        b.append("abc");
        b.add_style(font_size(12.0), 1, 1);
    });
    assert_eq!(
        collapsed.sub_sequence(1, 2),
        AnnotatedString::with_styles("b", [range(font_size(12.0), 0, 0)], [])
    );
    assert_eq!(
        collapsed.sub_sequence(1, 1),
        AnnotatedString::with_styles("", [range(font_size(12.0), 0, 0)], [])
    );
    let included = AnnotatedString::build(|b| {
        b.append("a");
        b.with_style(font_size(12.0), |b| {
            b.append("b");
        });
        b.append("c");
    });
    assert_eq!(
        included.sub_sequence(1, 1),
        AnnotatedString::with_styles("", [range(font_size(12.0), 0, 0)], [])
    );
    let covering = AnnotatedString::build(|b| {
        b.with_style(font_size(12.0), |b| {
            b.append("abc");
        });
    });
    assert_eq!(
        covering.sub_sequence(1, 1),
        AnnotatedString::with_styles("", [range(font_size(12.0), 0, 0)], [])
    );
    let text = AnnotatedString::build(|b| {
        b.with_style(font_size(12.0), |b| {
            b.append("ab");
        });
        b.with_style(font_size(12.0), |b| {
            b.append("c");
        });
        b.with_paragraph_style(line_height(14.0), |b| {
            b.append("de");
        });
    });
    let expected = AnnotatedString::build(|b| {
        b.with_style(font_size(12.0), |b| {
            b.append("b");
        });
        b.with_style(font_size(12.0), |b| {
            b.append("c");
        });
        b.with_paragraph_style(line_height(14.0), |b| {
            b.append("d");
        });
    });
    assert_eq!(text.sub_sequence(1, 4), expected);
}

fn annotated_cd() -> AnnotatedString {
    AnnotatedString::build(|b| {
        b.append("ab");
        b.push_string_annotation("scope1", "annotation1");
        b.append("cd");
        b.pop();
        b.append("ef");
    })
}

fn annotation1(text: &str, start: usize, end: usize) -> AnnotatedString {
    AnnotatedString::with_annotations(
        text.to_string(),
        [AnnotatedRange::tagged(
            Annotation::StringAnnotation(StringAnnotation("annotation1".into())),
            start,
            end,
            "scope1",
        )],
    )
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_withAnnotations_noIntersection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_withAnnotations_collapsedRange
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_withAnnotations_hasIntersection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_withAnnotations_containsRange
#[test]
fn a_sub_sequence_keeps_the_annotations_over_it() {
    let text = annotated_cd();
    assert_eq!(text.sub_sequence(0, 2), AnnotatedString::new("ab"));
    assert_eq!(text.sub_sequence(4, 6), AnnotatedString::new("ef"));
    assert_eq!(text.sub_sequence(4, 4), AnnotatedString::new(""));
    assert_eq!(text.sub_sequence(2, 2), annotation1("", 0, 0));
    assert_eq!(text.sub_sequence(3, 3), annotation1("", 0, 0));
    assert_eq!(text.sub_sequence(0, 3), annotation1("abc", 2, 3));
    assert_eq!(text.sub_sequence(3, 5), annotation1("de", 0, 1));
    assert_eq!(text.sub_sequence(0, 5), annotation1("abcde", 2, 4));
    assert_eq!(text.sub_sequence(2, 4), annotation1("cd", 0, 2));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_throws_exception_for_start_greater_than_end
#[test]
#[should_panic(expected = "should be less or equal to end")]
fn a_reversed_sub_sequence_panics() {
    AnnotatedString::new("ab").sub_sequence(1, 0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_throws_exception_when_start_is_out_of_bounds_values
// (subSequence_throws_exception_for_negative_values has no counterpart: offsets are unsigned)
#[test]
#[should_panic]
fn a_sub_sequence_past_the_text_panics() {
    AnnotatedString::new("abc").sub_sequence(3, 4);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, creating_item_with_start_greater_than_end_throws_exception
#[test]
#[should_panic(expected = "Reversed range is not supported")]
fn a_reversed_range_panics() {
    AnnotatedRange::new(SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT }, 1, 0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, creating_item_with_start_equal_to_end_does_not_throw_exception
#[test]
fn an_empty_range_is_kept() {
    let empty = AnnotatedRange::new(SpanStyle::DEFAULT, 1, 1);
    let text = AnnotatedString::with_styles("ab", [empty.clone()], []);
    assert_eq!(text.span_styles(), vec![empty]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, constructor_function_with_single_spanStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, constructor_function_with_single_paragraphStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, constructor_function_with_single_spanStyle_and_paragraphStyle
#[test]
fn a_whole_text_style_covers_the_text() {
    let red = SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT };
    let paragraph = line_height(12.0);
    assert_eq!(
        AnnotatedString::with_span_style("a", red.clone(), None),
        AnnotatedString::with_styles("a", [range(red.clone(), 0, 1)], [])
    );
    assert_eq!(
        AnnotatedString::with_paragraph_style("a", paragraph),
        AnnotatedString::with_styles("a", [], [range(paragraph, 0, 1)])
    );
    assert_eq!(
        AnnotatedString::with_span_style("a", red.clone(), Some(paragraph)),
        AnnotatedString::with_styles("a", [range(red, 0, 1)], [range(paragraph, 0, 1)])
    );
}

fn build_with_paragraphs(text: &str, paragraphs: &[(usize, usize)]) -> AnnotatedString {
    AnnotatedString::build(|b| {
        b.append(text);
        for &(start, end) in paragraphs {
            b.add_paragraph_style(ParagraphStyle::DEFAULT, start, end);
        }
    })
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, subSequence_throws_exception_for_overlapping_paragraphStyles
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, throws_exception_overlapping_exceedsLastMax
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, throws_exception_overlapping_addsCurrentToStack_whenEmptyStack
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, throws_exception_overlapsDisallowedRange
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, throws_exception_overlapsDisallowedRange_first
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, throws_exception_orderMatters_overlapping
#[test]
fn partly_overlapping_paragraphs_panic() {
    let cases: [(&str, &[(usize, usize)]); 5] = [
        ("1234", &[(0, 2), (1, 3)]),
        ("12345", &[(1, 4), (2, 3), (3, 5)]),
        ("12345", &[(1, 2), (2, 4), (3, 5)]),
        ("1234567890", &[(1, 10), (1, 8), (1, 6), (1, 4), (7, 9)]),
        ("1234567890", &[(1, 10), (1, 8), (1, 6), (1, 4), (3, 5)]),
    ];
    for (text, paragraphs) in cases {
        let result = std::panic::catch_unwind(|| build_with_paragraphs(text, paragraphs));
        assert!(result.is_err(), "{paragraphs:?} should panic");
    }
    let ordered = std::panic::catch_unwind(|| {
        AnnotatedString::build(|b| {
            b.append("1234");
            b.add_paragraph_style(par1(), 0, 2);
            b.add_paragraph_style(par2(), 0, 4);
        })
    });
    assert!(
        ordered.is_err(),
        "the order matters: a later paragraph can't enclose one at its start"
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, doesNot_throws_exceedsMax
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, doesNot_throws_stackCleared_insideMaxRange
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, doesNot_throw_fullyOverlapsDisallowedRange
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, doesNot_throw_insideAllowedRange
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, doesNot_throw_exception_orderMatters_nested
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, doesNot_throw_exception_if_paragraphStyles_are_nested
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, doesNot_throw_exception_if_paragraphStyles_are_fully_overlapped
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/AnnotatedStringTest.kt, doesNot_throw_exception_if_paragraphStyles_are_not_sorted
#[test]
fn nested_and_disjoint_paragraphs_are_allowed() {
    let cases: [(&str, &[(usize, usize)]); 7] = [
        ("12345", &[(0, 3), (1, 2), (4, 5)]),
        ("12345", &[(0, 3), (1, 2), (4, 5), (4, 4)]),
        ("1234567890", &[(1, 10), (1, 8), (1, 6), (1, 4), (7, 8)]),
        ("1234567890", &[(1, 9), (2, 8), (3, 6), (4, 6)]),
        ("1234", &[(3, 4), (0, 4)]),
        ("1234", &[(3, 4), (3, 4)]),
        ("1234", &[(3, 4), (0, 2)]),
    ];
    for (text, paragraphs) in cases {
        build_with_paragraphs(text, paragraphs);
    }
    AnnotatedString::build(|b| {
        b.append("1234");
        b.add_paragraph_style(par1(), 0, 4);
        b.add_paragraph_style(par2(), 0, 2);
    });
}

#[test]
fn spans_split_the_text_into_runs_in_their_styles() {
    let bold = SpanStyle { font_weight: Some(FontWeight::BOLD), ..SpanStyle::DEFAULT };
    let red = SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT };
    let text = AnnotatedString::build(|builder| {
        builder.append("Hello ");
        builder.with_style(bold, |builder| {
            builder.append("bold ");
            builder.with_style(red, |builder| {
                builder.append("red");
            });
        });
        builder.append("!");
    });
    assert_eq!(text.as_str(), "Hello bold red!");
    let runs = text.runs(&TextStyle::DEFAULT);
    let ranges: Vec<_> = runs.iter().map(|(range, _, _)| range.clone()).collect();
    assert_eq!(ranges, vec![0..6, 6..11, 11..14, 14..15]);
    assert_eq!(runs[0].1, TextStyle::DEFAULT);
    assert_eq!(runs[1].1.font_weight, Some(FontWeight::BOLD));
    assert_eq!((runs[2].1.font_weight, runs[2].1.color), (Some(FontWeight::BOLD), Color::RED));
    assert_eq!(runs[3].1, TextStyle::DEFAULT);
}

#[test]
fn an_em_font_size_in_a_span_is_relative_to_the_texts() {
    let text = AnnotatedString::build(|b| {
        b.with_style(SpanStyle { font_size: 2.em(), ..SpanStyle::DEFAULT }, |b| {
            b.append("big");
        });
    });
    let style = TextStyle { font_size: 10.sp(), ..TextStyle::DEFAULT };
    assert_eq!(text.runs(&style)[0].1.font_size, 20.sp());
    assert_eq!(text.runs(&TextStyle::DEFAULT)[0].1.font_size, 28.sp());
}

fn green_styles() -> TextLinkStyles {
    TextLinkStyles {
        style: Some(SpanStyle { color: Color::GREEN, ..SpanStyle::DEFAULT }),
        ..TextLinkStyles::default()
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/LinkAnnotationTest.kt, copy_keepAll_linkAnnotation_url
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/LinkAnnotationTest.kt, copy_changeAll_linkAnnotation_url
#[test]
fn url_links_copy_with_replaced_parts() {
    let original = LinkAnnotation::url("url1").with_styles(Some(TextLinkStyles::default()));
    assert_eq!(original.clone(), original);
    // Compose's `copy(url, styles, listener)`: the variant's fields, or the `with_` copies.
    let LinkAnnotation::Url { .. } = original else { unreachable!() };
    let changed = LinkAnnotation::url("url2")
        .with_styles(Some(green_styles()))
        .with_link_interaction_listener(Some(LinkInteractionListener::new(|_| {})));
    let LinkAnnotation::Url { url, .. } = &changed else { unreachable!() };
    assert_eq!(url.as_str(), "url2");
    assert_eq!(changed.styles(), Some(&green_styles()));
    assert!(changed.link_interaction_listener().is_some());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/LinkAnnotationTest.kt, copy_keepAll_linkAnnotation_clickable
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/LinkAnnotationTest.kt, copy_changeAll_linkAnnotation_clickable
#[test]
fn clickable_links_copy_with_replaced_parts() {
    let original = LinkAnnotation::clickable("tag", Some(LinkInteractionListener::new(|_| {})))
        .with_styles(Some(TextLinkStyles::default()));
    assert_eq!(original.clone(), original);
    let changed = LinkAnnotation::clickable("tag1", None)
        .with_styles(Some(green_styles()))
        .with_link_interaction_listener(None);
    let LinkAnnotation::Clickable { tag, .. } = &changed else { unreachable!() };
    assert_eq!(tag.as_str(), "tag1");
    assert_eq!(changed.styles(), Some(&green_styles()));
    assert!(changed.link_interaction_listener().is_none());
}
