//! The model-checked randomized test of docs/09-testing.md.
//!
//! A tree of items, each rendered by a hand-expanded restartable composable `Item(item)`, is
//! mutated at random: insertions, removals, keyed and unkeyed moves at every position, value
//! writes (some equal, which invalidate nothing), branch toggles, and replacing an item by a
//! copy (so a kept argument changes). Some items hold their content in reusable groups
//! (`ReusableContentHost(active) { ReusableContent(reuse_key) { … } }`, D430): deactivating one
//! freezes its content (its nodes stay, nothing in it runs, its values stay remembered), and
//! changing its reuse key or activating it again reuses its content (every body in it runs,
//! every value is remembered anew). After each pass, the runtime is checked against a
//! reference model that implements the same rules independently, with plain vectors:
//!
//! - the node tree, including which remembered token each node shows (state identity);
//! - the exact sequence of composable bodies that ran (tree order, none twice, none removed);
//! - the set of live remembered values (nothing leaked, nothing dropped early);
//! - the slot table's invariants.
//!
//! On disposal every remembered value must be dropped exactly once, and every cell and scope
//! freed.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashSet};
use std::fmt::{self, Write as _};
use std::rc::Rc;

use compose_runtime::platform::*;
use compose_runtime::*;

// ---- The data and the composables ----------------------------------------------------------

struct ItemData {
    id: u64,
    kind: usize,
    keyed: bool,
    value: OwnedState<u32>,
    branch: OwnedState<bool>,
    children: OwnedState<Vec<Rc<ItemData>>>,
    /// A reusable item's (kind 3): whether its content is active, and its reuse key.
    active: OwnedState<bool>,
    reuse_key: OwnedState<u32>,
}

impl ItemData {
    fn new(id: u64, kind: usize, keyed: bool, value: u32, branch: bool) -> Rc<ItemData> {
        Rc::new(ItemData {
            id,
            kind,
            keyed,
            value: mutable_state_of(value),
            branch: mutable_state_of(branch),
            children: mutable_state_of_with(Vec::new(), never_equal_policy()),
            active: mutable_state_of(true),
            reuse_key: mutable_state_of(0),
        })
    }

    /// A copy with fresh state cells holding the same values: a new identity for skipping.
    fn copy(&self) -> Rc<ItemData> {
        let copy =
            ItemData::new(self.id, self.kind, self.keyed, self.value.get(), self.branch.get());
        copy.children.set(self.children.get());
        copy.active.set(self.active.get());
        copy.reuse_key.set(self.reuse_key.get());
        copy
    }
}

#[derive(Clone, PartialEq)]
struct Label(String);

impl fmt::Debug for Label {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type Tree = NodeTree<Label>;

thread_local! {
    static RUNS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    static NEXT_TOKEN: Cell<u64> = const { Cell::new(0) };
    static LIVE_TOKENS: RefCell<BTreeSet<u64>> = const { RefCell::new(BTreeSet::new()) };
}

/// A remembered value that checks it is dropped exactly once.
struct Token(u64);

impl Token {
    fn new() -> Token {
        let serial = NEXT_TOKEN.with(|next| {
            let serial = next.get();
            next.set(serial + 1);
            serial
        });
        LIVE_TOKENS.with(|live| assert!(live.borrow_mut().insert(serial)));
        Token(serial)
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        let removed = LIVE_TOKENS.with(|live| live.borrow_mut().remove(&self.0));
        assert!(removed, "token {} dropped twice", self.0);
    }
}

/// Kinds 0 and 1 emit a node holding their branch and children (kind 0's reusable, kind 1's
/// not); kind 2 emits no node of its own, so its branch leaf and children are nodes of its
/// parent's node (a group of several nodes, or of none); kind 3 emits a reusable node whose
/// branch and children are reusable content.
const KIND_KEYS: [u64; 4] = [
    key_of("model::ItemA"),
    key_of("model::ItemB"),
    key_of("model::Ghost"),
    key_of("model::Reusable"),
];
const BRANCH_KEY: u64 = key_of("model::Item::branch");

/// A node: reusable (as `Layout`'s), which reused content keeps, or not, which it replaces.
fn Node(key: u64, reusable: bool, label: String, content: impl FnOnce()) {
    let composer = current();
    let node = if reusable {
        composer.start_reusable_node(key, || Label(label.clone()))
    } else {
        composer.start_node(key, || Label(label.clone()))
    };
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label;
        }
    });
    content();
    composer.end_group();
}

/// `#[composable] fn Item(item: Rc<ItemData>)`, expanded by hand.
fn Item(item: Rc<ItemData>) {
    let composer = current();
    composer.start_restart_group(KIND_KEYS[item.kind]);
    let changed =
        composer.update_kept(|kept: &(Rc<ItemData>,)| !kept.0.same(&item), || (item.clone(),));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        RUNS.with(|runs| runs.borrow_mut().push(item.id));
        let token = remember(|| Rc::new(Token::new()));
        let label = format!("{}:{}:t{}", item.id, item.value.get(), token.0);
        let inner = || {
            if item.branch.get() {
                composer.start_replace_group(BRANCH_KEY);
                let token = remember(|| Rc::new(Token::new()));
                Node(key_of("model::Leaf"), false, format!("b{}", token.0), || {});
                composer.end_group();
            }
            item.children.with(|children| {
                for child in children {
                    if child.keyed {
                        key(child.id, || Item(child.clone()));
                    } else {
                        Item(child.clone());
                    }
                }
            });
        };
        let content = || {
            if item.kind == 3 {
                ReusableContentHost(item.active.get(), || {
                    ReusableContent(item.reuse_key.get(), inner);
                });
            } else {
                inner();
            }
        };
        if item.kind == 2 {
            content();
        } else {
            // Kind 1's node isn't reusable: reused content replaces it, and all it holds.
            Node(key_of("model::ItemNode"), item.kind != 1, label, content);
        }
    }
    composer.end_restart_group(|kept| {
        let (item,) = kept.downcast_ref::<(Rc<ItemData>,)>().expect("kept arguments").clone();
        Item(item)
    });
}

// ---- The reference model -------------------------------------------------------------------

/// What a group matches on among its siblings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MatchKey {
    Keyed(u64),
    Unkeyed(usize),
}

fn match_key(item: &ItemData) -> MatchKey {
    if item.keyed { MatchKey::Keyed(item.id) } else { MatchKey::Unkeyed(item.kind) }
}

struct ReferenceGroup {
    uid: u64,
    key: MatchKey,
    item: Rc<ItemData>,
    /// The value its last run showed (a frozen group's may be older than the item's).
    shown_value: u32,
    token: u64,
    branch_token: Option<u64>,
    children: Vec<ReferenceGroup>,
    /// A reusable item's: whether its content was active at its last run, and its reuse key.
    active: bool,
    reuse_key: u32,
}

#[derive(Default)]
struct Reference {
    next_uid: u64,
    next_token: u64,
    invalid: HashSet<u64>,
    runs: Vec<u64>,
}

impl Reference {
    fn token(&mut self) -> u64 {
        self.next_token += 1;
        self.next_token - 1
    }

    fn create(&mut self, item: &Rc<ItemData>) -> ReferenceGroup {
        self.next_uid += 1;
        let mut group = ReferenceGroup {
            uid: self.next_uid,
            key: match_key(item),
            item: item.clone(),
            shown_value: 0,
            token: 0,
            branch_token: None,
            children: Vec::new(),
            active: true,
            reuse_key: 0,
        };
        self.runs.push(item.id);
        group.token = self.token();
        self.run_body(&mut group, false);
        group
    }

    /// The body of `group`'s item, in content being reused (`reuse`: everything runs, every
    /// value is remembered anew) or not.
    fn run_body(&mut self, group: &mut ReferenceGroup, reuse: bool) {
        let item = group.item.clone();
        group.shown_value = item.value.get();
        let mut reuse = reuse;
        if item.kind == 3 {
            if !item.active.get() {
                // Deactivated: its content frozen as it was.
                group.active = false;
                return;
            }
            let reuse_key = item.reuse_key.get();
            // Active again, or another reuse key: its content is reused.
            reuse |= !group.active || group.reuse_key != reuse_key;
            group.active = true;
            group.reuse_key = reuse_key;
        }
        group.branch_token = match group.branch_token {
            _ if !item.branch.get() => None,
            Some(token) if !reuse => Some(token),
            _ => Some(self.token()),
        };
        let mut old = std::mem::take(&mut group.children);
        if reuse && item.kind == 1 {
            // Its node replaced, with everything in it.
            old.clear();
        }
        group.children = self.match_children(old, &item.children.get(), reuse);
    }

    /// The composer's matching rule: each item takes the first unused old group with its key.
    fn match_children(
        &mut self,
        old: Vec<ReferenceGroup>,
        items: &[Rc<ItemData>],
        reuse: bool,
    ) -> Vec<ReferenceGroup> {
        let mut old: Vec<Option<ReferenceGroup>> = old.into_iter().map(Some).collect();
        let mut new = Vec::new();
        for item in items {
            let key = match_key(item);
            let found = old.iter().position(|group| group.as_ref().is_some_and(|g| g.key == key));
            match found {
                Some(index) => {
                    let group = old[index].take().expect("unused");
                    new.push(self.update(group, item, reuse));
                }
                None => new.push(self.create(item)),
            }
        }
        new
    }

    /// A group the composition reaches with `item`: its body runs if the argument changed, its
    /// scope is invalid or its content is being reused; otherwise its invalid descendants run.
    fn update(
        &mut self,
        mut group: ReferenceGroup,
        item: &Rc<ItemData>,
        reuse: bool,
    ) -> ReferenceGroup {
        let invalid = self.invalid.remove(&group.uid);
        if reuse || invalid || !Rc::ptr_eq(&group.item, item) {
            self.runs.push(item.id);
            group.item = item.clone();
            if reuse {
                group.token = self.token();
            }
            self.run_body(&mut group, reuse);
        } else if group.item.kind != 3 || group.active {
            let children = std::mem::take(&mut group.children);
            group.children = children
                .into_iter()
                .map(|child| {
                    let item = child.item.clone();
                    self.update(child, &item, false)
                })
                .collect();
        }
        group
    }

    /// Writing an item's state invalidates the group that rendered it last, unless it's in
    /// deactivated content (whose scopes were released).
    fn invalidate(&mut self, root: &ReferenceGroup, item: &Rc<ItemData>) {
        if let Some(uid) = find(root, item).map(|group| group.uid) {
            self.invalid.insert(uid);
        }
    }
}

impl Reference {
    /// Writing an item's branch or children: they were read by its last run unless its content
    /// was deactivated then.
    fn invalidate_content(&mut self, root: &ReferenceGroup, item: &Rc<ItemData>) {
        if find(root, item).is_some_and(|group| group.item.kind != 3 || group.active) {
            self.invalidate(root, item);
        }
    }
}

/// The group rendering `item`, outside deactivated content.
fn find<'a>(group: &'a ReferenceGroup, item: &Rc<ItemData>) -> Option<&'a ReferenceGroup> {
    if Rc::ptr_eq(&group.item, item) {
        return Some(group);
    }
    if group.item.kind == 3 && !group.active {
        return None;
    }
    group.children.iter().find_map(|child| find(child, item))
}

fn expected_tree(group: &ReferenceGroup, depth: usize, out: &mut String) {
    // A ghost (kind 2) has no node: its contents are at its own depth.
    let inner = if group.item.kind == 2 {
        depth
    } else {
        let indent = "  ".repeat(depth);
        let _ = writeln!(out, "{indent}{}:{}:t{}", group.item.id, group.shown_value, group.token);
        depth + 1
    };
    if let Some(token) = group.branch_token {
        let _ = writeln!(out, "{}b{token}", "  ".repeat(inner));
    }
    for child in &group.children {
        expected_tree(child, inner, out);
    }
}

fn expected_tokens(group: &ReferenceGroup, out: &mut BTreeSet<u64>) {
    out.insert(group.token);
    out.extend(group.branch_token);
    for child in &group.children {
        expected_tokens(child, out);
    }
}

/// The groups that can run: outside deactivated content (an invalidation inside it is dropped
/// when it's deactivated, as its scopes are released).
fn collect_uids(group: &ReferenceGroup, out: &mut HashSet<u64>) {
    out.insert(group.uid);
    if group.item.kind == 3 && !group.active {
        return;
    }
    for child in &group.children {
        collect_uids(child, out);
    }
}

// ---- Random mutations ----------------------------------------------------------------------

struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

fn all_items(item: &Rc<ItemData>, out: &mut Vec<Rc<ItemData>>) {
    out.push(item.clone());
    for child in item.children.get() {
        all_items(&child, out);
    }
}

/// One random mutation of the data, mirrored in the reference's invalidations.
fn mutate(
    random: &mut Random,
    root: &Rc<ItemData>,
    reference: &mut Reference,
    rendered: &ReferenceGroup,
    next_id: &mut u64,
) {
    let mut items = Vec::new();
    all_items(root, &mut items);
    let target = items[random.below(items.len())].clone();
    let len = target.children.with(Vec::len);
    match random.below(100) {
        // Insert a new child at any position.
        0..=34 => {
            *next_id += 1;
            let child = ItemData::new(
                *next_id,
                random.below(4),
                random.chance(50),
                random.below(3) as u32,
                random.chance(20),
            );
            let position = random.below(len + 1);
            target.children.edit(|children| children.insert(position, child));
            reference.invalidate_content(rendered, &target);
        }
        // Remove a child at any position.
        35..=49 if len > 0 => {
            let position = random.below(len);
            target.children.edit(|children| children.remove(position));
            reference.invalidate_content(rendered, &target);
        }
        // Move a child from any position to any position.
        50..=64 if len > 1 => {
            let from = random.below(len);
            let to = random.below(len);
            target.children.edit(|children| {
                let child = children.remove(from);
                children.insert(to, child);
            });
            reference.invalidate_content(rendered, &target);
        }
        // Write a value; an equal one invalidates nothing.
        65..=79 => {
            let value = random.below(3) as u32;
            if target.value.get() != value {
                reference.invalidate(rendered, &target);
            }
            target.value.set(value);
        }
        // Toggle the branch.
        80..=84 => {
            target.branch.set(!target.branch.get());
            reference.invalidate_content(rendered, &target);
        }
        // Deactivate or activate a reusable item's content.
        85..=87 if target.kind == 3 => {
            target.active.set(!target.active.get());
            reference.invalidate(rendered, &target);
        }
        // Give a reusable item's content another reuse key (or the same): read only while
        // active.
        88..=89 if target.kind == 3 => {
            let reuse_key = random.below(3) as u32;
            if target.reuse_key.get() != reuse_key
                && find(rendered, &target).is_some_and(|group| group.active)
            {
                reference.invalidate(rendered, &target);
            }
            target.reuse_key.set(reuse_key);
        }
        // Replace a child by a copy: same keys and values, new identity.
        _ if len > 0 => {
            let position = random.below(len);
            let copy = target.children.with(|children| children[position].copy());
            target.children.edit(|children| children[position] = copy);
            reference.invalidate_content(rendered, &target);
        }
        _ => {}
    }
}

fn run_seed(seed: u64, steps: usize) {
    RUNS.with(|runs| runs.borrow_mut().clear());
    NEXT_TOKEN.with(|next| next.set(0));
    let states_before = live_state_count();
    let scopes_before = live_scope_count();

    let mut random = Random(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let root = ItemData::new(0, 0, false, 0, false);
    let composition = Composition::new(Tree::new());
    let content_root = root.clone();
    composition.set_content(move || Item(content_root.clone()));

    let mut reference = Reference::default();
    let mut rendered = reference.create(&root);
    let mut next_id = 0;

    for step in 0..steps {
        let context = || format!("seed {seed}, step {step}");
        let expected_runs = std::mem::take(&mut reference.runs);
        let actual_runs = RUNS.with(|runs| std::mem::take(&mut *runs.borrow_mut()));
        assert_eq!(actual_runs, expected_runs, "bodies that ran, {}", context());

        let mut expected = String::new();
        expected_tree(&rendered, 0, &mut expected);
        let actual = composition.with_applier(|tree: &mut Tree| tree.dump());
        assert_eq!(actual, expected, "node tree, {}", context());

        let mut tokens = BTreeSet::new();
        expected_tokens(&rendered, &mut tokens);
        let live = LIVE_TOKENS.with(|live| live.borrow().clone());
        assert_eq!(live, tokens, "live remembered values, {}", context());
        composition.validate();

        for _ in 0..random.below(4) {
            mutate(&mut random, &root, &mut reference, &rendered, &mut next_id);
        }
        let expected_pass = !reference.invalid.is_empty();
        let ran = composition.recompose();
        if !ran {
            assert!(!expected_pass, "a pass was expected, {}", context());
        }
        let root_item = rendered.item.clone();
        rendered = reference.update(rendered, &root_item, false);
        // What is still invalid was removed before it could run.
        let mut uids = HashSet::new();
        collect_uids(&rendered, &mut uids);
        assert!(reference.invalid.iter().all(|uid| !uids.contains(uid)));
        reference.invalid.clear();
    }

    drop(composition);
    assert!(LIVE_TOKENS.with(|live| live.borrow().is_empty()), "seed {seed}: tokens leaked");
    drop(rendered);
    drop(root);
    assert_eq!(live_state_count(), states_before, "seed {seed}: state cells leaked");
    assert_eq!(live_scope_count(), scopes_before, "seed {seed}: scopes leaked");
}

#[test]
fn randomized_model() {
    let seeds: u64 =
        std::env::var("COMPOSE_MODEL_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let steps = if cfg!(miri) { 20 } else { 60 };
    let seeds = if cfg!(miri) { seeds.min(4) } else { seeds };
    for seed in 0..seeds {
        run_seed(seed, steps);
    }
}
