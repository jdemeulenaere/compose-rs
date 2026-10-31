//! `Modifier::non_interactive_scrollbar`, as Compose's Material 3 (`Scrollbar.kt`): a thin thumb
//! over a scrollable that says where it is, fading in while it scrolls.

use std::rc::Rc;
use std::time::Duration;

use compose_animation::{Animatable, tween};
use compose_foundation::ScrollIndicatorState;
use compose_foundation::gestures::Orientation;
use compose_runtime::{CoroutineScope, Job};
use compose_ui::{
    Color, Dp, DrawModifierNode, DrawScope, LayoutDirection, Modifier, ModifierNode,
    ModifierNodeElement, NodeContext, Paint, Rect,
};

use crate::theme::MaterialTheme;

/// How a [`non_interactive_scrollbar`](NonInteractiveScrollbarModifiers::non_interactive_scrollbar)
/// looks: `NonInteractiveScrollbarDefaults::style()`, changed by struct update.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NonInteractiveScrollbarStyle {
    pub thumb_color: Color,
    pub track_color: Color,
    pub thickness: Dp,
    pub thumb_min_length: Dp,
    /// Of the track's length, between 0 and 1.
    pub thumb_max_length_fraction: f32,
    /// Shown only while scrolling and for `fade_delay_millis` after, if so.
    pub is_fade_enabled: bool,
    pub fade_duration_millis: u32,
    pub fade_delay_millis: u32,
    pub main_axis_track_inset: Dp,
    pub cross_axis_track_inset: Dp,
}

/// As Compose's `NonInteractiveScrollbarDefaults`.
pub struct NonInteractiveScrollbarDefaults;

impl NonInteractiveScrollbarDefaults {
    pub const THUMB_OPACITY: f32 = 0.7;
    pub const THUMB_FADE_DURATION_MILLIS: u32 = 250;
    pub const THUMB_FADE_DELAY_MILLIS: u32 = 400;
    pub const THICKNESS: Dp = Dp(4.0);
    pub const THUMB_MIN_LENGTH: Dp = Dp(24.0);
    pub const THUMB_MAX_LENGTH_FRACTION: f32 = 0.9;
    pub const MAIN_AXIS_TRACK_INSET: Dp = Dp(2.0);
    pub const CROSS_AXIS_TRACK_INSET: Dp = Dp(0.0);

    /// The thumb's color, as `thumbColor`.
    pub fn thumb_color() -> Color {
        MaterialTheme::color_scheme().outline.with_alpha(Self::THUMB_OPACITY)
    }

    /// Compose's parameters' defaults.
    pub fn style() -> NonInteractiveScrollbarStyle {
        NonInteractiveScrollbarStyle {
            thumb_color: Self::thumb_color(),
            track_color: Color::TRANSPARENT,
            thickness: Self::THICKNESS,
            thumb_min_length: Self::THUMB_MIN_LENGTH,
            thumb_max_length_fraction: Self::THUMB_MAX_LENGTH_FRACTION,
            is_fade_enabled: true,
            fade_duration_millis: Self::THUMB_FADE_DURATION_MILLIS,
            fade_delay_millis: Self::THUMB_FADE_DELAY_MILLIS,
            main_axis_track_inset: Self::MAIN_AXIS_TRACK_INSET,
            cross_axis_track_inset: Self::CROSS_AXIS_TRACK_INSET,
        }
    }
}

/// `Modifier::non_interactive_scrollbar`.
pub trait NonInteractiveScrollbarModifiers {
    /// Draws a thumb over the content for where `state` is, as `Modifier.nonInteractiveScrollbar`;
    /// nothing while the content fits. It doesn't take input.
    fn non_interactive_scrollbar(
        self,
        state: Rc<dyn ScrollIndicatorState>,
        orientation: Orientation,
        style: NonInteractiveScrollbarStyle,
    ) -> Modifier;
}

impl NonInteractiveScrollbarModifiers for Modifier {
    fn non_interactive_scrollbar(
        self,
        state: Rc<dyn ScrollIndicatorState>,
        orientation: Orientation,
        style: NonInteractiveScrollbarStyle,
    ) -> Modifier {
        assert!(
            (0.0..=1.0).contains(&style.thumb_max_length_fraction),
            "thumbMaxLengthFraction must be between 0f and 1f"
        );
        self.then_custom(ScrollbarElement { state, orientation, style })
    }
}

struct ScrollbarElement {
    state: Rc<dyn ScrollIndicatorState>,
    orientation: Orientation,
    style: NonInteractiveScrollbarStyle,
}

struct ScrollbarNode {
    state: Rc<dyn ScrollIndicatorState>,
    orientation: Orientation,
    style: NonInteractiveScrollbarStyle,
    alpha: Animatable<f32>,
    /// The offset last drawn: a new one is a scroll, which shows the thumb.
    last_offset: i32,
    scope: Option<CoroutineScope>,
    /// Shows it, waits, fades it: started over by each scroll.
    fade: Option<Job>,
}

impl ModifierNodeElement for ScrollbarElement {
    type Node = ScrollbarNode;
    fn create(&self) -> ScrollbarNode {
        ScrollbarNode {
            state: self.state.clone(),
            orientation: self.orientation,
            style: self.style,
            alpha: Animatable::new(if self.style.is_fade_enabled { 0.0 } else { 1.0 }),
            last_offset: -1,
            scope: None,
            fade: None,
        }
    }
    /// As Compose's wrapper, which makes its node again: faded out if it fades, and what it finds
    /// of a new state isn't a scroll.
    fn update(&self, node: &mut ScrollbarNode) {
        node.alpha = Animatable::new(if self.style.is_fade_enabled { 0.0 } else { 1.0 });
        if let Some(fade) = node.fade.take() {
            fade.cancel();
        }
        node.state = self.state.clone();
        node.orientation = self.orientation;
        node.style = self.style;
        node.last_offset = node.state.scroll_offset();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.state, &other.state)
            && self.orientation == other.orientation
            && self.style == other.style
    }
    fn name(&self) -> &'static str {
        "nonInteractiveScrollbar"
    }
}

impl ModifierNode for ScrollbarNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.scope = Some(context.coroutine_scope());
        // Attached again, what it finds isn't a scroll.
        self.last_offset = self.state.scroll_offset();
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        if let Some(fade) = self.fade.take() {
            fade.cancel();
        }
        self.scope = None;
    }
}

impl ScrollbarNode {
    fn show(&mut self) {
        let Some(scope) = self.scope else { return };
        if let Some(fade) = self.fade.take() {
            fade.cancel();
        }
        let alpha = self.alpha.clone();
        let delay = Duration::from_millis(u64::from(self.style.fade_delay_millis));
        let duration = self.style.fade_duration_millis;
        self.fade = Some(scope.launch(async move {
            alpha.snap_to(1.0);
            compose_runtime::delay(delay).await;
            alpha.animate_to(0.0, tween(duration)).await;
        }));
    }
}

impl DrawModifierNode for ScrollbarNode {
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        let content = self.state.content_size() as f32;
        let viewport = self.state.viewport_size() as f32;
        // Nor reading the offset, which would draw this again for nothing.
        if content <= viewport || viewport == 0.0 || content == 0.0 {
            return;
        }
        let offset = self.state.scroll_offset();
        let style = self.style;
        if style.is_fade_enabled && offset != self.last_offset {
            self.last_offset = offset;
            self.show();
        }
        let alpha = if style.is_fade_enabled { self.alpha.value() } else { 1.0 };
        let faded = |color: Color| {
            if alpha == 1.0 {
                color
            } else {
                color.with_alpha(color.alpha() as f32 / 255.0 * alpha)
            }
        };
        let (thumb_color, track_color) = (faded(style.thumb_color), faded(style.track_color));
        let track_visible = track_color.alpha() > 0;
        let thumb_visible = thumb_color.alpha() > 0;
        if !track_visible && !thumb_visible {
            return;
        }
        let density = scope.density();
        let size = scope.size();
        let main_inset = density.to_px(style.main_axis_track_inset);
        let cross_inset = density.to_px(style.cross_axis_track_inset);
        let vertical = self.orientation == Orientation::Vertical;
        let track_length = if vertical { size.height } else { size.width } - 2.0 * main_inset;
        let min_thumb = density.to_px(style.thumb_min_length);
        // Too short for the shortest thumb.
        if track_length <= 0.0 || track_length < min_thumb {
            return;
        }
        let max_thumb = track_length * style.thumb_max_length_fraction;
        let thickness = density.to_px(style.thickness);
        // As `coerceIn`, whose order at a maximum under the minimum this keeps from panicking.
        let thumb_length =
            (track_length * (viewport / content)).max(min_thumb).min(max_thumb.max(min_thumb));
        let max_offset = content - viewport;
        let thumb_offset = if max_offset > 0.0 {
            offset as f32 / max_offset * (track_length - thumb_length)
        } else {
            0.0
        };
        let round = thickness / 2.0;
        let ltr = scope.layout_direction() == LayoutDirection::Ltr;
        let (track, thumb) = if vertical {
            let x = if ltr { size.width - cross_inset - thickness } else { cross_inset };
            let y = main_inset + thumb_offset;
            (
                Rect::new(x, main_inset, x + thickness, main_inset + track_length),
                Rect::new(x, y, x + thickness, y + thumb_length),
            )
        } else {
            let y = size.height - cross_inset - thickness;
            let x = if ltr {
                main_inset + thumb_offset
            } else {
                size.width - main_inset - thumb_length - thumb_offset
            };
            (
                Rect::new(main_inset, y, main_inset + track_length, y + thickness),
                Rect::new(x, y, x + thumb_length, y + thickness),
            )
        };
        if track_visible {
            scope.draw_rect(track, Paint::fill(track_color));
        }
        if thumb_visible {
            scope.draw_round_rect(thumb, [round; 4], Paint::fill(thumb_color));
        }
    }
}
