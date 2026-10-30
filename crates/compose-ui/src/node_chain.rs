//! A layout node's chain of modifier nodes, as Compose's `NodeChain`: each entry with its node
//! kinds and those of the entries after it, and the diff that matches a new modifier's elements
//! to the old ones' nodes (docs/06-ui.md, "Modifiers").

use std::fmt;
use std::ops::{BitOr, BitOrAssign};

use crate::modifier::ModifierNode;

/// A node's kinds, as Compose's `NodeKind` masks (node/NodeKind.kt): what it implements, probed
/// once when it's created, so walks look at bits instead of asking each node.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct NodeKinds(u32);

// Compose's whole set, though not every kind has its nodes yet
// (docs/parity/design/modifier-nodes.md).
#[allow(dead_code)]
impl NodeKinds {
    pub const NONE: NodeKinds = NodeKinds(0);
    /// Every kind.
    pub(crate) const ALL: NodeKinds = NodeKinds(u32::MAX);
    /// Every node has it, as Compose's `Nodes.Any`.
    pub const ANY: NodeKinds = NodeKinds(1 << 30);
    // Compose's kinds, in its order.
    pub const LAYOUT: NodeKinds = NodeKinds(1 << 0);
    pub const DRAW: NodeKinds = NodeKinds(1 << 1);
    pub const SEMANTICS: NodeKinds = NodeKinds(1 << 2);
    pub const POINTER_INPUT: NodeKinds = NodeKinds(1 << 3);
    pub const LOCALS: NodeKinds = NodeKinds(1 << 4);
    pub const PARENT_DATA: NodeKinds = NodeKinds(1 << 5);
    pub const ON_REMEASURED: NodeKinds = NodeKinds(1 << 6);
    pub const GLOBAL_POSITION_AWARE: NodeKinds = NodeKinds(1 << 7);
    pub const APPROACH_MEASURE: NodeKinds = NodeKinds(1 << 8);
    pub const FOCUS_TARGET: NodeKinds = NodeKinds(1 << 9);
    pub const FOCUS_PROPERTIES: NodeKinds = NodeKinds(1 << 10);
    pub const FOCUS_EVENT: NodeKinds = NodeKinds(1 << 11);
    pub const KEY_INPUT: NodeKinds = NodeKinds(1 << 12);
    pub const ROTARY_INPUT: NodeKinds = NodeKinds(1 << 13);
    pub const COMPOSITION_LOCAL_CONSUMER: NodeKinds = NodeKinds(1 << 14);
    pub const SOFT_KEYBOARD_KEY_INPUT: NodeKinds = NodeKinds(1 << 15);
    pub const TRAVERSABLE: NodeKinds = NodeKinds(1 << 16);
    pub const BRING_INTO_VIEW: NodeKinds = NodeKinds(1 << 17);
    pub const UNPLACED: NodeKinds = NodeKinds(1 << 18);
    pub const INDIRECT_POINTER_INPUT: NodeKinds = NodeKinds(1 << 19);
    pub const ON_PLACED: NodeKinds = NodeKinds(1 << 20);
    pub const BEYOND_BOUNDS_LAYOUT: NodeKinds = NodeKinds(1 << 21);
    // compose-rs's own: what were capability bits or chain scans by downcast.
    /// A user-defined element's node: its measuring and drawing are observed for state reads.
    pub const OBSERVED: NodeKinds = NodeKinds(1 << 24);
    /// A graphics layer (`graphics_layer`, `clip`, `alpha`, …).
    pub const GRAPHICS_LAYER: NodeKinds = NodeKinds(1 << 25);
    /// An overlay (`Modifier.overlay`: a popup's or a dialog's window).
    pub const OVERLAY: NodeKinds = NodeKinds(1 << 26);
    /// `Modifier.focus_requester`.
    pub const FOCUS_REQUESTER: NodeKinds = NodeKinds(1 << 27);
    /// A text a `SelectionContainer` selects in.
    pub const SELECTABLE: NodeKinds = NodeKinds(1 << 28);
    /// `Modifier.repaint_boundary` and the nodes asking to be one.
    pub const REPAINT_BOUNDARY: NodeKinds = NodeKinds(1 << 29);
    /// The kinds a delegate gives its owner: those the tree dispatches to delegates. The rest
    /// are the built-in elements' (which aren't delegated to) or answered by the chain's node
    /// itself, which forwards them if it wants to (`ModifierNode::delegates`).
    const DELEGATED: NodeKinds = NodeKinds(
        NodeKinds::ANY.0
            | NodeKinds::LAYOUT.0
            | NodeKinds::DRAW.0
            | NodeKinds::SEMANTICS.0
            | NodeKinds::POINTER_INPUT.0
            | NodeKinds::PARENT_DATA.0
            | NodeKinds::GLOBAL_POSITION_AWARE.0
            | NodeKinds::FOCUS_TARGET.0
            | NodeKinds::KEY_INPUT.0
            | NodeKinds::TRAVERSABLE.0,
    );

    /// Whether it has any of `other`'s kinds.
    #[inline]
    pub const fn any(self, other: NodeKinds) -> bool {
        self.0 & other.0 != 0
    }

    /// It and `other`'s kinds.
    #[inline]
    pub(crate) const fn with(self, other: NodeKinds) -> NodeKinds {
        NodeKinds(self.0 | other.0)
    }

    /// Whether it has every one of `other`'s kinds.
    #[inline]
    pub const fn all(self, other: NodeKinds) -> bool {
        self.0 & other.0 == other.0
    }

    /// The kinds it has that `other` has too.
    #[inline]
    pub(crate) const fn and(self, other: NodeKinds) -> NodeKinds {
        NodeKinds(self.0 & other.0)
    }

    /// The kinds it has that `other` hasn't.
    #[inline]
    pub(crate) const fn without(self, other: NodeKinds) -> NodeKinds {
        NodeKinds(self.0 & !other.0)
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The kinds of `node`, which an element of a user-defined type made if `observed`: what
    /// it answers to, asked once.
    pub(crate) fn of(node: &mut dyn ModifierNode, observed: bool) -> NodeKinds {
        let observed_or_lifecycle = observed || node.observes_reads();
        let mut kinds = if observed_or_lifecycle { NodeKinds::OBSERVED } else { NodeKinds::NONE };
        kinds |= NodeKinds::ANY;
        let mut add = |has: bool, kind: NodeKinds| {
            if has {
                kinds |= kind;
            }
        };
        add(node.as_layout().is_some(), NodeKinds::LAYOUT);
        add(node.as_draw().is_some(), NodeKinds::DRAW);
        add(node.as_pointer_input().is_some(), NodeKinds::POINTER_INPUT);
        add(node.as_semantics().is_some(), NodeKinds::SEMANTICS);
        add(node.as_focus_target().is_some(), NodeKinds::FOCUS_TARGET);
        add(node.as_key_input().is_some(), NodeKinds::KEY_INPUT);
        add(node.as_global_position_aware().is_some(), NodeKinds::GLOBAL_POSITION_AWARE);
        add(node.as_parent_data().is_some(), NodeKinds::PARENT_DATA);
        // Not `SELECTABLE`: a text answers `as_selectable` by its element's `selectable`, which
        // an update changes, so the selection walks ask each node.
        add(node.is_repaint_boundary(), NodeKinds::REPAINT_BOUNDARY);
        add(node.is_beyond_bounds_layout(), NodeKinds::BEYOND_BOUNDS_LAYOUT);
        add(
            node.downcast_ref::<crate::nodes::OnFocusChangedNode>().is_some(),
            NodeKinds::FOCUS_EVENT,
        );
        add(
            node.downcast_ref::<crate::graphics_layer::GraphicsLayerNode>().is_some(),
            NodeKinds::GRAPHICS_LAYER,
        );
        add(node.downcast_ref::<crate::overlay::OverlayNode>().is_some(), NodeKinds::OVERLAY);
        add(
            node.downcast_ref::<crate::focus_requester::FocusRequesterNode>().is_some(),
            NodeKinds::FOCUS_REQUESTER,
        );
        // Only a node with a lifecycle may be one.
        if observed_or_lifecycle {
            add(node.as_traversable().is_some(), NodeKinds::TRAVERSABLE);
        }
        kinds
    }

    /// The kinds of `node`, which is delegated to: only what a delegate gives its owner.
    pub(crate) fn of_delegate(node: &mut dyn ModifierNode) -> NodeKinds {
        NodeKinds(NodeKinds::of(node, true).0 & NodeKinds::DELEGATED.0)
    }
}

impl BitOr for NodeKinds {
    type Output = NodeKinds;
    #[inline]
    fn bitor(self, other: NodeKinds) -> NodeKinds {
        NodeKinds(self.0 | other.0)
    }
}

impl BitOrAssign for NodeKinds {
    #[inline]
    fn bitor_assign(&mut self, other: NodeKinds) {
        self.0 |= other.0;
    }
}

impl fmt::Debug for NodeKinds {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "NodeKinds({:#x})", self.0)
    }
}

/// A chain entry's identity: stable across chain diffs and reuse, from a counter of its tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct EntryId(pub(crate) u64);

impl EntryId {
    /// No delegate: the chain's own node, where a place names one (`HitEntry::delegate`).
    pub(crate) const NO_DELEGATE: EntryId = EntryId(0);
}

/// A chain entry's lifecycle flags (`ChainEntry::state`).
pub(crate) mod entry_state {
    /// Attached, as `Modifier.Node.isAttached`: marked as its layout node is attached (or the
    /// entry made in an attached one), before its `on_attach` runs; cleared after `on_detach`.
    pub(crate) const ATTACHED: u8 = 1;
    /// Its `on_attach` ran (and its `on_detach` hasn't): what detaching it calls `on_detach` on.
    pub(crate) const ATTACH_RAN: u8 = 2;
    /// It made a coroutine scope (`FocusRequests::scopes`), cancelled when it's detached.
    pub(crate) const HAS_SCOPE: u8 = 4;
    /// A handle was made for it: its attachment is mirrored where handles read it.
    pub(crate) const HAS_HANDLE: u8 = 8;
    /// It observed reads (`NodeContext::observe_reads`): forgotten when it's dropped.
    pub(crate) const OBSERVES: u8 = 16;
    /// Its node has delegates, or may get some (`ModifierNode::delegates` answers).
    pub(crate) const DELEGATING: u8 = 32;
}

/// The key of `node`, whose own kinds are `own`, if it's a `TraversableNode`.
#[inline]
pub(crate) fn traverse_key_of(
    node: &mut dyn ModifierNode,
    own: NodeKinds,
) -> Option<&'static crate::traversal::TraverseKey> {
    if !own.any(NodeKinds::TRAVERSABLE) {
        return None;
    }
    node.as_traversable().map(|node| node.traverse_key())
}

/// The kinds a chain would take `node` with, which an element of a user-defined type made if
/// `observed`: what tests of `ChainEntry::new` read. Not for apps.
#[doc(hidden)]
pub fn chain_entry_kinds_for_test(node: &mut dyn ModifierNode, observed: bool) -> NodeKinds {
    let own = NodeKinds::of(node, observed);
    if own.any(NodeKinds::OBSERVED)
        && let Some(delegates) = node.delegates()
    {
        return own | delegates.refresh_kinds(own);
    }
    own
}

/// What an element's update did to its node's delegates (`ChainEntry::after_update`).
#[derive(Clone, Copy, Default)]
pub(crate) struct DelegatesUpdated {
    /// It delegated or undelegated: the tree looks at the pass's end.
    pub(crate) delegated: bool,
    /// A layout delegate came or went: the node has a layer more or less.
    pub(crate) layers: bool,
}

/// One modifier node of a layout node's chain.
pub(crate) struct ChainEntry {
    pub(crate) node: Box<dyn ModifierNode>,
    /// Its kinds and its delegates' (Compose's `kindSet`).
    pub(crate) kinds: NodeKinds,
    /// Its kinds and those of every entry after it (Compose's `aggregateChildKindSet`).
    pub(crate) aggregate: NodeKinds,
    /// Stable across chain diffs and reuse: what the lifecycle and handles name it by.
    pub(crate) id: EntryId,
    /// Its `entry_state` flags.
    pub(crate) state: u8,
    /// What its node itself is, without its delegates (Compose's `selfKindSet`).
    pub(crate) own: NodeKinds,
}

impl ChainEntry {
    /// An entry for `node`, its kinds `fixed` if its element's type says them (checked in debug
    /// builds), else probed now.
    pub(crate) fn new(
        mut node: Box<dyn ModifierNode>,
        fixed: Option<NodeKinds>,
        id: EntryId,
    ) -> ChainEntry {
        let own = match fixed {
            Some(kinds) => {
                debug_assert_eq!(
                    kinds,
                    NodeKinds::of(&mut *node, false),
                    "compose-ui: a built-in element's kinds (`Element::fixed_kinds`)"
                );
                kinds
            }
            None => NodeKinds::of(&mut *node, true),
        };
        let mut kinds = own;
        let mut state = 0;
        // A user-defined element's node, or a built-in one with a lifecycle (`observes_reads`).
        if own.any(NodeKinds::OBSERVED)
            && let Some(delegates) = node.delegates()
        {
            kinds |= delegates.refresh_kinds(own);
            state = entry_state::DELEGATING;
        }
        ChainEntry { node, kinds, aggregate: kinds, id, state, own }
    }

    /// The node `delegate` names: a delegate of its node, or its node for `NO_DELEGATE`.
    pub(crate) fn resolve(&mut self, delegate: EntryId) -> Option<&mut dyn ModifierNode> {
        if delegate == EntryId::NO_DELEGATE {
            return Some(&mut *self.node);
        }
        crate::delegation::find_delegate(&mut *self.node, delegate).map(|found| found.node_mut())
    }

    /// Its focus target: its node, or its first delegate that is one.
    pub(crate) fn focus_target(
        &mut self,
    ) -> Option<&mut dyn crate::modifier::FocusTargetModifierNode> {
        crate::delegation::first_of_kind(&mut *self.node, self.own, NodeKinds::FOCUS_TARGET)?
            .as_focus_target()
    }

    /// The name of its focus target: `NO_DELEGATE` for its node, else a delegate's.
    pub(crate) fn focus_target_id(&mut self) -> Option<EntryId> {
        crate::delegation::first_id_of_kind(&mut *self.node, self.own, NodeKinds::FOCUS_TARGET)
    }

    /// What an update of its node auto-invalidates: its kinds, unless it says not to, as Compose's
    /// `autoInvalidateUpdatedNode`, which asks the node and each delegate apart.
    pub(crate) fn updated_kinds(&mut self) -> NodeKinds {
        if self.state & entry_state::DELEGATING != 0 {
            crate::delegation::updated_kinds(&mut *self.node, self.own, NodeKinds::ALL)
        } else if self.node.should_auto_invalidate() {
            self.kinds
        } else {
            NodeKinds::NONE
        }
    }

    /// After its element updated its node: its kinds again if the update delegated or
    /// undelegated. Says if it did (what's left of that runs when the pass ends), and if a layer
    /// came or went with it.
    pub(crate) fn after_update(&mut self) -> DelegatesUpdated {
        let mut updated = DelegatesUpdated::default();
        if self.state & entry_state::DELEGATING == 0 {
            return updated;
        }
        let own = self.own;
        let Some(delegates) = self.node.delegates() else { return updated };
        if delegates.needs_sync() {
            let kinds = own | delegates.refresh_kinds(own);
            updated.delegated = true;
            updated.layers = kinds.any(NodeKinds::LAYOUT) != self.kinds.any(NodeKinds::LAYOUT);
            self.kinds = kinds;
        }
        updated
    }
}

/// For each new item, the old item it keeps (of the same kind, `same_kind` says): a longest
/// common subsequence, as Compose's chain diff (a Myers diff), so nodes keep their state when
/// elements are added or removed around them. Equal prefixes and suffixes are matched without
/// the table.
pub(crate) fn match_kinds<T>(
    old: &[T],
    new: &[T],
    same_kind: impl Fn(&T, &T) -> bool,
) -> smallvec::SmallVec<[Option<usize>; 8]> {
    let mut matched: smallvec::SmallVec<[Option<usize>; 8]> =
        smallvec::SmallVec::from_elem(None, new.len());
    let shortest = old.len().min(new.len());
    let prefix = (0..shortest).take_while(|&i| same_kind(&old[i], &new[i])).count();
    let suffix = (0..shortest - prefix)
        .take_while(|&i| same_kind(&old[old.len() - 1 - i], &new[new.len() - 1 - i]))
        .count();
    for (i, slot) in matched.iter_mut().enumerate().take(prefix) {
        *slot = Some(i);
    }
    for i in 0..suffix {
        matched[new.len() - 1 - i] = Some(old.len() - 1 - i);
    }
    let old_middle = &old[prefix..old.len() - suffix];
    let new_middle = &new[prefix..new.len() - suffix];
    if old_middle.is_empty() || new_middle.is_empty() {
        return matched;
    }
    // lengths[i][j]: the LCS of old_middle[i..] and new_middle[j..].
    let (rows, columns) = (old_middle.len() + 1, new_middle.len() + 1);
    let mut lengths = vec![0u16; rows * columns];
    for i in (0..old_middle.len()).rev() {
        for j in (0..new_middle.len()).rev() {
            lengths[i * columns + j] = if same_kind(&old_middle[i], &new_middle[j]) {
                lengths[(i + 1) * columns + j + 1] + 1
            } else {
                lengths[(i + 1) * columns + j].max(lengths[i * columns + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < old_middle.len() && j < new_middle.len() {
        if same_kind(&old_middle[i], &new_middle[j]) {
            matched[prefix + j] = Some(prefix + i);
            i += 1;
            j += 1;
        } else if lengths[(i + 1) * columns + j] >= lengths[i * columns + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::match_kinds;

    /// The pairs `match_kinds` keeps (old index, new index), and `new` rebuilt from `old` with
    /// them: a kept item must equal the new one, the others are inserted.
    fn diff<T: PartialEq + Clone + std::fmt::Debug>(
        old: &[T],
        new: &[T],
    ) -> (Vec<(usize, usize)>, Vec<T>) {
        let matched = match_kinds(old, new, |a, b| a == b);
        let mut kept = Vec::new();
        let mut result = Vec::new();
        let mut last = None;
        for (j, slot) in matched.iter().enumerate() {
            match *slot {
                Some(i) => {
                    assert_eq!(old[i], new[j], "a kept item is of the new item's kind");
                    assert!(last.is_none_or(|last| last < i), "kept items stay in order");
                    last = Some(i);
                    kept.push((i, j));
                    result.push(old[i].clone());
                }
                None => result.push(new[j].clone()),
            }
        }
        (kept, result)
    }

    /// The length of a longest common subsequence, by the textbook table.
    fn longest_common<T: PartialEq>(old: &[T], new: &[T]) -> usize {
        let mut table = vec![vec![0usize; new.len() + 1]; old.len() + 1];
        for i in 0..old.len() {
            for j in 0..new.len() {
                table[i + 1][j + 1] = if old[i] == new[j] {
                    table[i][j] + 1
                } else {
                    table[i][j + 1].max(table[i + 1][j])
                };
            }
        }
        table[old.len()][new.len()]
    }

    fn string_diff(before: &str, after: &str) {
        let old: Vec<char> = before.chars().collect();
        let new: Vec<char> = after.chars().collect();
        let (kept, result) = diff(&old, &new);
        assert_eq!(result.into_iter().collect::<String>(), after);
        // As few edits as Myers's: as many items kept as can be.
        assert_eq!(kept.len(), longest_common(&old, &new), "{before} -> {after}");
    }

    // Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/MyersDiffTests.kt, testDiffWithRemovesAtStart
    #[test]
    fn removals_at_the_start_keep_the_items_after_them() {
        let (kept, result) = diff(&[0, 0, 0, 0, 0, 1, 2], &[1, 2, 3]);
        assert_eq!(kept, [(5, 0), (6, 1)]);
        assert_eq!(result, [1, 2, 3]);
    }

    // Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/MyersDiffTests.kt, testDiffWithRemovesAtEnd
    #[test]
    fn removals_at_the_end_keep_the_items_before_them() {
        let (kept, result) = diff(&[0, 1, 2, 3, 4], &[0, 1, 2]);
        assert_eq!(kept, [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(result, [0, 1, 2]);
    }

    // Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/MyersDiffTests.kt, testDiffWithInsertsAtEnd
    #[test]
    fn inserts_at_the_end_after_a_removal_at_the_start() {
        let (kept, result) = diff(&[0, 1, 2], &[1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(kept, [(1, 0), (2, 1)]);
        assert_eq!(result, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    // Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/MyersDiffTests.kt, testDiff
    #[test]
    fn a_removal_in_the_middle_keeps_every_other_item() {
        let old = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let new = [0, 1, 2, 3, 4, 6, 7, 8, 9, 10];
        let (kept, result) = diff(&old, &new);
        let expected: Vec<(usize, usize)> =
            (0..5).map(|i| (i, i)).chain((6..11).map(|i| (i, i - 1))).collect();
        assert_eq!(kept, expected);
        assert_eq!(result, new);
    }

    // Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/MyersDiffTests.kt, stringDiff
    #[test]
    fn strings_diff_with_as_few_edits_as_can_be() {
        string_diff("ihfiwjfowijefoiwjfe", "ihfawwjwfowwijefwicwfe");
        string_diff("", "abcde");
        string_diff("abcde", "");
        let (kept, _) = diff(&['a'; 4], &['b'; 4]);
        assert!(
            kept.is_empty(),
            "nothing in common: every item removed, then every new one inserted"
        );
        string_diff("aaaa", "bbbb");
        string_diff("abcd", "bcda");
        string_diff("abc", "abccbacbac");
    }
}
