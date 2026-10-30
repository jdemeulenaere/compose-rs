//! What the tests of `NavDisplay` share: a two-pane strategy (the test's `TestTwoPaneScene`), and
//! entries showing their key.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_navigation::*;
use compose_runtime::*;
use compose_ui::Modifier;

pub const FIRST: &str = "first";
pub const SECOND: &str = "second";
pub const THIRD: &str = "third";
pub const FOURTH: &str = "fourth";

/// What's on a test's screens, as `Text(key)`.
#[composable]
pub fn Shown(text: &'static str) {
    BasicText!(text);
}

/// An entry showing its key.
pub fn text_entry(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |key| Shown!(key))
}

thread_local! {
    static COUNTERS: RefCell<HashMap<&'static str, MutableState<i32>>> = RefCell::default();
}

/// A number saved at this place, shown as `name: number`, as the tests' `numberOnScreen`.
#[composable]
pub fn Counter(name: &'static str) {
    let count = remember_saveable(|| mutable_state_of(0));
    COUNTERS.with_borrow_mut(|counters| counters.insert(name, count));
    BasicText!("{name}: {}", count.get());
}

/// The number of the last `Counter` named `name` composed.
pub fn counter(name: &'static str) -> MutableState<i32> {
    COUNTERS.with_borrow(|counters| *counters.get(name).expect("a counter of that name"))
}

/// An entry showing a counter of its key.
pub fn counter_entry(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |key| Counter!(key))
}

pub struct TwoPaneScene {
    entries: [NavEntry<&'static str>; 2],
    previous_entries: Vec<NavEntry<&'static str>>,
}

impl Scene<&'static str> for TwoPaneScene {
    fn key(&self) -> ContentKey {
        ContentKey::new((
            format!("{:?}", self.entries[0].content_key()),
            format!("{:?}", self.entries[1].content_key()),
        ))
    }

    fn entries(&self) -> &[NavEntry<&'static str>] {
        &self.entries
    }

    fn previous_entries(&self) -> &[NavEntry<&'static str>] {
        &self.previous_entries
    }

    fn content(&self) {
        TwoPanes(self.entries[0].clone(), self.entries[1].clone());
    }
}

#[composable]
fn TwoPanes(left: NavEntry<&'static str>, right: NavEntry<&'static str>) {
    Row!(|scope: RowScope| {
        Box!(modifier = Modifier.weight(scope, 1.0), { left.Content() });
        Box!(modifier = Modifier.weight(scope, 1.0), { right.Content() });
    });
}

/// Two panes of the last two entries, when there are two.
pub fn two_pane_strategy() -> SceneStrategyRef<&'static str> {
    scene_strategy(|_: &SceneStrategyScope<&'static str>, entries: &[NavEntry<&'static str>]| {
        let (left, right) = match entries {
            [.., left, right] => (left, right),
            _ => return None,
        };
        let scene: SceneRef<&'static str> = Rc::new(TwoPaneScene {
            entries: [left.clone(), right.clone()],
            previous_entries: entries[..entries.len() - 1].to_vec(),
        });
        Some(scene)
    })
}
