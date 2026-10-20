//! The slot table: an arena tree of keyed groups, each with its own slots.
//!
//! Children are a singly linked list in composition order. During a pass, a parent's frame
//! splits its children into the ones placed so far and the old ones not yet matched; see
//! `composer.rs`.

use std::any::{Any, TypeId};
use std::rc::Rc;

use crate::applier::NodeRef;
use crate::remember::ErasedObserver;
use crate::scope::ScopeId;

/// No group.
pub(crate) const NONE: u32 = u32::MAX;

/// The group at index 0 of every slot table: the parent of the composition's content.
pub(crate) const SENTINEL: u32 = 0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GroupKind {
    /// The sentinel at the top of a composition.
    Root,
    /// A restartable composable: has a recompose scope, kept arguments and a restart function.
    Restart,
    /// A region (a branch, a loop) or a non-restartable composable.
    Replace,
    /// A keyed group (`key(…)`): matched by its data key too.
    Movable,
    /// A group that emits one node.
    Node,
    /// A free entry of the arena.
    Free,
}

/// The type a slot holds. Each value records its `TypeId` through `dyn Any`, so a key collision
/// or a mismatched group recreates a slot instead of misreading it.
pub(crate) enum SlotValue {
    Plain(Rc<dyn Any>),
    Observer(Rc<dyn ErasedObserver>),
}

impl SlotValue {
    pub fn value_type_id(&self) -> TypeId {
        match self {
            SlotValue::Plain(value) => Any::type_id(&**value),
            SlotValue::Observer(observer) => Any::type_id(observer.as_any()),
        }
    }

    pub fn to_any(&self) -> Rc<dyn Any> {
        match self {
            SlotValue::Plain(value) => value.clone(),
            SlotValue::Observer(observer) => observer.clone().into_any(),
        }
    }
}

pub(crate) struct Slot {
    pub value: SlotValue,
    pub type_name: &'static str,
}

pub(crate) struct Group {
    pub key: u64,
    pub data_key: u64,
    pub kind: GroupKind,
    /// A descendant restart group's scope is invalid: the next pass walks into this group.
    pub dirty: bool,
    pub parent: u32,
    pub first_child: u32,
    pub next_sibling: u32,
    /// The number of nodes this group contributes to its nearest node ancestor: 1 for a node
    /// group, the sum of its children's counts otherwise.
    pub node_count: u32,
    /// The node of a node group.
    pub node: NodeRef,
    /// The recompose scope of a restart group.
    pub scope: ScopeId,
    pub slots: Vec<Slot>,
    /// The kept arguments of a restart group, read back by its restart function.
    pub kept: Option<Rc<dyn Any>>,
    pub restart: Option<fn(&dyn Any)>,
    /// The identity of a restart group's composable: the `TypeId` of its restart closure.
    pub identity: Option<TypeId>,
}

pub(crate) struct SlotTable {
    pub groups: Vec<Group>,
    free: Vec<u32>,
}

impl SlotTable {
    pub fn new() -> SlotTable {
        let mut table = SlotTable { groups: Vec::new(), free: Vec::new() };
        let sentinel = table.alloc(GroupKind::Root, 0, 0, NONE);
        debug_assert_eq!(sentinel, SENTINEL);
        table
    }

    pub fn alloc(&mut self, kind: GroupKind, key: u64, data_key: u64, parent: u32) -> u32 {
        if let Some(index) = self.free.pop() {
            let group = &mut self.groups[index as usize];
            debug_assert!(group.kind == GroupKind::Free && group.slots.is_empty());
            group.key = key;
            group.data_key = data_key;
            group.kind = kind;
            group.dirty = false;
            group.parent = parent;
            group.first_child = NONE;
            group.next_sibling = NONE;
            group.node_count = 0;
            group.node = NodeRef::NONE;
            group.scope = ScopeId::NONE;
            group.identity = None;
            index
        } else {
            let index = u32::try_from(self.groups.len()).expect("too many groups");
            assert!(index != NONE, "too many groups");
            self.groups.push(Group {
                key,
                data_key,
                kind,
                dirty: false,
                parent,
                first_child: NONE,
                next_sibling: NONE,
                node_count: 0,
                node: NodeRef::NONE,
                scope: ScopeId::NONE,
                slots: Vec::new(),
                kept: None,
                restart: None,
                identity: None,
            });
            index
        }
    }

    /// Returns a group whose slots, kept arguments and scope were already taken to the free
    /// list. Its slot vector keeps its capacity for the next group.
    pub fn free(&mut self, index: u32) {
        let group = &mut self.groups[index as usize];
        debug_assert!(group.slots.is_empty() && group.kept.is_none() && group.scope.is_none());
        group.kind = GroupKind::Free;
        group.restart = None;
        group.dirty = false;
        self.free.push(index);
    }

    /// Links `group` after `after` among `parent`'s children, or first if `after` is `NONE`.
    /// The caller has set `group.next_sibling` already.
    pub fn link_after(&mut self, parent: u32, after: u32, group: u32) {
        if after == NONE {
            self.groups[parent as usize].first_child = group;
        } else {
            self.groups[after as usize].next_sibling = group;
        }
    }

    /// Ends `parent`'s children after `after` with `next`.
    pub fn set_next(&mut self, parent: u32, after: u32, next: u32) {
        self.link_after(parent, after, next);
    }

    pub fn live_count(&self) -> usize {
        self.groups.len() - self.free.len()
    }
}

impl std::ops::Index<u32> for SlotTable {
    type Output = Group;

    fn index(&self, index: u32) -> &Group {
        &self.groups[index as usize]
    }
}

impl std::ops::IndexMut<u32> for SlotTable {
    fn index_mut(&mut self, index: u32) -> &mut Group {
        &mut self.groups[index as usize]
    }
}
