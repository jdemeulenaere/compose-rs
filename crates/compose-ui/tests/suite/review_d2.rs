//! Second phase D review: failing tests for the UI core after the fixes of 4af1e12. Each test
//! states the input, what Compose (or the docs) expects, and what happens instead.
//!
//! Checked and sound (no test kept): an `on_focus_changed` added above an already focused node
//! reports `true` on attach; removing the focused node reports `false` to an ancestor's
//! listener and clears the focus; a clip hides children placed outside it from hits; a state
//! write in an `on_key_event` handler wakes the platform.

#![allow(non_snake_case)]

use crate::support;

use compose_runtime::*;
use compose_ui::*;
use support::*;

// ---- Modifier equality -------------------------------------------------------------------------

/// `Dp::UNSPECIFIED` is NaN, and `SizeSpec` derives `PartialEq`, so any size element with an
/// unspecified bound (`width`, `height`, `size_in` with open bounds — what `Button` and
/// `TextField` use) never equals itself: `Element::same` and `Modifier::same` are false for
/// identical modifiers. Compose compares `Dp.Unspecified` equal to itself (its `equals` is on
/// the raw bits), so equal modifiers are equal.
#[test]
fn an_unspecified_size_bound_compares_equal_to_itself() {
    assert!(Modifier.width(10.dp()).same(&Modifier.width(10.dp())));
}

#[composable]
fn Child(modifier: Modifier) {
    log("child");
    Leaf(modifier);
}

#[composable]
fn Parent(tick: MutableState<u32>) {
    let _ = tick.get();
    Child(Modifier.width(10.dp()));
}

/// The consequence: a composable receiving `Modifier.width(10.dp())` never skips (its body
/// runs whenever its parent's does), and its node is updated and measured again each time.
/// In Compose it skips, and nothing is measured.
#[test]
fn a_receiver_of_an_unchanged_width_modifier_skips_and_measures_nothing() {
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let mut host = host(move || Parent(handle));
    take_log();
    tick.set(1);
    let stats = host.frame();
    assert_eq!(take_log(), Vec::<String>::new(), "the child ran again");
    assert!(!stats.measured, "the frame measured");
}

// ---- Measuring twice ---------------------------------------------------------------------------

/// A layout modifier measuring its content twice: first to learn its natural size, then with
/// fixed constraints. Compose rejects the second `measure` of the wrapped measurable, as it
/// does for children; 4af1e12 added the check for a policy's children only (`measure` checks
/// `measured_in` when `layer == 0`). Unrejected, the in-place remeasure of the content's child
/// (with the last, fixed constraints) hides the change the natural measurement would see: the
/// node keeps its old natural size.
struct MeasureContentTwice;

struct MeasureContentTwiceNode;

impl ModifierNodeElement for MeasureContentTwice {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(MeasureContentTwiceNode)
    }
    fn update(&self, _: &mut dyn ModifierNode) {}
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<MeasureContentTwice>().is_some()
    }
}

impl ModifierNode for MeasureContentTwiceNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for MeasureContentTwiceNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        _constraints: Constraints,
    ) -> MeasureResult {
        let natural = scope.measure(content, Constraints::unbounded());
        let fixed = scope.measure(content, Constraints::fixed(10, 10));
        scope.place(&fixed, 0, 0);
        scope.layout(natural.width, natural.height)
    }
}

#[test]
#[should_panic(expected = "twice")]
fn a_layout_modifier_measuring_its_content_twice_is_rejected_or_stays_correct() {
    let width = mutable_state_of(20.0f32);
    let handle = width.handle();
    let mut changed = host(move || {
        Column!(modifier = Modifier.then_custom(MeasureContentTwice), {
            Leaf(Modifier.size_wh(handle.get().dp(), 5.dp()));
        });
    });
    width.set(30.0);
    changed.frame();
    let fresh = host(|| {
        Column!(modifier = Modifier.then_custom(MeasureContentTwice), {
            Leaf(Modifier.size_wh(30.dp(), 5.dp()));
        });
    });
    // Fresh: the node is 30 wide. Incremental: still 20.
    assert_eq!(changed.dump_tree(), fresh.dump_tree());
}
