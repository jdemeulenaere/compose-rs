//! Lazy lists reuse the composition and nodes of the items that left for those that come, as
//! Compose's lazy layouts (D430): an item leaving is deactivated and kept, up to 7 of each
//! content type (`LazyLayoutItemReusePolicy`), and an item of its content type coming in is
//! composed in its place, with its remembered values computed anew, its effects launched anew,
//! and its layout nodes kept and updated.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::NodeId;
use compose_ui::{
    Constraints, IntSize, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier, UnitExt,
};

thread_local! {
    /// The node each item's content was last measured in, by item, and every node an item was
    /// ever measured in.
    static NODES: RefCell<HashMap<u64, NodeId>> = RefCell::new(HashMap::new());
    static ALL_NODES: RefCell<HashMap<u64, HashSet<NodeId>>> = RefCell::new(HashMap::new());
    /// What happened to the items' remember observers, in order.
    static EVENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The items whose effect is live.
    static LIVE: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    /// Each composed item's saveable count, to change from the test.
    static COUNTS: RefCell<HashMap<u64, MutableState<u64>>> = RefCell::new(HashMap::new());
    /// The fixture's list height (dp).
    static LIST_HEIGHT: std::cell::Cell<u32> = const { std::cell::Cell::new(100) };
}

fn reset() {
    NODES.with(|nodes| nodes.borrow_mut().clear());
    ALL_NODES.with(|nodes| nodes.borrow_mut().clear());
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

/// Records the node it measures, for `item`.
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
        scope.layout(constraints.max_width.min(100), 20)
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

/// An item 20 px tall with a remembered value, an observer, an effect and a saveable count, all
/// of `item`: an item reusing another's composition must find none of the other's.
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

/// The list's items: each one's key, and content type (`None`: the forms without one).
type Items = Rc<Vec<(u64, Option<u32>)>>;

#[composable]
fn List(state: LazyListState, items: MutableState<Items>, shown: MutableState<bool>) {
    let items = items.get();
    if !shown.get() {
        return;
    }
    let height = LIST_HEIGHT.with(std::cell::Cell::get);
    LazyColumn!(
        modifier = Modifier.size((100.dp(), (height as f32).dp())),
        state = state,
        move |scope: LazyListScope| {
            let (keys, types, contents) = (items.clone(), items.clone(), items.clone());
            if items.iter().all(|(_, content_type)| content_type.is_none()) {
                scope.items_keyed(
                    items.len(),
                    move |index| keys[index].0,
                    move |index| Item(contents[index].0),
                );
            } else {
                scope.items_keyed_with(
                    items.len(),
                    move |index| keys[index].0,
                    move |index| types[index].1,
                    move |index| Item(contents[index].0),
                );
            }
        }
    );
}

/// A 100 px list (5 items of 20 px in view, and a few composed beyond) at its top.
struct Fixture {
    ui: ComposeTestRule,
    state: Owned<LazyListState>,
    items: OwnedState<Items>,
    shown: OwnedState<bool>,
}

impl Fixture {
    fn new(items: Vec<(u64, Option<u32>)>) -> Fixture {
        Fixture::sized(items, 100)
    }

    /// The same list 300 px tall: 15 items in view.
    fn tall(items: Vec<(u64, Option<u32>)>) -> Fixture {
        Fixture::sized(items, 300)
    }

    fn sized(items: Vec<(u64, Option<u32>)>, height: u32) -> Fixture {
        reset();
        LIST_HEIGHT.with(|list_height| list_height.set(height));
        let state = LazyListState::new(0, 0);
        let items = mutable_state_of(Rc::new(items));
        let shown = mutable_state_of(true);
        let (given, handle, shown_handle) = (*state, items.handle(), shown.handle());
        let mut ui = ComposeTestRule::sized(IntSize::new(200, 400), move || {
            List(given, handle, shown_handle)
        });
        ui.wait_for_idle();
        Fixture { ui, state, items, shown }
    }

    /// Scrolls `items` items down (or up), a frame for each, without prefetching.
    fn scroll(&mut self, items: i32) {
        for _ in 0..items.abs() {
            self.state.scroll_by(items.signum() as f32 * 20.0);
            self.ui.wait_for_idle();
        }
    }

    /// The nodes of the list's children: those of the items composed and those kept for reuse.
    fn children(&mut self, item: u64) -> usize {
        let node = node_of(item);
        self.ui.host().with_tree(|tree| {
            // The item's content is in its item's node.
            let item = tree.parent_of(node).expect("an item's node");
            let list = tree.parent_of(item).expect("an item in the list");
            tree.children_of(list).count()
        })
    }

    fn set_items(&mut self, items: Vec<(u64, Option<u32>)>) {
        self.items.set(Rc::new(items));
        self.ui.wait_for_idle();
    }
}

fn untyped(keys: impl IntoIterator<Item = u64>) -> Vec<(u64, Option<u32>)> {
    keys.into_iter().map(|key| (key, None)).collect()
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

#[test]
fn an_item_coming_in_reuses_the_nodes_of_one_that_left() {
    let mut list = Fixture::new(untyped(0..1000));
    list.scroll(100);
    let measured = NODES.with(|nodes| nodes.borrow().len());
    assert!(measured > 100, "{measured} items measured");
    // The items composed at once and those kept for reuse: far fewer nodes than items.
    let nodes = nodes_of(0..1000);
    assert!(nodes.len() < 30, "{} nodes for {measured} items", nodes.len());
    list.ui.on_node_with_tag("item 100: 1000").assert_exists();
    list.ui.on_node_with_tag("item 0: 0").assert_does_not_exist();
    assert_eq!(list.state.first_visible_item_index(), 100);
}

#[test]
fn a_reused_item_forgets_the_old_items_values_and_remembers_its_own() {
    let mut list = Fixture::new(untyped(0..1000));
    let composed = live();
    events();
    list.scroll(20);
    let events = events();
    assert!(!events.iter().any(|event| event.starts_with("abandoned")), "{events:?}");
    // Each item that came was remembered once, and each that went forgotten once (by its
    // deactivation, or by the item reusing its slot), never twice.
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
    let came: HashSet<u64> = now.iter().copied().filter(|item| !composed.contains(item)).collect();
    let went: HashSet<u64> = composed.iter().copied().filter(|item| !now.contains(item)).collect();
    // Some came and went (composed beyond the viewport for a few frames).
    let passing: HashSet<u64> = remembered.intersection(&forgotten).copied().collect();
    assert_eq!(remembered.difference(&passing).copied().collect::<HashSet<u64>>(), came);
    assert_eq!(forgotten.difference(&passing).copied().collect::<HashSet<u64>>(), went);
    assert!(passing.iter().all(|item| !composed.contains(item) && !now.contains(item)));
    // The live effects are those of the items composed: any kept for reuse disposed theirs.
    assert!(list.children(now[0]) >= now.len());
}

#[test]
fn an_item_scrolled_back_gets_its_saved_state_and_no_other_items() {
    let mut list = Fixture::new(untyped(0..1000));
    COUNTS.with(|counts| counts.borrow()[&2].set(7));
    list.ui.wait_for_idle();
    list.ui.on_node_with_tag("item 2: 7").assert_exists();
    list.scroll(40);
    // The item reusing item 2's slot (or any) starts at its own count.
    for item in 40..45 {
        list.ui.on_node_with_tag(&format!("item {item}: {}", item * 10)).assert_exists();
    }
    list.scroll(-40);
    list.ui.on_node_with_tag("item 2: 7").assert_exists();
    list.ui.on_node_with_tag("item 3: 30").assert_exists();
}

#[test]
fn up_to_seven_items_of_a_content_type_are_kept_for_reuse() {
    // A 300 px list: 15 items in view, more than 7 to leave.
    let mut list = Fixture::tall(untyped(0..30));
    let composed = live().len();
    assert!(composed > 10, "{composed} composed");
    // Two items left: the others leave at once, 7 of them kept for reuse, the rest disposed.
    list.set_items(untyped(0..2));
    assert_eq!(live(), [0, 1]);
    assert_eq!(list.children(0), 2 + 7);
    // Of two content types, up to 7 of each: items 0 to 8 of one, the others of the other.
    let content_type = |key: u64| Some(u32::from(key > 8));
    let mut list = Fixture::tall((0..40).map(|key| (key, content_type(key))).collect());
    let composed = live();
    assert!(composed.len() > 10, "{composed:?} composed");
    // Those laid out: an item composed beyond them (the first layout's estimate) and never
    // measured is disposed when it leaves (D460).
    let measured: HashSet<u64> = NODES.with(|nodes| nodes.borrow().keys().copied().collect());
    list.set_items(vec![(0, Some(0))]);
    let left = |kind: Option<u32>| {
        (composed.iter())
            .filter(move |key| **key != 0 && content_type(**key) == kind)
            .filter(|key| measured.contains(key))
    };
    assert!(left(Some(0)).count() > 7);
    assert_eq!(list.children(0), 1 + 7 + left(Some(1)).count().min(7));
}

#[test]
fn an_item_reuses_only_an_item_of_its_content_type() {
    // Items 0 to 49 of type 1, the others of type 2, and the forms without types: none.
    let items: Vec<(u64, Option<u32>)> =
        (0..200).map(|key| (key, Some(if key < 50 { 1 } else { 2 }))).collect();
    let mut list = Fixture::new(items);
    list.scroll(30);
    let first_type = nodes_of(0..50);
    list.scroll(40);
    // The items of type 2 reuse each other's nodes, never those of type 1, kept for reuse.
    let second_type = nodes_of(50..200);
    assert!(first_type.is_disjoint(&second_type), "{first_type:?} {second_type:?}");
    assert!(second_type.len() < 40, "{} nodes", second_type.len());
    // Untyped items (Compose's `null`) reuse untyped ones only.
    let mut list = Fixture::new(untyped(0..200));
    list.scroll(30);
    let untyped_nodes = nodes_of(0..200);
    list.set_items((0..200).map(|key| (key + 1000, Some(1))).collect());
    let typed_nodes = nodes_of(1000..1200);
    assert!(untyped_nodes.is_disjoint(&typed_nodes));
}

#[test]
fn prefetch_composes_the_next_items_in_the_slots_of_those_left_behind() {
    let mut list = Fixture::new(untyped(0..1000));
    let mut prefetched = 0;
    let mut before = HashSet::new();
    for step in 0..60 {
        if step == 20 {
            before = nodes_of(0..1000);
        }
        list.state.scroll_by(10.0);
        list.ui.wait_for_idle();
        let composed: HashSet<u64> = live().into_iter().collect();
        list.ui.host().prefetch(later());
        let ahead: Vec<u64> = live().into_iter().filter(|item| !composed.contains(item)).collect();
        prefetched += ahead.len();
        // Once the first ones were composed, measured ahead in nodes other items had: the
        // prefetch reuses the items it leaves behind, as a frame does.
        if step >= 20 {
            for item in &ahead {
                assert!(before.contains(&node_of(*item)), "step {step}: item {item} in a new node");
            }
        }
    }
    assert!(prefetched > 20, "{prefetched} items prefetched");
    assert!(nodes_of(0..1000).len() < 30);
}

#[test]
fn an_item_scrolled_back_to_reuses_its_own_slot() {
    // A content type for each run of as many items as are composed once scrolled (those in
    // view and the two kept behind): the item leaving the composed ones (at the top) and the
    // one coming (at the bottom) differ, so the one leaving is kept for reuse rather than
    // reused at once.
    let composed = {
        let mut probe = Fixture::new(untyped(0..1000));
        probe.scroll(20);
        live().len() as u64
    };
    let typed = (0..1000).map(|key| (key, Some(((key / composed) % 2) as u32))).collect();
    let mut list = Fixture::new(typed);
    list.scroll(20);
    let top = *live().first().expect("items composed");
    let node = node_of(top);
    list.scroll(1);
    assert!(!live().contains(&top), "item {top} left");
    // Back up: it comes back in its own slot, kept for reuse (Compose's same slot id first).
    list.scroll(-3);
    assert!(live().contains(&top));
    assert_eq!(node_of(top), node);
    list.ui.on_node_with_tag(&format!("item {top}: {}", top * 10)).assert_exists();
}

#[test]
fn reordered_items_keep_their_nodes_and_a_removed_ones_is_reused() {
    let mut list = Fixture::new(untyped(0..100));
    let first: Vec<NodeId> = (0..5).map(node_of).collect();
    // Reversed: each keeps its node.
    let mut reversed: Vec<u64> = (0..5).rev().collect();
    reversed.extend(5..100);
    list.set_items(untyped(reversed.clone()));
    for (item, node) in (0..5).zip(&first) {
        assert_eq!(node_of(item), *node, "item {item}");
    }
    // Item 2 removed: the composed range moves with the first visible item (item 0, one
    // place up, D458), so at most one item comes in, taking item 2's nodes; else item 2's slot
    // is kept for reuse, for the next item to come. No node is created or released.
    reversed.retain(|item| *item != 2);
    let composed: HashSet<u64> = live().into_iter().collect();
    let nodes = list.ui.host().with_tree(|tree| tree.node_count());
    list.set_items(untyped(reversed));
    let came: Vec<u64> = live().into_iter().filter(|item| !composed.contains(item)).collect();
    assert!(came.len() <= 1, "{came:?}");
    assert_eq!(list.ui.host().with_tree(|tree| tree.node_count()), nodes);
    list.ui.on_node_with_tag("item 2: 20").assert_does_not_exist();
    for (item, node) in [0, 1, 3, 4].into_iter().zip([first[0], first[1], first[3], first[4]]) {
        assert_eq!(node_of(item), node, "item {item}");
    }
}

#[test]
fn a_list_leaving_the_composition_disposes_its_items_and_those_kept_for_reuse_once() {
    let mut list = Fixture::new(untyped(0..1000));
    list.scroll(30);
    let composed = live();
    events();
    let nodes = list.ui.host().with_tree(|tree| tree.node_count());
    list.shown.set(false);
    list.ui.wait_for_idle();
    assert_eq!(live(), Vec::<u64>::new());
    // The items composed are forgotten now; those kept for reuse were when they left.
    let mut forgotten: Vec<u64> = events()
        .iter()
        .map(|event| event.strip_prefix("forgotten ").expect("forgotten").parse().unwrap())
        .collect();
    forgotten.sort_unstable();
    assert_eq!(forgotten, composed);
    let left = list.ui.host().with_tree(|tree| tree.node_count());
    assert!(left < nodes && left <= 3, "{left} nodes left of {nodes}");
}

thread_local! {
    static SIZES: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
}

/// An item telling its size, every one the same.
#[composable]
fn SizedItem(item: u64) {
    compose_ui::Layout(
        Modifier.on_size_changed(move |_| {
            SIZES.with(|sizes| sizes.borrow_mut().insert(item));
        }),
        remember(|| Rc::new(Recording { item }) as Rc<dyn MeasurePolicy>),
        callback(|| {}),
    );
}

#[test]
fn a_reused_items_modifier_nodes_are_reset_for_it() {
    reset();
    SIZES.with(|sizes| sizes.borrow_mut().clear());
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| scope.items(1000, |index| SizedItem(index as u64))
        );
    });
    ui.wait_for_idle();
    for _ in 0..40 {
        state.scroll_by(20.0);
        ui.wait_for_idle();
    }
    // Each item placed told its size, though it's the size the node it reused had told: the
    // reused node's `on_size_changed` was reset (`ModifierNode::on_reset`), as a new one's.
    let first = state.first_visible_item_index() as u64;
    for item in first..first + 5 {
        assert!(SIZES.with(|sizes| sizes.borrow().contains(&item)), "item {item}");
    }
    let nodes = nodes_of(0..1000);
    assert!(nodes.len() < 30, "{} nodes: reused", nodes.len());
}

/// Items inserted before others in a list whose items are all composed: Compose's measurement
/// finds each item's composition by its key, so only the new ones are composed. Here the
/// composed range, in indices, would end before the last items, which would leave (their state
/// forgotten) and come back: the items composed follow their keys (D453).
#[test]
fn items_inserted_before_others_leave_the_others_composition_alone() {
    let mut list = Fixture::new(untyped(0..4));
    COUNTS.with(|counts| counts.borrow()[&3].set(7));
    list.ui.wait_for_idle();
    let nodes: Vec<NodeId> = (0..4).map(node_of).collect();
    events();
    list.set_items(untyped([100].into_iter().chain(0..4)));
    assert_eq!(events(), ["remembered 100"]);
    assert_eq!((0..4).map(node_of).collect::<Vec<_>>(), nodes);
    list.ui.on_node_with_tag("item 3: 7").assert_exists();
    // In the middle too (item 3 now out of view, still composed, its count kept); and items
    // appended don't widen what's composed.
    list.set_items(untyped([100, 0, 1, 101, 102, 2, 3]));
    assert_eq!(events(), ["remembered 101", "remembered 102"]);
    assert!(live().contains(&3));
    assert_eq!(COUNTS.with(|counts| counts.borrow()[&3].get()), 7);
    let mut long = untyped([100, 0, 1, 101, 102, 2, 3]);
    long.extend(untyped(1000..2000));
    list.set_items(long);
    assert!(live().len() < 20, "{} composed", live().len());
}

/// A popup in deactivated content isn't placed, drawn, hit nor in the semantics; active again,
/// it shows.
#[test]
fn a_popup_in_deactivated_content_is_not_shown() {
    use compose_foundation::layout::*;
    use compose_ui::Popup;
    let active = mutable_state_of(true);
    let handle = active.handle();
    let mut ui = compose_test(move || {
        Column!(modifier = Modifier.size(100.dp()), {
            ReusableContentHost(handle.get(), move || {
                Popup!(on_dismiss_request = || {}, {
                    Box!(
                        modifier = Modifier
                            .size(20.dp())
                            .test_tag("in popup")
                            .background(compose_ui::Color::RED)
                    );
                });
            });
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("in popup").assert_exists();
    active.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("in popup").assert_does_not_exist();
    let drawn = ui.dump_display_list();
    assert!(!drawn.contains("#FFFF0000"), "drawn:\n{drawn}");
    active.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("in popup").assert_exists();
}

/// Measures `height` tall, recording its node for `item`.
struct Tall {
    item: u64,
    height: i32,
}

impl MeasurePolicy for Tall {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let node = scope.node_id();
        NODES.with(|nodes| nodes.borrow_mut().insert(self.item, node));
        ALL_NODES.with(|nodes| nodes.borrow_mut().entry(self.item).or_default().insert(node));
        scope.layout(constraints.max_width.min(100), self.height)
    }
}

/// An item `height` px tall, with an observer and an effect of `item`.
#[composable]
fn TallItem(item: u64, height: i32) {
    remember_observer(move || Observer(item));
    DisposableEffect((), move |scope| {
        LIVE.with(|live| live.borrow_mut().insert(item));
        scope.on_dispose(move || {
            LIVE.with(|live| live.borrow_mut().remove(&item));
        })
    });
    let policy = remember(move || Rc::new(Tall { item, height }) as Rc<dyn MeasurePolicy>);
    compose_ui::Layout(Modifier, policy, callback(|| {}));
}

/// A list's first layout estimates the items it lacks from those it has: here a 4 px header,
/// so 24 more of 40 px items to fill 100 px. Knowing few sizes, it asks for 4 at most (D472):
/// the header and items 1 to 3 fill the viewport, and item 4 is composed and never measured
/// (it once composed about 50, D460). An item that leaves without having been measured is
/// disposed rather than kept for reuse, as Compose, composing items as its measurement reaches
/// them, never composes it (D460); those laid out are kept.
#[test]
fn the_first_layout_composes_few_items_beyond_those_it_lays_out_and_keeps_none_for_reuse() {
    reset();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.item(|| TallItem(0, 4));
                // Two content types: the items a jump reaches reuse none of the first ones.
                scope.items_with(
                    1000,
                    |index| index >= 100,
                    |index| TallItem(index as u64 + 1, 40),
                );
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(live(), [0, 1, 2, 3, 4]);
    let measured: HashSet<u64> = NODES.with(|nodes| nodes.borrow().keys().copied().collect());
    assert_eq!(measured, HashSet::from([0, 1, 2, 3]), "item 4 composed, not measured");
    let children = |ui: &mut ComposeTestRule| {
        ui.host().with_tree(|tree| {
            let item = tree.parent_of(node_of(0)).expect("an item's node");
            tree.children_of(tree.parent_of(item).expect("the list")).count()
        })
    };
    // A jump: all five leave; the four laid out are kept for reuse, item 4 is disposed.
    state.scroll_to_item(500, 0);
    ui.wait_for_idle();
    let composed = live();
    assert!(composed.iter().all(|item| *item > 400), "{composed:?}");
    assert_eq!(children(&mut ui), composed.len() + 4);
}

// ---- Review of D450-D453 -----------------------------------------------------------------

/// Items removed before the composed ones, more than the composed range holds: Compose's
/// measurement finds the first visible item by its key and each item's composition by its key,
/// so the items in view keep their composition (their `remember`s). D453 follows a composed item
/// only if it lands within a range's length of the old range.
#[test]
fn review_items_removed_far_before_the_composed_ones_keep_their_composition() {
    let mut list = Fixture::new(untyped(0..300));
    list.scroll(100);
    assert_eq!(list.state.first_visible_item_index(), 100);
    let in_view: Vec<u64> = (100..105).collect();
    let nodes: Vec<NodeId> = in_view.iter().copied().map(node_of).collect();
    events();
    list.set_items(untyped(60..300));
    let events = events();
    assert_eq!(list.state.first_visible_item_index(), 40);
    let recomposed: Vec<&String> = events
        .iter()
        .filter(|event| in_view.iter().any(|item| **event == format!("remembered {item}")))
        .collect();
    assert!(recomposed.is_empty(), "items in view composed again: {recomposed:?}");
    assert_eq!(in_view.iter().copied().map(node_of).collect::<Vec<_>>(), nodes);
}

/// The same for items inserted before the composed ones (fewer than `KEY_WINDOW`).
#[test]
fn review_items_inserted_far_before_the_composed_ones_keep_their_composition() {
    let mut list = Fixture::new(untyped(0..300));
    list.scroll(100);
    events();
    list.set_items(untyped((1000..1080).chain(0..300)));
    let events = events();
    assert_eq!(list.state.first_visible_item_index(), 180);
    let recomposed: Vec<&String> = events
        .iter()
        .filter(|event| (100..110).any(|item| **event == format!("remembered {item}")))
        .collect();
    assert!(recomposed.is_empty(), "items in view composed again: {recomposed:?}");
}

// ---- Adversarial review of 36461e5 (D455-D457) -------------------------------------------

/// The item composed first (out of view, before the viewport) moved within the key window:
/// D455 moves the whole composed range by *its* shift, so the items in view, which didn't move,
/// fall out of the followed range and lose their composition.
#[test]
fn review2_first_composed_item_moved_forward_keeps_the_items_in_view() {
    crate::legacy::lazy_lists();
    let mut list = Fixture::new(untyped(0..300));
    list.scroll(100);
    assert_eq!(list.state.first_visible_item_index(), 100);
    let composed = live();
    let first = composed[0];
    assert!(first < 100, "{composed:?}");
    let in_view: Vec<u64> = (100..105).collect();
    let nodes: Vec<NodeId> = in_view.iter().copied().map(node_of).collect();
    events();
    // Item `first` moved 60 places down (within the key window).
    let mut keys: Vec<u64> = (0..300).filter(|key| *key != first).collect();
    keys.insert(first as usize + 60, first);
    list.set_items(untyped(keys));
    let events = events();
    assert_eq!(list.state.first_visible_item_index(), 99);
    let recomposed: Vec<&String> = events
        .iter()
        .filter(|event| in_view.iter().any(|item| **event == format!("remembered {item}")))
        .collect();
    assert!(recomposed.is_empty(), "items in view composed again: {recomposed:?}");
    assert_eq!(in_view.iter().copied().map(node_of).collect::<Vec<_>>(), nodes);
}

/// The same with the first composed item moved to the top (a "move to top" of a feed or chat
/// list): the range follows it back by ~95 places.
#[test]
fn review2_first_composed_item_moved_to_the_top_keeps_the_items_in_view() {
    crate::legacy::lazy_lists();
    let mut list = Fixture::new(untyped(0..300));
    list.scroll(100);
    let composed = live();
    let first = composed[0];
    let in_view: Vec<u64> = (100..105).collect();
    events();
    let mut keys: Vec<u64> = (0..300).filter(|key| *key != first).collect();
    keys.insert(0, first);
    list.set_items(untyped(keys));
    let events = events();
    assert_eq!(list.state.first_visible_item_index(), 100);
    let recomposed: Vec<&String> = events
        .iter()
        .filter(|event| in_view.iter().any(|item| **event == format!("remembered {item}")))
        .collect();
    assert!(recomposed.is_empty(), "items in view composed again: {recomposed:?}");
}

/// A list whose items all show (and are composed), reversed: more than `MOST_KEY_SEARCHES`
/// items each need a search. Those not found must not lose their composition when the range
/// would otherwise have held them.
#[test]
fn review2_reversed_list_keeps_every_items_composition() {
    let mut list = Fixture::new(untyped(0..12));
    let composed = live();
    events();
    list.set_items(untyped((0..12).rev()));
    let events = events();
    let recomposed: Vec<&String> = events
        .iter()
        .filter(|event| composed.iter().any(|item| **event == format!("remembered {item}")))
        .collect();
    assert!(recomposed.is_empty(), "composed {composed:?}; composed again: {recomposed:?}");
}

/// Scrolled to the end, items removed at the start (a log capped at its newest entries): the new
/// count is below the old composed indices, `LazyItems` clamps the range it follows to the new
/// count (empty), so `follow` finds nothing and every item in view loses its composition.
#[test]
fn review2_items_removed_before_while_at_the_end_keep_the_items_in_view() {
    let mut list = Fixture::new(untyped(0..300));
    list.scroll(400);
    assert!(!list.state.can_scroll_forward());
    let in_view: Vec<u64> = (295..300).collect();
    let nodes: Vec<NodeId> = in_view.iter().copied().map(node_of).collect();
    events();
    list.set_items(untyped(60..300));
    let events = events();
    let recomposed: Vec<&String> = events
        .iter()
        .filter(|event| in_view.iter().any(|item| **event == format!("remembered {item}")))
        .collect();
    assert!(recomposed.is_empty(), "items in view composed again: {recomposed:?}");
    assert_eq!(in_view.iter().copied().map(node_of).collect::<Vec<_>>(), nodes);
}
