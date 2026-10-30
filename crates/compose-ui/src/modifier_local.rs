//! Modifier locals, as Compose's `ModifierLocal`: values a modifier node provides to the nodes
//! after it in its chain and below it in the tree (docs/parity/design/modifier-nodes.md §3.9).

use std::any::Any;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;

use smallvec::SmallVec;

use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement};
use crate::node_chain::{EntryId, NodeKinds, entry_state};
use crate::node_context::NodeContext;
use crate::traversal::TraverseDescendantsAction;
use crate::tree::{NodeId, UiTree};

/// A modifier local, as `ModifierLocal<T>` and `ProvidableModifierLocal<T>`: declared as a
/// `static` (a `const`'s address isn't `'static`, which the `AtomicBool` makes a compile error),
/// and compared by address, so two of one name are two.
pub struct ModifierLocal<T: 'static> {
    name: &'static str,
    default: fn() -> T,
    /// Keeps each one's address its own, as `SemanticsPropertyKey`'s.
    _identity: AtomicBool,
}

/// A modifier local whose value is `default()` where nothing provides it, as `modifierLocalOf`.
pub const fn modifier_local_of<T>(name: &'static str, default: fn() -> T) -> ModifierLocal<T> {
    ModifierLocal { name, default, _identity: AtomicBool::new(false) }
}

impl<T> ModifierLocal<T> {
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Its value where nothing provides it.
    pub fn default_value(&self) -> T {
        (self.default)()
    }

    fn key(&'static self) -> LocalKey {
        LocalKey(self as *const Self as *const () as usize)
    }
}

/// A local's identity: its address.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct LocalKey(usize);

/// What a map holds for a key.
enum Slot<T: 'static> {
    /// `modifierLocalMapOf(key)`: `null` in Compose's, nothing to read here: the default.
    Empty,
    Value(T),
    /// A legacy provider's value, read when it's asked (observed if that's in an observation).
    Provider(Rc<dyn Fn() -> T>),
}

/// The values a node provides, as `ModifierLocalMap` and `modifierLocalMapOf`: a few keys, inline.
#[derive(Default)]
pub struct ModifierLocalMap {
    entries: SmallVec<[(LocalKey, Box<dyn Any>); 2]>,
    /// Keys that left it since the tree last looked (a legacy provider's new local).
    replaced: SmallVec<[LocalKey; 1]>,
}

/// An empty map, as `modifierLocalMapOf()`.
pub fn modifier_local_map_of() -> ModifierLocalMap {
    ModifierLocalMap::default()
}

impl ModifierLocalMap {
    /// With `key`, no value yet, as `modifierLocalMapOf(key)`.
    pub fn with_key<T: 'static>(mut self, key: &'static ModifierLocal<T>) -> ModifierLocalMap {
        self.put(key.key(), Slot::<T>::Empty);
        self
    }

    /// With `key`'s `value`, as `modifierLocalMapOf(key to value)`.
    pub fn with<T: 'static>(
        mut self,
        key: &'static ModifierLocal<T>,
        value: T,
    ) -> ModifierLocalMap {
        self.put(key.key(), Slot::Value(value));
        self
    }

    fn put<T: 'static>(&mut self, key: LocalKey, slot: Slot<T>) {
        let slot: Box<dyn Any> = Box::new(slot);
        match self.entries.iter_mut().find(|(found, _)| *found == key) {
            Some(entry) => entry.1 = slot,
            None => self.entries.push((key, slot)),
        }
    }

    pub fn contains<T: 'static>(&self, key: &'static ModifierLocal<T>) -> bool {
        self.contains_key(key.key())
    }

    pub(crate) fn contains_key(&self, key: LocalKey) -> bool {
        self.entries.iter().any(|(found, _)| *found == key)
    }

    /// `key`'s value, if the map has one, as `ModifierLocalMap.get` (`None` for Compose's `null`).
    pub fn get<T: Clone + 'static>(&self, key: &'static ModifierLocal<T>) -> Option<T> {
        self.get_key(key.key())
    }

    fn get_key<T: Clone + 'static>(&self, key: LocalKey) -> Option<T> {
        let (_, slot) = self.entries.iter().find(|(found, _)| *found == key)?;
        match slot.downcast_ref::<Slot<T>>()? {
            Slot::Empty => None,
            Slot::Value(value) => Some(value.clone()),
            Slot::Provider(value) => Some(value()),
        }
    }

    /// A new value for `key`, as `ModifierLocalModifierNode.provide`: nodes below read it, but
    /// aren't told, as Compose's.
    ///
    /// Panics if the map hasn't `key`, as Compose's.
    pub fn provide<T: 'static>(&mut self, key: &'static ModifierLocal<T>, value: T) {
        assert!(
            self.contains(key),
            "compose-ui: any provided key must be initially provided in the node's \
             `provided_values`: {} wasn't found",
            key.name
        );
        self.put(key.key(), Slot::Value(value));
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = LocalKey> + '_ {
        self.entries.iter().map(|(key, _)| *key)
    }

    fn take_replaced(&mut self) -> SmallVec<[LocalKey; 1]> {
        std::mem::take(&mut self.replaced)
    }
}

/// A node reading or providing modifier locals, as `ModifierLocalModifierNode`; returned from
/// `ModifierNode::as_modifier_local`. A node reads with `NodeContext::modifier_local_current`.
pub trait ModifierLocalModifierNode {
    /// What it provides, as `providedValues`: `None`, the default, if it only reads. Values it
    /// changes later go through `ModifierLocalMap::provide`, for keys it has from the start.
    fn provided_values(&mut self) -> Option<&mut ModifierLocalMap> {
        None
    }
}

/// Where a consumer reads, as `ModifierLocalReadScope`.
pub struct ModifierLocalReadScope<'a, 'b> {
    /// `None`, its node being detached: every local reads its default, as
    /// `DetachedModifierLocalReadScope`.
    context: Option<&'a mut NodeContext<'b>>,
    reads: SmallVec<[LocalKey; 2]>,
}

impl ModifierLocalReadScope<'_, '_> {
    /// The value of `local` here: the nearest provider before its node, else the default, as
    /// `ModifierLocal.current`.
    pub fn current<T: Clone + 'static>(&mut self, local: &'static ModifierLocal<T>) -> T {
        self.reads.push(local.key());
        match &mut self.context {
            Some(context) => context.modifier_local_current(local),
            None => local.default_value(),
        }
    }
}

impl NodeContext<'_> {
    /// The value of `local` here, as `ModifierLocal.current` in a `ModifierLocalModifierNode`: the
    /// nearest provider among the nodes before it in its chain and its ancestors' chains, else
    /// the default.
    ///
    /// Panics if the node isn't attached, as Compose's.
    pub fn modifier_local_current<T: Clone + 'static>(
        &mut self,
        local: &'static ModifierLocal<T>,
    ) -> T {
        assert!(self.is_attached(), "compose-ui: a modifier local read from an unattached node");
        let key = local.key();
        let mut found = None;
        self.visit_ancestors(NodeKinds::LOCALS, false, &mut |node, _| {
            if let Some(map) = node.as_modifier_local().and_then(|node| node.provided_values())
                && map.contains_key(key)
            {
                found = map.get_key::<T>(key);
                return std::ops::ControlFlow::Break(());
            }
            std::ops::ControlFlow::Continue(())
        });
        found.unwrap_or_else(|| local.default_value())
    }
}

// ---- The legacy elements, as `Modifier.modifierLocalProvider` and `modifierLocalConsumer` ------

impl Modifier {
    /// Provides `value()` for `local` to what's after it and below it, as
    /// `Modifier.modifierLocalProvider`. What `value` reads is observed where it's read.
    pub fn modifier_local_provider<T: Clone + 'static>(
        self,
        local: &'static ModifierLocal<T>,
        value: impl Fn() -> T + 'static,
    ) -> Modifier {
        self.then_custom(ProviderElement { local, value: Rc::new(value) })
    }

    /// Runs `consumer` when its node is attached, and when what it read changes (a provider come,
    /// gone or changed before it, or the state a provider's value reads); with defaults as its
    /// node is detached. As `Modifier.modifierLocalConsumer`.
    pub fn modifier_local_consumer(
        self,
        consumer: impl Fn(&mut ModifierLocalReadScope<'_, '_>) + 'static,
    ) -> Modifier {
        self.then_custom(ConsumerElement { consumer: Rc::new(consumer) })
    }
}

struct ProviderElement<T: 'static> {
    local: &'static ModifierLocal<T>,
    value: Rc<dyn Fn() -> T>,
}

struct ProviderNode<T: 'static> {
    local: &'static ModifierLocal<T>,
    map: ModifierLocalMap,
}

impl<T: Clone + 'static> ModifierNodeElement for ProviderElement<T> {
    type Node = ProviderNode<T>;
    fn create(&self) -> ProviderNode<T> {
        let mut map = ModifierLocalMap::default();
        map.put(self.local.key(), Slot::Provider(self.value.clone()));
        ProviderNode { local: self.local, map }
    }
    fn update(&self, node: &mut ProviderNode<T>) {
        if !std::ptr::eq(node.local, self.local) {
            node.map.entries.clear();
            node.map.replaced.push(node.local.key());
            node.local = self.local;
        }
        node.map.put(self.local.key(), Slot::Provider(self.value.clone()));
    }
    fn same(&self, other: &Self) -> bool {
        std::ptr::eq(self.local, other.local) && Rc::ptr_eq(&self.value, &other.value)
    }
    fn name(&self) -> &'static str {
        "modifierLocalProvider"
    }
}

impl<T: 'static> ModifierNode for ProviderNode<T> {
    fn as_modifier_local(&mut self) -> Option<&mut dyn ModifierLocalModifierNode> {
        Some(self)
    }
}

impl<T: 'static> ModifierLocalModifierNode for ProviderNode<T> {
    fn provided_values(&mut self) -> Option<&mut ModifierLocalMap> {
        Some(&mut self.map)
    }
}

struct ConsumerElement {
    consumer: Rc<dyn Fn(&mut ModifierLocalReadScope<'_, '_>)>,
}

struct ConsumerNode {
    consumer: Rc<dyn Fn(&mut ModifierLocalReadScope<'_, '_>)>,
    /// What it read in its last run: a provider's change of another local doesn't run it.
    reads: SmallVec<[LocalKey; 2]>,
}

impl ModifierNodeElement for ConsumerElement {
    type Node = ConsumerNode;
    fn create(&self) -> ConsumerNode {
        ConsumerNode { consumer: self.consumer.clone(), reads: SmallVec::new() }
    }
    fn update(&self, node: &mut ConsumerNode) {
        node.consumer = self.consumer.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.consumer, &other.consumer)
    }
    fn name(&self) -> &'static str {
        "modifierLocalConsumer"
    }
}

impl ConsumerNode {
    /// Runs the consumer, observing what it reads, as `updateModifierLocalConsumer`.
    fn run(&mut self, context: &mut NodeContext<'_>) {
        let requests = context.tree.focus_requests.clone();
        let (node, entry) = (context.node, context.entry);
        let consumer = self.consumer.clone();
        let mut scope =
            ModifierLocalReadScope { context: Some(&mut *context), reads: SmallVec::new() };
        requests.observe_entry_reads(node, entry, || consumer(&mut scope));
        self.reads = scope.reads;
    }
}

impl ModifierNode for ConsumerNode {
    fn as_modifier_local(&mut self) -> Option<&mut dyn ModifierLocalModifierNode> {
        Some(self)
    }
    fn as_observer(&mut self) -> Option<&mut dyn crate::observer_node::ObserverModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.run(context);
    }
    /// With the defaults, as `DetachedModifierLocalReadScope`.
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        let mut scope = ModifierLocalReadScope { context: None, reads: SmallVec::new() };
        (self.consumer)(&mut scope);
    }
}

impl ModifierLocalModifierNode for ConsumerNode {}

impl crate::observer_node::ObserverModifierNode for ConsumerNode {
    fn on_observed_reads_changed(&mut self, context: &mut NodeContext<'_>) {
        self.run(context);
    }
}

// ---- What the tree does when providers change, as `ModifierLocalManager` -----------------------

/// A provider of `key` changed at `start` of `node`'s chain (come, gone or updated); or, with no
/// key, the consumer `entry` was updated.
pub(crate) enum LocalChange {
    Provider { node: NodeId, start: usize, key: LocalKey },
    Consumer { node: NodeId, entry: EntryId },
}

impl UiTree {
    /// Notes what entry `index` of `node`, updated or inserted, changes of its locals: what it
    /// provides, for what's after it; itself, if it's a consumer updated.
    pub(crate) fn note_local_changes(&mut self, node: NodeId, index: usize, updated: bool) {
        let entry = &mut self.node_mut(node).chain[index];
        let (id, own) = (entry.id, entry.own);
        let mut changes: SmallVec<[LocalChange; 2]> = SmallVec::new();
        crate::delegation::dispatch_for_kind(
            &mut *entry.node,
            own,
            NodeKinds::LOCALS,
            EntryId::NO_DELEGATE,
            &mut |modifier_node, delegate| {
                if updated && modifier_node.downcast_ref::<ConsumerNode>().is_some() {
                    let entry = if delegate == EntryId::NO_DELEGATE { id } else { delegate };
                    changes.push(LocalChange::Consumer { node, entry });
                }
                let Some(map) = modifier_node.as_modifier_local().and_then(|n| n.provided_values())
                else {
                    return;
                };
                for key in map.take_replaced().into_iter().chain(map.keys()) {
                    changes.push(LocalChange::Provider { node, start: index + 1, key });
                }
            },
        );
        self.modifier_local_changes.extend(changes);
    }

    /// Notes what `removed`, a provider a chain change removed from `node`, provided: what's
    /// below it looks again, from the chain's head, as `removedProvider`.
    pub(crate) fn note_removed_locals(
        &mut self,
        node: NodeId,
        removed: &mut crate::node_chain::ChainEntry,
    ) {
        let mut changes: SmallVec<[LocalChange; 2]> = SmallVec::new();
        crate::delegation::dispatch_for_kind(
            &mut *removed.node,
            removed.own,
            NodeKinds::LOCALS,
            EntryId::NO_DELEGATE,
            &mut |modifier_node, _| {
                let Some(map) = modifier_node.as_modifier_local().and_then(|n| n.provided_values())
                else {
                    return;
                };
                for key in map.keys() {
                    changes.push(LocalChange::Provider { node, start: 0, key });
                }
            },
        );
        self.modifier_local_changes.extend(changes);
    }

    /// Runs, once each, the consumers below the providers that changed that read what they
    /// provide, and those updated, as `ModifierLocalManager.triggerUpdates`: when the pass ends,
    /// a provider's and a consumer's changes together one run.
    pub(crate) fn run_modifier_local_changes(&mut self) {
        if self.modifier_local_changes.is_empty() {
            return;
        }
        let changes = std::mem::take(&mut self.modifier_local_changes);
        let mut to_run: SmallVec<[(NodeId, EntryId); 4]> = SmallVec::new();
        for change in changes {
            match change {
                LocalChange::Consumer { node, entry } => {
                    if !to_run.contains(&(node, entry)) {
                        to_run.push((node, entry));
                    }
                }
                LocalChange::Provider { node, start, key } => {
                    if !self.get(node).is_some_and(|value| value.attached && !value.deactivated) {
                        continue;
                    }
                    let mut branches = crate::traversal::Branches::new();
                    branches.push((node, start));
                    self.walk_subtree_if(
                        branches,
                        NodeKinds::LOCALS,
                        false,
                        &mut |visited, context| {
                            if let Some(consumer) = visited.downcast_ref::<ConsumerNode>()
                                && consumer.reads.contains(&key)
                                && !to_run.contains(&(context.node, context.entry))
                            {
                                to_run.push((context.node, context.entry));
                            }
                            // Only below what doesn't provide it too.
                            let provides = visited
                                .as_modifier_local()
                                .and_then(|node| node.provided_values())
                                .is_some_and(|map| map.contains_key(key));
                            if provides {
                                TraverseDescendantsAction::SkipSubtreeAndContinueTraversal
                            } else {
                                TraverseDescendantsAction::ContinueTraversal
                            }
                        },
                    );
                }
            }
        }
        for (node, entry) in to_run {
            let Some(index) = self.entry_index(node, entry) else { continue };
            if self.entry_or_delegate_state(node, index, entry) & entry_state::ATTACHED == 0 {
                continue;
            }
            self.with_entry_or_delegate(node, index, entry, |modifier_node, context| {
                if let Some(consumer) = modifier_node.downcast_mut::<ConsumerNode>() {
                    consumer.run(context);
                }
            });
        }
    }
}
