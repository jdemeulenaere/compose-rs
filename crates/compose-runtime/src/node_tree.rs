//! A built-in node tree: a generic arena of nodes and its [`Applier`]. Runtime tests use it, and
//! so can any composition whose nodes need no more than a value and children.

use std::fmt::{self, Write as _};

use crate::applier::{Applier, NodeRef};

const NO_PARENT: u32 = u32::MAX;

struct TreeEntry<N> {
    /// Odd while live; bumped on allocation and on release.
    generation: u32,
    value: Option<N>,
    parent: u32,
    children: Vec<u32>,
}

/// A tree of `N` values. Its root holds no value.
pub struct NodeTree<N> {
    entries: Vec<TreeEntry<N>>,
    free: Vec<u32>,
}

impl<N> Default for NodeTree<N> {
    fn default() -> Self {
        NodeTree::new()
    }
}

fn pack(index: u32, generation: u32) -> NodeRef {
    NodeRef::new((u64::from(generation) << 32) | u64::from(index))
}

fn unpack(node: NodeRef) -> (u32, u32) {
    (node.bits() as u32, (node.bits() >> 32) as u32)
}

impl<N> NodeTree<N> {
    pub fn new() -> NodeTree<N> {
        NodeTree {
            entries: vec![TreeEntry {
                generation: 1,
                value: None,
                parent: NO_PARENT,
                children: Vec::new(),
            }],
            free: Vec::new(),
        }
    }

    pub fn root(&self) -> NodeRef {
        pack(0, 1)
    }

    fn index(&self, node: NodeRef) -> u32 {
        let (index, generation) = unpack(node);
        match self.entries.get(index as usize) {
            Some(entry) if entry.generation == generation => index,
            _ => panic!("compose-runtime: NodeTree: stale or foreign node {node:?}"),
        }
    }

    fn reference(&self, index: u32) -> NodeRef {
        pack(index, self.entries[index as usize].generation)
    }

    /// The node's value; `None` for the root or a released node.
    pub fn get(&self, node: NodeRef) -> Option<&N> {
        let (index, generation) = unpack(node);
        self.entries
            .get(index as usize)
            .filter(|entry| entry.generation == generation)
            .and_then(|entry| entry.value.as_ref())
    }

    pub fn get_mut(&mut self, node: NodeRef) -> Option<&mut N> {
        let (index, generation) = unpack(node);
        self.entries
            .get_mut(index as usize)
            .filter(|entry| entry.generation == generation)
            .and_then(|entry| entry.value.as_mut())
    }

    /// Whether the node is live (inserted and not released).
    pub fn contains(&self, node: NodeRef) -> bool {
        let (index, generation) = unpack(node);
        self.entries.get(index as usize).is_some_and(|entry| entry.generation == generation)
    }

    pub fn children(&self, node: NodeRef) -> impl Iterator<Item = NodeRef> + '_ {
        let index = self.index(node);
        self.entries[index as usize].children.iter().map(|&child| self.reference(child))
    }

    pub fn parent(&self, node: NodeRef) -> Option<NodeRef> {
        let parent = self.entries[self.index(node) as usize].parent;
        (parent != NO_PARENT).then(|| self.reference(parent))
    }

    /// The number of live nodes, not counting the root.
    pub fn len(&self) -> usize {
        self.entries.len() - self.free.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn alloc(&mut self, value: N, parent: u32) -> u32 {
        if let Some(index) = self.free.pop() {
            let entry = &mut self.entries[index as usize];
            entry.generation = entry.generation.wrapping_add(1);
            entry.value = Some(value);
            entry.parent = parent;
            index
        } else {
            let index = u32::try_from(self.entries.len()).expect("too many nodes");
            self.entries.push(TreeEntry {
                generation: 1,
                value: Some(value),
                parent,
                children: Vec::new(),
            });
            index
        }
    }

    fn check_range(&self, parent: u32, index: usize, count: usize) {
        let len = self.entries[parent as usize].children.len();
        assert!(
            index.checked_add(count).is_some_and(|end| end <= len),
            "compose-runtime: NodeTree: range {index}..{index}+{count} outside {len} children"
        );
    }
}

/// Compose's `MutableList.move(from, to, count)`: `to` is an index before the move.
fn move_range<T>(list: &mut [T], from: usize, to: usize, count: usize) {
    let destination = if from > to { to } else { to - count };
    assert!(destination + count <= list.len(), "compose-runtime: NodeTree: move past the end");
    if destination < from {
        list[destination..from + count].rotate_right(count);
    } else if destination > from {
        list[from..destination + count].rotate_left(count);
    }
}

impl<N: fmt::Debug> NodeTree<N> {
    /// An indented dump of the tree below the root, one `{:?}` value per line.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        let mut stack: Vec<(u32, usize)> =
            self.entries[0].children.iter().rev().map(|&child| (child, 0)).collect();
        while let Some((index, depth)) = stack.pop() {
            let entry = &self.entries[index as usize];
            let _ =
                writeln!(out, "{}{:?}", "  ".repeat(depth), entry.value.as_ref().expect("live"));
            stack.extend(entry.children.iter().rev().map(|&child| (child, depth + 1)));
        }
        out
    }
}

impl<N: 'static> Applier for NodeTree<N> {
    type Node = N;

    fn root(&self) -> NodeRef {
        NodeTree::root(self)
    }

    fn insert(&mut self, parent: NodeRef, index: usize, node: N) -> NodeRef {
        let parent = self.index(parent);
        let len = self.entries[parent as usize].children.len();
        assert!(index <= len, "compose-runtime: NodeTree: insert at {index} past {len} children");
        let child = self.alloc(node, parent);
        self.entries[parent as usize].children.insert(index, child);
        self.reference(child)
    }

    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        let parent = self.index(parent);
        self.check_range(parent, index, count);
        let mut removed = std::mem::take(&mut self.entries[parent as usize].children);
        for &child in &removed[index..index + count] {
            self.entries[child as usize].parent = NO_PARENT;
        }
        removed.drain(index..index + count);
        self.entries[parent as usize].children = removed;
    }

    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        let parent = self.index(parent);
        self.check_range(parent, from, count);
        let children = &mut self.entries[parent as usize].children;
        move_range(children, from, to, count);
    }

    fn release(&mut self, node: NodeRef) {
        let index = self.index(node);
        assert!(index != 0, "compose-runtime: NodeTree: the root can't be released");
        let parent = self.entries[index as usize].parent;
        if parent != NO_PARENT {
            self.entries[parent as usize].children.retain(|&child| child != index);
        }
        let mut stack = vec![index];
        let mut values = Vec::new();
        while let Some(index) = stack.pop() {
            let entry = &mut self.entries[index as usize];
            stack.append(&mut entry.children);
            entry.generation = entry.generation.wrapping_add(1);
            entry.parent = NO_PARENT;
            values.push(entry.value.take());
            self.free.push(index);
        }
        // Values drop last, with the tree consistent, in case their Drop looks at it.
        drop(values);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compose's `MutableList.move`, transcribed.
    fn compose_move(list: &mut Vec<u32>, from: usize, to: usize, count: usize) {
        let destination = if from > to { to } else { to - count };
        let moved: Vec<u32> = list.drain(from..from + count).collect();
        for (offset, item) in moved.into_iter().enumerate() {
            list.insert(destination + offset, item);
        }
    }

    #[test]
    fn moves_match_compose() {
        for len in 1..7usize {
            for count in 1..=len {
                for from in 0..=len - count {
                    for to in 0..=len {
                        let destination = if from > to { to } else { to.wrapping_sub(count) };
                        if to < count && from <= to || destination + count > len {
                            continue;
                        }
                        let mut expected: Vec<u32> = (0..len as u32).collect();
                        compose_move(&mut expected, from, to, count);
                        let mut actual: Vec<u32> = (0..len as u32).collect();
                        move_range(&mut actual, from, to, count);
                        assert_eq!(actual, expected, "len {len} from {from} to {to} count {count}");
                    }
                }
            }
        }
    }

    #[test]
    fn insert_remove_release() {
        let mut tree = NodeTree::<u32>::new();
        let root = tree.root();
        let a = Applier::insert(&mut tree, root, 0, 1);
        let b = Applier::insert(&mut tree, a, 0, 2);
        Applier::insert(&mut tree, root, 1, 3);
        assert_eq!(tree.dump(), "1\n  2\n3\n");
        Applier::remove(&mut tree, root, 0, 1);
        assert_eq!(tree.dump(), "3\n");
        assert!(tree.contains(b), "removed nodes live until released");
        Applier::release(&mut tree, a);
        assert!(!tree.contains(a) && !tree.contains(b));
        assert_eq!(tree.len(), 1);
    }
}
