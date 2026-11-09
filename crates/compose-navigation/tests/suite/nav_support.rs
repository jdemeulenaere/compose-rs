//! What the tests of `NavDisplay` share: a two-pane strategy (the test's `TestTwoPaneScene`), and
//! entries showing their key.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use compose_animation::{
    AnimatedContent, AnimatedContentScope, AnimatedContentTransitionScope, ExitTransition,
    SlideDirection, tween,
};
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

pub const LIST_KEY: &str = "ListDetailScene-List";
pub const DETAIL_KEY: &str = "ListDetailScene-Detail";

/// `ListDetailScene` from `ListDetailSceneTest.kt`: a 40/60 split with `AnimatedContent` on the
/// detail pane, keyed by the list entry's content key.
pub struct ListDetailScene<T: 'static> {
    key: ContentKey,
    entries: [NavEntry<T>; 2],
    previous_entries: Vec<NavEntry<T>>,
}

impl ListDetailScene<()> {
    pub fn list_pane() -> NavMetadata {
        NavMetadata::new().with(LIST_KEY, true)
    }

    pub fn detail_pane() -> NavMetadata {
        NavMetadata::new().with(DETAIL_KEY, true)
    }
}

impl<T: Clone + PartialEq + 'static> Scene<T> for ListDetailScene<T> {
    fn key(&self) -> ContentKey {
        self.key.clone()
    }

    fn entries(&self) -> &[NavEntry<T>] {
        &self.entries
    }

    fn previous_entries(&self) -> &[NavEntry<T>] {
        &self.previous_entries
    }

    fn content(&self) {
        ListDetailPanes(self.entries[0].clone(), self.entries[1].clone());
    }
}

#[composable]
#[allow(non_snake_case)]
fn ListDetailPanes<T: Clone + PartialEq + 'static>(
    list_entry: NavEntry<T>,
    detail_entry: NavEntry<T>,
) {
    let content_key = remember(|| {
        compose_animation::ContentKey::of(|entry: &NavEntry<T>| entry.content_key().clone())
    });
    Row!(modifier = Modifier.fill_max_size(), move |scope: RowScope| {
        let (list_entry, detail_entry, content_key) =
            (list_entry.clone(), detail_entry.clone(), content_key.clone());
        Column!(modifier = Modifier.weight(scope, 0.4), { list_entry.Content() });
        Column!(modifier = Modifier.weight(scope, 0.6), {
            AnimatedContent!(
                detail_entry.clone(),
                transition_spec = |scope: AnimatedContentTransitionScope<NavEntry<T>>| {
                    scope
                        .slide_into_container_with(SlideDirection::Up, tween(1000), |offset| offset)
                        .together_with(ExitTransition::KEEP_UNTIL_TRANSITIONS_FINISHED)
                },
                content_key = content_key.clone(),
                content = |_: AnimatedContentScope, entry: NavEntry<T>| {
                    entry.Content();
                }
            );
        });
    });
}

/// `ListDetailSceneStrategy` from `ListDetailSceneTest.kt`.
pub fn list_detail_strategy<T: Clone + PartialEq + 'static>(
    is_width_wide_enough: bool,
) -> SceneStrategyRef<T> {
    scene_strategy(move |_: &SceneStrategyScope<T>, entries: &[NavEntry<T>]| {
        if !is_width_wide_enough {
            return None;
        }
        let detail_entry = entries.last().filter(|e| e.metadata().contains_named(DETAIL_KEY))?;
        let list_entry = entries.iter().rfind(|e| e.metadata().contains_named(LIST_KEY))?;
        let scene: SceneRef<T> = Rc::new(ListDetailScene {
            key: list_entry.content_key().clone(),
            entries: [list_entry.clone(), detail_entry.clone()],
            previous_entries: entries[..entries.len() - 1].to_vec(),
        });
        Some(scene)
    })
}

#[composable(non_restartable)]
pub fn remember_list_detail_scene_strategy<T: Clone + PartialEq + 'static>() -> SceneStrategyRef<T>
{
    remember(|| list_detail_strategy(true))
}
