//! `Modifier.offset` and its kinds, as Compose's `OffsetTest`: what they change moves what they
//! wrap and measures nothing. Density 1 unless a test says otherwise.

use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

/// Counts what follows it being measured and placed, and keeps where it is.
#[derive(Clone, Default)]
struct OnLayout {
    measures: Rc<Cell<u32>>,
    places: Rc<Cell<u32>>,
    x: Rc<Cell<f32>>,
}

struct OnLayoutNode(OnLayout);

impl ModifierNode for OnLayoutNode {
    fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
        Some(self)
    }
}

impl LayoutAwareModifierNode for OnLayoutNode {
    fn on_placed(&mut self, coordinates: LayoutCoordinates, _: &mut NodeContext<'_>) {
        self.0.places.set(self.0.places.get() + 1);
        self.0.x.set(coordinates.position_in_root().x);
    }
    fn on_remeasured(&mut self, _: IntSize, _: &mut NodeContext<'_>) {
        self.0.measures.set(self.0.measures.get() + 1);
    }
}

struct OnLayoutElement(OnLayout);

impl ModifierNodeElement for OnLayoutElement {
    type Node = OnLayoutNode;
    fn create(&self) -> OnLayoutNode {
        OnLayoutNode(self.0.clone())
    }
    fn update(&self, _: &mut OnLayoutNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

/// Where the node tagged is, by what it says when placed.
fn at(told: &Rc<Cell<Offset>>) -> Modifier {
    let told = told.clone();
    Modifier
        .on_placed(move |coordinates: LayoutCoordinates| told.set(coordinates.position_in_root()))
}

fn rtl(content: impl Fn() + 'static) {
    CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), content);
}

/// In a container of 100, a box of 10 offset by 10, 20.
fn position(modifier: impl Fn() -> Modifier + 'static, in_rtl: bool) -> Offset {
    let told: Rc<Cell<Offset>> = Rc::default();
    let t = told.clone();
    let modifier = Rc::new(modifier);
    let _host = host(move || {
        let (t, modifier) = (t.clone(), modifier.clone());
        let content = move || {
            let (t, modifier) = (t.clone(), modifier.clone());
            Stack!(modifier = Modifier.required_size(100.dp()), {
                Leaf(modifier().then(at(&t)).size(10.dp()));
            });
        };
        if in_rtl { rtl(content) } else { content() }
    });
    told.get()
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, offset_positionIsModified
#[test]
fn offset_moves() {
    assert_eq!(position(|| Modifier.offset(10.dp(), 20.dp()), false), Offset::new(10.0, 20.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, offset_positionIsModified_rtl
#[test]
fn offset_moves_from_the_start_right_to_left() {
    // Its right edge is 10 from the right of the layout it's placed in, its own size.
    assert_eq!(position(|| Modifier.offset(10.dp(), 20.dp()), true).x, 80.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, absoluteOffset_positionModified
#[test]
fn absolute_offset_moves() {
    let at = position(|| Modifier.absolute_offset(10.dp(), 20.dp()), false);
    assert_eq!(at, Offset::new(10.0, 20.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, absoluteOffset_positionModified_rtl
#[test]
fn absolute_offset_moves_by_the_same_right_to_left() {
    // From where a box in a right-to-left stack starts: its right edge, 90.
    assert_eq!(position(|| Modifier.absolute_offset(10.dp(), 20.dp()), true).x, 100.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, offsetPx_positionIsModified
#[test]
fn offset_with_moves() {
    let at = position(|| Modifier.offset_with(|_| IntOffset::new(10, 20)), false);
    assert_eq!(at, Offset::new(10.0, 20.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, offsetPx_positionIsModified_rtl
#[test]
fn offset_with_moves_from_the_start_right_to_left() {
    let at = position(|| Modifier.offset_with(|_| IntOffset::new(10, 20)), true);
    assert_eq!(at.x, 80.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, absoluteOffsetPx_positionIsModified
#[test]
fn absolute_offset_with_moves() {
    let at = position(|| Modifier.absolute_offset_with(|_| IntOffset::new(10, 20)), false);
    assert_eq!(at, Offset::new(10.0, 20.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, absoluteOffsetPx_positionIsModified_rtl
#[test]
fn absolute_offset_with_moves_by_the_same_right_to_left() {
    let at = position(|| Modifier.absolute_offset_with(|_| IntOffset::new(10, 20)), true);
    assert_eq!(at.x, 100.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, updateOffsetDp_doesNotRemeasure
#[test]
fn a_new_offset_places_and_doesn_t_measure() {
    let told = OnLayout::default();
    let offset = mutable_state_of(10.0f32);
    let (t, o) = (told.clone(), offset.handle());
    let mut host = host(move || {
        Leaf(
            Modifier
                .required_size(10.dp())
                .offset(o.get().dp(), 0.dp())
                .then_custom(OnLayoutElement(t.clone())),
        );
    });
    assert_eq!((told.x.get(), told.measures.get(), told.places.get()), (10.0, 1, 1));
    offset.set(20.0);
    host.frame();
    assert_eq!((told.x.get(), told.measures.get(), told.places.get()), (20.0, 1, 2));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt, updateOffsetPx_doesNotRemeasureAndDoesNotRedraw
#[test]
fn a_new_offset_block_places_and_doesn_t_measure() {
    let told = OnLayout::default();
    let offset = mutable_state_of(10);
    let (t, o) = (told.clone(), offset.handle());
    let mut host = host(move || {
        let x = o.get();
        Leaf(
            Modifier
                .required_size(10.dp())
                .offset_with(move |_| IntOffset::new(x, 0))
                .then_custom(OnLayoutElement(t.clone())),
        );
    });
    assert_eq!((told.x.get(), told.measures.get(), told.places.get()), (10.0, 1, 1));
    offset.set(20);
    host.frame();
    assert_eq!((told.x.get(), told.measures.get(), told.places.get()), (20.0, 1, 2));
}

/// What the block reads places it again, as what `layout_with` reads does.
#[test]
fn what_an_offset_block_reads_places_and_doesn_t_measure() {
    let told = OnLayout::default();
    let offset = mutable_state_of(10);
    let (t, o) = (told.clone(), offset.handle());
    let composed = Rc::new(Cell::new(0));
    let c = composed.clone();
    let mut host = host(move || {
        c.set(c.get() + 1);
        Leaf(
            Modifier
                .required_size(10.dp())
                .offset_with(move |_| IntOffset::new(o.get(), 0))
                .then_custom(OnLayoutElement(t.clone())),
        );
    });
    offset.set(20);
    host.frame();
    assert_eq!((told.x.get(), told.measures.get(), told.places.get()), (20.0, 1, 2));
    assert_eq!(composed.get(), 1);
}
