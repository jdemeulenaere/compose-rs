//! Bounds assertions in dp, as Compose's `BoundsAssertions.kt`: a node's size and position in
//! the root, with a tolerance (0.5 dp by default).

use compose_ui::{Density, Dp, DpRect, Rect};

use crate::interaction::SemanticsNodeInteraction;

/// Comparing a `Dp` with a tolerance, as Compose's `Dp.assertIsEqualTo`.
pub trait DpAssertions {
    /// Panics unless within `tolerance` of `expected` (an unspecified one only matches an
    /// unspecified value, an infinite one only itself), naming `subject` in the message.
    #[track_caller]
    fn assert_is_equal_to(self, expected: Dp, subject: &str, tolerance: Dp);
}

impl DpAssertions for Dp {
    #[track_caller]
    fn assert_is_equal_to(self, expected: Dp, subject: &str, tolerance: Dp) {
        assert!(
            is_within_tolerance(self, expected, tolerance),
            "compose-test: Actual {subject} is {}, expected {} (tolerance: {})",
            dp_string(self),
            dp_string(expected),
            dp_string(tolerance)
        );
    }
}

/// A `Dp` as Kotlin prints it: `10.0.dp`, `Dp.Unspecified`.
fn dp_string(dp: Dp) -> String {
    if dp.is_specified() { format!("{:?}.dp", dp.0) } else { "Dp.Unspecified".to_string() }
}

fn is_within_tolerance(actual: Dp, expected: Dp, tolerance: Dp) -> bool {
    if !expected.is_specified() {
        !actual.is_specified()
    } else if expected.0.is_infinite() {
        actual.0 == expected.0
    } else {
        (actual.0 - expected.0).abs() <= tolerance.0
    }
}

fn to_dp(rect: Rect, density: Density) -> DpRect {
    DpRect::new(
        density.to_dp(rect.left),
        density.to_dp(rect.top),
        density.to_dp(rect.right),
        density.to_dp(rect.bottom),
    )
}

impl SemanticsNodeInteraction<'_> {
    /// The node's bounds in the root in pixels, unclipped and clipped.
    #[track_caller]
    fn bounds_in_root_px(&mut self) -> (Rect, Rect) {
        let node = self.fetch_semantics_node();
        let bounds = self.rule.host.with_tree(|tree| tree.semantics_bounds_in_root(node.id));
        // The root has no semantics of its own: its bounds are the window's.
        bounds.unwrap_or((node.bounds, node.bounds))
    }

    fn tolerance(&self, tolerance: Option<Dp>) -> Dp {
        tolerance.unwrap_or_else(|| self.rule.bounds_assertion_tolerance())
    }

    /// The node's bounds in the root, unclipped, at its position through the graphics layers
    /// around it, as `getUnclippedBoundsInRoot`.
    #[track_caller]
    pub fn get_unclipped_bounds_in_root(&mut self) -> DpRect {
        let (unclipped, _) = self.bounds_in_root_px();
        to_dp(unclipped, self.rule.density())
    }

    /// The node's bounds in the root clipped by the clips around it (a scroll's viewport,
    /// `clip`), as `getBoundsInRoot`.
    #[track_caller]
    pub fn get_bounds_in_root(&mut self) -> DpRect {
        let (_, clipped) = self.bounds_in_root_px();
        to_dp(clipped, self.rule.density())
    }

    /// The node's touch bounds in the root: its bounds, as compose-rs's pointer input has no
    /// minimum touch target yet (Compose widens a small clickable's to 48 dp).
    #[track_caller]
    fn touch_bounds_in_root(&mut self) -> DpRect {
        self.get_unclipped_bounds_in_root()
    }

    #[track_caller]
    fn check(
        &mut self,
        subject: &str,
        expected: Dp,
        tolerance: Option<Dp>,
        pick: fn(&DpRect) -> Dp,
    ) {
        let tolerance = self.tolerance(tolerance);
        let bounds = self.get_unclipped_bounds_in_root();
        pick(&bounds).assert_is_equal_to(expected, subject, tolerance);
    }

    /// As `assertWidthIsEqualTo` (the default tolerance, 0.5 dp unless the rule's is set).
    #[track_caller]
    pub fn assert_width_is_equal_to(&mut self, expected: Dp) -> &mut Self {
        self.check("width", expected, None, DpRect::width);
        self
    }

    /// As `assertWidthIsEqualTo(expected, tolerance)`.
    #[track_caller]
    pub fn assert_width_is_equal_to_with_tolerance(
        &mut self,
        expected: Dp,
        tolerance: Dp,
    ) -> &mut Self {
        self.check("width", expected, Some(tolerance), DpRect::width);
        self
    }

    /// As `assertHeightIsEqualTo`.
    #[track_caller]
    pub fn assert_height_is_equal_to(&mut self, expected: Dp) -> &mut Self {
        self.check("height", expected, None, DpRect::height);
        self
    }

    /// As `assertHeightIsEqualTo(expected, tolerance)`.
    #[track_caller]
    pub fn assert_height_is_equal_to_with_tolerance(
        &mut self,
        expected: Dp,
        tolerance: Dp,
    ) -> &mut Self {
        self.check("height", expected, Some(tolerance), DpRect::height);
        self
    }

    /// As `assertWidthIsAtLeast`: at least `expected`, or within the tolerance of it.
    #[track_caller]
    pub fn assert_width_is_at_least(&mut self, expected: Dp) -> &mut Self {
        let width = self.get_unclipped_bounds_in_root().width();
        assert_is_at_least(width, expected, "width");
        self
    }

    /// As `assertHeightIsAtLeast`.
    #[track_caller]
    pub fn assert_height_is_at_least(&mut self, expected: Dp) -> &mut Self {
        let height = self.get_unclipped_bounds_in_root().height();
        assert_is_at_least(height, expected, "height");
        self
    }

    /// As `assertTouchWidthIsEqualTo`: the width of the node's touch bounds (its bounds in
    /// compose-rs, which has no minimum touch target yet).
    #[track_caller]
    pub fn assert_touch_width_is_equal_to(&mut self, expected: Dp) -> &mut Self {
        let tolerance = self.tolerance(None);
        self.touch_bounds_in_root().width().assert_is_equal_to(expected, "width", tolerance);
        self
    }

    /// As `assertTouchHeightIsEqualTo`.
    #[track_caller]
    pub fn assert_touch_height_is_equal_to(&mut self, expected: Dp) -> &mut Self {
        let tolerance = self.tolerance(None);
        self.touch_bounds_in_root().height().assert_is_equal_to(expected, "height", tolerance);
        self
    }

    /// As `assertPositionInRootIsEqualTo`: the node's top left in the root.
    #[track_caller]
    pub fn assert_position_in_root_is_equal_to(
        &mut self,
        expected_left: Dp,
        expected_top: Dp,
    ) -> &mut Self {
        self.check("left", expected_left, None, |bounds| bounds.left);
        self.check("top", expected_top, None, |bounds| bounds.top);
        self
    }

    /// As `assertPositionInRootIsEqualTo(left, top, tolerance)`.
    #[track_caller]
    pub fn assert_position_in_root_is_equal_to_with_tolerance(
        &mut self,
        expected_left: Dp,
        expected_top: Dp,
        tolerance: Dp,
    ) -> &mut Self {
        self.check("left", expected_left, Some(tolerance), |bounds| bounds.left);
        self.check("top", expected_top, Some(tolerance), |bounds| bounds.top);
        self
    }

    /// As `assertTopPositionInRootIsEqualTo`.
    #[track_caller]
    pub fn assert_top_position_in_root_is_equal_to(&mut self, expected_top: Dp) -> &mut Self {
        self.check("top", expected_top, None, |bounds| bounds.top);
        self
    }

    /// As `assertTopPositionInRootIsEqualTo(top, tolerance)`.
    #[track_caller]
    pub fn assert_top_position_in_root_is_equal_to_with_tolerance(
        &mut self,
        expected_top: Dp,
        tolerance: Dp,
    ) -> &mut Self {
        self.check("top", expected_top, Some(tolerance), |bounds| bounds.top);
        self
    }

    /// As `assertLeftPositionInRootIsEqualTo`.
    #[track_caller]
    pub fn assert_left_position_in_root_is_equal_to(&mut self, expected_left: Dp) -> &mut Self {
        self.check("left", expected_left, None, |bounds| bounds.left);
        self
    }

    /// As `assertLeftPositionInRootIsEqualTo(left, tolerance)`.
    #[track_caller]
    pub fn assert_left_position_in_root_is_equal_to_with_tolerance(
        &mut self,
        expected_left: Dp,
        tolerance: Dp,
    ) -> &mut Self {
        self.check("left", expected_left, Some(tolerance), |bounds| bounds.left);
        self
    }
}

#[track_caller]
fn assert_is_at_least(actual: Dp, expected: Dp, subject: &str) {
    let tolerance = Dp(0.5);
    assert!(
        is_within_tolerance(actual, expected, tolerance)
            || (actual.is_specified() && actual.0 > expected.0),
        "compose-test: Actual {subject} is {}, expected at least {} (tolerance: {})",
        dp_string(actual),
        dp_string(expected),
        dp_string(tolerance)
    );
}
