//! Modifier nodes observing state reads of their own, as Compose's `ObserverModifierNode` and
//! `observeReads` (docs/parity/design/modifier-nodes.md §3.9, step MN-7).

use std::collections::HashMap;
use std::rc::{Rc, Weak};

use compose_runtime::platform::ReadObserver;
use smallvec::SmallVec;

use crate::node_chain::{EntryId, entry_state};
use crate::node_context::NodeContext;
use crate::tree::{NodeId, UiTree};

/// A node told when state read in its `NodeContext::observe_reads` block changes, as
/// `ObserverModifierNode`: `on_observed_reads_changed` is called once after such a write, where
/// the tree next applies its nodes' requests (a frame's start, before each layout), if the node
/// is attached then. To keep observing, observe again (from the callback, usually).
pub trait ObserverModifierNode {
    fn on_observed_reads_changed(&mut self, context: &mut NodeContext<'_>);
}

/// The reads modifier nodes observe (`NodeContext::observe_reads`): each observation under a key
/// of its own, so one that fired, or that a new one replaced, is forgotten with its reads.
#[derive(Default)]
pub(crate) struct NodeObservations {
    /// Made on first use.
    observer: Option<Rc<ReadObserver>>,
    /// Each observation's entry.
    entries: HashMap<u64, (NodeId, EntryId)>,
    /// Each observing entry's current observation.
    current: HashMap<EntryId, u64>,
    next_key: u64,
}

impl UiTree {
    /// Runs `block` for `entry` of `node`, observing what it reads in place of what its last
    /// observation read.
    pub(crate) fn observe_entry_reads<R>(
        &mut self,
        node: NodeId,
        entry: EntryId,
        block: impl FnOnce() -> R,
    ) -> R {
        let requests = Rc::downgrade(&self.focus_requests);
        let observations = &mut self.observations;
        let observer = observations
            .observer
            .get_or_insert_with(|| {
                let requests: Weak<crate::focus_requester::FocusRequests> = requests;
                // A write asks for a frame, whose start tells the nodes.
                Rc::new(ReadObserver::new(move || {
                    if let Some(requests) = requests.upgrade() {
                        requests.wake();
                    }
                }))
            })
            .clone();
        if let Some(previous) = observations.current.remove(&entry) {
            observations.entries.remove(&previous);
            observer.forget(previous);
        }
        observations.next_key += 1;
        let key = observations.next_key;
        observations.entries.insert(key, (node, entry));
        observations.current.insert(entry, key);
        observer.observe(key, block)
    }

    /// Forgets what `entry` observed: it's dropped.
    pub(crate) fn forget_entry_reads(&mut self, entry: EntryId, state: u8) {
        if state & entry_state::OBSERVES == 0 {
            return;
        }
        let observations = &mut self.observations;
        if let Some(key) = observations.current.remove(&entry) {
            observations.entries.remove(&key);
            if let Some(observer) = &observations.observer {
                observer.forget(key);
            }
        }
    }

    /// Tells the nodes whose observed reads a write changed, once each: those still attached,
    /// through `ObserverModifierNode::on_observed_reads_changed`. Each observation fired is
    /// forgotten, as Compose's. What the callbacks' own writes change is told the next time
    /// (the next layout round or frame, which they ask for).
    pub(crate) fn notify_observer_nodes(&mut self) {
        let Some(observer) = self.observations.observer.clone() else { return };
        let mut fired: SmallVec<[(u64, NodeId, EntryId); 4]> = SmallVec::new();
        observer.drain_invalidated(|key| {
            if let Some((node, entry)) = self.observations.entries.remove(&key) {
                self.observations.current.remove(&entry);
                fired.push((key, node, entry));
            }
        });
        for (key, node, entry) in fired {
            // Forgotten: a later write tells nothing until it observes again.
            observer.forget(key);
            let Some(index) = self.entry_index(node, entry) else { continue };
            if self.entry_or_delegate_state(node, index, entry) & entry_state::ATTACHED == 0 {
                continue;
            }
            self.with_entry_or_delegate(node, index, entry, |modifier_node, context| {
                if let Some(observer) = modifier_node.as_observer() {
                    observer.on_observed_reads_changed(context);
                }
            });
        }
    }
}
