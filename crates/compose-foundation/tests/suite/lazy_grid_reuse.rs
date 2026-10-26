//! Lazy grids compose item by item, as Compose's (D450): each item in a slot of its own, kept by
//! its key, and an item coming in reuses the composition and nodes of an item of its content
//! type that left (up to 7 kept per content type), as a lazy list's (D430). Spans (`GridItemSpan`,
//! D451) lay items out in lines as Compose's `LazyGridSpanLayoutProvider`. Each test runs on a
//! vertical and a horizontal grid.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::NodeId;
use compose_ui::{
    Constraints, IntSize, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier, Rect,
    UnitExt,
};

thread_local! {
    /// The node each item's content was last measured in, by item, every node an item was ever
    /// measured in, and the constraints it was last measured with.
    static NODES: RefCell<HashMap<u64, NodeId>> = RefCell::new(HashMap::new());
    static ALL_NODES: RefCell<HashMap<u64, HashSet<NodeId>>> = RefCell::new(HashMap::new());
    static CONSTRAINTS: RefCell<HashMap<u64, Constraints>> = RefCell::new(HashMap::new());
    /// What happened to the items' remember observers, in order.
    static EVENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The items whose effect is live.
    static LIVE: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    /// Each composed item's saveable count, to change from the test.
    static COUNTS: RefCell<HashMap<u64, MutableState<u64>>> = RefCell::new(HashMap::new());
}

fn reset() {
    NODES.with(|nodes| nodes.borrow_mut().clear());
    ALL_NODES.with(|nodes| nodes.borrow_mut().clear());
    CONSTRAINTS.with(|constraints| constraints.borrow_mut().clear());
    EVENTS.with(|events| events.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    COUNTS.with(|counts| counts.borrow_mut().clear());
}

fn node_of(item: u64) -> NodeId {
    NODES.with(|nodes| nodes.borrow().get(&item).copied()).expect("an item measured")
}

/// The nodes items in `items` were ever measured in.
fn nodes_of(items: impl IntoIterator<Item = u64>) -> HashSet<NodeId> {
    let all = ALL_NODES.with(|nodes| nodes.borrow().clone());
    items.into_iter().flat_map(|item| all.get(&item).cloned().unwrap_or_default()).collect()
}

fn events() -> Vec<String> {
    EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

fn live() -> Vec<u64> {
    let mut live: Vec<u64> = LIVE.with(|live| live.borrow().iter().copied().collect());
    live.sort_unstable();
    live
}

/// Records the node and constraints it measures with, for `item`: as large as its cell across
/// the grid, 20 px along it.
struct Recording {
    item: u64,
}

impl MeasurePolicy for Recording {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let node = scope.node_id();
        NODES.with(|nodes| nodes.borrow_mut().insert(self.item, node));
        ALL_NODES.with(|nodes| nodes.borrow_mut().entry(self.item).or_default().insert(node));
        CONSTRAINTS.with(|all| all.borrow_mut().insert(self.item, constraints));
        let along = |max: i32| if max == Constraints::INFINITY { 20 } else { max };
        scope.layout(along(constraints.max_width), along(constraints.max_height))
    }
}

/// An observer telling what happens to it.
struct Observer(u64);

impl RememberObserver for Observer {
    fn on_remembered(&self) {
        EVENTS.with(|events| events.borrow_mut().push(format!("remembered {}", self.0)));
    }
    fn on_forgotten(&self) {
        EVENTS.with(|events| events.borrow_mut().push(format!("forgotten {}", self.0)));
    }
    fn on_abandoned(&self) {
        EVENTS.with(|events| events.borrow_mut().push(format!("abandoned {}", self.0)));
    }
}

/// An item with a remembered value, an observer, an effect and a saveable count, all of `item`:
/// an item reusing another's composition must find none of the other's.
#[composable]
fn Item(item: u64) {
    let remembered = remember(move || Rc::new(item));
    assert_eq!(*remembered, item, "a reused item kept the value remembered by another");
    remember_observer(move || Observer(item));
    DisposableEffect((), move |scope| {
        LIVE.with(|live| live.borrow_mut().insert(item));
        scope.on_dispose(move || {
            LIVE.with(|live| live.borrow_mut().remove(&item));
        })
    });
    let count = remember_saveable(move || mutable_state_of(item * 10));
    COUNTS.with(|counts| counts.borrow_mut().insert(item, count));
    let policy = remember(move || Rc::new(Recording { item }) as Rc<dyn MeasurePolicy>);
    compose_ui::Layout(
        Modifier.test_tag(format!("item {item}: {}", count.get())),
        policy,
        callback(|| {}),
    );
}

/// The grid's items: each one's key, content type (`None`: the forms without one) and span
/// (`None`: the default).
type Items = Rc<Vec<(u64, Option<u32>, Option<usize>)>>;

/// Declares `items` with `items_keyed`, `items_keyed_with` if any has a type, or
/// `items_keyed_with_span` if any has a span.
fn declare(scope: &LazyGridScope, items: Items) {
    let (keys, spans, types, contents) =
        (items.clone(), items.clone(), items.clone(), items.clone());
    if items.iter().all(|(_, content_type, span)| content_type.is_none() && span.is_none()) {
        scope.items_keyed(
            items.len(),
            move |index| keys[index].0,
            move |index| Item(contents[index].0),
        );
    } else if items.iter().all(|(_, _, span)| span.is_none()) {
        scope.items_keyed_with(
            items.len(),
            move |index| keys[index].0,
            move |index| types[index].1,
            move |index| Item(contents[index].0),
        );
    } else {
        scope.items_keyed_with_span(
            items.len(),
            move |index| keys[index].0,
            move |_, index| GridItemSpan(spans[index].2.unwrap_or(1)),
            move |index| types[index].1,
            move |index| Item(contents[index].0),
        );
    }
}

#[composable]
fn Grid(
    state: LazyGridState,
    items: MutableState<Items>,
    shown: MutableState<bool>,
    vertical: bool,
) {
    let items = items.get();
    if !shown.get() {
        return;
    }
    let modifier = Modifier.size((100.dp(), 100.dp()));
    if vertical {
        LazyVerticalGrid!(GridCells::Fixed(2), modifier = modifier, state = state, move |scope| {
            declare(&scope, items.clone())
        });
    } else {
        LazyHorizontalGrid!(
            GridCells::Fixed(2),
            modifier = modifier,
            state = state,
            move |scope| { declare(&scope, items.clone()) }
        );
    }
}

/// A 100 px grid of two cells across (5 lines of 20 px in view, and a few composed beyond) at
/// its start.
struct Fixture {
    ui: ComposeTestRule,
    state: Owned<LazyGridState>,
    items: OwnedState<Items>,
    shown: OwnedState<bool>,
}

impl Fixture {
    fn new(vertical: bool, items: Vec<(u64, Option<u32>, Option<usize>)>) -> Fixture {
        reset();
        let state = LazyGridState::new(0, 0);
        let items = mutable_state_of(Rc::new(items));
        let shown = mutable_state_of(true);
        let (given, handle, shown_handle) = (*state, items.handle(), shown.handle());
        let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
            Grid(given, handle, shown_handle, vertical)
        });
        ui.wait_for_idle();
        Fixture { ui, state, items, shown }
    }

    /// Scrolls `lines` lines on (or back), a frame for each, without prefetching.
    fn scroll(&mut self, lines: i32) {
        for _ in 0..lines.abs() {
            self.state.dispatch_raw_delta(lines.signum() as f32 * 20.0);
            self.ui.wait_for_idle();
        }
    }

    /// The nodes of the grid's children: those of the items composed and those kept for reuse.
    fn children(&mut self, item: u64) -> usize {
        let node = node_of(item);
        self.ui.host().with_tree(|tree| {
            let item = tree.parent_of(node).expect("an item's node");
            let grid = tree.parent_of(item).expect("an item in the grid");
            tree.children_of(grid).count()
        })
    }

    fn set_items(&mut self, items: Vec<(u64, Option<u32>, Option<usize>)>) {
        self.items.set(Rc::new(items));
        self.ui.wait_for_idle();
    }
}

fn untyped(keys: impl IntoIterator<Item = u64>) -> Vec<(u64, Option<u32>, Option<usize>)> {
    keys.into_iter().map(|key| (key, None, None)).collect()
}

fn typed(
    keys: impl IntoIterator<Item = u64>,
    content_type: impl Fn(u64) -> u32,
) -> Vec<(u64, Option<u32>, Option<usize>)> {
    keys.into_iter().map(|key| (key, Some(content_type(key)), None)).collect()
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

const ORIENTATIONS: [bool; 2] = [true, false];

#[test]
fn a_grid_item_coming_in_reuses_the_nodes_of_one_that_left() {
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, untyped(0..1000));
        grid.scroll(100);
        let measured = NODES.with(|nodes| nodes.borrow().len());
        assert!(measured > 200, "{measured} items measured");
        // The items composed at once and those kept for reuse: far fewer nodes than items.
        let nodes = nodes_of(0..1000);
        assert!(nodes.len() < 50, "vertical {vertical}: {} nodes for {measured}", nodes.len());
        grid.ui.on_node_with_tag("item 200: 2000").assert_exists();
        grid.ui.on_node_with_tag("item 0: 0").assert_does_not_exist();
        assert_eq!(grid.state.first_visible_item_index(), 200);
        // Each item in its own node, as Compose's grid items (not a row's).
        assert_ne!(node_of(200), node_of(201));
    }
}

#[test]
fn a_reused_grid_item_forgets_the_old_items_values_and_remembers_its_own() {
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, untyped(0..1000));
        let composed = live();
        events();
        grid.scroll(20);
        let events = events();
        assert!(!events.iter().any(|event| event.starts_with("abandoned")), "{events:?}");
        // Each item that came was remembered once, and each that went forgotten once.
        let mut remembered = HashSet::new();
        let mut forgotten = HashSet::new();
        for event in &events {
            let (kind, item) = event.split_once(' ').expect("an event");
            let item: u64 = item.parse().expect("an item");
            let first = match kind {
                "remembered" => remembered.insert(item),
                _ => forgotten.insert(item),
            };
            assert!(first, "{event} twice: {events:?}");
        }
        let now = live();
        let came: HashSet<u64> =
            now.iter().copied().filter(|item| !composed.contains(item)).collect();
        let went: HashSet<u64> =
            composed.iter().copied().filter(|item| !now.contains(item)).collect();
        let passing: HashSet<u64> = remembered.intersection(&forgotten).copied().collect();
        assert_eq!(remembered.difference(&passing).copied().collect::<HashSet<u64>>(), came);
        assert_eq!(forgotten.difference(&passing).copied().collect::<HashSet<u64>>(), went);
        assert!(grid.children(now[0]) >= now.len());
    }
}

#[test]
fn a_grid_item_scrolled_back_gets_its_saved_state_and_no_other_items() {
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, untyped(0..1000));
        COUNTS.with(|counts| counts.borrow()[&3].set(7));
        grid.ui.wait_for_idle();
        grid.ui.on_node_with_tag("item 3: 7").assert_exists();
        grid.scroll(40);
        // The items reusing item 3's slot (or any) start at their own counts.
        for item in 80..90 {
            grid.ui.on_node_with_tag(&format!("item {item}: {}", item * 10)).assert_exists();
        }
        grid.scroll(-40);
        grid.ui.on_node_with_tag("item 3: 7").assert_exists();
        grid.ui.on_node_with_tag("item 2: 20").assert_exists();
    }
}

#[test]
fn up_to_seven_grid_items_of_a_content_type_are_kept_for_reuse() {
    crate::legacy::lazy_lists();
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, untyped(0..60));
        let composed = live().len();
        assert!(composed > 10, "{composed} composed");
        // Two items left: the others leave at once, 7 of them kept for reuse, the rest disposed.
        grid.set_items(untyped(0..2));
        assert_eq!(live(), [0, 1]);
        assert_eq!(grid.children(0), 2 + 7);
        // Of two content types, up to 7 of each: items 0 to 8 of one, the others of the other.
        let content_type = |key: u64| u32::from(key > 8);
        let mut grid = Fixture::new(vertical, typed(0..60, content_type));
        // Those measured (one composed and never measured is disposed, D460).
        let composed: Vec<u64> = live()
            .into_iter()
            .filter(|item| NODES.with(|nodes| nodes.borrow().contains_key(item)))
            .collect();
        assert!(composed.len() > 10, "{composed:?} composed");
        grid.set_items(vec![(0, Some(0), None)]);
        let left = |kind: u32| {
            composed.iter().filter(move |key| **key != 0 && content_type(**key) == kind)
        };
        assert!(left(0).count() > 7);
        assert_eq!(grid.children(0), 1 + 7 + left(1).count().min(7));
    }
}

#[test]
fn a_grid_item_reuses_only_an_item_of_its_content_type() {
    for vertical in ORIENTATIONS {
        // Items 0 to 99 of type 1, the others of type 2.
        let mut grid = Fixture::new(vertical, typed(0..400, |key| if key < 100 { 1 } else { 2 }));
        grid.scroll(30);
        let first_type = nodes_of(0..100);
        grid.scroll(40);
        // The items of type 2 reuse each other's nodes, never those of type 1, kept for reuse.
        let second_type = nodes_of(100..400);
        assert!(first_type.is_disjoint(&second_type), "{first_type:?} {second_type:?}");
        assert!(second_type.len() < 50, "{} nodes", second_type.len());
        // Untyped items (Compose's `null`) reuse untyped ones only.
        let mut grid = Fixture::new(vertical, untyped(0..400));
        grid.scroll(30);
        let untyped_nodes = nodes_of(0..400);
        grid.set_items(typed(1000..1400, |_| 1));
        let typed_nodes = nodes_of(1000..1400);
        assert!(untyped_nodes.is_disjoint(&typed_nodes));
    }
}

#[test]
fn a_mixed_line_reuses_each_items_slot_by_its_own_type() {
    // Every third item of type 1, the others of type 2: lines mix them differently (1 2, 2 1,
    // 2 2, …), and each item reuses one of its type, whichever line or cell it was in (a grid
    // reusing whole rows would need a row of the same mix).
    let first = |key: &u64| key.is_multiple_of(3);
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, typed(0..1000, |key| u32::from(key % 3 == 0) + 1));
        grid.scroll(20);
        let ones = nodes_of((0..1000).filter(first));
        let twos = nodes_of((0..1000).filter(|key| !first(key)));
        grid.scroll(80);
        let ones_later = nodes_of((0..1000).filter(first));
        let twos_later = nodes_of((0..1000).filter(|key| !first(key)));
        assert!(ones_later.is_disjoint(&twos_later), "vertical {vertical}");
        // Few new nodes after the first scroll: those it composed were reused.
        assert!(ones_later.len() <= ones.len() + 7 && twos_later.len() <= twos.len() + 7);
        assert!(ones_later.len() + twos_later.len() < 60);
    }
}

/// A line's cells' bounds by item.
fn bounds(ui: &mut ComposeTestRule, item: u64) -> Rect {
    let count = COUNTS.with(|counts| counts.borrow()[&item].get());
    ui.on_node_with_tag(&format!("item {item}: {count}")).fetch_semantics_node().bounds
}

thread_local! {
    /// The `max_current_line_span` each item's span was asked with, in order.
    static ASKED: RefCell<HashMap<usize, Vec<usize>>> = RefCell::new(HashMap::new());
}

#[test]
fn spans_lay_items_out_in_lines_as_compose() {
    for vertical in ORIENTATIONS {
        reset();
        ASKED.with(|asked| asked.borrow_mut().clear());
        // A header over a whole line, three cells, a cell and two cells, two cells, and two
        // cells that don't fit the cell left: at the start of the next line.
        let spans: [Option<usize>; 8] =
            [None, Some(1), Some(1), Some(1), Some(1), Some(2), Some(2), Some(2)];
        let content = move || {
            let span = move |scope: LazyGridItemSpanScope, index: usize| {
                ASKED.with(|asked| {
                    asked.borrow_mut().entry(index).or_default().push(scope.max_current_line_span())
                });
                GridItemSpan(spans[index].unwrap_or(scope.max_line_span()))
            };
            let grid = move |scope: LazyGridScope| {
                scope.items_with_span(8, span, |index| index == 0, |index| Item(index as u64));
            };
            if vertical {
                LazyVerticalGrid!(
                    GridCells::Fixed(3),
                    modifier = Modifier.size((300.dp(), 300.dp())),
                    horizontal_arrangement = Arrangement::spaced_by(10.dp()),
                    content = grid
                );
            } else {
                LazyHorizontalGrid!(
                    GridCells::Fixed(3),
                    modifier = Modifier.size((300.dp(), 300.dp())),
                    vertical_arrangement = Arrangement::spaced_by(10.dp()),
                    content = grid
                );
            }
        };
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), content);
        ui.wait_for_idle();
        // Cells of (300 − 2 × 10) / 3: 94, 93 and 93 across, at 0, 104 and 207, as Compose's.
        // (offset across, size across, line) for each item.
        let expected: [(f32, f32, f32); 8] = [
            (0.0, 300.0, 0.0),
            (0.0, 94.0, 1.0),
            (104.0, 93.0, 1.0),
            (207.0, 93.0, 1.0),
            (0.0, 94.0, 2.0),
            (104.0, 196.0, 2.0),
            (0.0, 197.0, 3.0),
            (0.0, 197.0, 4.0),
        ];
        for (item, (across, size, line)) in expected.into_iter().enumerate() {
            let rect = bounds(&mut ui, item as u64);
            let found = if vertical {
                (rect.left, rect.width(), rect.top / 20.0)
            } else {
                (rect.top, rect.height(), rect.left / 20.0)
            };
            assert_eq!(found, (across, size, line), "vertical {vertical}: item {item}");
        }
        // Each item asked with the cells left in its line; item 7 again for a new line.
        let asked = ASKED.with(|asked| asked.borrow().clone());
        assert_eq!(asked[&0][0], 3);
        assert_eq!(asked[&3][0], 1);
        assert_eq!(asked[&5][0], 2);
        assert_eq!(asked[&7][..2], [1, 3], "vertical {vertical}");
        // A spanned item is measured fixed across its cells, as Compose's.
        let constraints = CONSTRAINTS.with(|all| all.borrow()[&5]);
        if vertical {
            assert_eq!((constraints.min_width, constraints.max_width), (196, 196));
            assert_eq!(constraints.max_height, Constraints::INFINITY);
        } else {
            assert_eq!((constraints.min_height, constraints.max_height), (196, 196));
            assert_eq!(constraints.max_width, Constraints::INFINITY);
        }
    }
}

#[test]
fn a_spanned_header_reuses_only_headers_and_the_first_visible_item_is_its_lines_first() {
    // Sections of a header (a whole line, type 0) and 9 cells (type 1), as a photo gallery.
    let sections = |key: u64| -> (u64, Option<u32>, Option<usize>) {
        if key.is_multiple_of(10) { (key, Some(0), Some(2)) } else { (key, Some(1), None) }
    };
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, (0..2000).map(sections).collect());
        grid.scroll(150);
        let headers = nodes_of((0..2000).step_by(10));
        let cells = nodes_of((0..2000).filter(|key| key % 10 != 0));
        assert!(headers.is_disjoint(&cells));
        assert!(headers.len() <= 7 + 6, "{} header nodes", headers.len());
        assert!(cells.len() < 50, "{} cell nodes", cells.len());
        // Lines of a header, then 2, 2, 2, 2 and 1 cells: 6 lines a section.
        let first = grid.state.first_visible_item_index();
        assert_eq!(first, 250, "vertical {vertical}: line 150 starts section 25");
        // A jump to an item: its line first.
        grid.state.request_scroll_to_item(1004, 0);
        grid.ui.wait_for_idle();
        assert_eq!(grid.state.first_visible_item_index(), 1003);
    }
}

#[test]
fn prefetch_composes_the_next_grid_items_in_the_slots_of_those_left_behind() {
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, untyped(0..2000));
        let mut prefetched = 0;
        let mut before = HashSet::new();
        for step in 0..60 {
            if step == 20 {
                before = nodes_of(0..2000);
            }
            grid.state.dispatch_raw_delta(10.0);
            grid.ui.wait_for_idle();
            let composed: HashSet<u64> = live().into_iter().collect();
            grid.ui.host().prefetch(later());
            let ahead: Vec<u64> =
                live().into_iter().filter(|item| !composed.contains(item)).collect();
            prefetched += ahead.len();
            // Measured ahead, each in its cell, in nodes other items had: the prefetch reuses
            // the items it leaves behind, as a frame does.
            if step >= 20 {
                for item in &ahead {
                    assert!(
                        before.contains(&node_of(*item)),
                        "step {step}: item {item} in a new node"
                    );
                    let constraints = CONSTRAINTS.with(|all| all.borrow()[item]);
                    let across =
                        if vertical { constraints.max_width } else { constraints.max_height };
                    assert_eq!(across, 50);
                }
            }
        }
        assert!(prefetched > 40, "{prefetched} items prefetched");
        assert!(nodes_of(0..2000).len() < 50);
    }
}

#[test]
fn reordered_grid_items_keep_their_nodes_and_a_removed_ones_is_reused() {
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, untyped(0..100));
        let first: Vec<NodeId> = (0..6).map(node_of).collect();
        // Reversed: each keeps its node, in another cell.
        let mut reversed: Vec<u64> = (0..6).rev().collect();
        reversed.extend(6..100);
        grid.set_items(untyped(reversed.clone()));
        for (item, node) in (0..6).zip(&first) {
            assert_eq!(node_of(item), *node, "item {item}");
        }
        // The first visible item followed by its key, as Compose's: item 0's line (the third,
        // items 1 and 0) first.
        assert_eq!(grid.state.first_visible_item_index(), 4);
        let (zero, one) = (bounds(&mut grid.ui, 0), bounds(&mut grid.ui, 1));
        assert_eq!((zero.left, zero.top), if vertical { (50.0, 0.0) } else { (0.0, 50.0) });
        assert_eq!((one.left, one.top), (0.0, 0.0));
        // Item 2 removed: the composed range moves with the first visible item (item 0, one
        // place up, D458), so at most one item comes in, taking item 2's nodes; else item 2's slot
        // is kept for reuse, for the next item to come. No node is created or released.
        reversed.retain(|item| *item != 2);
        let composed: HashSet<u64> = live().into_iter().collect();
        let nodes = grid.ui.host().with_tree(|tree| tree.node_count());
        grid.set_items(untyped(reversed));
        let came: Vec<u64> = live().into_iter().filter(|item| !composed.contains(item)).collect();
        assert!(came.len() <= 1, "{came:?}");
        assert_eq!(grid.ui.host().with_tree(|tree| tree.node_count()), nodes);
        grid.ui.on_node_with_tag("item 2: 20").assert_does_not_exist();
        for (item, node) in [0, 1, 3, 4, 5].into_iter().zip([0, 1, 3, 4, 5].map(|i| first[i])) {
            assert_eq!(node_of(item), node, "item {item}");
        }
    }
}

#[test]
fn grid_items_inserted_before_others_leave_their_state_and_nodes_alone() {
    // Compose keeps each item's composition by its key: items inserted first move the others
    // to other cells and lines, each with its state (a grid keyed by rows lost it, D144).
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, untyped(0..8));
        COUNTS.with(|counts| counts.borrow()[&7].set(70_000));
        grid.ui.wait_for_idle();
        let nodes: Vec<NodeId> = (0..8).map(node_of).collect();
        events();
        grid.set_items(untyped([100, 101].into_iter().chain(0..8)));
        let events = events();
        assert_eq!(
            events,
            ["remembered 100", "remembered 101"],
            "vertical {vertical}: only the new ones composed"
        );
        assert_eq!((0..8).map(node_of).collect::<Vec<_>>(), nodes);
        grid.ui.on_node_with_tag("item 7: 70000").assert_exists();
        let seven = bounds(&mut grid.ui, 7);
        assert_eq!((seven.left, seven.top), if vertical { (50.0, 80.0) } else { (80.0, 50.0) });
    }
}

#[test]
fn a_grid_leaving_the_composition_disposes_its_items_and_those_kept_for_reuse_once() {
    for vertical in ORIENTATIONS {
        let mut grid = Fixture::new(vertical, untyped(0..1000));
        grid.scroll(30);
        let composed = live();
        events();
        let nodes = grid.ui.host().with_tree(|tree| tree.node_count());
        grid.shown.set(false);
        grid.ui.wait_for_idle();
        assert_eq!(live(), Vec::<u64>::new());
        let mut forgotten: Vec<u64> = events()
            .iter()
            .map(|event| event.strip_prefix("forgotten ").expect("forgotten").parse().unwrap())
            .collect();
        forgotten.sort_unstable();
        assert_eq!(forgotten, composed);
        let left = grid.ui.host().with_tree(|tree| tree.node_count());
        assert!(left < nodes && left <= 3, "{left} nodes left of {nodes}");
    }
}

#[test]
fn the_list_and_indexed_forms_take_keys_spans_and_content_types() {
    reset();
    let names: Vec<&'static str> = vec!["a", "b", "c", "d", "e"];
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let names = names.clone();
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.size((200.dp(), 200.dp())),
            move |scope: LazyGridScope| {
                scope.item_keyed_with_span(
                    "title",
                    |line: LazyGridItemSpanScope| GridItemSpan(line.max_line_span()),
                    "title",
                    || Item(1000),
                );
                scope.items_list_keyed_with_span(
                    names.clone(),
                    |name: &&str| *name,
                    |_, name: &&str| GridItemSpan(if *name == "c" { 2 } else { 1 }),
                    |_: &&str| "name",
                    |name: &&str| Item(name.as_bytes()[0] as u64),
                );
                scope.items_indexed_with_span(
                    names.clone(),
                    |_, index, _: &&str| GridItemSpan(1 + index % 2),
                    |_, _: &&str| "indexed",
                    |index, _: &&str| Item(2000 + index as u64),
                );
                scope.item(|| Item(3000));
            }
        );
    });
    ui.wait_for_idle();
    let top_left = |ui: &mut ComposeTestRule, item: u64| {
        let rect = bounds(ui, item);
        (rect.left, rect.top, rect.width())
    };
    // The title over the line; a b; c over a line; d e; then 2000 alone (2001 doesn't fit the
    // cell left), 2001 over a line, 2002 alone, 2003 over a line, 2004 and the last item.
    assert_eq!(top_left(&mut ui, 1000), (0.0, 0.0, 200.0));
    assert_eq!(top_left(&mut ui, b'b' as u64), (100.0, 20.0, 100.0));
    assert_eq!(top_left(&mut ui, b'c' as u64), (0.0, 40.0, 200.0));
    assert_eq!(top_left(&mut ui, b'e' as u64), (100.0, 60.0, 100.0));
    assert_eq!(top_left(&mut ui, 2000), (0.0, 80.0, 100.0));
    assert_eq!(top_left(&mut ui, 2001), (0.0, 100.0, 200.0));
    assert_eq!(top_left(&mut ui, 2002), (0.0, 120.0, 100.0));
    assert_eq!(top_left(&mut ui, 2004), (0.0, 160.0, 100.0));
    assert_eq!(top_left(&mut ui, 3000), (100.0, 160.0, 100.0));
}

/// A grid showing one item at a time, jumped to another: as a list's (D1264), the item's slot is
/// reused for the new one after the measurement found it not composed, and the grid places it.
#[test]
fn a_one_item_viewports_item_reused_after_a_jump_is_placed() {
    let state = LazyGridState::new(0, 0);
    let grid = *state;
    let mut ui = compose_test(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.size((50.dp(), 20.dp())),
            state = grid,
            |scope: LazyGridScope| {
                scope.items(100, |index| {
                    Box!(modifier = Modifier.size((50.dp(), 20.dp())).test_tag(format!("{index}")));
                });
            }
        );
    });
    for index in 1..6 {
        state.scroll_to_item(index, 0);
        ui.on_node_with_tag(&format!("{index}"))
            .assert_is_displayed()
            .assert_top_position_in_root_is_equal_to(0.dp());
    }
}
