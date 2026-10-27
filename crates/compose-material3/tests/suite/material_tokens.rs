//! Material 3 1.4.0's tokens, as the Kotlin bench app gets them (read with `javap` from the
//! material3 1.4.0 classes in the Gradle cache; docs/15, "What differs"): the type scale, and
//! the colors of the components the stress screen shows (D570, D572, D573).

use std::cell::Cell;
use std::rc::Rc;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, FontWeight, LineHeightAlignment, LineHeightStyle, LineHeightTrim, TextStyle, TextUnit,
    UnitExt,
};

/// `TypeScaleTokens`: each style's size, line height, tracking and weight, over Material's
/// default text style (`DefaultLineHeightStyle`: centered, untrimmed).
#[test]
fn the_type_scale_is_material_3_1_4s() {
    let typography = Typography::default();
    let (normal, medium) = (FontWeight::NORMAL, FontWeight::MEDIUM);
    let expected = [
        (typography.display_large, 57.0, 64.0, -0.2, normal),
        (typography.display_medium, 45.0, 52.0, 0.0, normal),
        (typography.display_small, 36.0, 44.0, 0.0, normal),
        (typography.headline_large, 32.0, 40.0, 0.0, normal),
        (typography.headline_medium, 28.0, 36.0, 0.0, normal),
        (typography.headline_small, 24.0, 32.0, 0.0, normal),
        (typography.title_large, 22.0, 28.0, 0.0, normal),
        (typography.title_medium, 16.0, 24.0, 0.2, medium),
        (typography.title_small, 14.0, 20.0, 0.1, medium),
        (typography.body_large, 16.0, 24.0, 0.5, normal),
        (typography.body_medium, 14.0, 20.0, 0.2, normal),
        (typography.body_small, 12.0, 16.0, 0.4, normal),
        (typography.label_large, 14.0, 20.0, 0.1, medium),
        (typography.label_medium, 12.0, 16.0, 0.5, medium),
        (typography.label_small, 11.0, 16.0, 0.5, medium),
    ];
    let centered = LineHeightStyle::new(LineHeightAlignment::CENTER, LineHeightTrim::None);
    for (style, size, line_height, tracking, weight) in expected {
        assert_eq!(style.font_size, TextUnit::sp(size));
        assert_eq!(style.line_height, TextUnit::sp(line_height), "{size} sp");
        assert_eq!(style.letter_spacing, TextUnit::sp(tracking), "{size} sp");
        assert_eq!(style.font_weight, Some(weight), "{size} sp");
        assert_eq!(style.line_height_style, Some(centered), "{size} sp");
        assert_eq!(style.color, Color::UNSPECIFIED);
    }
    assert_eq!(DEFAULT_LINE_HEIGHT_STYLE, centered);
}

/// A style keeps its tracking and line height style through a merge that doesn't set them, as
/// `Text`'s parameters over the theme's style; a merge that sets one replaces it.
#[test]
fn a_merged_style_keeps_its_tracking_and_line_height_style() {
    let body = Typography::default().body_medium;
    let merged = body.merge(&TextStyle { font_size: TextUnit::sp(20.0), ..TextStyle::DEFAULT });
    assert_eq!(merged.letter_spacing, TextUnit::sp(0.2));
    assert_eq!(merged.line_height_style, Some(DEFAULT_LINE_HEIGHT_STYLE));
    let proportional = LineHeightStyle::DEFAULT;
    assert_eq!(proportional.alignment, LineHeightAlignment::PROPORTIONAL);
    assert_eq!(proportional.trim, LineHeightTrim::Both);
    let restyled =
        body.merge(&TextStyle { line_height_style: Some(proportional), ..TextStyle::DEFAULT });
    assert_eq!(restyled.line_height_style, Some(proportional));
    // A different line height style lays the text out differently.
    assert!(!restyled.has_same_layout_affecting_attributes(&body));
}

/// The button colors and outline of Material 3 1.4's tokens: `OutlinedButtonTokens`' label in
/// the on-surface-variant color and outline in the outline-variant one (the primary and outline
/// colors before), the disabled colors of `FilledButtonTokens`, `ElevatedButtonTokens`,
/// `FilledTonalButtonTokens` and `TextButtonTokens`, and `ProgressIndicatorTokens.TrackColor`.
#[test]
fn the_buttons_and_tracks_colors_are_material_3_1_4s() {
    type Seen = (
        ColorScheme,
        [ButtonColors; 5],
        compose_foundation::BorderStroke,
        compose_foundation::BorderStroke,
        Color,
    );
    let seen: Rc<Cell<Option<Seen>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let _ui = compose_test(move || {
        sink.set(Some((
            MaterialTheme::color_scheme(),
            [
                ButtonDefaults::button_colors(),
                ButtonDefaults::elevated_button_colors(),
                ButtonDefaults::filled_tonal_button_colors(),
                ButtonDefaults::outlined_button_colors(),
                ButtonDefaults::text_button_colors(),
            ],
            ButtonDefaults::outlined_button_border(true),
            ButtonDefaults::outlined_button_border(false),
            ProgressIndicatorDefaults::linear_track_color(),
        )));
    });
    let (scheme, [filled, elevated, tonal, outlined, text], border, disabled_border, track) =
        seen.take().unwrap();
    let dim_variant = scheme.on_surface_variant.with_alpha(0.38);
    assert_eq!(
        (outlined.container_color, outlined.content_color, outlined.disabled_content_color),
        (Color::TRANSPARENT, scheme.on_surface_variant, dim_variant)
    );
    assert_eq!((border.width, border.brush), (1.dp(), scheme.outline_variant.into()));
    assert_eq!(disabled_border.brush, scheme.outline_variant.with_alpha(0.1).into());
    assert_eq!(
        (filled.disabled_container_color, filled.disabled_content_color),
        (scheme.on_surface.with_alpha(0.1), dim_variant)
    );
    assert_eq!(
        (elevated.disabled_container_color, elevated.disabled_content_color),
        (scheme.on_surface.with_alpha(0.1), dim_variant)
    );
    assert_eq!(
        (tonal.disabled_container_color, tonal.disabled_content_color),
        (scheme.on_surface.with_alpha(0.12), scheme.on_surface.with_alpha(0.38))
    );
    assert_eq!((text.content_color, text.disabled_content_color), (scheme.primary, dim_variant));
    assert_eq!(track, scheme.secondary_container);
}

/// The flat chips' outline is `FlatOutlineColor` (`AssistChipTokens`, `SuggestionChipTokens`;
/// `FilterChipTokens.FlatUnselectedOutlineColor`): 1 dp of the outline-variant color; a
/// selected filter chip has none. Each is drawn outside the chip's clip, as Compose's
/// `Surface` orders its border, color and clip (D573).
#[test]
fn the_chips_outlines_are_outline_variant_outside_their_clip() {
    #[composable]
    #[allow(non_snake_case)]
    fn Chips() {
        compose_foundation::layout::Column!({
            AssistChip!(|| {}, {
                Text!("Assist");
            });
            SuggestionChip!(|| {}, {
                Text!("Hint");
            });
            FilterChip!(false, || {}, {
                Text!("Off");
            });
            FilterChip!(true, || {}, {
                Text!("On");
            });
        });
    }
    let ui = compose_test(|| {
        MaterialTheme!({
            Chips();
        });
    });
    let outline = format!("{:?}", light_color_scheme().outline_variant);
    let dump = ui.dump_tree();
    let chips: Vec<&str> = dump.lines().filter(|line| line.contains("clickable")).collect();
    assert_eq!(chips.len(), 4, "{dump}");
    for chip in &chips[..3] {
        let border = chip.find(&format!("border(1.dp, {outline}")).expect(chip);
        assert!(border < chip.find("clip(").expect(chip), "the border outside the clip: {chip}");
    }
    assert!(!chips[3].contains("border("), "a selected filter chip has no outline");
}

/// `LocalTextStyle`'s default is Material's `DefaultTextStyle` (material3 1.4.0's
/// `TextKt.LocalTextStyle` reads `TypographyTokensKt.getDefaultTextStyle`): a text outside a
/// theme centers its line height too.
#[test]
fn the_local_text_styles_default_is_materials_default_text_style() {
    let seen = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let _ui = compose_test(move || sink.set(Some(LOCAL_TEXT_STYLE.current())));
    let style = seen.take().unwrap();
    assert_eq!(style, DEFAULT_TEXT_STYLE);
    assert_eq!(style.line_height_style, Some(DEFAULT_LINE_HEIGHT_STYLE));
    assert_eq!(TextStyle { line_height_style: None, ..style }, TextStyle::DEFAULT);
}
