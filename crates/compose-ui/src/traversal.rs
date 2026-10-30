//! Finding modifier nodes up and down the tree, as Compose's `TraversableNode` and its functions
//! (`findNearestAncestor`, `traverseAncestors`, `traverseChildren`, `traverseDescendants`), on the
//! internal visitors (`visitAncestors`, `visitChildren`, `visitSubtreeIf`, …) that walk chains and
//! layout nodes, pruning by kinds (docs/parity/design/modifier-nodes.md §3.8, docs/06-ui.md
//! "Modifiers").

use std::fmt;
use std::ops::ControlFlow;
use std::sync::atomic::AtomicBool;

use smallvec::SmallVec;

use crate::delegation::dispatch_with_context;
use crate::modifier::ModifierNode;
use crate::node_chain::{EntryId, NodeKinds, entry_state};
use crate::node_context::NodeContext;
use crate::tree::{NodeId, UiTree};

/// A key shared by the nodes a traversal looks for, as `TraversableNode.traverseKey`. Compared
/// by address, so declare one as a `static` (a `const` would be a new one at each use).
///
/// ```compile_fail
/// use compose_ui::{ModifierNode, TraversableNode, TraverseKey};
/// const KEY: TraverseKey = TraverseKey::new("a key");
/// struct Node;
/// impl ModifierNode for Node {}
/// impl TraversableNode for Node {
///     fn traverse_key(&self) -> &'static TraverseKey {
///         &KEY // a `const`'s copy isn't `'static`: two uses would be two keys
///     }
/// }
/// ```
pub struct TraverseKey {
    name: &'static str,
    /// Keeps each key's address its own, as `SemanticsPropertyKey`'s: a type with interior
    /// mutability isn't `Freeze`, so a static holding it goes to writable data, where statics of
    /// equal contents aren't folded into one address, and `&KEY` is never promoted to a
    /// constant's copy.
    _identity: AtomicBool,
}

impl TraverseKey {
    pub const fn new(name: &'static str) -> TraverseKey {
        TraverseKey { name, _identity: AtomicBool::new(false) }
    }

    /// Its name, for debugging.
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

impl PartialEq for TraverseKey {
    fn eq(&self, other: &TraverseKey) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for TraverseKey {}

impl fmt::Debug for TraverseKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "TraverseKey({})", self.name)
    }
}

/// A node traversals find by its key, as `TraversableNode`: returned from
/// `ModifierNode::as_traversable`.
pub trait TraversableNode: ModifierNode {
    fn traverse_key(&self) -> &'static TraverseKey;
}

impl dyn TraversableNode {
    /// The node as `T`, if it is one.
    pub fn downcast_ref<T: ModifierNode>(&self) -> Option<&T> {
        (self as &dyn ModifierNode).downcast_ref::<T>()
    }

    /// The node as `T`, if it is one.
    pub fn downcast_mut<T: ModifierNode>(&mut self) -> Option<&mut T> {
        (self as &mut dyn ModifierNode).downcast_mut::<T>()
    }
}

/// What a descendants' traversal does after visiting a node, as Compose's
/// `TraverseDescendantsAction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraverseDescendantsAction {
    /// Goes on, below the node too.
    ContinueTraversal,
    /// Goes on, but not below the node: past what follows it in its chain and its layout node's
    /// children.
    SkipSubtreeAndContinueTraversal,
    /// Stops.
    CancelTraversal,
}

/// A visitor of nodes: each with its own context, its chain's entry out of the chain meanwhile.
type Visitor<'f> = dyn FnMut(&mut dyn ModifierNode, &mut NodeContext<'_>) -> ControlFlow<()> + 'f;

/// The branches of a walk down the tree: a layout node and the chain index to start at, the next
/// to visit last.
pub(crate) type Branches = SmallVec<[(NodeId, usize); 8]>;

/// `node`'s children as branches from their chains' heads, the first visited on top: in draw
/// order (`z_order`), else in child order. Not its overlays: a popup is a root of its own, as
/// Compose's popup is a separate composition's.
fn push_children(tree: &UiTree, node: NodeId, z_order: bool, branches: &mut Branches) {
    let at = branches.len();
    let children = &tree.node(node).children;
    let in_tree = |child: NodeId| !tree.node(child).overlay;
    match if z_order { tree.placement_order(node) } else { None } {
        Some(order) => branches.extend(
            order
                .iter()
                .map(|&index| children[index as usize].node)
                .filter(|&child| in_tree(child))
                .map(|child| (child, 0)),
        ),
        None => branches.extend(
            children
                .iter()
                .map(|child| child.node)
                .filter(|&child| in_tree(child))
                .map(|child| (child, 0)),
        ),
    }
    branches[at..].reverse();
}

impl NodeContext<'_> {
    /// Where this node's chain has the nodes before it: up to its entry's index.
    fn before(&self) -> usize {
        self.index.unwrap_or(self.place)
    }

    /// Where this node's chain has the nodes after it.
    fn after(&self) -> usize {
        self.index.map_or(self.place, |index| index + 1)
    }

    /// Visits entry `index` of `node`: the node if it's of `kinds`, else its delegates that are;
    /// also the delegates of a node that is, with `include_delegates`.
    fn visit_entry(
        &mut self,
        node: NodeId,
        index: usize,
        kinds: NodeKinds,
        include_delegates: bool,
        f: &mut Visitor<'_>,
    ) -> ControlFlow<()> {
        let own = self.tree.node(node).chain[index].own;
        self.tree.with_entry(node, index, |modifier_node, context| {
            dispatch_with_context(modifier_node, own, kinds, include_delegates, context, f)
        })
    }

    /// Visits the attached nodes of `kinds` above this one, nearest first: those before it in its
    /// chain, then each ancestor's, tail to head, as `visitAncestors`. A delegate's start at its
    /// owner's, which it isn't told about.
    #[doc(hidden)]
    pub fn visit_ancestors(
        &mut self,
        kinds: NodeKinds,
        include_delegates: bool,
        f: &mut Visitor<'_>,
    ) {
        let mut layout = Some(self.node);
        let mut before = Some(self.before());
        while let Some(node) = layout {
            let Some(value) = self.tree.get(node) else { return };
            let end = before.take().unwrap_or(value.chain.len());
            // A popup is a root of its own, as Compose's.
            layout = if value.overlay {
                None
            } else {
                value.parent.or_else(|| {
                    let mut removed = self.tree.removed_parents.iter();
                    removed.rfind(|(child, _)| *child == node).map(|(_, parent)| *parent)
                })
            };
            if !value.kinds.any(kinds) {
                continue;
            }
            for index in (0..end).rev() {
                let Some(entry) = self.tree.get(node).and_then(|value| value.chain.get(index))
                else {
                    continue;
                };
                if entry.kinds.any(kinds)
                    && entry.state & entry_state::ATTACHED != 0
                    && self.visit_entry(node, index, kinds, include_delegates, f).is_break()
                {
                    return;
                }
            }
        }
    }

    /// Visits the attached nodes of `kinds` before this one in its chain, nearest first, as
    /// `visitLocalAncestors`.
    #[doc(hidden)]
    pub fn visit_local_ancestors(&mut self, kinds: NodeKinds, f: &mut Visitor<'_>) {
        let node = self.node;
        for index in (0..self.before()).rev() {
            let Some(entry) = self.tree.get(node).and_then(|value| value.chain.get(index)) else {
                continue;
            };
            if entry.kinds.any(kinds)
                && entry.state & entry_state::ATTACHED != 0
                && self.visit_entry(node, index, kinds, false, f).is_break()
            {
                return;
            }
        }
    }

    /// Visits the attached nodes of `kinds` after this one in its chain, in order, as
    /// `visitLocalDescendants`.
    #[doc(hidden)]
    pub fn visit_local_descendants(&mut self, kinds: NodeKinds, f: &mut Visitor<'_>) {
        let node = self.node;
        let mut index = self.after();
        while let Some(entry) = self.tree.get(node).and_then(|value| value.chain.get(index)) {
            if entry.kinds.any(kinds)
                && entry.state & entry_state::ATTACHED != 0
                && self.visit_entry(node, index, kinds, false, f).is_break()
            {
                return;
            }
            index += 1;
        }
    }

    /// The branches a walk down from this node starts with: the rest of its chain, or its
    /// layout node's children.
    fn first_branches(&self, z_order: bool) -> Branches {
        let mut branches = Branches::new();
        let start = self.after();
        if start < self.tree.get(self.node).map_or(0, |value| value.chain.len()) {
            branches.push((self.node, start));
        } else if self.tree.get(self.node).is_some() {
            push_children(&self.tree, self.node, z_order, &mut branches);
        }
        branches
    }

    /// Visits the first attached node of `kinds` down each branch below this one, as
    /// `visitChildren`: the next one in its chain if any, else the first in each child, and so
    /// on, its children in draw order with `z_order`. Content kept for reuse isn't visited.
    #[doc(hidden)]
    pub fn visit_children(&mut self, kinds: NodeKinds, z_order: bool, f: &mut Visitor<'_>) {
        let mut branches = self.first_branches(z_order);
        while let Some((node, start)) = branches.pop() {
            let Some(value) = self.tree.get(node) else { continue };
            if value.deactivated {
                continue;
            }
            let found = (start < value.chain.len() && value.chain[start].aggregate.any(kinds))
                .then(|| {
                    (start..value.chain.len()).find(|&index| {
                        let entry = &value.chain[index];
                        entry.kinds.any(kinds) && entry.state & entry_state::ATTACHED != 0
                    })
                })
                .flatten();
            match found {
                Some(index) => {
                    if self.visit_entry(node, index, kinds, false, f).is_break() {
                        return;
                    }
                }
                None => push_children(&self.tree, node, z_order, &mut branches),
            }
        }
    }

    /// Visits the attached nodes of `kinds` below this one, depth first, as `visitSubtreeIf`: what
    /// `f` returns says whether to go below each. Content kept for reuse isn't visited.
    #[doc(hidden)]
    pub fn visit_subtree_if(
        &mut self,
        kinds: NodeKinds,
        z_order: bool,
        f: &mut dyn FnMut(&mut dyn ModifierNode, &mut NodeContext<'_>) -> TraverseDescendantsAction,
    ) {
        let branches = self.first_branches(z_order);
        self.tree.walk_subtree_if(branches, kinds, z_order, f);
    }

    /// Finds the nearest traversable node above this one with `key`, as
    /// `findNearestAncestor(key)`: nodes before it in its chain and its ancestors', delegates
    /// included. A node's own running code never finds it: it's out of its chain.
    pub fn find_nearest_ancestor_with_key(
        &mut self,
        key: &'static TraverseKey,
    ) -> Option<&mut dyn TraversableNode> {
        let found = self.find_ancestor(&mut |node| {
            node.as_traversable().is_some_and(|node| node.traverse_key() == key)
        })?;
        found.as_traversable()
    }

    /// Finds the nearest node above this one of type `T` and this node's key, as
    /// `findNearestAncestor()`: nodes before it in its chain and its ancestors', delegates
    /// included.
    pub fn find_nearest_ancestor<T: TraversableNode>(&mut self) -> Option<&mut T> {
        let own = self.traverse_key;
        let found = self.find_ancestor(&mut |node| matching::<T>(node, own).is_some())?;
        found.downcast_mut::<T>()
    }

    /// The nearest ancestor that `matches`, delegates included.
    fn find_ancestor(
        &mut self,
        matches: &mut dyn FnMut(&mut dyn ModifierNode) -> bool,
    ) -> Option<&mut dyn ModifierNode> {
        let mut found: Option<(NodeId, usize, EntryId)> = None;
        self.visit_ancestors(NodeKinds::TRAVERSABLE, true, &mut |node, context| {
            if !matches(node) {
                return ControlFlow::Continue(());
            }
            let delegate = if context.delegated { context.entry } else { EntryId::NO_DELEGATE };
            found = context.index.map(|index| (context.node, index, delegate));
            ControlFlow::Break(())
        });
        let (node, index, delegate) = found?;
        // The caller may delegate or undelegate on what it's given: looked at when the outermost
        // node's code returns.
        let entry = self.tree.get(node)?.chain.get(index)?;
        if entry.state & entry_state::DELEGATING != 0 {
            let id = entry.id;
            self.tree.delegates_noted.push((node, id));
        }
        self.tree.get_mut(node)?.chain.get_mut(index)?.resolve(delegate)
    }

    /// Calls `f` with each traversable node above this one with `key`, nearest first, until it
    /// returns `false`, as `traverseAncestors(key)`.
    pub fn traverse_ancestors_with_key(
        &mut self,
        key: &'static TraverseKey,
        mut f: impl FnMut(&mut dyn TraversableNode, &mut NodeContext<'_>) -> bool,
    ) {
        self.visit_ancestors(NodeKinds::TRAVERSABLE, false, &mut |node, context| match node
            .as_traversable()
        {
            Some(node) if node.traverse_key() == key => {
                if f(node, context) {
                    ControlFlow::Continue(())
                } else {
                    ControlFlow::Break(())
                }
            }
            _ => ControlFlow::Continue(()),
        });
    }

    /// Calls `f` with each node above this one of type `T` and this node's key, nearest first,
    /// until it returns `false`, as `traverseAncestors`. Each is given its own context (to
    /// invalidate it, or traverse from it). Called from a node that isn't traversable, the key
    /// doesn't count.
    pub fn traverse_ancestors<T: TraversableNode>(
        &mut self,
        mut f: impl FnMut(&mut T, &mut NodeContext<'_>) -> bool,
    ) {
        let own = self.traverse_key;
        self.visit_ancestors(
            NodeKinds::TRAVERSABLE,
            false,
            &mut |node, context| match matching::<T>(node, own) {
                Some(node) => {
                    if f(node, context) {
                        ControlFlow::Continue(())
                    } else {
                        ControlFlow::Break(())
                    }
                }
                None => ControlFlow::Continue(()),
            },
        );
    }

    /// Calls `f` with the traversable nodes directly below this one with `key`, until it returns
    /// `false`, as `traverseChildren(key)`: the first traversable node down each branch, not
    /// below it, whatever its key.
    pub fn traverse_children_with_key(
        &mut self,
        key: &'static TraverseKey,
        mut f: impl FnMut(&mut dyn TraversableNode, &mut NodeContext<'_>) -> bool,
    ) {
        self.visit_children(NodeKinds::TRAVERSABLE, false, &mut |node, context| match node
            .as_traversable()
        {
            Some(node) if node.traverse_key() == key => {
                if f(node, context) {
                    ControlFlow::Continue(())
                } else {
                    ControlFlow::Break(())
                }
            }
            _ => ControlFlow::Continue(()),
        });
    }

    /// Calls `f` with the nodes directly below this one of type `T` and this node's key, until it
    /// returns `false`, as `traverseChildren`.
    pub fn traverse_children<T: TraversableNode>(
        &mut self,
        mut f: impl FnMut(&mut T, &mut NodeContext<'_>) -> bool,
    ) {
        let own = self.traverse_key;
        self.visit_children(
            NodeKinds::TRAVERSABLE,
            false,
            &mut |node, context| match matching::<T>(node, own) {
                Some(node) => {
                    if f(node, context) {
                        ControlFlow::Continue(())
                    } else {
                        ControlFlow::Break(())
                    }
                }
                None => ControlFlow::Continue(()),
            },
        );
    }

    /// Calls `f` with each traversable node below this one with `key`, as
    /// `traverseDescendants(key)`: what it returns says how to go on.
    pub fn traverse_descendants_with_key(
        &mut self,
        key: &'static TraverseKey,
        mut f: impl FnMut(&mut dyn TraversableNode, &mut NodeContext<'_>) -> TraverseDescendantsAction,
    ) {
        self.visit_subtree_if(NodeKinds::TRAVERSABLE, false, &mut |node, context| match node
            .as_traversable()
        {
            Some(node) if node.traverse_key() == key => f(node, context),
            _ => TraverseDescendantsAction::ContinueTraversal,
        });
    }

    /// Calls `f` with each node below this one of type `T` and this node's key, as
    /// `traverseDescendants`: what it returns says how to go on.
    pub fn traverse_descendants<T: TraversableNode>(
        &mut self,
        mut f: impl FnMut(&mut T, &mut NodeContext<'_>) -> TraverseDescendantsAction,
    ) {
        let own = self.traverse_key;
        self.visit_subtree_if(
            NodeKinds::TRAVERSABLE,
            false,
            &mut |node, context| match matching::<T>(node, own) {
                Some(node) => f(node, context),
                None => TraverseDescendantsAction::ContinueTraversal,
            },
        );
    }
}

/// `node` as a `T` of the key `own`, as `areObjectsOfSameType` and the key's comparison (`None`,
/// from a node that isn't traversable: any key).
fn matching<'a, T: TraversableNode>(
    node: &'a mut dyn ModifierNode,
    own: Option<&'static TraverseKey>,
) -> Option<&'a mut T> {
    let key = node.as_traversable()?.traverse_key();
    if own.is_some_and(|own| own != key) {
        return None;
    }
    node.downcast_mut::<T>()
}

impl UiTree {
    /// What `NodeContext::visit_subtree_if` does, from `branches`.
    pub(crate) fn walk_subtree_if(
        &mut self,
        mut branches: Branches,
        kinds: NodeKinds,
        z_order: bool,
        f: &mut dyn FnMut(&mut dyn ModifierNode, &mut NodeContext<'_>) -> TraverseDescendantsAction,
    ) {
        'branches: while let Some((node, start)) = branches.pop() {
            let Some(value) = self.get(node) else { continue };
            if value.deactivated {
                continue;
            }
            if value.chain.get(start).is_some_and(|entry| entry.aggregate.any(kinds)) {
                let mut index = start;
                while let Some(entry) = self.get(node).and_then(|value| value.chain.get(index)) {
                    if entry.state & entry_state::ATTACHED == 0 {
                        break;
                    }
                    if entry.kinds.any(kinds) {
                        let own = entry.own;
                        let mut action = TraverseDescendantsAction::ContinueTraversal;
                        let _ = self.with_entry(node, index, |modifier_node, context| {
                            dispatch_with_context(
                                modifier_node,
                                own,
                                kinds,
                                false,
                                context,
                                &mut |visited, context| {
                                    action = f(visited, context);
                                    match action {
                                        TraverseDescendantsAction::ContinueTraversal => {
                                            ControlFlow::Continue(())
                                        }
                                        _ => ControlFlow::Break(()),
                                    }
                                },
                            )
                        });
                        match action {
                            TraverseDescendantsAction::ContinueTraversal => {}
                            TraverseDescendantsAction::SkipSubtreeAndContinueTraversal => {
                                continue 'branches;
                            }
                            TraverseDescendantsAction::CancelTraversal => return,
                        }
                    }
                    index += 1;
                }
            }
            if self.get(node).is_some() {
                push_children(self, node, z_order, &mut branches);
            }
        }
    }
}
