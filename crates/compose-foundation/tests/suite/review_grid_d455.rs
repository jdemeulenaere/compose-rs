//! Adversarial review of 36461e5 (D455-D457): grids' key following, composed range, cost.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, UnitExt};

thread_local! {
    static TIMES: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    static LIVE: RefCell<std::collections::HashSet<usize>> =
        RefCell::new(std::collections::HashSet::new());
}

#[composable]
fn Tracked(key: usize) {
    remember(|| TIMES.with(|times| *times.borrow_mut().entry(key).or_default() += 1));
    DisposableEffect((), move |scope| {
        LIVE.with(|live| live.borrow_mut().insert(key));
        scope.on_dispose(move || {
            LIVE.with(|live| live.borrow_mut().remove(&key));
        })
    });
    Box!(modifier = Modifier.height(20.dp()).test_tag(format!("{key}")));
}

fn times(key: usize) -> usize {
    TIMES.with(|times| times.borrow().get(&key).copied().unwrap_or(0))
}

fn live() -> Vec<usize> {
    let mut live: Vec<usize> = LIVE.with(|live| live.borrow().iter().copied().collect());
    live.sort_unstable();
    live
}

struct GridFixture {
    ui: ComposeTestRule,
    state: Owned<LazyGridState>,
    keys: OwnedState<Rc<Vec<usize>>>,
}

fn grid(keys: Vec<usize>) -> GridFixture {
    TIMES.with(|times| times.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    let keys = mutable_state_of(Rc::new(keys));
    let handle = keys.handle();
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let keys = handle.get();
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = given,
            move |scope: LazyGridScope| {
                let (k, c) = (keys.clone(), keys.clone());
                scope.items_keyed(
                    keys.len(),
                    move |index| k[index],
                    move |index| Tracked(c[index]),
                );
            }
        );
    });
    ui.wait_for_idle();
    GridFixture { ui, state, keys }
}

/// The first composed item (two lines above the viewport) moved 60 places down: the range
/// follows *its* shift and the items in view, which didn't move, are composed again.
#[test]
fn review3_grid_first_composed_item_moved_keeps_the_items_in_view() {
    crate::legacy::lazy_lists();
    let mut g = grid((0..3000).collect());
    g.state.request_scroll_to_item(900, 0);
    g.ui.wait_for_idle();
    let composed = live();
    let first = composed[0];
    assert!(first < 900, "{composed:?}");
    let mut keys: Vec<usize> = (0..3000).filter(|key| *key != first).collect();
    keys.insert(first + 60, first);
    g.keys.set(Rc::new(keys));
    g.ui.wait_for_idle();
    let again: Vec<usize> = (900..930).filter(|key| times(*key) > 1).collect();
    assert!(again.is_empty(), "items in view composed again: {again:?}");
}

/// Cost of one data change on a 100,000-item grid scrolled to its middle, items shuffled in
/// view (every composed item needs a search): bounded, reported.
#[test]
fn review3_grid_data_change_cost_on_100k_items() {
    let mut g = grid((0..100_000).collect());
    g.state.request_scroll_to_item(50_000, 0);
    g.ui.wait_for_idle();
    let mut keys: Vec<usize> = (0..100_000).collect();
    keys[49_950..50_100].reverse();
    let start = std::time::Instant::now();
    g.keys.set(Rc::new(keys));
    g.ui.wait_for_idle();
    let shuffled = start.elapsed();
    let mut keys: Vec<usize> = (0..100_000).collect();
    keys.insert(0, 1_000_000);
    let start = std::time::Instant::now();
    g.keys.set(Rc::new(keys));
    g.ui.wait_for_idle();
    println!(
        "100k grid: reversed around the view {shuffled:?}, one inserted {:?}",
        start.elapsed()
    );
    for count in [1_000usize, 100_000] {
        let mut g = grid((0..count).collect());
        g.state.request_scroll_to_item(count / 2, 0);
        g.ui.wait_for_idle();
        let mut best = std::time::Duration::MAX;
        for round in 0..20 {
            let keys: Vec<usize> =
                (0..count).map(|k| if k == 0 { 10_000_000 + round } else { k }).collect();
            let start = std::time::Instant::now();
            g.keys.set(Rc::new(keys));
            g.ui.wait_for_idle();
            best = best.min(start.elapsed());
        }
        println!("{count} items: a change far from view, best {best:?}");
    }
}
