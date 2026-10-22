//! `LazyVerticalGrid`, as Compose's: items in columns (a fixed count, or as many as fit at a
//! minimum width), composed lazily by rows. Built on `LazyColumn`, a row of cells per item.

use std::cell::{Cell, RefCell};
use std::hash::Hash;
use std::rc::Rc;

use compose_runtime::{ComposeArg, ObjectHandle, Owned, composable, hash_key};
use compose_ui::{Alignment, Dp, Modifier, PaddingValues};

use crate::layout::{Arrangement, Box, BoxWithConstraints, BoxWithConstraintsScope};
use crate::lazy::{LazyColumn, LazyListScope, LazyListState};

/// How a grid's width is cut into columns, as Compose's `GridCells`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GridCells {
    /// This many columns.
    Fixed(usize),
    /// As many columns as fit at least this wide.
    Adaptive(Dp),
}

impl ComposeArg for GridCells {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl GridCells {
    /// The columns in `available` pixels with `spacing` between them, as Compose's
    /// `calculateCrossAxisCellSizes`' count.
    fn count(self, available: i32, spacing: i32, density: compose_ui::Density) -> usize {
        match self {
            GridCells::Fixed(count) => count.max(1),
            GridCells::Adaptive(min) => {
                let min = density.round_to_px(min).max(1);
                ((available + spacing) / (min + spacing)).max(1) as usize
            }
        }
    }
}

/// A grid's scroll position, as Compose's `LazyGridState`: in items (a row holds several).
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_lazy_grid_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LazyGridState {
    list: LazyListState,
    object: ObjectHandle<GridInner>,
}

struct GridInner {
    _list: Owned<LazyListState>,
    /// The columns of the last composition (0 before the first).
    columns: Cell<usize>,
    /// An item to scroll to once the columns are known, and the pixels into its row.
    pending: Cell<Option<(usize, i32)>>,
}

impl LazyGridState {
    /// Scrolled to item `first_visible_item_index`'s row, `first_visible_item_scroll_offset`
    /// pixels into it.
    #[track_caller]
    pub fn new(
        first_visible_item_index: usize,
        first_visible_item_scroll_offset: i32,
    ) -> Owned<LazyGridState> {
        let list = LazyListState::new(0, 0);
        let handle = *list;
        let inner = GridInner {
            _list: list,
            columns: Cell::new(0),
            pending: Cell::new(Some((first_visible_item_index, first_visible_item_scroll_offset))),
        };
        Owned::new(inner, |object| LazyGridState { list: handle, object })
    }

    #[track_caller]
    fn inner(&self) -> Rc<GridInner> {
        self.object.get()
    }

    /// The first item of the first visible row, as `firstVisibleItemIndex`.
    pub fn first_visible_item_index(&self) -> usize {
        self.list.first_visible_item_index() * self.inner().columns.get().max(1)
    }

    pub fn first_visible_item_scroll_offset(&self) -> i32 {
        self.list.first_visible_item_scroll_offset()
    }

    /// Scrolls to the row of item `index`, as `scrollToItem`.
    pub fn scroll_to_item(&self, index: usize, scroll_offset: i32) {
        let inner = self.inner();
        match inner.columns.get() {
            // Not composed yet: once the columns are known.
            0 => inner.pending.set(Some((index, scroll_offset))),
            columns => self.list.scroll_to_item(index / columns, scroll_offset),
        }
    }

    /// Scrolls by `pixels`, returning whether it moved.
    pub fn scroll_by(&self, pixels: f32) -> bool {
        self.list.scroll_by(pixels)
    }

    pub fn can_scroll_forward(&self) -> bool {
        self.list.can_scroll_forward()
    }

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
                let position = state.inner().pending.get().unwrap_or_else(|| {
                    (state.first_visible_item_index(), state.first_visible_item_scroll_offset())
                });
                Some(position.save())
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

struct GridInterval {
    count: usize,
    key: Option<Rc<dyn Fn(usize) -> u64>>,
    content: Rc<dyn Fn(usize)>,
}

/// What a grid's content declares its items with, as Compose's `LazyGridScope`.
#[derive(Clone, Default)]
pub struct LazyGridScope {
    intervals: Rc<RefCell<Vec<GridInterval>>>,
}

impl LazyGridScope {
    /// One item, as `item { … }`.
    pub fn item(&self, content: impl Fn() + 'static) {
        self.intervals.borrow_mut().push(GridInterval {
            count: 1,
            key: None,
            content: Rc::new(move |_| content()),
        });
    }

    /// `count` items, as `items(count) { index -> … }`.
    pub fn items(&self, count: usize, content: impl Fn(usize) + 'static) {
        self.intervals.borrow_mut().push(GridInterval {
            count,
            key: None,
            content: Rc::new(content),
        });
    }

    /// `count` items with keys, as `items(count, key = { … }) { … }`: a row keeps its state
    /// with its first item's key.
    pub fn items_keyed<K: Hash>(
        &self,
        count: usize,
        key: impl Fn(usize) -> K + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        self.intervals.borrow_mut().push(GridInterval {
            count,
            key: Some(Rc::new(move |index| hash_key(&key(index)))),
            content: Rc::new(content),
        });
    }
}

impl ComposeArg for LazyGridScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.intervals, &other.intervals)
    }
}

/// The items declared, flattened: each item's content and key by its index.
struct GridItems {
    intervals: Vec<GridInterval>,
    count: usize,
}

impl GridItems {
    fn find(&self, mut index: usize) -> Option<(&GridInterval, usize)> {
        for interval in &self.intervals {
            if index < interval.count {
                return Some((interval, index));
            }
            index -= interval.count;
        }
        None
    }

    fn key(&self, index: usize) -> u64 {
        match self.find(index) {
            Some((interval, local)) => interval.key.as_ref().map_or(index as u64, |key| key(local)),
            None => index as u64,
        }
    }

    fn content(&self, index: usize) {
        if let Some((interval, local)) = self.find(index) {
            (interval.content)(local);
        }
    }
}

/// A vertically scrolling grid, as Compose's `LazyVerticalGrid`: `columns` cut its width
/// (inside `content_padding`) into equal cells, `horizontal_arrangement` between them, and its
/// items fill rows from the start, composed lazily row by row.
#[composable]
#[allow(non_snake_case)]
pub fn LazyVerticalGrid(
    columns: GridCells,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_lazy_grid_state(0, 0))] state: LazyGridState,
    #[default(crate::layout::PaddingValues::ZERO)] content_padding: PaddingValues,
    #[default(crate::layout::Arrangement::TOP)] vertical_arrangement: Arrangement,
    #[default(crate::layout::Arrangement::Start)] horizontal_arrangement: Arrangement,
    content: impl Fn(LazyGridScope) + 'static,
) {
    let scope = LazyGridScope::default();
    content(scope.clone());
    let intervals = scope.intervals.take();
    let count = intervals.iter().map(|interval| interval.count).sum();
    let items = Rc::new(GridItems { intervals, count });
    let density = compose_ui::LOCAL_DENSITY.current();
    BoxWithConstraints(
        modifier,
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |box_scope: BoxWithConstraintsScope| {
            let padding = density.round_to_px(content_padding.start)
                + density.round_to_px(content_padding.end);
            let spacing = horizontal_arrangement.spacing(&density);
            let available = (box_scope.constraints.max_width - padding).max(0);
            let count = columns.count(available, spacing, density);
            // The cells' constraints, for those composing for theirs (`BoxWithConstraints`).
            let cell = (available - spacing * (count as i32 - 1)).max(0) / count as i32;
            let cell_hint = Some(compose_ui::Constraints::new(
                cell,
                cell,
                0,
                compose_ui::Constraints::INFINITY,
            ));
            // A new column count keeps the first item in view (and its row's offset), as
            // Compose's; the first count applies a scroll asked for before it.
            let inner = state.inner();
            let before = inner.columns.replace(count);
            if let Some((item, offset)) = inner.pending.take() {
                state.list.set_position_quietly(item / count, offset);
            } else if before != 0 && before != count {
                let (row, offset) = state.list.position_untracked();
                state.list.set_position_quietly(row * before / count, offset);
            }
            let rows = items.count.div_ceil(count);
            let items = items.clone();
            LazyColumn(
                Modifier.fill_max_size(),
                state.list,
                content_padding,
                false,
                vertical_arrangement,
                Alignment::START,
                true,
                compose_runtime::callback(move |list: LazyListScope| {
                    let items = items.clone();
                    let keys = items.clone();
                    list.items_keyed(
                        rows,
                        move |row| keys.key(row * count),
                        move |row| {
                            let items = items.clone();
                            let policy = Rc::new(GridRowLayout { columns: count, spacing })
                                as Rc<dyn compose_ui::MeasurePolicy>;
                            compose_ui::Layout(
                                Modifier.fill_max_width(),
                                policy,
                                compose_runtime::callback(move || {
                                    let items = items.clone();
                                    compose_runtime::CompositionLocalProvider(
                                        compose_ui::LOCAL_CONSTRAINTS_HINT.provides(cell_hint),
                                        move || {
                                            for column in 0..count {
                                                let index = row * count + column;
                                                if index < items.count {
                                                    let items = items.clone();
                                                    Box(
                                                        Modifier,
                                                        Alignment::TOP_START,
                                                        false,
                                                        compose_runtime::callback(move |_| {
                                                            items.content(index)
                                                        }),
                                                    );
                                                }
                                            }
                                        },
                                    );
                                }),
                            );
                        },
                    );
                }),
            );
        }),
    );
}

/// A grid row: its cells side by side, `spacing` apart, as wide as Compose's
/// `calculateCellsCrossAxisSize` makes them (the width without spacing divided evenly, the
/// remaining pixels one each to the first cells), as tall as the tallest.
struct GridRowLayout {
    columns: usize,
    spacing: i32,
}

impl compose_ui::MeasurePolicy for GridRowLayout {
    fn measure(
        &self,
        scope: &mut compose_ui::MeasureScope<'_>,
        measurables: &[compose_ui::Measurable],
        constraints: compose_ui::Constraints,
    ) -> compose_ui::MeasureResult {
        let width = constraints.max_width;
        let columns = self.columns.max(1) as i32;
        let without_spacing = (width - self.spacing * (columns - 1)).max(0);
        let (cell, remaining) = (without_spacing / columns, without_spacing % columns);
        let mut x = 0;
        let mut height = 0;
        let mut placeables = Vec::with_capacity(measurables.len());
        for (index, &measurable) in measurables.iter().enumerate() {
            let size = cell + i32::from((index as i32) < remaining);
            let placeable = scope.measure(
                measurable,
                compose_ui::Constraints::new(size, size, 0, constraints.max_height),
            );
            height = height.max(placeable.height);
            placeables.push((placeable, x));
            x += size + self.spacing;
        }
        for (placeable, x) in &placeables {
            scope.place_relative(placeable, *x, 0);
        }
        scope.layout(width, constraints.constrain_height(height))
    }

    fn debug_name(&self) -> &'static str {
        "LazyGridRow"
    }
}
