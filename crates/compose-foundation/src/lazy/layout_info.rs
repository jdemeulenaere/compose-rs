//! What a lazy list or grid laid out, as Compose's `LazyListLayoutInfo` and
//! `LazyGridLayoutInfo`: the items in view with their offsets and sizes, the viewport and the
//! padding. The measurement keeps its items' indices, offsets and sizes in a buffer of its own
//! (no allocation per frame); the info is built when read, its keys and content types then.

use std::cell::{Cell, RefCell};

use compose_runtime::OwnedState;
use compose_ui::{IntOffset, IntSize};

use crate::gestures::Orientation;

/// An item in view, as Compose's `LazyListItemInfo`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LazyListItemInfo {
    /// Its index in the list.
    pub index: usize,
    /// Its key, hashed (compose-rs keys are 64-bit hashes, D170): `hash_key(&key)` for a
    /// keyed item.
    pub key: u64,
    /// Its start along the axis, from the start of the content (after the start padding;
    /// negative under it), in the list's direction (from the bottom, reversed).
    pub offset: i32,
    /// Its length along the axis.
    pub size: i32,
    /// Its content type, hashed (`None`: Compose's `null`).
    pub content_type: Option<u64>,
}

/// What a lazy list laid out last, as Compose's `LazyListLayoutInfo`.
#[derive(Clone, PartialEq, Debug)]
pub struct LazyListLayoutInfo {
    /// The items in view (those under the padding too, and the sticky header), by index.
    pub visible_items_info: Vec<LazyListItemInfo>,
    /// Where the viewport starts, from the start of the content: minus the start padding.
    pub viewport_start_offset: i32,
    /// Where the viewport ends, from the start of the content.
    pub viewport_end_offset: i32,
    pub total_items_count: usize,
    /// The list's size.
    pub viewport_size: IntSize,
    pub orientation: Orientation,
    pub reverse_layout: bool,
    /// The padding before the first item (the bottom's, reversed).
    pub before_content_padding: i32,
    pub after_content_padding: i32,
    /// The spacing between items.
    pub main_axis_item_spacing: i32,
}

impl Default for LazyListLayoutInfo {
    fn default() -> LazyListLayoutInfo {
        LazyListLayoutInfo {
            visible_items_info: Vec::new(),
            viewport_start_offset: 0,
            viewport_end_offset: 0,
            total_items_count: 0,
            viewport_size: IntSize::new(0, 0),
            orientation: Orientation::Vertical,
            reverse_layout: false,
            before_content_padding: 0,
            after_content_padding: 0,
            main_axis_item_spacing: 0,
        }
    }
}

/// An item in view in a grid, as Compose's `LazyGridItemInfo`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LazyGridItemInfo {
    pub index: usize,
    /// Its key, hashed (D170).
    pub key: u64,
    /// Its position from the start of the content (after the start padding), in the grid's
    /// directions.
    pub offset: IntOffset,
    pub size: IntSize,
    /// Its row (a vertical grid's line; [`UNKNOWN_ROW`](Self::UNKNOWN_ROW) for a horizontal
    /// grid's sticky item).
    pub row: i32,
    /// Its column (a horizontal grid's line).
    pub column: i32,
    pub content_type: Option<u64>,
    /// The cells it spans.
    pub span: usize,
}

impl LazyGridItemInfo {
    /// As `LazyGridItemInfo.UnknownRow`.
    pub const UNKNOWN_ROW: i32 = -1;
    /// As `LazyGridItemInfo.UnknownColumn`.
    pub const UNKNOWN_COLUMN: i32 = -1;
}

/// What a lazy grid laid out last, as Compose's `LazyGridLayoutInfo`.
#[derive(Clone, PartialEq, Debug)]
pub struct LazyGridLayoutInfo {
    pub visible_items_info: Vec<LazyGridItemInfo>,
    pub viewport_start_offset: i32,
    pub viewport_end_offset: i32,
    pub total_items_count: usize,
    pub viewport_size: IntSize,
    pub orientation: Orientation,
    pub reverse_layout: bool,
    pub before_content_padding: i32,
    pub after_content_padding: i32,
    /// The spacing between lines.
    pub main_axis_item_spacing: i32,
    /// The cells in a line.
    pub max_span: usize,
}

impl Default for LazyGridLayoutInfo {
    fn default() -> LazyGridLayoutInfo {
        LazyGridLayoutInfo {
            visible_items_info: Vec::new(),
            viewport_start_offset: 0,
            viewport_end_offset: 0,
            total_items_count: 0,
            viewport_size: IntSize::new(0, 0),
            orientation: Orientation::Vertical,
            reverse_layout: false,
            before_content_padding: 0,
            after_content_padding: 0,
            main_axis_item_spacing: 0,
            max_span: 1,
        }
    }
}

/// An item laid out in view: its index, its position (along the axis for a list; a grid's
/// cross-axis position and its line too) and its size.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct LaidItem {
    pub index: usize,
    pub main: i32,
    pub cross: i32,
    pub main_size: i32,
    pub cross_size: i32,
    pub line: i32,
    /// A grid item's first cell across its line.
    pub lane: i32,
    pub span: usize,
    /// A list's sticky header, where it sticks (not where it would be).
    pub sticky: bool,
}

/// Where a lazy layout will be once the scrolling asked since its last layout is laid out (a
/// scroll scope's view, D858): the first visible item and the pixels its line is scrolled off,
/// the last visible item, and the visible lines' average size with the spacing.
pub(crate) struct Ahead {
    pub first: usize,
    pub offset: i32,
    pub last: usize,
    pub average: i32,
}

/// A line laid out (a list's item, a grid's line): its first and last items, its start along
/// the axis and its size.
#[derive(Clone, Copy)]
pub(crate) struct LaidLine {
    pub first: usize,
    pub last: usize,
    pub main: i32,
    pub size: i32,
}

/// The lines of `laid` (sticky headers left out), in order: a list's items, a grid's lines.
pub(crate) fn laid_lines(laid: &[LaidItem], grid: bool) -> Vec<LaidLine> {
    let mut lines: Vec<(i32, LaidLine)> = Vec::new();
    for item in laid.iter().filter(|item| !item.sticky) {
        match lines.last_mut() {
            Some((line_index, line)) if grid && *line_index == item.line => {
                line.last = item.index;
                line.size = line.size.max(item.main_size);
            }
            _ => lines.push((
                item.line,
                LaidLine {
                    first: item.index,
                    last: item.index,
                    main: item.main,
                    size: item.main_size,
                },
            )),
        }
    }
    lines.into_iter().map(|(_, line)| line).collect()
}

/// The position ahead of `lines` (from the last layout) once `pending` pixels more are laid out
/// (positive: towards the end), as Compose's `copyWithScrollDeltaWithoutRemeasure`: from the
/// lines laid out where it falls in them, else estimated at their average size; `cells` items a
/// line, `count` items, the viewport ending at `end`.
pub(crate) fn ahead(
    lines: &[LaidLine],
    pending: f32,
    spacing: i32,
    end: i32,
    count: usize,
    cells: usize,
) -> Option<Ahead> {
    let (first_line, last_line) = (lines.first()?, lines.last()?);
    let cells = cells.max(1);
    let average =
        (lines.iter().map(|line| line.size).sum::<i32>() / lines.len() as i32 + spacing).max(1);
    let start = |line: &LaidLine| line.main as f32 - pending;
    let found = lines.iter().find(|line| start(line) + (line.size + spacing) as f32 > 0.0);
    let (first, offset) = match found {
        // Before the first line laid out: estimated at the average size.
        Some(line) if line.first == first_line.first && start(line) > 0.0 => {
            let before = start(first_line);
            let lines_before = first_line.first / cells;
            let back = ((before / average as f32).ceil() as usize).min(lines_before);
            let offset = back as f32 * average as f32 - before;
            (first_line.first - back * cells, offset.round() as i32)
        }
        Some(line) => (line.first, (-start(line)).round() as i32),
        // Past the last line laid out: estimated too.
        None => {
            let past = -(start(last_line) + (last_line.size + spacing) as f32);
            let more = (past / average as f32).floor().max(0.0) as usize;
            let offset = past - more as f32 * average as f32;
            (last_line.last + 1 + more * cells, offset.round() as i32)
        }
    };
    let end = end as f32;
    let mut last = lines
        .iter()
        .rev()
        .find(|line| start(line) < end)
        .map_or(first, |line| line.last.max(first));
    // Room left after the last line laid out: the lines after it, at the average size.
    let last_end = start(last_line) + (last_line.size + spacing) as f32;
    if last == last_line.last && last_end < end {
        let more = ((end - last_end) / average as f32).ceil() as usize;
        last += more * cells;
    }
    let last = last.min(count.saturating_sub(1));
    Some(Ahead { first: first.min(count.saturating_sub(1)), offset, last, average })
}

/// The viewport and padding of a layout, without its items.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct LaidViewport {
    pub start: i32,
    pub end: i32,
    pub count: usize,
    pub size: IntSize,
    pub vertical: bool,
    pub reverse: bool,
    pub before: i32,
    pub after: i32,
    pub spacing: i32,
    pub max_span: usize,
}

/// What the measurement keeps of its layout for `layout_info`: the items in view and the
/// viewport, written when they change, with a state bumped then (what a read observes).
pub(crate) struct LaidOut {
    items: RefCell<Vec<LaidItem>>,
    /// The measurement's scratch, swapped with `items` when they differ.
    scratch: RefCell<Vec<LaidItem>>,
    viewport: Cell<LaidViewport>,
    /// Bumped by each change; what was last written to it, so the measurement writes it
    /// without reading it (a read would measure it again, D471).
    version: OwnedState<u64>,
    written: Cell<u64>,
}

impl LaidOut {
    #[track_caller]
    pub(crate) fn new() -> LaidOut {
        LaidOut {
            items: RefCell::new(Vec::new()),
            scratch: RefCell::new(Vec::new()),
            viewport: Cell::new(LaidViewport { max_span: 1, ..LaidViewport::default() }),
            version: compose_runtime::mutable_state_of(0),
            written: Cell::new(0),
        }
    }

    /// The measurement's buffer for this layout's items, empty.
    pub(crate) fn begin(&self) -> std::cell::RefMut<'_, Vec<LaidItem>> {
        let mut scratch = self.scratch.borrow_mut();
        scratch.clear();
        scratch
    }

    /// This layout's items (in `begin`'s buffer, sorted by index here) and viewport: kept, and
    /// the readers told, if they changed.
    /// `keys_changed`: the items' keys may differ for the same indices (new items laid out).
    pub(crate) fn end(&self, viewport: LaidViewport, keys_changed: bool) {
        let mut scratch = self.scratch.borrow_mut();
        scratch.sort_unstable_by_key(|item| item.index);
        let mut items = self.items.borrow_mut();
        if !keys_changed && *items == *scratch && self.viewport.get() == viewport {
            return;
        }
        std::mem::swap(&mut *items, &mut *scratch);
        self.viewport.set(viewport);
        let version = self.written.get().wrapping_add(1);
        self.written.set(version);
        self.version.set(version);
    }

    /// The items and viewport last laid out (a state read: a later layout that changes them
    /// tells the reader).
    pub(crate) fn read<R>(&self, read: impl FnOnce(&[LaidItem], LaidViewport) -> R) -> R {
        self.version.get();
        read(&self.items.borrow(), self.viewport.get())
    }
}
