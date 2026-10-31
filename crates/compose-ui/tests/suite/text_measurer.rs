//! Compose's public `TextMeasurer`: its cache's key, its cache, and `measure`, laid out by the
//! fixed measurer with Compose's test font metrics (`FixedFontMetrics::SAMPLE_FONT`, D1105), as
//! Compose's tests lay out with its `sample_font`.

use std::rc::Rc;

use compose_ui::platform::{
    CacheTextLayoutInput, FixedFontMetrics, FixedTextMeasurer, SharedTextMeasurer, TextLayoutCache,
};
use compose_ui::{
    AnnotatedString, Brush, Color, Constraints, Density, FontFamilyResolver, LayoutDirection,
    LocaleList, Shadow, TextDecoration, TextDirection, TextLayoutInput, TextLayoutResult,
    TextMeasureOptions, TextMeasurer, TextOverflow, TextStyle, UnitExt,
    create_font_family_resolver,
};

const LONG_TEXT: &str = "Lorem ipsum dolor sit amet, consectetur \
    adipiscing elit. Curabitur augue leo, finibus vitae felis ac, pretium condimentum \
    augue. Nullam non libero sed lectus aliquet venenatis non at purus. Fusce id arcu \
    eu mauris pulvinar laoreet.";

const MULTI_LINE_TEXT: &str = "Lorem\nipsum\ndolor\nsit\namet";

fn platform() -> SharedTextMeasurer {
    SharedTextMeasurer::new(Box::new(FixedTextMeasurer::with_metrics(
        FixedFontMetrics::SAMPLE_FONT,
    )))
}

fn resolver() -> FontFamilyResolver {
    create_font_family_resolver(&platform())
}

fn locale_list() -> LocaleList {
    LocaleList::from_language_tags("en-US")
}

fn text_measurer(cache_size: usize) -> TextMeasurer {
    TextMeasurer::new(
        resolver(),
        locale_list(),
        Density::new(1.0),
        LayoutDirection::Ltr,
        cache_size,
    )
}

/// A text's layout with `options`, on a measurer of its own without a cache.
fn lay_out(text: &str, options: TextMeasureOptions) -> TextLayoutResult {
    text_measurer(0).measure(text.to_string(), options)
}

/// What a test of the cache varies: Compose's `textLayoutInput` arguments, from one resolver.
struct Inputs {
    resolver: FontFamilyResolver,
}

impl Inputs {
    fn new() -> Inputs {
        Inputs { resolver: resolver() }
    }

    fn input(&self, text: &str) -> TextLayoutInput {
        TextLayoutInput {
            text: AnnotatedString::from(text),
            style: TextStyle::DEFAULT,
            max_lines: u32::MAX,
            soft_wrap: true,
            overflow: TextOverflow::Clip,
            density: Density::new(1.0),
            layout_direction: LayoutDirection::Ltr,
            font_family_resolver: self.resolver.clone(),
            default_locale_list: locale_list(),
            constraints: Constraints::unbounded(),
            placeholders: None,
        }
    }

    fn styled(&self, text: &str, style: TextStyle) -> TextLayoutInput {
        TextLayoutInput { style, ..self.input(text) }
    }

    /// `input` laid out on a measurer of its own, as Compose's `layoutText`.
    fn layout(&self, input: &TextLayoutInput) -> TextLayoutResult {
        TextMeasurer::new(
            input.font_family_resolver.clone(),
            input.default_locale_list.clone(),
            input.density,
            input.layout_direction,
            0,
        )
        .measure(
            input.text.clone(),
            TextMeasureOptions {
                style: input.style.clone(),
                overflow: input.overflow,
                soft_wrap: input.soft_wrap,
                max_lines: input.max_lines,
                constraints: input.constraints,
                ..TextMeasureOptions::default()
            },
        )
    }
}

fn red() -> TextStyle {
    TextStyle { color: Color::RED, ..TextStyle::DEFAULT }
}

fn blue() -> TextStyle {
    TextStyle { color: Color::BLUE, ..TextStyle::DEFAULT }
}

fn gradient(colors: Vec<Color>) -> TextStyle {
    TextStyle::DEFAULT.with_brush(Some(Brush::linear_gradient(colors)), f32::NAN)
}

fn shadowed(color: Color) -> TextStyle {
    TextStyle { shadow: Some(Shadow { color, ..Shadow::NONE }), ..TextStyle::DEFAULT }
}

fn decorated(decoration: TextDecoration) -> TextStyle {
    TextStyle { text_decoration: Some(decoration), ..TextStyle::DEFAULT }
}

fn cache_key(inputs: &Inputs, change: impl FnOnce(&mut TextLayoutInput)) -> CacheTextLayoutInput {
    let mut input = inputs.input("Hello");
    change(&mut input);
    CacheTextLayoutInput::new(input)
}

/// Each of these inputs makes another key than the default's.
fn assert_key_differs(change: impl FnOnce(&mut TextLayoutInput)) {
    let inputs = Inputs::new();
    assert!(cache_key(&inputs, |_| {}) != cache_key(&inputs, change));
}

/// A style changed in what it draws only is the same key.
fn assert_key_same(first: TextStyle, second: TextStyle) {
    let inputs = Inputs::new();
    assert!(
        CacheTextLayoutInput::new(inputs.styled("Hello", first))
            == CacheTextLayoutInput::new(inputs.styled("Hello", second))
    );
}

// ---- CacheTextLayoutInputTest -------------------------------------------------------------

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, default_ctor_should_be_equal
#[test]
fn two_keys_of_the_same_input_are_equal() {
    let inputs = Inputs::new();
    assert!(cache_key(&inputs, |_| {}) == cache_key(&inputs, |_| {}));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, text_should_differ
#[test]
fn another_text_is_another_key() {
    assert_key_differs(|input| input.text = AnnotatedString::from("Hello, World"));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, maxLines_should_differ
#[test]
fn other_max_lines_are_another_key() {
    assert_key_differs(|input| input.max_lines = 2);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, placeholders_should_differ
#[test]
fn another_placeholder_is_another_key() {
    let placeholder = compose_ui::Placeholder::new(
        1.0.em(),
        1.0.em(),
        compose_ui::PlaceholderVerticalAlign::AboveBaseline,
    );
    let one = compose_ui::AnnotatedRange::new(placeholder, 0, 1);
    assert_key_differs(move |input| {
        input.placeholders = Some(std::rc::Rc::new([one.clone()]));
    });
}

#[test]
fn no_placeholders_and_empty_ones_are_the_same_key() {
    let inputs = Inputs::new();
    let none = inputs.input("Hello");
    let empty =
        TextLayoutInput { placeholders: Some(std::rc::Rc::new([])), ..inputs.input("Hello") };
    assert!(CacheTextLayoutInput::new(none) == CacheTextLayoutInput::new(empty));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, softWrap_should_differ
#[test]
fn another_soft_wrap_is_another_key() {
    assert_key_differs(|input| input.soft_wrap = false);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, overflow_should_differ
#[test]
fn another_overflow_is_another_key() {
    assert_key_differs(|input| input.overflow = TextOverflow::Ellipsis);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, density_should_differ
#[test]
fn another_density_is_another_key() {
    assert_key_differs(|input| input.density = Density::new(2.0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, layoutDirection_should_differ
#[test]
fn another_layout_direction_is_another_key() {
    assert_key_differs(|input| input.layout_direction = LayoutDirection::Rtl);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, fontFamilyResolver_should_differ
#[test]
fn another_font_family_resolver_is_another_key_by_identity() {
    let inputs = Inputs::new();
    let shared = platform();
    let first = create_font_family_resolver(&shared);
    let second = create_font_family_resolver(&shared);
    assert!(
        cache_key(&inputs, |input| input.font_family_resolver = first.clone())
            == cache_key(&inputs, |input| input.font_family_resolver = first.clone())
    );
    assert!(
        cache_key(&inputs, |input| input.font_family_resolver = first)
            != cache_key(&inputs, |input| input.font_family_resolver = second)
    );
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, constraints_maxWidth_should_differ
#[test]
fn another_maximum_width_is_another_key() {
    let inputs = Inputs::new();
    let first = cache_key(&inputs, |input| input.constraints = Constraints::new(0, 200, 0, 300));
    let second = cache_key(&inputs, |input| input.constraints = Constraints::new(0, 100, 0, 300));
    assert!(first != second);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, constraints_maxHeight_should_differ
#[test]
fn another_maximum_height_is_another_key() {
    let inputs = Inputs::new();
    let first = cache_key(&inputs, |input| input.constraints = Constraints::new(0, 200, 0, 300));
    let second = cache_key(&inputs, |input| input.constraints = Constraints::new(0, 200, 0, 100));
    assert!(first != second);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, minConstraints_should_differ
#[test]
fn other_minimum_constraints_are_another_key() {
    let inputs = Inputs::new();
    let unbounded = Constraints::INFINITY;
    let first = cache_key(&inputs, |input| {
        input.constraints = Constraints::new(10, unbounded, 20, unbounded)
    });
    let second = cache_key(&inputs, |input| {
        input.constraints = Constraints::new(20, unbounded, 10, unbounded)
    });
    assert!(first != second);
}

#[test]
fn another_default_locale_list_is_another_key() {
    assert_key_differs(|input| input.default_locale_list = LocaleList::from_language_tags("he"));
}

/// A text with a span of `size` over its first letters.
fn with_a_span(size: f32) -> AnnotatedString {
    AnnotatedString::build(|builder| {
        builder.with_style(
            compose_ui::SpanStyle { font_size: size.sp(), ..compose_ui::SpanStyle::DEFAULT },
            |builder| {
                builder.append("Hel");
            },
        );
        builder.append("lo");
    })
}

#[test]
fn another_span_that_affects_the_layout_is_another_key() {
    assert_key_differs(|input| input.text = with_a_span(30.0));
    let inputs = Inputs::new();
    assert!(
        cache_key(&inputs, |input| input.text = with_a_span(30.0))
            != cache_key(&inputs, |input| input.text = with_a_span(40.0))
    );
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, color_should_not_differ
#[test]
fn another_color_is_the_same_key() {
    assert_key_same(red(), blue());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, brush_should_not_differ
#[test]
fn another_brush_is_the_same_key() {
    assert_key_same(
        gradient(vec![Color::RED, Color::GREEN]),
        gradient(vec![Color::RED, Color::BLUE]),
    );
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, shadow_should_not_differ
#[test]
fn another_shadow_is_the_same_key() {
    assert_key_same(shadowed(Color::RED), shadowed(Color::BLUE));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, textDecoration_should_not_differ
#[test]
fn another_decoration_is_the_same_key() {
    assert_key_same(decorated(TextDecoration::UNDERLINE), decorated(TextDecoration::LINE_THROUGH));
}

// ---- TextLayoutCacheTest ------------------------------------------------------------------

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, capacity_cannot_be_zero
#[test]
#[should_panic(expected = "capacity")]
fn a_cache_of_no_capacity_panics() {
    TextLayoutCache::new(0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, capacity_one_shouldEvictTheCache_forEveryDifferentLayoutInput
#[test]
fn a_cache_of_one_keeps_the_last_input_only() {
    let inputs = Inputs::new();
    let mut cache = TextLayoutCache::new(1);
    let mut last: Option<TextLayoutInput> = None;
    for text in ["W", "Wo", "Wor", "Worl", "World"] {
        let input = inputs.styled(text, red());
        cache.put(&input, inputs.layout(&input));
        assert!(cache.get(&input).is_some());
        if let Some(last) = &last {
            assert!(cache.get(last).is_none());
        }
        last = Some(input);
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, exactInput_shouldReturnTheSameResult
#[test]
fn the_same_input_gives_the_result_put() {
    let inputs = Inputs::new();
    let mut cache = TextLayoutCache::new(16);
    let input = inputs.styled("Hello", red());
    let result = inputs.layout(&input);
    cache.put(&input, result.clone());
    assert_eq!(cache.get(&input), Some(result));
}

/// The result put for `first` is what `second` finds.
fn assert_cache_finds(first: TextStyle, second: TextStyle) {
    let inputs = Inputs::new();
    let mut cache = TextLayoutCache::new(16);
    let first = inputs.styled("Hello", first);
    let second = inputs.styled("Hello", second);
    let result = inputs.layout(&first);
    cache.put(&first, result.clone());
    assert_eq!(cache.get(&second), Some(result));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, colorChange_shouldReturnFromCache
#[test]
fn another_color_finds_the_layout() {
    assert_cache_finds(red(), blue());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, brushChange_shouldReturnFromCache
#[test]
fn another_brush_finds_the_layout() {
    assert_cache_finds(
        gradient(vec![Color::RED, Color::GREEN]),
        gradient(vec![Color::RED, Color::BLUE]),
    );
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, shadowChange_shouldReturnFromCache
#[test]
fn another_shadow_finds_the_layout() {
    assert_cache_finds(shadowed(Color::RED), shadowed(Color::BLUE));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, textDecorationChange_shouldReturnFromCache
#[test]
fn another_decoration_finds_the_layout() {
    assert_cache_finds(
        decorated(TextDecoration::UNDERLINE),
        decorated(TextDecoration::LINE_THROUGH),
    );
}

/// The result put for an input isn't what the input changed by `change` finds.
fn assert_cache_misses(change: impl FnOnce(&mut TextLayoutInput)) {
    let inputs = Inputs::new();
    let mut cache = TextLayoutCache::new(16);
    let first = TextLayoutInput {
        constraints: Constraints::new(20, 200, 0, 300),
        ..inputs.styled("Hello", red())
    };
    let mut second = first.clone();
    change(&mut second);
    cache.put(&first, inputs.layout(&first));
    assert!(cache.get(&second).is_none());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, constraintsMinChanges_shouldReturnNull
#[test]
fn another_minimum_width_misses() {
    assert_cache_misses(|input| input.constraints = Constraints::new(60, 200, 0, 300));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, textChanges_shouldReturnNull
#[test]
fn another_text_misses() {
    assert_cache_misses(|input| input.text = AnnotatedString::from("Hello World"));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, fontSizeChange_shouldReturnNull
#[test]
fn another_font_size_misses() {
    assert_cache_misses(|input| input.style.font_size = 24.sp());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, densityChange_shouldReturnNull
#[test]
fn another_density_misses() {
    assert_cache_misses(|input| input.density = Density::new(2.0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, layoutDirectionChange_shouldReturnNull
#[test]
fn another_layout_direction_misses() {
    assert_cache_misses(|input| input.layout_direction = LayoutDirection::Rtl);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, constraintsMaxChanges_shouldReturnNull
#[test]
fn another_maximum_width_misses() {
    assert_cache_misses(|input| input.constraints = Constraints::new(20, 400, 0, 300));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextLayoutCacheTest.kt, cacheShouldEvict_leastRecentlyUsedLayout
#[test]
fn the_cache_evicts_the_least_recently_used_layout() {
    let inputs = Inputs::new();
    let mut cache = TextLayoutCache::new(2);
    let (first, second, third) = (inputs.input("1"), inputs.input("2"), inputs.input("3"));
    cache.put(&first, inputs.layout(&first));
    cache.put(&second, inputs.layout(&second));
    cache.get(&first);
    cache.put(&third, inputs.layout(&third));
    assert!(cache.get(&first).is_some());
    assert!(cache.get(&second).is_none());
    assert!(cache.get(&third).is_some());
}

// ---- TextMeasurerTest ---------------------------------------------------------------------

fn at_most_200() -> Constraints {
    Constraints::new(0, 200, 0, Constraints::INFINITY)
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, stringAndAnnotatedString_shouldReturnTheSameInstance_whenCacheIsEnabled
#[test]
fn a_string_and_an_annotated_string_share_a_cached_layout() {
    let measurer = text_measurer(8);
    let first = measurer.measure("Hello", TextMeasureOptions::default());
    let second = measurer.measure(AnnotatedString::from("Hello"), TextMeasureOptions::default());
    assert!(Rc::ptr_eq(&first.multi_paragraph, &second.multi_paragraph));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, width_shouldMatter_ifSoftwrapIsEnabled
#[test]
fn a_wrapping_text_is_laid_out_at_the_maximum_width() {
    let result = lay_out(
        LONG_TEXT,
        TextMeasureOptions {
            soft_wrap: true,
            constraints: at_most_200(),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.multi_paragraph.width(), 200.0);
}

/// With soft wrap off, `overflow` is one line at most, and at the width given.
fn assert_width_and_lines_matter(overflow: TextOverflow) {
    let result = text_measurer(0).measure(
        LONG_TEXT,
        TextMeasureOptions {
            soft_wrap: false,
            overflow,
            max_lines: 4,
            constraints: Constraints::new(0, 200, 0, Constraints::INFINITY),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.multi_paragraph.width(), 200.0);
    assert_eq!(result.line_count, 1);
}

/// With soft wrap on, the maximum lines stay.
fn assert_max_lines_stay(overflow: TextOverflow) {
    let result = text_measurer(0).measure(
        LONG_TEXT,
        TextMeasureOptions {
            overflow,
            max_lines: 2,
            constraints: Constraints::new(0, 100, 0, Constraints::INFINITY),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.layout_input.max_lines, 2);
    assert!(result.line_count <= 2, "{}", result.line_count);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, width_shouldMatter_ifSoftwrapIsDisabled_butOverflowIsStartEllipsis
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, overwriteMaxLines_ifSoftwrapIsDisabled_andTextOverflowIsStartEllipsis
#[test]
fn a_start_ellipsis_is_one_line_at_the_width_without_soft_wrap() {
    assert_width_and_lines_matter(TextOverflow::StartEllipsis);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, width_shouldMatter_ifSoftwrapIsDisabled_butOverflowIsMiddleEllipsis
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, overwriteMaxLines_ifSoftwrapIsDisabled_andTextOverflowIsMiddleEllipsis
#[test]
fn a_middle_ellipsis_is_one_line_at_the_width_without_soft_wrap() {
    assert_width_and_lines_matter(TextOverflow::MiddleEllipsis);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, dontOverwriteMaxLines_startEllipsis_ifSoftwrapIsEnabled
#[test]
fn a_start_ellipsis_keeps_the_maximum_lines_with_soft_wrap() {
    assert_max_lines_stay(TextOverflow::StartEllipsis);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, dontOverwriteMaxLines_middleEllipsis_ifSoftwrapIsEnabled
#[test]
fn a_middle_ellipsis_keeps_the_maximum_lines_with_soft_wrap() {
    assert_max_lines_stay(TextOverflow::MiddleEllipsis);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, width_shouldMatter_ifSoftwrapIsDisabled_butOverflowIsEllipsis
#[test]
fn an_ellipsized_text_without_wrapping_is_laid_out_at_the_maximum_width() {
    let result = lay_out(
        LONG_TEXT,
        TextMeasureOptions {
            soft_wrap: false,
            overflow: TextOverflow::Ellipsis,
            constraints: at_most_200(),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.multi_paragraph.width(), 200.0);
}

/// `text`'s own width, as Compose's `multiParagraphIntrinsics(text).maxIntrinsicWidth`.
fn max_intrinsic_width(text: &str) -> f32 {
    lay_out(text, TextMeasureOptions::default()).multi_paragraph.max_intrinsic_width()
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, width_shouldBeMaxIntrinsicWidth_ifSoftwrapIsDisabled_andOverflowIsClip
#[test]
fn a_clipped_text_without_wrapping_is_laid_out_at_its_own_width() {
    let result = lay_out(
        LONG_TEXT,
        TextMeasureOptions {
            soft_wrap: false,
            overflow: TextOverflow::Clip,
            constraints: at_most_200(),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.multi_paragraph.width(), max_intrinsic_width(LONG_TEXT));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, width_shouldBeMaxIntrinsicWidth_ifSoftwrapIsDisabled_andOverflowIsVisible
#[test]
fn a_text_drawn_past_its_bounds_without_wrapping_is_laid_out_at_its_own_width() {
    let result = lay_out(
        LONG_TEXT,
        TextMeasureOptions {
            soft_wrap: false,
            overflow: TextOverflow::Visible,
            constraints: at_most_200(),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.multi_paragraph.width(), max_intrinsic_width(LONG_TEXT));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, overwriteMaxLines_ifSoftwrapIsDisabled_andTextOverflowIsEllipsis
#[test]
fn an_ellipsis_without_wrapping_is_one_line() {
    let result = lay_out(
        MULTI_LINE_TEXT,
        TextMeasureOptions {
            soft_wrap: false,
            overflow: TextOverflow::Ellipsis,
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.multi_paragraph.line_count(), 1);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, dontOverwriteMaxLines_endEllipsis_ifSoftwrapIsEnabled
#[test]
fn an_ellipsis_with_wrapping_keeps_the_lines() {
    let result = lay_out(
        MULTI_LINE_TEXT,
        TextMeasureOptions {
            soft_wrap: true,
            overflow: TextOverflow::Ellipsis,
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.multi_paragraph.line_count(), 5);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, disabledSoftwrap_andOverflowClip_shouldConstrainLayoutSize
#[test]
fn a_clipped_text_without_wrapping_measures_its_constraints() {
    let result = lay_out(
        LONG_TEXT,
        TextMeasureOptions {
            soft_wrap: false,
            overflow: TextOverflow::Clip,
            constraints: at_most_200(),
            ..TextMeasureOptions::default()
        },
    );
    assert_ne!(result.multi_paragraph.width(), 200.0);
    assert_eq!(result.size.width, 200);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, disabledSoftwrap_andOverflowVisible_shouldConstrainLayoutSize
#[test]
fn a_text_drawn_past_its_bounds_without_wrapping_measures_its_constraints() {
    let result = lay_out(
        LONG_TEXT,
        TextMeasureOptions {
            soft_wrap: false,
            overflow: TextOverflow::Visible,
            constraints: at_most_200(),
            ..TextMeasureOptions::default()
        },
    );
    assert_ne!(result.multi_paragraph.width(), 200.0);
    assert_eq!(result.size.width, 200);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, textLayout_cannotBeSmallerThan_minWidth
#[test]
fn a_text_measures_at_least_the_minimum_width() {
    let result = lay_out(
        "A",
        TextMeasureOptions {
            soft_wrap: false,
            constraints: Constraints::new(400, 400, 0, Constraints::INFINITY),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.size.width, 400);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, textLayout_canBeSmallerThan_maxWidth
#[test]
fn a_text_can_measure_less_than_the_maximum_width() {
    let result = lay_out(
        "A",
        TextMeasureOptions {
            style: TextStyle { font_size: 10.sp(), ..TextStyle::DEFAULT },
            soft_wrap: false,
            constraints: Constraints::new(0, 400, 0, Constraints::INFINITY),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.size.width, 10);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, textLayout_cannotBeSmallerThan_minHeight
#[test]
fn a_text_measures_at_least_the_minimum_height() {
    let result = lay_out(
        "A",
        TextMeasureOptions {
            soft_wrap: false,
            constraints: Constraints::new(0, Constraints::INFINITY, 400, 400),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.size.height, 400);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, textLayout_canBeSmallerThan_maxHeight
#[test]
fn a_text_can_measure_less_than_the_maximum_height() {
    let result = lay_out(
        "A",
        TextMeasureOptions {
            style: TextStyle { font_size: 10.sp(), ..TextStyle::DEFAULT },
            soft_wrap: false,
            constraints: Constraints::new(0, Constraints::INFINITY, 0, 400),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.size.height, 10);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, layoutDirection_shouldDictate_textDirection
#[test]
fn the_layout_direction_gives_a_text_without_a_direction_its_direction() {
    let by_layout = lay_out(
        MULTI_LINE_TEXT,
        TextMeasureOptions {
            layout_direction: Some(LayoutDirection::Rtl),
            ..TextMeasureOptions::default()
        },
    );
    let by_style = lay_out(
        MULTI_LINE_TEXT,
        TextMeasureOptions {
            style: TextStyle { text_direction: Some(TextDirection::Rtl), ..TextStyle::DEFAULT },
            ..TextMeasureOptions::default()
        },
    );
    // Compose compares pictures; here, where each line is. This vouches for the fixed measurer
    // only: SkParagraph's and Android's go by the first strong character, whatever the direction
    // (the ledger's text-direction), and would lay this Latin text out left to right both ways.
    for line in 0..by_layout.line_count as usize {
        assert_eq!(by_layout.get_line_left(line), by_style.get_line_left(line));
        assert_eq!(by_layout.get_line_right(line), by_style.get_line_right(line));
    }
    assert_eq!(by_layout.get_paragraph_direction(0), by_style.get_paragraph_direction(0));
}

/// Two layouts in `first` and `second`, on one measurer with a cache: the same paragraph, each
/// result with its own style.
fn cached_results(first: TextStyle, second: TextStyle) -> (TextLayoutResult, TextLayoutResult) {
    let measurer = text_measurer(8);
    let measure = |style| {
        measurer.measure(LONG_TEXT, TextMeasureOptions { style, ..TextMeasureOptions::default() })
    };
    let (first, second) = (measure(first), measure(second));
    assert!(Rc::ptr_eq(&first.multi_paragraph, &second.multi_paragraph));
    (first, second)
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, colorShouldChangeInResult_whenCacheIsActive
#[test]
fn a_cached_layout_has_the_color_asked_for() {
    let (first, second) = cached_results(red(), blue());
    assert_eq!(first.layout_input.style.color, Color::RED);
    assert_eq!(second.layout_input.style.color, Color::BLUE);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, brushShouldChangeInResult_whenCacheIsActive
#[test]
fn a_cached_layout_has_the_brush_asked_for() {
    let first_style = gradient(vec![Color::RED, Color::GREEN]);
    let second_style = gradient(vec![Color::RED, Color::BLUE]);
    let (first, second) = cached_results(first_style.clone(), second_style.clone());
    assert_eq!(first.layout_input.style.brush, first_style.brush);
    assert_eq!(second.layout_input.style.brush, second_style.brush);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, shadowShouldChangeInResult_whenCacheIsActive
#[test]
fn a_cached_layout_has_the_shadow_asked_for() {
    let (first, second) = cached_results(shadowed(Color::RED), shadowed(Color::BLUE));
    assert_eq!(first.layout_input.style.shadow, shadowed(Color::RED).shadow);
    assert_eq!(second.layout_input.style.shadow, shadowed(Color::BLUE).shadow);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, textDecorationShouldChangeInResult_whenCacheIsActive
#[test]
fn a_cached_layout_has_the_decoration_asked_for() {
    let (first, second) = cached_results(
        decorated(TextDecoration::UNDERLINE),
        decorated(TextDecoration::LINE_THROUGH),
    );
    assert_eq!(first.layout_input.style.text_decoration, Some(TextDecoration::UNDERLINE));
    assert_eq!(second.layout_input.style.text_decoration, Some(TextDecoration::LINE_THROUGH));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, decreasingMinWidth_decreasesTheCalculatedWidth
#[test]
fn a_smaller_minimum_width_is_a_new_layout_at_a_smaller_width() {
    let measurer = text_measurer(8);
    let at = |minimum| {
        measurer.measure(
            "Hello",
            TextMeasureOptions {
                constraints: Constraints::new(
                    minimum,
                    Constraints::INFINITY,
                    0,
                    Constraints::INFINITY,
                ),
                ..TextMeasureOptions::default()
            },
        )
    };
    let (first, second) = (at(1000), at(500));
    assert!(!Rc::ptr_eq(&first.multi_paragraph, &second.multi_paragraph));
    assert_eq!(first.size.width, 1000);
    assert_eq!(second.size.width, 500);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextMeasurerTest.kt, emptyConstraints_hugeString_dontCrash
#[test]
fn a_huge_text_without_constraints_measures() {
    let result = lay_out(&"A".repeat(100_000), TextMeasureOptions::default());
    assert!(result.size.width > 0);
}

// ---- compose-rs's own: what the measurer shares with its host ----------------------------

#[test]
fn a_cached_result_is_copied_with_its_own_input_and_size() {
    let measurer = text_measurer(8);
    let first = measurer.measure("Hello", TextMeasureOptions::default());
    let second = measurer.measure("Hello", TextMeasureOptions::default());
    assert_eq!(first, second);
    assert_eq!(second.size, compose_ui::IntSize::new(5 * 14, 14));
    assert_eq!(second.first_baseline, first.first_baseline);
    assert_eq!(second.line_count, 1);
}

#[test]
fn another_span_misses_a_measurers_cache() {
    let measurer = text_measurer(8);
    let plain = measurer.measure("Hello", TextMeasureOptions::default());
    let styled = measurer.measure(with_a_span(30.0), TextMeasureOptions::default());
    assert!(!Rc::ptr_eq(&plain.multi_paragraph, &styled.multi_paragraph));
    assert!(styled.size.width > plain.size.width);
}

#[test]
fn a_cached_result_under_minimum_constraints_keeps_its_constrained_size() {
    let measurer = text_measurer(8);
    let options = || TextMeasureOptions {
        constraints: Constraints::new(400, 400, 300, 300),
        ..TextMeasureOptions::default()
    };
    let first = measurer.measure("Hello", options());
    let second = measurer.measure("Hello", options());
    assert!(Rc::ptr_eq(&first.multi_paragraph, &second.multi_paragraph));
    assert_eq!(second.size, compose_ui::IntSize::new(400, 300));
    assert_eq!(second.size, first.size);
}

#[test]
fn skipping_the_cache_lays_the_text_out_anew() {
    let measurer = text_measurer(8);
    let first = measurer.measure("Hello", TextMeasureOptions::default());
    let second = measurer
        .measure("Hello", TextMeasureOptions { skip_cache: true, ..TextMeasureOptions::default() });
    assert!(!Rc::ptr_eq(&first.multi_paragraph, &second.multi_paragraph));
    // The new layout is what's kept, as Compose's puts it whatever `skipCache` says.
    let third = measurer.measure("Hello", TextMeasureOptions::default());
    assert!(Rc::ptr_eq(&second.multi_paragraph, &third.multi_paragraph));
}

#[test]
fn the_measurers_defaults_are_what_a_measurement_lays_out_with() {
    let measurer =
        TextMeasurer::new(resolver(), locale_list(), Density::new(2.0), LayoutDirection::Rtl, 0);
    let result = measurer.measure("Hello", TextMeasureOptions::default());
    assert_eq!(result.layout_input.density, Density::new(2.0));
    assert_eq!(result.layout_input.layout_direction, LayoutDirection::Rtl);
    assert_eq!(result.layout_input.default_locale_list, locale_list());
    // 14 sp at density 2, five glyphs.
    assert_eq!(result.size.width, 5 * 28);
    let other = measurer.measure(
        "Hello",
        TextMeasureOptions {
            density: Some(Density::new(1.0)),
            layout_direction: Some(LayoutDirection::Ltr),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(other.size.width, 5 * 14);
    assert_eq!(other.layout_input.layout_direction, LayoutDirection::Ltr);
}

#[test]
#[should_panic(expected = "max_lines")]
fn no_lines_at_all_panics_as_composes_measure_throws() {
    lay_out("Hello", TextMeasureOptions { max_lines: 0, ..TextMeasureOptions::default() });
}

/// The paragraphs live in `shared`'s fixed measurer.
fn live_paragraphs(shared: &SharedTextMeasurer) -> usize {
    let measurer = shared.borrow();
    (&*measurer as &dyn std::any::Any)
        .downcast_ref::<FixedTextMeasurer>()
        .expect("the fixed measurer")
        .live()
}

#[test]
fn a_dropped_result_is_released_at_the_next_measurement() {
    let shared = platform();
    let measurer = TextMeasurer::new(
        create_font_family_resolver(&shared),
        locale_list(),
        Density::new(1.0),
        LayoutDirection::Ltr,
        0,
    );
    let first = measurer.measure("Hello", TextMeasureOptions::default());
    assert_eq!(live_paragraphs(&shared), 1);
    // A result answers queries while it's kept, and its clones keep it.
    let clone = first.clone();
    drop(first);
    assert_eq!(clone.get_line_end(0, false), 5);
    assert_eq!(live_paragraphs(&shared), 1);
    drop(clone);
    let _second = measurer.measure("World", TextMeasureOptions::default());
    assert_eq!(live_paragraphs(&shared), 1);
}

#[test]
fn an_evicted_layout_lives_while_a_result_keeps_it() {
    let shared = platform();
    let measurer = TextMeasurer::new(
        create_font_family_resolver(&shared),
        locale_list(),
        Density::new(1.0),
        LayoutDirection::Ltr,
        1,
    );
    let first = measurer.measure("Hello", TextMeasureOptions::default());
    measurer.measure("World", TextMeasureOptions::default());
    // "Hello" left the cache, its result keeps it; "World" left it, and nothing keeps it.
    measurer.measure("Again", TextMeasureOptions::default());
    shared.release_dropped();
    assert_eq!(first.get_line_end(0, false), 5);
    assert_eq!(live_paragraphs(&shared), 2);
}
