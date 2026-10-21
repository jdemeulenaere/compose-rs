//! Lazy lists, as Compose's `LazyColumn` and `LazyRow`: only the items in view (and a few
//! beyond) are composed, measured and drawn.
//!
//! Compose composes items while measuring (subcomposition). Here the list composes a range of
//! items, and its measurement, which fills the viewport from the scroll position, writes the
//! range it needs when the composed one falls short; the host then recomposes and measures again
//! within the same frame (`UiHost::frame`), so scrolling shows no empty frame.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;
use std::rc::Rc;

use compose_runtime::{
    ComposeArg, OwnedState, composable, hash_key, key, mutable_state_of, remember,
};
use compose_ui::{
    Alignment, Constraints, Dp, EventContext, IntSize, Layout, Measurable, MeasurePolicy,
    MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement, PaddingValues,
    Placeable, PointerEvent, PointerEventPass, PointerInputModifierNode,
};

use crate::layout::Arrangement;
use crate::scroll::DragScroller;

/// Items composed beyond the viewport on each side, ready when scrolling reaches them.
const BEYOND: usize = 2;

/// Where a lazy list is scrolled, as Compose's `LazyListState`: the first visible item and how
/// far it is scrolled off. Reading them in composition recomposes on scrolls.
#[derive(Clone)]
pub struct LazyListState {
    inner: Rc<LazyListInner>,
}

struct LazyListInner {
    /// The position the layout works from (the item, the pixels it is scrolled off).
    position: Cell<(usize, i32)>,
    /// Scrolling asked for since the last measurement, in pixels.
    pending: Cell<f32>,
    /// Bumped by scrolls: the layout reads it, so a scroll measures the list again.
    remeasure: OwnedState<u64>,
    /// The items composed: what the layout asks for.
    composed: OwnedState<(usize, usize)>,
    /// The position as state, for composition.
    first_index: OwnedState<usize>,
    first_offset: OwnedState<i32>,
    /// Item sizes along the axis, as last measured: estimates for items not composed.
    sizes: RefCell<HashMap<usize, i32>>,
    can_scroll_backward: Cell<bool>,
    can_scroll_forward: Cell<bool>,
}

impl LazyListState {
    pub fn new(
        first_visible_item_index: usize,
        first_visible_item_scroll_offset: i32,
    ) -> LazyListState {
        let offset = first_visible_item_scroll_offset.max(0);
        LazyListState {
            inner: Rc::new(LazyListInner {
                position: Cell::new((first_visible_item_index, offset)),
                pending: Cell::new(0.0),
                remeasure: mutable_state_of(0),
                composed: mutable_state_of((
                    first_visible_item_index,
                    first_visible_item_index + 1,
                )),
                first_index: mutable_state_of(first_visible_item_index),
                first_offset: mutable_state_of(offset),
                sizes: RefCell::new(HashMap::new()),
                can_scroll_backward: Cell::new(false),
                can_scroll_forward: Cell::new(true),
            }),
        }
    }

    /// The first item in view, as `firstVisibleItemIndex` (a state read).
    pub fn first_visible_item_index(&self) -> usize {
        self.inner.first_index.get()
    }

    /// How many pixels of it are scrolled off, as `firstVisibleItemScrollOffset` (a state read).
    pub fn first_visible_item_scroll_offset(&self) -> i32 {
        self.inner.first_offset.get()
    }

    /// Scrolls to `index` (and `scroll_offset` pixels into it) at the next layout, as
    /// `scrollToItem`, without animation.
    pub fn scroll_to_item(&self, index: usize, scroll_offset: i32) {
        self.inner.position.set((index, scroll_offset.max(0)));
        self.inner.pending.set(0.0);
        self.inner.remeasure.update(|count| count.wrapping_add(1));
    }

    /// Scrolls by `pixels` (positive: towards the end) at the next layout, as
    /// `scrollBy`/`dispatchRawDelta`; returns whether the list can move that way.
    pub fn scroll_by(&self, pixels: f32) -> bool {
        let can = if pixels > 0.0 {
            self.inner.can_scroll_forward.get()
        } else {
            pixels < 0.0 && self.inner.can_scroll_backward.get()
        };
        if can {
            self.inner.pending.set(self.inner.pending.get() + pixels);
            self.inner.remeasure.update(|count| count.wrapping_add(1));
        }
        can
    }

    /// Whether the list can scroll towards its end, as `canScrollForward` (as last laid out).
    pub fn can_scroll_forward(&self) -> bool {
        self.inner.can_scroll_forward.get()
    }

    /// Whether the list can scroll towards its start, as `canScrollBackward`.
    pub fn can_scroll_backward(&self) -> bool {
        self.inner.can_scroll_backward.get()
    }
}

impl ComposeArg for LazyListState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl std::fmt::Debug for LazyListState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (index, offset) = self.inner.position.get();
        write!(formatter, "LazyListState({index}, {offset})")
    }
}

/// A lazy list state kept across recompositions, as `rememberLazyListState()`.
#[composable(non_restartable)]
pub fn remember_lazy_list_state(
    #[default(0)] initial_first_visible_item_index: usize,
    #[default(0)] initial_first_visible_item_scroll_offset: i32,
) -> LazyListState {
    remember(|| {
        LazyListState::new(
            initial_first_visible_item_index,
            initial_first_visible_item_scroll_offset,
        )
    })
}

/// A run of items: their count, keys and content.
struct Interval {
    count: usize,
    key: Option<Rc<dyn Fn(usize) -> u64>>,
    content: Rc<dyn Fn(usize)>,
}

/// What a lazy list's content declares its items with, as Compose's `LazyListScope`:
/// `scope.item(|| …)`, `scope.items(count, |index| …)`, `scope.items_keyed(count, key, …)`.
#[derive(Clone, Default)]
pub struct LazyListScope {
    intervals: Rc<RefCell<Vec<Interval>>>,
}

impl LazyListScope {
    /// One item, as `item { … }`.
    pub fn item(&self, content: impl Fn() + 'static) {
        self.intervals.borrow_mut().push(Interval {
            count: 1,
            key: None,
            content: Rc::new(move |_| content()),
        });
    }

    /// `count` items, as `items(count) { index -> … }`. Without keys, an item's state goes with
    /// its index.
    pub fn items(&self, count: usize, content: impl Fn(usize) + 'static) {
        self.intervals.borrow_mut().push(Interval { count, key: None, content: Rc::new(content) });
    }

    /// `count` items with keys, as `items(count, key = { … }) { … }`: an item keeps its state
    /// when items before it come and go. Keys must be unique in the list.
    pub fn items_keyed<K: Hash>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.intervals.borrow_mut().push(Interval {
            count,
            key: Some(Rc::new(move |index| hash_key(&key(index)))),
            content: Rc::new(content),
        });
    }
}

impl ComposeArg for LazyListScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.intervals, &other.intervals)
    }
}

/// What the layout needs from composition: a new value is a new policy, which measures the
/// list again.
#[derive(Clone, PartialEq, Debug)]
struct ListInfo {
    count: usize,
    /// The items composed, in the children's order.
    range: Range<usize>,
    vertical: bool,
    padding: PaddingValues,
    arrangement: Arrangement,
    alignment: Alignment,
}

/// A vertical lazy list, as Compose's `LazyColumn`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LazyColumn(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_list_state(0, 0))] state: LazyListState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(crate::layout::Arrangement::TOP)] vertical_arrangement: Arrangement,
    #[default(crate::Alignment::TOP_START)] horizontal_alignment: Alignment,
    content: impl Fn(LazyListScope) + 'static,
) {
    LazyList(
        modifier,
        state,
        content_padding,
        true,
        vertical_arrangement,
        horizontal_alignment,
        content,
    );
}

/// A horizontal lazy list, as Compose's `LazyRow`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub fn LazyRow(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_list_state(0, 0))] state: LazyListState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(crate::layout::Arrangement::Start)] horizontal_arrangement: Arrangement,
    #[default(crate::Alignment::TOP_START)] vertical_alignment: Alignment,
    content: impl Fn(LazyListScope) + 'static,
) {
    LazyList(
        modifier,
        state,
        content_padding,
        false,
        horizontal_arrangement,
        vertical_alignment,
        content,
    );
}

#[composable(non_restartable)]
#[allow(non_snake_case)]
fn LazyList(
    modifier: Modifier,
    state: LazyListState,
    content_padding: PaddingValues,
    vertical: bool,
    arrangement: Arrangement,
    alignment: Alignment,
    content: impl Fn(LazyListScope) + 'static,
) {
    let scope = LazyListScope::default();
    content(scope.clone());
    let intervals: Rc<Vec<Interval>> = Rc::new(std::mem::take(&mut *scope.intervals.borrow_mut()));
    let count: usize = intervals.iter().map(|interval| interval.count).sum();
    let (start, end) = state.inner.composed.get();
    let range = start.min(count)..end.min(count);
    let info = ListInfo {
        count,
        range: range.clone(),
        vertical,
        padding: content_padding,
        arrangement,
        alignment,
    };
    let policy = {
        let state = state.clone();
        compose_runtime::remember_keyed((state.clone(), InfoKey(info.clone())), || {
            Rc::new(LazyListLayout { state: state.inner.clone(), info }) as Rc<dyn MeasurePolicy>
        })
    };
    let item_policy = remember(|| Rc::new(ItemLayout) as Rc<dyn MeasurePolicy>);
    let modifier = modifier
        .clip(compose_ui::RECTANGLE_SHAPE)
        .then_custom(LazyScrollElement { state: state.clone(), vertical });
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            for index in range.clone() {
                // The interval holding the item, and the item's index in it.
                let mut local = index;
                let Some(interval) = intervals.iter().find(|interval| {
                    if local < interval.count {
                        true
                    } else {
                        local -= interval.count;
                        false
                    }
                }) else {
                    break;
                };
                let item_key =
                    interval.key.as_ref().map_or(hash_key(&("index", index)), |key| key(local));
                let content = interval.content.clone();
                let item_policy = item_policy.clone();
                key(item_key, move || {
                    Layout(
                        Modifier,
                        item_policy.clone(),
                        compose_runtime::callback(move || content(local)),
                    );
                });
            }
        }),
    );
}

/// An item: its children stacked at the top start, as large as the largest.
struct ItemLayout;

impl MeasurePolicy for ItemLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&measurable| scope.measure(measurable, constraints)).collect();
        let size = placeables.iter().fold(
            IntSize::new(constraints.min_width, constraints.min_height),
            |size, placeable| {
                IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
            },
        );
        for placeable in &placeables {
            scope.place(placeable, 0, 0);
        }
        scope.layout(size.width, size.height)
    }

    fn debug_name(&self) -> &'static str {
        "LazyItem"
    }
}

/// Measures an item by index (once), recording its size.
type MeasureItem<'a> = dyn FnMut(usize, &mut HashMap<usize, i32>) -> Option<Placeable> + 'a;

/// The lazy list's measurement: fills the viewport from the scroll position with the composed
/// items, and asks for the items it lacks.
struct LazyListLayout {
    state: Rc<LazyListInner>,
    info: ListInfo,
}

/// The list's info, compared by value.
#[derive(Clone, PartialEq, Debug)]
struct InfoKey(ListInfo);

impl ComposeArg for InfoKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl MeasurePolicy for LazyListLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let state = &self.state;
        // Observed: a scroll measures the list again.
        state.remeasure.get();
        let info = &self.info;
        let vertical = info.vertical;
        let density = scope.density();
        let px = |dp: Dp| density.round_to_px(dp);
        let (before, after, cross_before, cross_after) = if vertical {
            (
                px(info.padding.top),
                px(info.padding.bottom),
                px(info.padding.start),
                px(info.padding.end),
            )
        } else {
            (
                px(info.padding.start),
                px(info.padding.end),
                px(info.padding.top),
                px(info.padding.bottom),
            )
        };
        let spacing = info.arrangement.spacing(&density);
        let main_max = if vertical { constraints.max_height } else { constraints.max_width };
        assert!(
            main_max != Constraints::INFINITY,
            "compose-foundation: a lazy list was measured with an infinite size along its axis \
             (in a scrolling parent?): give it a size"
        );
        let cross_max = if vertical { constraints.max_width } else { constraints.max_height };
        let cross_room = if cross_max == Constraints::INFINITY {
            Constraints::INFINITY
        } else {
            (cross_max - cross_before - cross_after).max(0)
        };
        let item_constraints = if vertical {
            Constraints::new(0, cross_room, 0, Constraints::INFINITY)
        } else {
            Constraints::new(0, Constraints::INFINITY, 0, cross_room)
        };
        let count = info.count;
        let range = info.range.clone();
        // Each child measured once, when first needed.
        let mut placeables: Vec<Option<Placeable>> = vec![None; measurables.len()];
        let mut sizes = state.sizes.borrow_mut();
        let main_of =
            |placeable: &Placeable| if vertical { placeable.height } else { placeable.width };
        let mut measure = |index: usize, sizes: &mut HashMap<usize, i32>| -> Option<Placeable> {
            if !range.contains(&index) || index - range.start >= measurables.len() {
                return None;
            }
            let child = index - range.start;
            if placeables[child].is_none() {
                let placeable = scope.measure(measurables[child], item_constraints);
                sizes.insert(index, main_of(&placeable));
                placeables[child] = Some(placeable);
            }
            placeables[child]
        };
        let estimate = |sizes: &HashMap<usize, i32>| {
            if sizes.is_empty() {
                1
            } else {
                (sizes.values().map(|size| *size as i64).sum::<i64>() / sizes.len() as i64).max(1)
                    as i32
            }
        };
        // The position, with the scrolling asked for.
        let (mut index, offset) = state.position.get();
        let mut offset = offset as f32 + state.pending.replace(0.0);
        if count == 0 {
            index = 0;
            offset = 0.0;
        }
        index = index.min(count.saturating_sub(1));
        let size_of = |index: usize, sizes: &mut HashMap<usize, i32>, measure: &mut MeasureItem<'_>| {
            match measure(index, sizes) {
                Some(placeable) => main_of(&placeable),
                None => sizes.get(&index).copied().unwrap_or_else(|| estimate(sizes)),
            }
        };
        let mut placed: Vec<(Placeable, i32)> = Vec::new();
        let mut end_index;
        let mut missing;
        let mut content_end;
        let mut attempt = 0;
        loop {
            attempt += 1;
            // Back over the items before, while scrolled before the first.
            while offset < 0.0 && index > 0 {
                index -= 1;
                offset += (size_of(index, &mut sizes, &mut measure) + spacing) as f32;
            }
            if offset < 0.0 {
                offset = 0.0;
            }
            // On over the items scrolled past.
            while index + 1 < count {
                let size = (size_of(index, &mut sizes, &mut measure) + spacing) as f32;
                if offset >= size {
                    offset -= size;
                    index += 1;
                } else {
                    break;
                }
            }
            // Fill the viewport.
            placed.clear();
            missing = false;
            let mut main = before - offset.round() as i32;
            end_index = index;
            while end_index < count && main < main_max {
                match measure(end_index, &mut sizes) {
                    Some(placeable) => {
                        let size = main_of(&placeable);
                        placed.push((placeable, main));
                        main += size + spacing;
                        end_index += 1;
                    }
                    None => {
                        missing = true;
                        break;
                    }
                }
            }
            content_end = if placed.is_empty() { main } else { main - spacing } + after;
            // At the end with room left: scrolled back so the last item ends the viewport.
            let gap = main_max - content_end;
            if !missing
                && end_index == count
                && gap > 0
                && (index > 0 || offset > 0.0)
                && attempt < 3
            {
                offset -= gap as f32;
                continue;
            }
            break;
        }
        // The items wanted composed: those in view and a few beyond, and as many more as the
        // room left would take if items were missing.
        let wanted_start = index.saturating_sub(BEYOND);
        let mut wanted_end = (end_index + BEYOND).min(count);
        if missing {
            let room = (main_max - content_end).max(0);
            let more = (room / estimate(&sizes)) as usize + 1;
            wanted_end = (end_index + more + BEYOND).min(count);
        }
        let (composed_start, composed_end) = state.composed.get();
        let covers = composed_start <= wanted_start && composed_end >= wanted_end;
        let too_many =
            composed_start + 2 * BEYOND < wanted_start || composed_end > wanted_end + 2 * BEYOND;
        if !covers || too_many {
            state.composed.set((wanted_start, wanted_end));
        }
        let offset = offset.round() as i32;
        state.position.set((index, offset));
        if state.first_index.get() != index {
            state.first_index.set(index);
        }
        if state.first_offset.get() != offset {
            state.first_offset.set(offset);
        }
        state.can_scroll_backward.set(index > 0 || offset > 0);
        state.can_scroll_forward.set(end_index < count || content_end > main_max);
        // Placed along the axis, aligned across it.
        let cross_size = placed
            .iter()
            .map(|(placeable, _)| if vertical { placeable.width } else { placeable.height })
            .max()
            .unwrap_or(0);
        let main_size = content_end.min(main_max);
        let (width, height) = if vertical {
            (
                constraints.constrain_width(cross_size + cross_before + cross_after),
                constraints.constrain_height(main_size),
            )
        } else {
            (
                constraints.constrain_width(main_size),
                constraints.constrain_height(cross_size + cross_before + cross_after),
            )
        };
        let direction = scope.layout_direction();
        for (placeable, main) in &placed {
            if vertical {
                let space = IntSize::new(width - cross_before - cross_after, placeable.height);
                let x = info.alignment.align(placeable.size(), space, direction).x;
                scope.place(placeable, cross_before + x, *main);
            } else {
                let space = IntSize::new(placeable.width, height - cross_before - cross_after);
                let y = info.alignment.align(placeable.size(), space, direction).y;
                scope.place(placeable, *main, cross_before + y);
            }
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "LazyList"
    }
}

/// The wheel and drags scroll the list.
struct LazyScrollElement {
    state: LazyListState,
    vertical: bool,
}

struct LazyScrollNode {
    state: LazyListState,
    vertical: bool,
    drag: DragScroller,
}

impl ModifierNodeElement for LazyScrollElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(LazyScrollNode {
            state: self.state.clone(),
            vertical: self.vertical,
            drag: DragScroller::default(),
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<LazyScrollNode>() {
            node.state = self.state.clone();
            node.vertical = self.vertical;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other
            .downcast_ref::<LazyScrollElement>()
            .is_some_and(|other| other.vertical == self.vertical && other.state.same(&self.state))
    }

    fn name(&self) -> &'static str {
        "lazy_scroll"
    }
}

impl ModifierNode for LazyScrollNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }

    fn hit_clip(&self) -> Option<compose_ui::Shape> {
        Some(compose_ui::Shape::Rectangle)
    }
}

impl PointerInputModifierNode for LazyScrollNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        _context: &mut EventContext<'_>,
    ) {
        let state = self.state.clone();
        self.drag.on_pointer_event(event, pass, self.vertical, |delta| state.scroll_by(delta));
    }

    fn on_cancel_pointer_input(&mut self) {
        self.drag.cancel();
    }
}
