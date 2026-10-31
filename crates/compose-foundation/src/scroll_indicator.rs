//! `ScrollIndicatorState`, as Compose's: what a scroll indicator needs of a scrollable, whichever
//! it is. Reading them may read state, so an indicator drawn from them draws again as they change.

use crate::gestures::Orientation;
use crate::lazy::pager::PagerState;
use crate::lazy::{LazyGridLayoutInfo, LazyGridState, LazyListLayoutInfo, LazyListState};
use crate::scroll::ScrollState;

/// Where a scrollable is and how long it is along its axis, in pixels, as `ScrollIndicatorState`:
/// lazy layouts' are estimates from the items in view.
pub trait ScrollIndicatorState {
    /// From the visual start of the container, whatever the reverse layout or scrolling, as
    /// `scrollOffset`.
    fn scroll_offset(&self) -> i32;
    /// The content's length, as `contentSize`.
    fn content_size(&self) -> i32;
    /// What's in view of it, as `viewportSize`.
    fn viewport_size(&self) -> i32;
}

impl ScrollIndicatorState for ScrollState {
    fn scroll_offset(&self) -> i32 {
        if self.reverse_scrolling() { self.max_value() - self.value() } else { self.value() }
    }
    fn content_size(&self) -> i32 {
        ScrollState::content_size(self)
    }
    fn viewport_size(&self) -> i32 {
        ScrollState::viewport_size(self)
    }
}

fn axis_of(orientation: Orientation, size: compose_ui::IntSize) -> i32 {
    match orientation {
        Orientation::Vertical => size.height,
        Orientation::Horizontal => size.width,
    }
}

/// A length worked out in `i64`: a list's count can be `i32::MAX`, whose estimates don't fit. The
/// largest then, as the pager's, where Compose's `Int`s wrap.
fn clamped(length: i64) -> i32 {
    length.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// As `visibleItemsAverageSize`.
fn list_average(info: &LazyListLayoutInfo) -> i32 {
    let items = &info.visible_items_info;
    if items.is_empty() {
        return 0;
    }
    let sum: i32 = items.iter().map(|item| item.size).sum();
    sum / items.len() as i32 + info.main_axis_item_spacing
}

/// As `calculateContentSize`.
fn list_content_size(info: &LazyListLayoutInfo) -> i32 {
    let padding = i64::from(info.before_content_padding + info.after_content_padding);
    let count = info.total_items_count as i64;
    if count == 0 {
        return padding as i32;
    }
    let spacing = i64::from(info.main_axis_item_spacing);
    clamped((i64::from(list_average(info)) - spacing) * count + (count - 1) * spacing + padding)
}

impl ScrollIndicatorState for LazyListState {
    fn scroll_offset(&self) -> i32 {
        let info = self.layout_info();
        let offset = i64::from(list_average(&info)) * self.first_visible_item_index() as i64
            + i64::from(self.first_visible_item_scroll_offset());
        if info.reverse_layout {
            let viewport = axis_of(info.orientation, info.viewport_size);
            clamped(i64::from(list_content_size(&info)) - i64::from(viewport) - offset)
        } else {
            clamped(offset)
        }
    }
    fn content_size(&self) -> i32 {
        list_content_size(&self.layout_info())
    }
    fn viewport_size(&self) -> i32 {
        let info = self.layout_info();
        axis_of(info.orientation, info.viewport_size)
    }
}

/// An item's line along the scroll, -1 for a sticky one's.
fn line_of(item: &crate::lazy::LazyGridItemInfo, orientation: Orientation) -> i32 {
    match orientation {
        Orientation::Vertical => item.row,
        Orientation::Horizontal => item.column,
    }
}

/// As `visibleLinesAverageMainAxisSize`.
fn grid_average(info: &LazyGridLayoutInfo) -> i32 {
    let items = &info.visible_items_info;
    let vertical = info.orientation == Orientation::Vertical;
    let length = |index: usize| {
        if vertical { items[index].size.height } else { items[index].size.width }
    };
    let (mut start, mut total, mut lines) = (0, 0, 0);
    while start < items.len() {
        let line = line_of(&items[start], info.orientation);
        // A sticky item or one leaving isn't in a line.
        if line == -1 {
            start += 1;
            continue;
        }
        let mut longest = length(start);
        let mut end = start + 1;
        while end < items.len() && line_of(&items[end], info.orientation) == line {
            longest = longest.max(length(end));
            end += 1;
        }
        total += longest;
        lines += 1;
        start = end;
    }
    if lines == 0 { 0 } else { total / lines + info.main_axis_item_spacing }
}

/// As `calculateContentSize`.
fn grid_content_size(info: &LazyGridLayoutInfo) -> i32 {
    let padding = i64::from(info.before_content_padding + info.after_content_padding);
    if info.total_items_count == 0 {
        return padding as i32;
    }
    let lines = info.total_items_count.div_ceil(info.max_span.max(1)) as i64;
    let spacing = i64::from(info.main_axis_item_spacing);
    clamped((i64::from(grid_average(info)) - spacing) * lines + (lines - 1) * spacing + padding)
}

impl ScrollIndicatorState for LazyGridState {
    fn scroll_offset(&self) -> i32 {
        let info = self.layout_info();
        let items = &info.visible_items_info;
        let first_index = self.first_visible_item_index();
        let first_line = items
            .iter()
            .find(|item| item.index == first_index)
            .map_or(-1, |item| line_of(item, info.orientation));
        // A sticky item first has no line: the one before the first that has.
        let line = if first_line != -1 {
            first_line
        } else {
            let next = items
                .iter()
                .map(|item| line_of(item, info.orientation))
                .find(|line| *line != -1)
                .unwrap_or(0);
            (next - 1).max(0)
        };
        let offset = i64::from(grid_average(&info)) * i64::from(line)
            + i64::from(self.first_visible_item_scroll_offset());
        if info.reverse_layout {
            let viewport = axis_of(info.orientation, info.viewport_size);
            clamped(i64::from(grid_content_size(&info)) - i64::from(viewport) - offset)
        } else {
            clamped(offset)
        }
    }
    fn content_size(&self) -> i32 {
        grid_content_size(&self.layout_info())
    }
    fn viewport_size(&self) -> i32 {
        let info = self.layout_info();
        axis_of(info.orientation, info.viewport_size)
    }
}

impl ScrollIndicatorState for PagerState {
    fn scroll_offset(&self) -> i32 {
        let info = self.layout_info();
        let with_spacing = i64::from(info.page_size + info.page_spacing);
        let (first, first_offset) = info
            .visible_pages_info
            .first()
            .map_or((0, 0), |page| (page.index as i64, i64::from(-page.offset)));
        let offset = (with_spacing * first + first_offset).min(i64::from(i32::MAX)) as i32;
        if info.reverse_layout {
            self.content_size() - axis_of(info.orientation, info.viewport_size) - offset
        } else {
            offset
        }
    }
    fn content_size(&self) -> i32 {
        let info = self.layout_info();
        let padding = i64::from(info.before_content_padding + info.after_content_padding);
        let pages = self.page_count() as i64;
        if pages == 0 {
            return padding as i32;
        }
        let total = i64::from(info.page_size) * pages
            + (pages - 1) * i64::from(info.page_spacing)
            + padding;
        total.min(i64::from(i32::MAX)) as i32
    }
    fn viewport_size(&self) -> i32 {
        let info = self.layout_info();
        axis_of(info.orientation, info.viewport_size)
    }
}
