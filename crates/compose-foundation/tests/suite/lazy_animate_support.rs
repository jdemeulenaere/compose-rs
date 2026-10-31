//! What the `animate_item` tests share: stepping an animation frame by frame, positions in the
//! root, and the color a point of the display list comes to, for what Compose's tests read from
//! pixels (this suite has no renderer). At density 1.

use std::time::Duration;

use compose_animation::{AnimationSpec, LINEAR_EASING, tween};
use compose_test::*;
use compose_ui::platform::{DrawOp, FixedTextMeasurer, UiHost};
use compose_ui::{Color, Density, IntSize, Offset};

/// Compose's tests' `Duration` and `FrameDuration`: 4 frames, so fractions of a quarter.
pub const DURATION: u64 = 64;
pub const FRAME: u64 = 16;

/// A rule at density 1 whose clock moves only as the test says, as Compose's animation tests'.
pub fn paused(size: IntSize, content: impl Fn() + 'static) -> ComposeTestRule {
    let host = UiHost::new(size, Density::default(), Box::new(FixedTextMeasurer::new()));
    let mut rule = ComposeTestRule::empty(host);
    rule.set_auto_advance(false);
    rule.set_content(content);
    rule
}

/// Compose's tests' `AnimSpec`: linear, `DURATION` long.
pub fn anim_spec() -> AnimationSpec {
    tween(DURATION as u32).easing(LINEAR_EASING).into()
}

/// Runs the frame that lays out what's asked and the one its animations start at, then `on_frame`
/// at each `FRAME` of `duration`, with the fraction of it passed, as Compose's tests'
/// `onAnimationFrame`, with a frame more: here `items(list)` is read in the content's composition
/// at the frame, where Compose's is read in measure, and what changes in a composition takes this
/// frame there too (`animateAlignmentChange` adds it itself, `LazyListItemPlacementAnimationTest.kt`,
/// lines 831-832). The frame of the change draws the old place.
pub fn on_animation_frame_of(
    ui: &mut ComposeTestRule,
    duration: u64,
    mut on_frame: impl FnMut(&mut ComposeTestRule, f32),
) {
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let mut passed = 0;
    loop {
        on_frame(ui, passed as f32 / duration as f32);
        if passed == duration {
            return;
        }
        ui.advance_time_by(Duration::from_millis(FRAME));
        passed += FRAME;
    }
}

pub fn on_animation_frame(
    ui: &mut ComposeTestRule,
    on_frame: impl FnMut(&mut ComposeTestRule, f32),
) {
    on_animation_frame_of(ui, DURATION, on_frame);
}

/// Where `tag` is in the root, rounded, as `positionInRoot.round()`.
pub fn position_of(ui: &mut ComposeTestRule, tag: &str) -> (i32, i32) {
    let position: Offset = ui.on_node_with_tag(tag).fetch_semantics_node().position_in_root();
    (position.x.round() as i32, position.y.round() as i32)
}

pub fn size_of(ui: &mut ComposeTestRule, tag: &str) -> (i32, i32) {
    let size = ui.on_node_with_tag(tag).fetch_semantics_node().size();
    (size.width, size.height)
}

/// A color over transparent, in floats: what's painted so far at a point.
#[derive(Clone, Copy, Debug, Default)]
pub struct Composite {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Composite {
    fn over(&mut self, color: Color, alpha: f32) {
        let source = color.alpha() as f32 / 255.0 * alpha;
        let channel = |under: f32, top: u8| top as f32 / 255.0 * source + under * (1.0 - source);
        self.red = channel(self.red, color.red());
        self.green = channel(self.green, color.green());
        self.blue = channel(self.blue, color.blue());
        self.alpha = source + self.alpha * (1.0 - source);
    }

    /// Whether it's `color` at `alpha` (premultiplied, as the point's painted), give or take a
    /// rounding of the animation's.
    pub fn is(&self, color: Color, alpha: f32) -> bool {
        let close = |one: f32, other: f32| (one - other).abs() < 0.02;
        let channel = |channel: u8| channel as f32 / 255.0 * alpha;
        close(self.alpha, color.alpha() as f32 / 255.0 * alpha)
            && close(self.red, channel(color.red()))
            && close(self.green, channel(color.green()))
            && close(self.blue, channel(color.blue()))
    }

    pub fn is_transparent(&self) -> bool {
        self.alpha < 0.02
    }
}

/// What the last frame painted at (`x`, `y`) in the root: rectangles, in their clips, their layers'
/// alphas and moves.
pub fn color_at(ui: &mut ComposeTestRule, x: f32, y: f32) -> Composite {
    #[derive(Clone, Copy)]
    struct Saved {
        at: (f32, f32),
        alpha: f32,
        clipped: bool,
    }
    let mut state = Saved { at: (0.0, 0.0), alpha: 1.0, clipped: false };
    let mut stack = Vec::new();
    let mut result = Composite::default();
    let list = ui.host().display_list();
    for op in &list.ops {
        match op {
            DrawOp::Save => stack.push(state),
            DrawOp::SaveLayerAlpha { alpha, .. } => {
                stack.push(state);
                state.alpha *= *alpha;
            }
            DrawOp::Restore => state = stack.pop().expect("a save"),
            DrawOp::Translate(by) => {
                state.at = (state.at.0 + by.x, state.at.1 + by.y);
            }
            DrawOp::Transform { translation, .. } => {
                state.at = (state.at.0 + translation.x, state.at.1 + translation.y);
            }
            DrawOp::ClipRect(rect) => {
                let (px, py) = (x - state.at.0, y - state.at.1);
                if px < rect.left || px >= rect.right || py < rect.top || py >= rect.bottom {
                    state.clipped = true;
                }
            }
            DrawOp::DrawRect(rect, paint) if !state.clipped => {
                let (px, py) = (x - state.at.0, y - state.at.1);
                if px >= rect.left && px < rect.right && py >= rect.top && py < rect.bottom {
                    result.over(paint.color, state.alpha);
                }
            }
            _ => {}
        }
    }
    result
}
