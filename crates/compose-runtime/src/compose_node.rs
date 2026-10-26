//! `ComposeNode` and `ReusableComposeNode`, as Compose's: emit a node of a custom applier's
//! tree, made by a factory when new and updated through an [`Updater`], without writing the
//! composer's node protocol by hand.

use std::marker::PhantomData;

use crate::applier::{Applier, NodeRef};
use crate::compose_arg::ComposeArg;
use crate::composer::current;
use crate::hash::key_of;

/// The key of every node group `ComposeNode` starts, as Compose's node groups share one key:
/// the composable around it gives it its identity.
const NODE_KEY: u64 = key_of("compose_runtime::ComposeNode");

/// The key of `SkippableUpdater::update`'s group.
const SKIPPABLE_UPDATE_KEY: u64 = key_of("compose_runtime::SkippableUpdater::update");

/// The key of the group around a `ComposeNode`'s content, when it has a skippable update.
const CONTENT_KEY: u64 = key_of("compose_runtime::ComposeNode::content");

/// Updates the node a [`ComposeNode`] emits, as Compose's `Updater<T>`: each call remembers its
/// value in the node's group and runs its block on the node (`A::Node`, through
/// [`Applier::node_mut`]) only when it should.
pub struct Updater<A: Applier> {
    node: NodeRef,
    inserting: bool,
    _applier: PhantomData<fn() -> A>,
}

impl<A: Applier> Updater<A> {
    /// The node being updated.
    pub fn node(&self) -> NodeRef {
        self.node
    }

    /// Runs `block` when the node is new or `value` isn't [`same`](ComposeArg::same) as the last
    /// one, as `Updater.set`.
    pub fn set<V: ComposeArg + Clone>(&mut self, value: V, block: impl FnOnce(&mut A::Node, &V)) {
        if crate::remember::update_remembered(&value) || self.inserting {
            self.apply(|node| block(node, &value));
        }
    }

    /// Runs `block` when `value` isn't `same` as the last one, but not on a new node (whose
    /// factory set it up), as `Updater.update`.
    pub fn update<V: ComposeArg + Clone>(
        &mut self,
        value: V,
        block: impl FnOnce(&mut A::Node, &V),
    ) {
        if crate::remember::update_remembered(&value) && !self.inserting {
            self.apply(|node| block(node, &value));
        }
    }

    /// Runs `block` on a new node only, as `Updater.init`.
    pub fn init(&mut self, block: impl FnOnce(&mut A::Node)) {
        if self.inserting {
            self.apply(block);
        }
    }

    /// Runs `block` with `value` on a new node only, as `Updater.init(value, block)`.
    pub fn init_with<V>(&mut self, value: V, block: impl FnOnce(&mut A::Node, V)) {
        if self.inserting {
            self.apply(|node| block(node, value));
        }
    }

    /// Runs `block` at every update, as `Updater.reconcile`.
    pub fn reconcile(&mut self, block: impl FnOnce(&mut A::Node)) {
        self.apply(block);
    }

    fn apply(&mut self, block: impl FnOnce(&mut A::Node)) {
        let node = self.node;
        current().with_applier(|applier: &mut A| block(applier.node_mut(node)));
    }
}

/// Updates the node a [`ComposeNodeWithSkippableUpdate`] emits in a group of its own, as
/// Compose's `SkippableUpdater<T>`.
pub struct SkippableUpdater<A: Applier> {
    updater: Updater<A>,
}

impl<A: Applier> SkippableUpdater<A> {
    /// Runs `block` with the node's [`Updater`], in a group of its own, as
    /// `SkippableUpdater.update`.
    pub fn update(&mut self, block: impl FnOnce(&mut Updater<A>)) {
        let composer = current();
        composer.start_replace_group(SKIPPABLE_UPDATE_KEY);
        block(&mut self.updater);
        composer.end_group();
    }
}

/// Starts a node group of `A`'s nodes, made by `factory` when new: the node's updater.
fn start<A: Applier>(reusable: bool, factory: impl FnOnce() -> A::Node) -> Updater<A> {
    let composer = current();
    if !composer.applier_is::<A>() {
        panic!(
            "compose-runtime: invalid applier: ComposeNode::<{}> in a composition of another \
             applier",
            std::any::type_name::<A>()
        );
    }
    let node = if reusable {
        composer.start_reusable_node(NODE_KEY, factory)
    } else {
        composer.start_node(NODE_KEY, factory)
    };
    Updater { node, inserting: composer.inserting(), _applier: PhantomData }
}

/// Emits a node of `A`'s tree, made by `factory` when new and updated by `update`, as Compose's
/// `ComposeNode<T, E>(factory, update)`. Content being reused (`ReusableContent`) replaces it.
#[allow(non_snake_case)]
pub fn ComposeNode<A: Applier>(
    factory: impl FnOnce() -> A::Node,
    update: impl FnOnce(&mut Updater<A>),
) {
    let mut updater = start::<A>(false, factory);
    update(&mut updater);
    current().end_group();
}

/// Like [`ComposeNode`], with `content` composed inside the node, as
/// `ComposeNode(factory, update, content)`.
#[allow(non_snake_case)]
pub fn ComposeNodeWithContent<A: Applier>(
    factory: impl FnOnce() -> A::Node,
    update: impl FnOnce(&mut Updater<A>),
    content: impl FnOnce(),
) {
    let mut updater = start::<A>(false, factory);
    update(&mut updater);
    content();
    current().end_group();
}

/// Like [`ComposeNodeWithContent`], with a skippable update after `update`, as
/// `ComposeNode(factory, update, skippableUpdate, content)`.
#[allow(non_snake_case)]
pub fn ComposeNodeWithSkippableUpdate<A: Applier>(
    factory: impl FnOnce() -> A::Node,
    update: impl FnOnce(&mut Updater<A>),
    skippable_update: impl FnOnce(&mut SkippableUpdater<A>),
    content: impl FnOnce(),
) {
    with_skippable_update(false, factory, update, skippable_update, content);
}

/// Emits a node of `A`'s tree that content being reused keeps, as Compose's
/// `ReusableComposeNode<T, E>(factory, update)`: the applier resets it ([`Applier::reuse`]) and
/// `update` runs as on a new node's values (the remembered ones read as empty).
#[allow(non_snake_case)]
pub fn ReusableComposeNode<A: Applier>(
    factory: impl FnOnce() -> A::Node,
    update: impl FnOnce(&mut Updater<A>),
) {
    let mut updater = start::<A>(true, factory);
    update(&mut updater);
    current().end_group();
}

/// Like [`ReusableComposeNode`], with `content` composed inside the node.
#[allow(non_snake_case)]
pub fn ReusableComposeNodeWithContent<A: Applier>(
    factory: impl FnOnce() -> A::Node,
    update: impl FnOnce(&mut Updater<A>),
    content: impl FnOnce(),
) {
    let mut updater = start::<A>(true, factory);
    update(&mut updater);
    content();
    current().end_group();
}

/// Like [`ReusableComposeNodeWithContent`], with a skippable update after `update`.
#[allow(non_snake_case)]
pub fn ReusableComposeNodeWithSkippableUpdate<A: Applier>(
    factory: impl FnOnce() -> A::Node,
    update: impl FnOnce(&mut Updater<A>),
    skippable_update: impl FnOnce(&mut SkippableUpdater<A>),
    content: impl FnOnce(),
) {
    with_skippable_update(true, factory, update, skippable_update, content);
}

fn with_skippable_update<A: Applier>(
    reusable: bool,
    factory: impl FnOnce() -> A::Node,
    update: impl FnOnce(&mut Updater<A>),
    skippable_update: impl FnOnce(&mut SkippableUpdater<A>),
    content: impl FnOnce(),
) {
    let mut updater = start::<A>(reusable, factory);
    update(&mut updater);
    let mut skippable = SkippableUpdater { updater };
    skippable_update(&mut skippable);
    let composer = current();
    composer.start_replace_group(CONTENT_KEY);
    content();
    composer.end_group();
    composer.end_group();
}
