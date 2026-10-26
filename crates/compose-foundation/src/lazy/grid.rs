//! Lazy grids, as Compose's `LazyVerticalGrid` and `LazyHorizontalGrid`: items in lines of
//! cells (a vertical grid's rows of columns, a horizontal one's columns of rows), as many cells
//! to a line as `GridCells` says, an item taking one cell or more (`GridItemSpan`).
//!
//! As Compose's, a grid composes item by item (D450): each item is composed in a slot of its own
//! by the lazy list's item composition (`LazyItems`), so it keeps its state by its key, and an
//! item coming in reuses the slot of an item of its content type that left (D430). Its
//! measurement lays the composed items out in lines, as Compose's `measureLazyGrid`: it works
//! from the line holding the first visible item, and asks for the items of the lines it needs.

use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

use compose_runtime::{ComposeArg, ImmutableList, ObjectHandle, Owned, composable};
use compose_ui::{
    Constraints, Density, Dp, Layout, Measurable, MeasurePolicy, MeasureResult, MeasureScope,
    Modifier, PaddingValues, Placeable, PrefetchScheduler,
};

use super::prefetch::{Laid, Policy};
use super::{
    BEYOND, FIRST_STEP, Interval, Items, ItemsRef, LazyItems, LazyListInner, LazyListState,
    LazyScrollElement, MOST_IN_A_ROUND, MOST_MISSING, Sizes, composed_range, content_type_of,
};
use crate::layout::{HorizontalArrangement, MainAxisArrangement, VerticalArrangement};

/// How a grid's cross axis is cut into cells, as Compose's `GridCells`: a vertical grid's
/// columns, a horizontal one's rows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GridCells {
    /// This many cells.
    Fixed(usize),
    /// As many cells as fit at least this large.
    Adaptive(Dp),
}

impl ComposeArg for GridCells {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl GridCells {
    /// The cells' sizes in `available` pixels with `spacing` between them, as Compose's
    /// `calculateCrossAxisCellSizes`: the size without spacing divided evenly, the remaining
    /// pixels one each to the first cells.
    fn sizes(self, available: i32, spacing: i32, density: Density) -> Vec<i32> {
        let count = match self {
            GridCells::Fixed(count) => count.max(1),
            GridCells::Adaptive(min) => {
                let min = density.round_to_px(min).max(1);
                ((available + spacing) / (min + spacing)).max(1) as usize
            }
        };
        let without_spacing = (available - spacing * (count as i32 - 1)).max(0);
        let (size, remaining) = (without_spacing / count as i32, without_spacing % count as i32);
        (0..count as i32).map(|cell| size + i32::from(cell < remaining)).collect()
    }
}

/// How many cells of a line an item takes, as Compose's `GridItemSpan`: `GridItemSpan(1)` is
/// the default, `GridItemSpan(scope.max_line_span())` a whole line. Compose requires a span
/// within 1 and the line's cells (it throws otherwise); here one outside them is kept within
/// them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GridItemSpan(pub usize);

impl GridItemSpan {
    /// The cells it takes, as `currentLineSpan`.
    pub fn current_line_span(self) -> usize {
        self.0
    }
}

/// What an item's span is worked out from, as Compose's `LazyGridItemSpanScope`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LazyGridItemSpanScope {
    max_line_span: usize,
    max_current_line_span: usize,
}

impl LazyGridItemSpanScope {
    /// The cells in a line, as `maxLineSpan`: the span of an item taking a whole line.
    pub fn max_line_span(&self) -> usize {
        self.max_line_span
    }

    /// The cells left in the line the item would go in, as `maxCurrentLineSpan`: a larger span
    /// puts the item at the start of the next line.
    pub fn max_current_line_span(&self) -> usize {
        self.max_current_line_span
    }
}

/// An item's span by its index in its interval.
pub(super) type SpanOf = Rc<dyn Fn(LazyGridItemSpanScope, usize) -> GridItemSpan>;

/// A grid's scroll position, as Compose's `LazyGridState`: the first visible item (the first of
/// the first visible line) and the pixels its line is scrolled off.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_lazy_grid_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LazyGridState {
    list: LazyListState,
    object: ObjectHandle<GridInner>,
}

/// The list state a grid's position is kept in (in items, as a list's).
struct GridInner {
    _list: Owned<LazyListState>,
}

impl LazyGridState {
    /// Scrolled to the line of item `first_visible_item_index`,
    /// `first_visible_item_scroll_offset` pixels into it.
    #[track_caller]
    pub fn new(
        first_visible_item_index: usize,
        first_visible_item_scroll_offset: i32,
    ) -> Owned<LazyGridState> {
        let list = LazyListState::new(first_visible_item_index, first_visible_item_scroll_offset);
        let handle = *list;
        Owned::new(GridInner { _list: list }, |object| LazyGridState { list: handle, object })
    }

    /// The first item of the first visible line, as `firstVisibleItemIndex` (a state read).
    pub fn first_visible_item_index(&self) -> usize {
        self.list.first_visible_item_index()
    }

    /// How many pixels of the first visible line are scrolled off, as
    /// `firstVisibleItemScrollOffset` (a state read).
    pub fn first_visible_item_scroll_offset(&self) -> i32 {
        self.list.first_visible_item_scroll_offset()
    }

    /// Scrolls to the line of item `index` (and `scroll_offset` pixels into it) at the next
    /// layout, as `scrollToItem`.
    pub fn scroll_to_item(&self, index: usize, scroll_offset: i32) {
        self.list.scroll_to_item(index, scroll_offset);
    }

    /// Scrolls by `pixels` (positive: towards the end), returning whether it moved.
    pub fn scroll_by(&self, pixels: f32) -> bool {
        self.list.scroll_by(pixels)
    }

    /// Whether the grid can scroll towards its end, as `canScrollForward` (as last laid out).
    pub fn can_scroll_forward(&self) -> bool {
        self.list.can_scroll_forward()
    }

    /// Whether the grid can scroll towards its start, as `canScrollBackward` (as last laid
    /// out).
    pub fn can_scroll_backward(&self) -> bool {
        self.list.can_scroll_backward()
    }
}

impl ComposeArg for LazyGridState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl LazyGridState {
    /// Saves the first visible item and its offset, as Compose's `LazyGridState.Saver`.
    pub fn saver() -> impl compose_runtime::Saver<Owned<LazyGridState>> {
        use compose_runtime::Saveable;
        compose_runtime::saver(
            |state: &Owned<LazyGridState>| {
                let inner = state.list.inner()?;
                let (index, offset) = inner.position.get();
                Some((index, offset.round() as i32).save())
            },
            |saved| {
                <(usize, i32)>::restore(saved)
                    .map(|(index, offset)| LazyGridState::new(index, offset))
            },
        )
    }
}

/// A grid state remembered at its call site and saved across the host's recreation, as
/// `rememberLazyGridState()` (a `rememberSaveable`).
#[composable]
pub fn remember_lazy_grid_state(
    #[default(0)] initial_first_visible_item_index: usize,
    #[default(0)] initial_first_visible_item_scroll_offset: i32,
) -> LazyGridState {
    compose_runtime::remember_saveable_with(LazyGridState::saver(), || {
        LazyGridState::new(
            initial_first_visible_item_index,
            initial_first_visible_item_scroll_offset,
        )
    })
}

/// What a grid's content declares its items with, as Compose's `LazyGridScope`:
/// `scope.item(|| …)`, `scope.items(count, |index| …)`, `scope.items_keyed(count, key, …)`, and
/// over a list, `scope.items_list(list, |item| …)` and `scope.items_indexed(list, |index, item|
/// …)`.
///
/// Each form has a `_with` variant taking Compose's `contentType` too, after the key, as a
/// list's (`items_keyed_with(count, key, content_type, …)`): an item composed where none was
/// reuses the composition and nodes of an item of its content type that left (D430). And a
/// `_with_span` variant taking Compose's `span` and `contentType`, in Compose's order
/// (`items_keyed_with_span(count, key, span, content_type, …)`, D454): `span` gives an item's
/// `GridItemSpan` (`|_, _| GridItemSpan(1)` is the default). The forms without take Compose's
/// defaults: one cell, and the content type `{ null }` (such items reuse each other's).
///
/// Spans cost a walk: which items each line holds is worked out by asking the items' spans from
/// the first, as far as the grid is scrolled, once per set of items (each time the grid's
/// content runs again, as when its caller recomposes with other captures). Without spans, it's
/// arithmetic: give a span only where an item takes more than a cell.
#[derive(Clone, Default)]
pub struct LazyGridScope {
    intervals: Rc<RefCell<Vec<Interval>>>,
}

impl LazyGridScope {
    /// One item, as `item { … }`.
    pub fn item(&self, content: impl Fn() + 'static) {
        self.push(Interval::new(1, Rc::new(move |_| content())));
    }

    /// One item with a span and a content type, as `item(span = { … }, contentType = …) { … }`.
    pub fn item_with_span<C: Hash>(
        &self,
        span: impl Fn(LazyGridItemSpanScope) -> GridItemSpan + 'static,
        content_type: C,
        content: impl Fn() + 'static,
    ) {
        let content_type = content_type_of(&content_type);
        self.push(
            Interval::new(1, Rc::new(move |_| content()))
                .typed(Rc::new(move |_| content_type))
                .spanned(Rc::new(move |scope, _| span(scope))),
        );
    }

    /// One item with a key, as `item(key) { … }`.
    pub fn item_keyed<K: Hash + PartialEq + Clone + 'static>(
        &self,
        key: K,
        content: impl Fn() + 'static,
    ) {
        self.push(Interval::keyed(1, move |_| key.clone(), Rc::new(move |_| content())));
    }

    /// One item with a key, a span and a content type, as `item(key, span, contentType) { … }`.
    pub fn item_keyed_with_span<K: Hash + PartialEq + Clone + 'static, C: Hash>(
        &self,
        key: K,
        span: impl Fn(LazyGridItemSpanScope) -> GridItemSpan + 'static,
        content_type: C,
        content: impl Fn() + 'static,
    ) {
        let content_type = content_type_of(&content_type);
        self.push(
            Interval::keyed(1, move |_| key.clone(), Rc::new(move |_| content()))
                .typed(Rc::new(move |_| content_type))
                .spanned(Rc::new(move |scope, _| span(scope))),
        );
    }

    /// `count` items, as `items(count) { index -> … }`. Without keys, an item's state goes with
    /// its index.
    pub fn items(&self, count: usize, content: impl Fn(usize) + 'static) {
        self.push(Interval::new(count, Rc::new(content)));
    }

    /// `count` items with their spans and content types, as
    /// `items(count, span = { … }, contentType = { … }) { … }`.
    pub fn items_with_span<C: Hash>(
        &self,
        count: usize,
        span: impl Fn(LazyGridItemSpanScope, usize) -> GridItemSpan + 'static,
        content_type: impl Fn(usize) -> C + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(
            Interval::new(count, Rc::new(content))
                .typed(Rc::new(move |index| content_type_of(&content_type(index))))
                .spanned(Rc::new(span)),
        );
    }

    /// `count` items with keys, as `items(count, key = { … }) { … }`: an item keeps its state
    /// (and the scroll position follows the first visible one) when items before it come and
    /// go. Keys must be unique in the grid (debug builds check those composed).
    pub fn items_keyed<K: Hash + PartialEq + 'static>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(Interval::keyed(count, key, Rc::new(content)));
    }

    /// `count` items with keys, spans and content types, as
    /// `items(count, key = { … }, span = { … }, contentType = { … }) { … }`.
    pub fn items_keyed_with_span<K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        span: impl Fn(LazyGridItemSpanScope, usize) -> GridItemSpan + 'static,
        content_type: impl Fn(usize) -> C + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(
            Interval::keyed(count, key, Rc::new(content))
                .typed(Rc::new(move |index| content_type_of(&content_type(index))))
                .spanned(Rc::new(span)),
        );
    }

    /// An item for each of `items`, as `items(items) { item -> … }`: `items` is a `Vec`, a
    /// slice or an array (copied), or an `ImmutableList` (shared).
    pub fn items_list<T: 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content: impl Fn(&T) + 'static,
    ) {
        let items = items.into();
        self.push(Interval::new(items.len(), Rc::new(move |index| content(&items[index]))));
    }

    /// `items_list` with spans and content types, as
    /// `items(items, span = { item -> … }, contentType = { item -> … })`.
    pub fn items_list_with_span<T: 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        span: impl Fn(LazyGridItemSpanScope, &T) -> GridItemSpan + 'static,
        content_type: impl Fn(&T) -> C + 'static,
        content: impl Fn(&T) + 'static,
    ) {
        let items = items.into();
        let (spans, types) = (items.clone(), items.clone());
        self.push(
            Interval::new(items.len(), Rc::new(move |index| content(&items[index])))
                .typed(Rc::new(move |index| content_type_of(&content_type(&types[index]))))
                .spanned(Rc::new(move |scope, index| span(scope, &spans[index]))),
        );
    }

    /// `items_list` with keys, as `items(items, key = { item -> … })`.
    pub fn items_list_keyed<T: 'static, K: Hash + PartialEq + 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(&T) -> K + 'static,
        content: impl Fn(&T) + 'static,
    ) {
        let items = items.into();
        let keys = items.clone();
        self.push(Interval::keyed(
            items.len(),
            move |index| key(&keys[index]),
            Rc::new(move |index| content(&items[index])),
        ));
    }

    /// `items_list` with keys, spans and content types, as
    /// `items(items, key = { … }, span = { … }, contentType = { … })`.
    pub fn items_list_keyed_with_span<T: 'static, K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(&T) -> K + 'static,
        span: impl Fn(LazyGridItemSpanScope, &T) -> GridItemSpan + 'static,
        content_type: impl Fn(&T) -> C + 'static,
        content: impl Fn(&T) + 'static,
    ) {
        let items = items.into();
        let (keys, spans, types) = (items.clone(), items.clone(), items.clone());
        self.push(
            Interval::keyed(
                items.len(),
                move |index| key(&keys[index]),
                Rc::new(move |index| content(&items[index])),
            )
            .typed(Rc::new(move |index| content_type_of(&content_type(&types[index]))))
            .spanned(Rc::new(move |scope, index| span(scope, &spans[index]))),
        );
    }

    /// An item for each of `items`, with its index, as `itemsIndexed(items) { index, item -> … }`.
    pub fn items_indexed<T: 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content: impl Fn(usize, &T) + 'static,
    ) {
        let items = items.into();
        self.push(Interval::new(items.len(), Rc::new(move |index| content(index, &items[index]))));
    }

    /// `items_indexed` with spans and content types, as
    /// `itemsIndexed(items, span = { index, item -> … }, contentType = { index, item -> … })`.
    pub fn items_indexed_with_span<T: 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        span: impl Fn(LazyGridItemSpanScope, usize, &T) -> GridItemSpan + 'static,
        content_type: impl Fn(usize, &T) -> C + 'static,
        content: impl Fn(usize, &T) + 'static,
    ) {
        let items = items.into();
        let (spans, types) = (items.clone(), items.clone());
        self.push(
            Interval::new(items.len(), Rc::new(move |index| content(index, &items[index])))
                .typed(Rc::new(move |index| content_type_of(&content_type(index, &types[index]))))
                .spanned(Rc::new(move |scope, index| span(scope, index, &spans[index]))),
        );
    }

    /// `items_indexed` with keys, as `itemsIndexed(items, key = { index, item -> … })`.
    pub fn items_indexed_keyed<T: 'static, K: Hash + PartialEq + 'static>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(usize, &T) -> K + 'static,
        content: impl Fn(usize, &T) + 'static,
    ) {
        let items = items.into();
        let keys = items.clone();
        self.push(Interval::keyed(
            items.len(),
            move |index| key(index, &keys[index]),
            Rc::new(move |index| content(index, &items[index])),
        ));
    }

    /// `items_indexed` with keys, spans and content types, as
    /// `itemsIndexed(items, key = { … }, span = { … }, contentType = { … })`.
    pub fn items_indexed_keyed_with_span<T: 'static, K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(usize, &T) -> K + 'static,
        span: impl Fn(LazyGridItemSpanScope, usize, &T) -> GridItemSpan + 'static,
        content_type: impl Fn(usize, &T) -> C + 'static,
        content: impl Fn(usize, &T) + 'static,
    ) {
        let items = items.into();
        let (keys, spans, types) = (items.clone(), items.clone(), items.clone());
        self.push(
            Interval::keyed(
                items.len(),
                move |index| key(index, &keys[index]),
                Rc::new(move |index| content(index, &items[index])),
            )
            .typed(Rc::new(move |index| content_type_of(&content_type(index, &types[index]))))
            .spanned(Rc::new(move |scope, index| span(scope, index, &spans[index]))),
        );
    }

    /// One item of `content_type`, as `item(contentType = …) { … }`.
    pub fn item_with<C: Hash>(&self, content_type: C, content: impl Fn() + 'static) {
        let content_type = content_type_of(&content_type);
        self.push(
            Interval::new(1, Rc::new(move |_| content())).typed(Rc::new(move |_| content_type)),
        );
    }

    /// One item with a key and a content type, as `item(key, contentType = …) { … }`.
    pub fn item_keyed_with<K: Hash + PartialEq + Clone + 'static, C: Hash>(
        &self,
        key: K,
        content_type: C,
        content: impl Fn() + 'static,
    ) {
        let content_type = content_type_of(&content_type);
        self.push(
            Interval::keyed(1, move |_| key.clone(), Rc::new(move |_| content()))
                .typed(Rc::new(move |_| content_type)),
        );
    }

    /// `count` items with their content types, as `items(count, contentType = { … }) { … }`.
    pub fn items_with<C: Hash>(
        &self,
        count: usize,
        content_type: impl Fn(usize) -> C + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(
            Interval::new(count, Rc::new(content))
                .typed(Rc::new(move |index| content_type_of(&content_type(index)))),
        );
    }

    /// `count` items with keys and content types, as
    /// `items(count, key = { … }, contentType = { … }) { … }`.
    pub fn items_keyed_with<K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content_type: impl Fn(usize) -> C + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.push(
            Interval::keyed(count, key, Rc::new(content))
                .typed(Rc::new(move |index| content_type_of(&content_type(index)))),
        );
    }

    /// `items_list` with content types, as `items(items, contentType = { item -> … })`.
    pub fn items_list_with<T: 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content_type: impl Fn(&T) -> C + 'static,
        content: impl Fn(&T) + 'static,
    ) {
        let items = items.into();
        let types = items.clone();
        self.push(
            Interval::new(items.len(), Rc::new(move |index| content(&items[index])))
                .typed(Rc::new(move |index| content_type_of(&content_type(&types[index])))),
        );
    }

    /// `items_list` with keys and content types, as
    /// `items(items, key = { … }, contentType = { … })`.
    pub fn items_list_keyed_with<T: 'static, K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(&T) -> K + 'static,
        content_type: impl Fn(&T) -> C + 'static,
        content: impl Fn(&T) + 'static,
    ) {
        let items = items.into();
        let (keys, types) = (items.clone(), items.clone());
        self.push(
            Interval::keyed(
                items.len(),
                move |index| key(&keys[index]),
                Rc::new(move |index| content(&items[index])),
            )
            .typed(Rc::new(move |index| content_type_of(&content_type(&types[index])))),
        );
    }

    /// `items_indexed` with content types, as
    /// `itemsIndexed(items, contentType = { index, item -> … })`.
    pub fn items_indexed_with<T: 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        content_type: impl Fn(usize, &T) -> C + 'static,
        content: impl Fn(usize, &T) + 'static,
    ) {
        let items = items.into();
        let types = items.clone();
        self.push(
            Interval::new(items.len(), Rc::new(move |index| content(index, &items[index])))
                .typed(Rc::new(move |index| content_type_of(&content_type(index, &types[index])))),
        );
    }

    /// `items_indexed` with keys and content types, as
    /// `itemsIndexed(items, key = { … }, contentType = { … })`.
    pub fn items_indexed_keyed_with<T: 'static, K: Hash + PartialEq + 'static, C: Hash>(
        &self,
        items: impl Into<ImmutableList<T>>,
        key: impl Fn(usize, &T) -> K + 'static,
        content_type: impl Fn(usize, &T) -> C + 'static,
        content: impl Fn(usize, &T) + 'static,
    ) {
        let items = items.into();
        let (keys, types) = (items.clone(), items.clone());
        self.push(
            Interval::keyed(
                items.len(),
                move |index| key(index, &keys[index]),
                Rc::new(move |index| content(index, &items[index])),
            )
            .typed(Rc::new(move |index| content_type_of(&content_type(index, &types[index])))),
        );
    }

    fn push(&self, interval: Interval) {
        self.intervals.borrow_mut().push(interval);
    }
}

impl ComposeArg for LazyGridScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.intervals, &other.intervals)
    }
}

impl Interval {
    /// With the items' spans (a grid's).
    fn spanned(mut self, span: SpanOf) -> Interval {
        self.span = Some(span);
        self
    }
}

impl Items {
    /// Item `index`'s span in `scope`, in cells (1 without one).
    fn span(&self, index: usize, scope: LazyGridItemSpanScope) -> usize {
        match self.find(index) {
            Some((interval, local)) => {
                interval.span.as_ref().map_or(1, |span| span(scope, local).0)
            }
            None => 1,
        }
    }
}

/// A vertically scrolling grid, as Compose's `LazyVerticalGrid`: `columns` cut its width (inside
/// `content_padding`) into cells, `horizontal_arrangement` between them, and its items fill
/// rows from the start, each in as many cells as its span, composed lazily item by item. Cells
/// fill the width, so `horizontal_arrangement` gives only its spacing (a custom arrangement's
/// `arrange` isn't asked: there is no free space to place).
#[composable]
#[allow(non_snake_case)]
pub fn LazyVerticalGrid(
    columns: GridCells,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_grid_state(0, 0))] state: LazyGridState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(crate::layout::Arrangement::TOP)] vertical_arrangement: impl Into<VerticalArrangement>,
    #[default(crate::layout::Arrangement::START)] horizontal_arrangement: impl Into<
        HorizontalArrangement,
    >,
    content: impl Fn(LazyGridScope) + 'static,
) {
    // (The `#[composable]` transform converts `impl Into` parameters.)
    let cross_spacing = horizontal_arrangement.spacing();
    LazyGrid(
        modifier,
        state,
        columns,
        content_padding,
        true,
        vertical_arrangement.0,
        cross_spacing,
        content,
    );
}

/// A horizontally scrolling grid, as Compose's `LazyHorizontalGrid`: `rows` cut its height
/// (inside `content_padding`) into cells, `vertical_arrangement` between them, and its items
/// fill columns from the top, each in as many cells as its span. Right to left, its first
/// column is at the right, as a `LazyRow`'s first item.
#[composable]
#[allow(non_snake_case)]
pub fn LazyHorizontalGrid(
    rows: GridCells,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_grid_state(0, 0))] state: LazyGridState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(crate::layout::Arrangement::START)] horizontal_arrangement: impl Into<
        HorizontalArrangement,
    >,
    #[default(crate::layout::Arrangement::TOP)] vertical_arrangement: impl Into<VerticalArrangement>,
    content: impl Fn(LazyGridScope) + 'static,
) {
    let cross_spacing = vertical_arrangement.spacing();
    LazyGrid(
        modifier,
        state,
        rows,
        content_padding,
        false,
        horizontal_arrangement.0,
        cross_spacing,
        content,
    );
}

/// What the grid's layout needs from composition: a new value is a new policy, which measures
/// the grid again.
#[derive(Clone)]
struct GridInfo {
    items: Rc<Items>,
    cells: GridCells,
    vertical: bool,
    padding: PaddingValues,
    arrangement: MainAxisArrangement,
    cross_spacing: Dp,
}

impl PartialEq for GridInfo {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.items, &other.items)
            && self.cells == other.cells
            && self.vertical == other.vertical
            && self.padding == other.padding
            && self.arrangement == other.arrangement
            && self.cross_spacing == other.cross_spacing
    }
}

/// The grid's info, compared by value (its items by identity).
#[derive(Clone)]
struct GridInfoKey(GridInfo);

impl ComposeArg for GridInfoKey {
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[composable(non_restartable)]
#[allow(non_snake_case, clippy::too_many_arguments)]
fn LazyGrid(
    modifier: Modifier,
    state: LazyGridState,
    cells: GridCells,
    content_padding: PaddingValues,
    vertical: bool,
    arrangement: MainAxisArrangement,
    cross_spacing: Dp,
    content: impl Fn(LazyGridScope) + 'static,
) {
    let scope = LazyGridScope::default();
    content(scope.clone());
    let intervals = std::mem::take(&mut *scope.intervals.borrow_mut());
    let items = Rc::new(Items::new(intervals));
    // The list state its position is in (detached once the grid state's owner went, D240).
    let list = state.list;
    let info = GridInfo {
        items: items.clone(),
        cells,
        vertical,
        padding: content_padding,
        arrangement,
        cross_spacing,
    };
    let policy = {
        let scheduler = compose_ui::LOCAL_PREFETCH_SCHEDULER.current();
        compose_runtime::remember_keyed(
            (list, GridInfoKey(info.clone()), scheduler.clone()),
            || {
                let state = list.inner_or_detached();
                let lines = Rc::new(RefCell::new(Lines::new(info.items.clone(), 0)));
                let slots = RefCell::new(None);
                Rc::new(LazyGridLayout { state, info, scheduler, lines, slots })
                    as Rc<dyn MeasurePolicy>
            },
        )
    };
    let modifier = modifier.clip(compose_ui::RECTANGLE_SHAPE).then_custom(LazyScrollElement {
        state: list,
        vertical,
        reverse: false,
        enabled: true,
    });
    let items = ItemsRef(items);
    // The items' saveable state, kept by key while they're scrolled away (D290).
    let holder = compose_runtime::remember_saveable_state_holder();
    // Until the grid is measured, its items' likely constraints: a cell's, in the space around.
    let density = compose_ui::LOCAL_DENSITY.current();
    let fallback = compose_ui::platform::LOCAL_CONSTRAINTS_HINT.current().and_then(|around| {
        let px = |dp: Dp| density.round_to_px(dp);
        let (available, padding) = if vertical {
            (around.max_width, px(content_padding.start) + px(content_padding.end))
        } else {
            (around.max_height, px(content_padding.top) + px(content_padding.bottom))
        };
        if available == Constraints::INFINITY {
            return None;
        }
        let spacing = px(cross_spacing);
        let size = cells.sizes((available - padding).max(0), spacing, density)[0];
        Some(cell_constraints(vertical, size))
    });
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || LazyItems(list, items.clone(), holder, fallback)),
    );
}

/// An item's constraints, `size` across the grid: fixed across, unbounded along, as Compose's
/// `Constraints.fixedWidth` (a vertical grid's) and `fixedHeight`.
fn cell_constraints(vertical: bool, size: i32) -> Constraints {
    if vertical {
        Constraints::new(size, size, 0, Constraints::INFINITY)
    } else {
        Constraints::new(0, Constraints::INFINITY, size, size)
    }
}

/// The cells across a grid, as Compose's `LazyGridSlots`: their sizes and offsets.
struct Slots {
    sizes: Vec<i32>,
    positions: Vec<i32>,
}

impl Slots {
    fn new(sizes: Vec<i32>, spacing: i32) -> Slots {
        let mut positions = Vec::with_capacity(sizes.len());
        let mut position = 0;
        for size in &sizes {
            positions.push(position);
            position += size + spacing;
        }
        Slots { sizes, positions }
    }

    fn count(&self) -> usize {
        self.sizes.len()
    }

    /// The offset and size across the grid of an item taking `span` cells from `slot`.
    fn of(&self, slot: usize, span: usize) -> (i32, i32) {
        let last = (slot + span).min(self.count()) - 1;
        let start = self.positions[slot];
        (start, self.positions[last] + self.sizes[last] - start)
    }
}

/// Which items each line holds, as Compose's `LazyGridSpanLayoutProvider`: worked out by
/// arithmetic when no item has a span, else by walking the items' spans from the first, only
/// as far as asked, and kept while the items and the line's cells are the same.
struct Lines {
    items: Rc<Items>,
    /// The cells in a line.
    slots: usize,
    /// With spans: each item walked's span, each line's first item, the cells the last line
    /// walked takes.
    spans: Vec<u32>,
    starts: Vec<usize>,
    taken: usize,
}

impl Lines {
    fn new(items: Rc<Items>, slots: usize) -> Lines {
        Lines { items, slots, spans: Vec::new(), starts: Vec::new(), taken: 0 }
    }

    fn count(&self) -> usize {
        self.items.count
    }

    /// Walks the next item: its span in the space left in the last line, or a new line if it
    /// doesn't fit (its span asked again for a whole line), as Compose's
    /// `getLineConfiguration`.
    fn walk(&mut self) {
        let item = self.spans.len();
        if item == 0 || self.taken >= self.slots {
            self.starts.push(item);
            self.taken = 0;
        }
        let mut span = self.span_of(item, self.slots - self.taken);
        if self.taken + span > self.slots {
            self.starts.push(item);
            self.taken = 0;
            span = self.span_of(item, self.slots);
        }
        self.spans.push(span as u32);
        self.taken += span;
    }

    fn span_of(&self, item: usize, left: usize) -> usize {
        let scope =
            LazyGridItemSpanScope { max_line_span: self.slots, max_current_line_span: left };
        self.items.span(item, scope).clamp(1, self.slots)
    }

    fn walked_all(&self) -> bool {
        self.spans.len() >= self.count()
    }

    /// The line holding item `item` (less than the count).
    fn line_of(&mut self, item: usize) -> usize {
        if !self.items.has_custom_spans {
            return item / self.slots;
        }
        while self.spans.len() <= item && !self.walked_all() {
            self.walk();
        }
        self.starts.partition_point(|start| *start <= item).saturating_sub(1)
    }

    /// Line `line`'s first item: the count past the last line.
    fn start(&mut self, line: usize) -> usize {
        if !self.items.has_custom_spans {
            return line.saturating_mul(self.slots).min(self.count());
        }
        while self.starts.len() <= line && !self.walked_all() {
            self.walk();
        }
        self.starts.get(line).copied().unwrap_or(self.count())
    }

    /// Whether line `line` holds items.
    fn exists(&mut self, line: usize) -> bool {
        self.start(line) < self.count()
    }

    /// Item `item`'s span (walked).
    fn span(&self, item: usize) -> usize {
        if self.items.has_custom_spans { self.spans[item] as usize } else { 1 }
    }

    /// `line`, or the last line if it's past it.
    fn at_most_last(&mut self, line: usize) -> usize {
        if self.exists(line) { line } else { self.line_of(self.count().saturating_sub(1)) }
    }

    /// The lines, known once walked to the end; else estimated from the items left at one cell
    /// each.
    fn count_estimate(&self) -> usize {
        if !self.items.has_custom_spans {
            return self.count().div_ceil(self.slots);
        }
        self.starts.len() + (self.count() - self.spans.len()).div_ceil(self.slots)
    }

    /// The cell item `item` starts at in its line.
    fn slot_of(&mut self, item: usize) -> usize {
        let line = self.line_of(item);
        let start = self.start(line);
        (start..item).map(|before| self.span(before)).sum()
    }
}

/// The grid's measurement: fills the viewport with lines from the scroll position, and asks for
/// the items of the lines it lacks.
struct LazyGridLayout {
    state: Rc<LazyListInner>,
    info: GridInfo,
    /// Where it asks for the items after a scroll to be prefetched.
    scheduler: Option<PrefetchScheduler>,
    /// The lines, shared with prefetching (an item's constraints depend on its cell and span).
    lines: Rc<RefCell<Lines>>,
    /// The cells, as last measured, for the room across, the spacing and the density.
    slots: RefCell<Option<(SlotsKey, Rc<Slots>)>>,
}

/// What the cells are worked out from: the room across the grid, the spacing between cells, the
/// density.
type SlotsKey = (i32, i32, Density);

impl LazyGridLayout {
    /// The cells for `room` pixels across, `spacing` apart: as last measured if those are the
    /// same; else worked out anew, and prefetching told each item's constraints in them.
    fn slots(&self, room: i32, spacing: i32, density: Density) -> Rc<Slots> {
        let key = (room, spacing, density);
        let mut cached = self.slots.borrow_mut();
        if let Some((cached_key, slots)) = &*cached
            && *cached_key == key
        {
            return slots.clone();
        }
        let slots = Rc::new(Slots::new(self.info.cells.sizes(room, spacing, density), spacing));
        *cached = Some((key, slots.clone()));
        let (lines, cells, vertical) = (self.lines.clone(), slots.clone(), self.info.vertical);
        let constraints_of = Rc::new(move |item: usize| {
            let mut lines = lines.try_borrow_mut().ok()?;
            if item >= lines.count() || lines.slots != cells.count() {
                return None;
            }
            let (slot, span) = (lines.slot_of(item), lines.span(item));
            Some(cell_constraints(vertical, cells.of(slot, span).1))
        });
        *self.state.prefetch.constraints_of.borrow_mut() = Some(constraints_of);
        slots
    }
}

/// Measures the composed items line by line, each once.
struct Measurer<'a, 'scope> {
    scope: &'a mut MeasureScope<'scope>,
    measurables: &'a [Measurable],
    /// The items composed, in the children's order, and the first of those in a run.
    children: &'a [usize],
    composed_start: usize,
    placeables: Vec<Option<Placeable>>,
    lines: &'a mut Lines,
    slots: &'a Slots,
    vertical: bool,
    /// The lines measured: their cells' range in `cells`, and their size along the grid.
    measured: MeasuredLines,
    /// The cells of the lines measured: each item and its offset across the grid.
    cells: Vec<(Placeable, i32)>,
    /// The items' sizes along the grid (by item: what a slot leaving was measured by, D460), and
    /// the lines' (the estimates).
    sizes: &'a mut Sizes,
    line_sizes: &'a mut Sizes,
    /// The list's prefetch and items: an item whose content isn't applied (a request's paused
    /// composition) or was removed is one it lacks (D592), if any is (`unready`).
    prefetch: &'a super::prefetch::Prefetch,
    items: &'a Items,
    unready: Option<u64>,
    /// It lacked such an item: composed in the frame's next round.
    lacked_unready: std::cell::Cell<bool>,
}

/// The lines a measurement measured, by line: from the first one measured, a measurement's lines
/// being next to each other (a look-up is an index, D456).
#[derive(Default)]
struct MeasuredLines {
    first: usize,
    /// From `first`: the line's cells' range in `cells` and its size, if measured.
    lines: Vec<Option<(usize, usize, i32)>>,
}

impl MeasuredLines {
    fn get(&self, line: usize) -> Option<(usize, usize, i32)> {
        *self.lines.get(line.checked_sub(self.first)?)?
    }

    fn insert(&mut self, line: usize, measured: (usize, usize, i32)) {
        if self.lines.is_empty() {
            self.first = line;
        } else if line < self.first {
            self.lines.splice(0..0, std::iter::repeat_n(None, self.first - line));
            self.first = line;
        }
        let at = line - self.first;
        if at >= self.lines.len() {
            self.lines.resize(at + 1, None);
        }
        self.lines[at] = Some(measured);
    }
}

impl Measurer<'_, '_> {
    /// Item `item`'s child, if it's composed.
    fn child_of(&self, item: usize) -> Option<usize> {
        if self.unready.is_some_and(|key| key == self.items.key(item)) {
            self.prefetch.complete_pending();
            self.lacked_unready.set(true);
            return None;
        }
        let child = match item.checked_sub(self.composed_start) {
            Some(position) if self.children.get(position) == Some(&item) => Some(position),
            _ => self.children.iter().position(|composed| *composed == item),
        };
        child.filter(|child| *child < self.measurables.len())
    }

    /// A line measured: its cells' range in `cells`, and its size along the grid.
    fn measured_line(&self, line: usize) -> Option<(usize, usize, i32)> {
        self.measured.get(line)
    }

    /// A line measured's size along the grid.
    fn size_of(&self, line: usize) -> i32 {
        self.measured_line(line).map_or(0, |(_, _, size)| size)
    }

    /// Measures item `item` taking `span` cells from `slot`.
    fn measure(&mut self, item: usize, slot: usize, span: usize) -> Option<(Placeable, i32)> {
        let child = self.child_of(item)?;
        let (offset, size) = self.slots.of(slot, span);
        let placeable = match self.placeables[child] {
            Some(placeable) => placeable,
            None => {
                let constraints = cell_constraints(self.vertical, size);
                let placeable = self.scope.measure(self.measurables[child], constraints);
                self.placeables[child] = Some(placeable);
                let main = if self.vertical { placeable.height } else { placeable.width };
                self.sizes.insert(item, main);
                placeable
            }
        };
        Some((placeable, offset))
    }

    /// Line `line`'s size along the grid, its items measured; `None` if it has no items or they
    /// aren't all composed.
    fn line(&mut self, line: usize) -> Option<i32> {
        if let Some((_, _, size)) = self.measured_line(line) {
            return Some(size);
        }
        let (start, end) = (self.lines.start(line), self.lines.start(line + 1));
        if start >= end {
            return None;
        }
        if (start..end).any(|item| self.child_of(item).is_none()) {
            // Not all composed (the first item alone, before the first layout): those that are
            // tell the size of the lines to come, for how many to compose.
            if !self.line_sizes.contains_key(line) {
                let mut slot = 0;
                let mut size = None;
                for item in start..end {
                    let span = self.lines.span(item);
                    if let Some((placeable, _)) = self.measure(item, slot, span) {
                        let main = if self.vertical { placeable.height } else { placeable.width };
                        size = Some(size.unwrap_or(0).max(main));
                    }
                    slot += span;
                }
                if let Some(size) = size {
                    self.line_sizes.insert(line, size);
                }
            }
            return None;
        }
        let first = self.cells.len();
        let mut slot = 0;
        let mut size = 0;
        for item in start..end {
            let span = self.lines.span(item);
            let (placeable, offset) = self.measure(item, slot, span)?;
            size = size.max(if self.vertical { placeable.height } else { placeable.width });
            self.cells.push((placeable, offset));
            slot += span;
        }
        self.measured.insert(line, (first, self.cells.len(), size));
        self.line_sizes.insert(line, size);
        Some(size)
    }

    /// Item `item` alone (pinned out of the composed lines), in its cell.
    fn item(&mut self, item: usize) -> Option<(Placeable, i32)> {
        if item >= self.lines.count() {
            return None;
        }
        let slot = self.lines.slot_of(item);
        let span = self.lines.span(item);
        self.measure(item, slot, span)
    }
}

/// Steps the position back over the lines before it while `offset` is negative, as a list's:
/// over those composed (their sizes known), far back by the average size, and near, waits for
/// them (the scrolling left pending): returns how many lines it wants composed before.
fn step_back(
    measurer: &mut Measurer<'_, '_>,
    state: &LazyListInner,
    spacing: i32,
    index: &mut usize,
    offset: &mut f32,
) -> usize {
    while *offset < 0.0 && *index > 0 {
        match measurer.line(*index - 1) {
            Some(size) => {
                *index -= 1;
                *offset += (size + spacing) as f32;
            }
            None => {
                let average = measurer.line_sizes.average() as f32 + spacing as f32;
                let skip = ((-*offset) / average) as usize;
                if skip > MOST_MISSING {
                    let skip = skip.min(*index);
                    *index -= skip;
                    *offset += skip as f32 * average;
                    continue;
                }
                // As many lines as cover the offset (D576).
                let wanted = ((-*offset) / average).ceil().max(1.0) as usize;
                state.pending.set(state.pending.get() + *offset);
                *offset = 0.0;
                return wanted;
            }
        }
    }
    *offset = offset.max(0.0);
    0
}

impl MeasurePolicy for LazyGridLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let state = &self.state;
        // Observed: a scroll measures the grid again.
        state.remeasure.get();
        let info = &self.info;
        let vertical = info.vertical;
        let density = scope.density();
        let node = scope.node_id();
        // Laid out left to right, and mirrored by `place_relative` right to left: a vertical
        // grid's first column, a horizontal one's first line, at the right.
        let layout_direction = scope.layout_direction();
        state.rtl.set(layout_direction == compose_ui::LayoutDirection::Rtl);
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
        let (main_max, cross_max) = if vertical {
            (constraints.max_height, constraints.max_width)
        } else {
            (constraints.max_width, constraints.max_height)
        };
        assert!(
            main_max != Constraints::INFINITY,
            "compose-foundation: a lazy grid was measured with an infinite size along its axis \
             (in a scrolling parent?): give it a size"
        );
        assert!(
            cross_max != Constraints::INFINITY,
            "compose-foundation: a lazy grid was measured with an infinite size across its axis \
             (a vertical grid's width, a horizontal one's height): give it a size"
        );
        let cross_room = (cross_max - cross_before - cross_after).max(0);
        let cross_spacing = px(info.cross_spacing);
        let slots = self.slots(cross_room, cross_spacing, density);
        // A cell's constraints, for the items composing for theirs (`BoxWithConstraints`).
        let hint = cell_constraints(vertical, slots.sizes[0]);
        if state.item_constraints_written.replace(Some(hint)) != Some(hint) {
            state.item_constraints.set(Some(hint));
        }
        let mut lines = self.lines.borrow_mut();
        if lines.slots != slots.count() {
            *lines = Lines::new(info.items.clone(), slots.count());
        }
        let count = info.items.count;
        let children = state.children.borrow().clone();
        let (composed_start, composed_end) = composed_range(&children, count);
        let (mut sizes, mut line_sizes) = (state.sizes.borrow_mut(), state.line_sizes.borrow_mut());
        let mut measurer = Measurer {
            scope,
            measurables,
            children: &children,
            composed_start,
            placeables: vec![None; measurables.len()],
            lines: &mut lines,
            slots: &slots,
            vertical,
            measured: MeasuredLines::default(),
            cells: Vec::new(),
            sizes: &mut sizes,
            line_sizes: &mut line_sizes,
            prefetch: &state.prefetch,
            items: &info.items,
            unready: state.prefetch.layout_started(),
            lacked_unready: std::cell::Cell::new(false),
        };
        let m = &mut measurer;
        // The position, with the scrolling asked for; the first visible item found again by
        // its key if items came or went before it.
        let (mut first_item, offset) = state.position.get();
        let scrolled = state.pending.replace(0.0);
        let mut offset = offset + scrolled;
        const MOST_OFFSET: f32 = 1.0e9;
        offset = if offset.is_nan() { 0.0 } else { offset.clamp(-MOST_OFFSET, MOST_OFFSET) };
        if let Some(first_key) = state.first_key.get()
            && (first_item >= count || info.items.key(first_item) != first_key)
            && let Some(found) = info.items.index_of(first_key, first_item)
        {
            first_item = found;
        }
        if count == 0 {
            // As a list's: the initial position waits for the first items; a grid emptied
            // starts again at its top.
            if state.had_items.get() {
                state.position.set((0, 0.0));
                state.first_key.set(None);
                state.had_items.set(false);
            } else {
                state.pending.set(state.pending.get() + offset - state.position.get().1);
            }
            // Known: an empty list takes no scroll (D480).
            state.room_backward.set(0.0);
            state.room_forward.set(0.0);
            state.room_estimated.set((false, false));
            state.beyond_more.set((false, false));
            drop(measurer);
            return scope.layout(constraints.min_width, constraints.min_height);
        }
        state.had_items.set(true);
        // From here on, in lines: the first visible one, and the pixels into it.
        let mut index = m.lines.line_of(first_item.min(count - 1));
        let mut wanted_before = step_back(m, state, spacing, &mut index, &mut offset);
        // On over the lines scrolled past, while they're composed; far ahead, jumped over by the
        // average size (an estimate).
        while m.lines.exists(index + 1) {
            let Some(size) = m.line(index) else {
                let average = m.line_sizes.average() as f32 + spacing as f32;
                let skip = (offset / average) as usize;
                if skip > MOST_MISSING {
                    let target = m.lines.at_most_last(index + skip);
                    offset -= (target - index) as f32 * average;
                    index = target;
                }
                break;
            };
            let size = (size + spacing) as f32;
            if offset >= size {
                offset -= size;
                index += 1;
            } else {
                break;
            }
        }
        // Past the end of the last line: at its end.
        if !m.lines.exists(index + 1)
            && let Some(size) = m.line(index)
        {
            offset = offset.min(size as f32);
        }
        let mut placed: Vec<(usize, i32)> = Vec::new();
        let mut end_index;
        let mut missing;
        let mut content_end;
        let mut attempt = 0;
        loop {
            attempt += 1;
            placed.clear();
            missing = false;
            let mut main = before - offset.round() as i32;
            end_index = index;
            while m.lines.exists(end_index) && main < main_max {
                match m.line(end_index) {
                    Some(size) => {
                        placed.push((end_index, main));
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
            // At the end with room left: scrolled back so the last line ends the viewport.
            let gap = (main_max - content_end) as f32;
            if !missing
                && !m.lines.exists(end_index)
                && gap > 0.0
                && (index > 0 || offset > 0.0)
                // Not while items before wait to be composed: the scrolling left pending holds
                // the gap already (a second measurement would step it back again, D457).
                && wanted_before == 0
                && attempt < 3
            {
                offset -= gap;
                let wanted = step_back(m, state, spacing, &mut index, &mut offset);
                wanted_before = wanted_before.max(wanted);
                if wanted > 0 {
                    break;
                }
                continue;
            }
            break;
        }
        // The lines before the first visible one under the start padding are laid out and shown
        // too, as a list's and Compose's (`measureLazyGrid` fills `beforeContentPadding`): `lead`
        // is the first line laid out; those not composed are, in this frame's next round.
        let mut lead = index;
        if let Some(&(_, first_main)) = placed.first() {
            let mut start = first_main;
            let mut leading: Vec<(usize, i32)> = Vec::new();
            while start > spacing.min(0) && lead > 0 {
                let Some(size) = m.line(lead - 1) else {
                    let room = (start - spacing.min(0)) as usize;
                    let wanted = room.div_ceil((m.line_sizes.average() + spacing).max(1) as usize);
                    wanted_before += wanted.min(lead);
                    break;
                };
                start -= size + spacing;
                leading.push((lead - 1, start));
                lead -= 1;
            }
            leading.reverse();
            placed.splice(0..0, leading);
        }
        let at_end = !m.lines.exists(end_index);
        let scrolled_into = index > 0 || offset > 0.0;
        let main_size = if scrolled_into { main_max } else { content_end.min(main_max) };
        let main_total = if vertical {
            constraints.constrain_height(main_size)
        } else {
            constraints.constrain_width(main_size)
        };
        // Lines shorter than the grid, not scrolled: placed by the arrangement.
        if !missing
            && at_end
            && index == 0
            && offset == 0.0
            && content_end < main_total
            && !placed.is_empty()
        {
            let line_sizes: Vec<i32> = placed.iter().map(|(line, _)| m.size_of(*line)).collect();
            let mut positions = vec![0; line_sizes.len()];
            info.arrangement.arrange(
                main_total - before - after,
                &line_sizes,
                compose_ui::LayoutDirection::Ltr,
                &density,
                &mut positions,
            );
            for ((_, main), position) in placed.iter_mut().zip(positions) {
                *main = before + position;
            }
        }
        // The lines wanted composed: those in view (with `GRID_LINES_BEYOND`, and `BEYOND`
        // beyond each side, D456: where a list composed a viewport's worth, a grid's lines hold
        // several items, all placed and drawn), and as many more as the room left would take if
        // lines were missing. Compose's grid composes those in view (D565).
        let average = m.line_sizes.average();
        let optimizations = m.scope.optimizations();
        let policy = Policy::of(optimizations);
        let beyond =
            if optimizations.contains(compose_ui::platform::Optimization::GRID_LINES_BEYOND) {
                BEYOND
            } else {
                0
            };
        // And the lines a focus search asked for beyond those (D477).
        let (beyond_before, beyond_after) = state.beyond_bounds.get();
        let wanted_start = lead.saturating_sub(beyond + beyond_before + wanted_before);
        let mut wanted_end =
            m.lines.at_most_last((end_index + beyond + beyond_after).saturating_sub(1)) + 1;
        if missing {
            let in_view = if placed.is_empty() {
                average
            } else {
                (placed.iter().map(|(line, _)| m.size_of(*line)).sum::<i32>() / placed.len() as i32)
                    .max(1)
            };
            let room = (main_max - content_end).max(0);
            // As many lines as cover the room left, no more (D576), and as a list's (D472,
            // D487), no more than `FIRST_STEP` lines, as many as it placed, or twice what the
            // round before asked for: a first layout knowing one short line doesn't compose
            // dozens (D578).
            let cap = placed
                .len()
                .max(FIRST_STEP)
                .max(2 * state.asked.get())
                .min(MOST_IN_A_ROUND)
                .min(MOST_MISSING);
            let more =
                (room.max(1) as usize).div_ceil((in_view + spacing).max(1) as usize).min(cap);
            state.asked.set(more);
            wanted_end = m.lines.at_most_last(end_index + more + beyond + beyond_after - 1) + 1;
        }
        if !missing {
            state.asked.set(0);
        }
        // The items composed, from the lines: whole lines.
        let wanted_items = (m.lines.start(wanted_start), m.lines.start(wanted_end));
        let covers = composed_start <= wanted_items.0 && composed_end >= wanted_items.1;
        let prefetch = &state.prefetch;
        prefetch.policy.set(policy);
        prefetch.visible.set(m.lines.start(end_index) - m.lines.start(lead));
        // Compose's grid prefetch strategy, once the layout settled: a line at a time, each of
        // its items a request.
        if !missing && wanted_before == 0 {
            let laid = match (placed.first().copied(), placed.last().copied()) {
                (Some((_, first_main)), Some((last_line, last_main))) => {
                    let first = lead.saturating_sub(beyond);
                    let last = m.lines.at_most_last(end_index - 1 + beyond);
                    Some(Laid {
                        first,
                        last,
                        first_item: m.lines.start(first),
                        last_item: m.lines.start(last + 1).saturating_sub(1),
                        start_distance: -first_main,
                        // The last visible item's own size, as Compose's `lastItem.size` (an item
                        // shorter than its line ends before the line does).
                        end_distance: last_main
                            + m.sizes
                                .get(m.lines.start(last_line + 1).saturating_sub(1))
                                .unwrap_or_else(|| m.size_of(last_line))
                            + spacing
                            - main_max,
                    })
                }
                _ => None,
            };
            prefetch.after_layout(
                state,
                self.scheduler.as_ref(),
                laid.as_ref(),
                count,
                &mut |line| m.lines.start(line)..m.lines.start(line + 1),
            );
        }
        // The items that may stay composed: with `KEEP_ITEMS_BEHIND`, those ahead in the scroll
        // direction and just behind it kept (lines: the limits prefetching composes within too,
        // in items, whatever the lines' spans, D456); else, as Compose's, those wanted and those
        // requests composed next to them.
        let keep = policy.keep_behind > 0;
        // A focus search that laid lines out beyond the bounds is over: they go (D482, D577).
        let beyond_ended = state.beyond_ended.take();
        let limits = if keep {
            let (behind, ahead) = if prefetch.forward.get() {
                (policy.keep_behind, policy.ahead)
            } else {
                (policy.ahead, policy.keep_behind)
            };
            (m.lines.start(wanted_start.saturating_sub(behind)), m.lines.start(wanted_end + ahead))
        } else {
            // The line after the wanted ones the scroll's way, if no focus search just ended.
            let next = if prefetch.forward.get() {
                m.lines.start(wanted_end + 1)
            } else {
                m.lines.start(wanted_start.saturating_sub(1))
            };
            prefetch.target(
                wanted_items,
                (composed_start, composed_end),
                (!beyond_ended).then_some(next),
            )
        };
        let past_limits = composed_start < limits.0 || composed_end > limits.1;
        let search_ended = beyond_ended && past_limits;
        if search_ended {
            state.dispose_leaving.set(true);
        }
        let mut composed = (composed_start, composed_end);
        let composed_more = state.composed_more.take();
        // A paused composition cancelled: the items composed again first (D594).
        let recompose = m.lacked_unready.get() || prefetch.take_cancelled();
        if keep {
            let slack = 2 * BEYOND;
            let too_many = (wanted_start > slack
                && composed_start < m.lines.start(wanted_start - slack))
                || composed_end > m.lines.start(wanted_end + slack)
                || search_ended;
            if composed_more && covers && !too_many {
                state.composed.set(composed);
            }
            if !covers || too_many || recompose {
                let start = if (composed_start..=composed_end).contains(&wanted_items.0) {
                    composed_start.max(limits.0)
                } else {
                    wanted_items.0
                };
                let end = if (composed_start..=composed_end).contains(&wanted_items.1) {
                    composed_end.min(limits.1)
                } else {
                    wanted_items.1
                };
                composed = (start, end);
                state.compose(composed);
            }
        } else if !covers || search_ended || recompose {
            // Items to compose (or a focus search's to let go): this composition deactivates
            // those no longer wanted too.
            composed = limits;
            state.compose(composed);
        } else if composed_more {
            state.composed.set(composed);
        }
        prefetch.node.set(Some(node));
        prefetch.count.set(count);
        prefetch.wanted.set(wanted_items);
        prefetch.limits.set(limits);
        // Those no longer wanted are deactivated by the next composition (D575).
        prefetch.trim_between_frames(state, self.scheduler.as_ref(), composed);
        let first_item = m.lines.start(index);
        state.position.set((first_item, offset));
        state.first_key.set(Some(info.items.key(first_item)));
        // In items, as `first_visible_item_index` (a scrollbar's estimates): a line's extent
        // shared among its items.
        let lines = m.lines.count_estimate().max(1);
        let per_item = (average + spacing) as f32 * lines as f32 / count as f32;
        let layout_info = (count, per_item, spacing, main_max, before + after);
        if state.layout_info_written.replace(layout_info) != layout_info {
            state.layout_info.set(layout_info);
        }
        let rounded = (first_item, offset.round() as i32);
        if state.published.get() != rounded {
            state.published.set(rounded);
            state.first_index.set(rounded.0);
            state.first_offset.set(rounded.1);
        }
        // The whole lines composed out of view are placed too (clipped away), so focus can move
        // to them; those composed beyond the wanted ones (prefetched) aren't. A focused item's
        // line among those is placed, next to the others.
        let composed_lines = if composed_start < composed_end {
            let first = m.lines.line_of(composed_start);
            let first = if m.lines.start(first) == composed_start { first } else { first + 1 };
            let end = if composed_end >= count {
                m.lines.line_of(count - 1) + 1
            } else {
                m.lines.line_of(composed_end)
            };
            (first, end.max(first))
        } else {
            (index, index)
        };
        let (mut placed_start, mut placed_end) =
            (composed_lines.0.max(wanted_start), composed_lines.1.min(wanted_end));
        if let Some(key) = state.pinned.get()
            && let Some(pinned) = info.items.index_of(key, state.pinned_near.get())
        {
            let line = m.lines.line_of(pinned);
            if (placed_end..composed_lines.1).contains(&line) {
                placed_end = line + 1;
            } else if (composed_lines.0..placed_start).contains(&line) {
                placed_start = line;
            }
        }
        let mut beyond_lines: Vec<(usize, i32)> = Vec::new();
        let mut main =
            placed.last().map_or(before, |(line, main)| main + m.size_of(*line) + spacing);
        for next in end_index..placed_end {
            if let Some(size) = m.line(next) {
                beyond_lines.push((next, main));
                main += size + spacing;
            }
        }
        let extent_end = main - spacing + after;
        // Those before, placed first, in the items' order.
        let mut ahead_lines: Vec<(usize, i32)> = Vec::new();
        let mut main = placed.first().map_or(before, |(_, main)| *main);
        for previous in (placed_start.min(lead)..lead).rev() {
            if let Some(size) = m.line(previous) {
                main -= size + spacing;
                ahead_lines.push((previous, main));
            }
        }
        ahead_lines.reverse();
        let extent_start = main - before;
        let pending = state.pending.get();
        let reaches_start = (lead == 0 || placed_start == 0) && wanted_before == 0;
        let average = m.line_sizes.average() as f32 + spacing as f32;
        state.room_backward.set(if reaches_start {
            (-extent_start) as f32 + pending
        } else {
            (placed_start.min(lead) as f32 * average - extent_start as f32 + pending).max(1.0)
        });
        let reaches_end = !missing && (at_end || !m.lines.exists(placed_end));
        state.room_estimated.set((!reaches_start, !reaches_end));
        // What input taken past an estimated end couldn't use goes to the parents once the
        // layout is over, as a list's (D479, D490); the distance moved in lines measured.
        let mut post_scroll: Option<Box<dyn FnOnce()>> = None;
        if pending == 0.0
            && !missing
            && wanted_before == 0
            && let Some(overshoot) = state.overshoot.borrow_mut().take()
        {
            let (from_item, from_offset) = overshoot.start(&info.items);
            let from_line = m.lines.line_of(from_item.min(count - 1));
            if let Some(moved) =
                m.line_sizes.distance((from_line, from_offset), (index, offset), spacing)
            {
                post_scroll = overshoot.rest(moved);
            }
        }
        state.beyond_more.set((placed_start.min(lead) > 0, m.lines.exists(placed_end)));
        state.room_forward.set(if reaches_end {
            (extent_end.max(content_end) - main_max).max(0) as f32 - pending
        } else {
            let lines_after =
                m.lines.count_estimate().saturating_sub(placed_end.max(end_index)) as f32;
            (lines_after * average + (extent_end.max(content_end) - main_max) as f32 - pending)
                .max(1.0)
        });
        // A focused item composed out of the composed lines (pinned), placed out of view.
        let mut pinned: Vec<(Placeable, i32, i32)> = Vec::new();
        for &item in &children {
            if (item < composed_start || item >= composed_end)
                && let Some((placeable, cross)) = m.item(item)
            {
                let size = if vertical { placeable.height } else { placeable.width };
                let at = if item < first_item { -size - main_max } else { main_max * 2 };
                pinned.push((placeable, at, cross));
            }
        }
        // Sizes far from view are forgotten, so the estimates follow the data.
        if m.line_sizes.len() > 4 * MOST_MISSING {
            m.line_sizes.retain(|line| line.abs_diff(index) <= MOST_MISSING);
        }
        let near = MOST_MISSING * slots.count();
        if m.sizes.len() > 4 * near {
            m.sizes.retain(|item| item.abs_diff(first_item) <= near);
        }
        let (width, height) =
            if vertical { (cross_max, main_total) } else { (main_total, cross_max) };
        let place = |scope: &mut MeasureScope<'_>, placeable: &Placeable, main: i32, cross: i32| {
            let cross = cross_before + cross;
            if vertical {
                scope.place_relative(placeable, cross, main);
            } else {
                scope.place_relative(placeable, main, cross);
            }
        };
        for &(line, main) in ahead_lines.iter().chain(&placed).chain(&beyond_lines) {
            let (start, end, _) = m.measured_line(line).expect("a line measured");
            for &(placeable, cross) in &m.cells[start..end] {
                place(m.scope, &placeable, main, cross);
            }
        }
        for (placeable, main, cross) in &pinned {
            place(m.scope, placeable, *main, *cross);
        }
        if let Some(post_scroll) = post_scroll {
            m.scope.run_after_layout(post_scroll);
        }
        m.scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "LazyGrid"
    }
}
