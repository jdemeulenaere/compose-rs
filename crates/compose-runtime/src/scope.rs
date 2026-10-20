//! Recompose scopes: one per restart group, invalidated by state writes.

use std::marker::PhantomData;

use crate::runtime::{invalidate_scope, request_frames, with_runtime};

/// A generational handle to a recompose scope in the thread's scope arena.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct ScopeId {
    pub index: u32,
    pub generation: u32,
}

impl ScopeId {
    pub const NONE: ScopeId = ScopeId { index: u32::MAX, generation: 0 };

    pub fn is_none(self) -> bool {
        self.index == u32::MAX
    }
}

pub(crate) struct ScopeEntry {
    /// Odd while the scope is live; bumped on allocation and on free.
    pub generation: u32,
    /// The index of the composition the scope belongs to.
    pub composition: u32,
    /// Its restart group in that composition's slot table.
    pub group: u32,
    /// Bumped each time the scope's body runs; a state read records it, so reads from an
    /// earlier run that weren't repeated are recognised as stale.
    pub epoch: u32,
    /// Invalidated: its body runs in the next pass.
    pub invalid: bool,
}

#[derive(Default)]
pub(crate) struct Scopes {
    entries: Vec<ScopeEntry>,
    free: Vec<u32>,
}

impl Scopes {
    pub const fn new() -> Scopes {
        Scopes { entries: Vec::new(), free: Vec::new() }
    }

    pub fn alloc(&mut self, composition: u32, group: u32) -> ScopeId {
        if let Some(index) = self.free.pop() {
            let entry = &mut self.entries[index as usize];
            entry.generation = entry.generation.wrapping_add(1);
            entry.composition = composition;
            entry.group = group;
            entry.epoch = 0;
            entry.invalid = false;
            ScopeId { index, generation: entry.generation }
        } else {
            let index = u32::try_from(self.entries.len()).expect("too many recompose scopes");
            self.entries.push(ScopeEntry {
                generation: 1,
                composition,
                group,
                epoch: 0,
                invalid: false,
            });
            ScopeId { index, generation: 1 }
        }
    }

    pub fn free(&mut self, id: ScopeId) {
        let entry = &mut self.entries[id.index as usize];
        debug_assert_eq!(entry.generation, id.generation, "freeing a stale scope");
        entry.generation = entry.generation.wrapping_add(1);
        entry.invalid = false;
        self.free.push(id.index);
    }

    pub fn get(&self, id: ScopeId) -> Option<&ScopeEntry> {
        self.entries.get(id.index as usize).filter(|entry| entry.generation == id.generation)
    }

    pub fn get_mut(&mut self, id: ScopeId) -> Option<&mut ScopeEntry> {
        self.entries.get_mut(id.index as usize).filter(|entry| entry.generation == id.generation)
    }

    /// Whether a read recorded at `epoch` still belongs to the scope's latest run.
    pub fn is_current(&self, id: ScopeId, epoch: u32) -> bool {
        self.get(id).is_some_and(|entry| entry.epoch == epoch)
    }

    pub fn live_count(&self) -> usize {
        self.entries.len() - self.free.len()
    }
}

/// The recompose scope of a restartable composable, as Compose's `RecomposeScope`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RecomposeScope {
    pub(crate) id: ScopeId,
    pub(crate) _not_send: PhantomData<*const ()>,
}

impl RecomposeScope {
    /// Schedules the scope's composable to run again in the next pass. Does nothing if the scope
    /// has left the composition.
    pub fn invalidate(&self) {
        let requested = with_runtime(|runtime| invalidate_scope(runtime, self.id));
        if requested {
            request_frames();
        }
    }

    /// Whether the scope is still in a composition.
    pub fn is_valid(&self) -> bool {
        with_runtime(|runtime| runtime.scopes.get(self.id).is_some())
    }
}
