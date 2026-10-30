//! A differential fuzz of snapshots: random operations on the real thing and on a small model of
//! its rules (docs/parity/design/snapshots.md §3.4-§3.9, D769-D790), seeded and bounded. The model
//! keeps every record and reclaims nothing, as naively as the rules read: a value read, an apply's
//! outcome and what the apply observers hear must agree, whatever reclamation dropped. Then an
//! invariant with no model: a derived state reads as its calculation would, in any snapshot.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::platform::state_records;
use compose_runtime::snapshots::{MutableSnapshot, Snapshot, StateObjectId};
use compose_runtime::*;

struct Random(u64);

impl Random {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) % bound.max(1) as u64) as usize
    }
}

#[derive(Clone, Copy)]
struct Add;

impl MutationPolicy<i32> for Add {
    fn equivalent(a: &i32, b: &i32) -> bool {
        a == b
    }
    fn merge(previous: &i32, current: &i32, applied: &i32) -> Option<i32> {
        Some(current + applied - previous)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Policy {
    Structural,
    Never,
    Add,
}

#[derive(Clone)]
struct ModelRecord {
    id: u64,
    value: i32,
}

struct ModelState {
    records: Vec<ModelRecord>,
    policy: Policy,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    ReadOnly,
    Mutable,
    NestedReadOnly,
    NestedMutable,
}

struct ModelSnapshot {
    id: u64,
    invalid: BTreeSet<u64>,
    previous: BTreeSet<u64>,
    modified: BTreeSet<usize>,
    parent: Option<usize>,
    kind: Kind,
    /// Itself and the nested ones active.
    nested: u32,
    applied: bool,
    disposed: bool,
    deactivated: bool,
}

impl ModelSnapshot {
    fn read_only(&self) -> bool {
        matches!(self.kind, Kind::ReadOnly | Kind::NestedReadOnly)
    }
    fn active(&self) -> bool {
        !self.applied && !self.disposed
    }
}

const PREEXISTING: u64 = 1;

/// The rules, on plain data.
struct Model {
    next: u64,
    global: u64,
    open: BTreeSet<u64>,
    states: Vec<ModelState>,
    snapshots: Vec<ModelSnapshot>,
    global_modified: BTreeSet<usize>,
    /// What apply observers heard since it was last looked at.
    heard: Vec<BTreeSet<usize>>,
}

fn valid(id: u64, view: u64, invalid: &BTreeSet<u64>) -> bool {
    id != 0 && id <= view && !invalid.contains(&id)
}

fn readable(state: &ModelState, view: u64, invalid: &BTreeSet<u64>) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (index, record) in state.records.iter().enumerate() {
        if valid(record.id, view, invalid)
            && best.is_none_or(|best| state.records[best].id < record.id)
        {
            best = Some(index);
        }
    }
    best
}

impl Model {
    fn new() -> Model {
        Model {
            next: 3,
            global: 2,
            open: BTreeSet::new(),
            states: Vec::new(),
            snapshots: Vec::new(),
            global_modified: BTreeSet::new(),
            heard: Vec::new(),
        }
    }

    fn take_id(&mut self) -> u64 {
        self.next += 1;
        self.next - 1
    }

    /// The view of a context: `None` is the global snapshot.
    fn view(&self, context: Option<usize>) -> (u64, BTreeSet<u64>) {
        match context {
            Some(index) => (self.snapshots[index].id, self.snapshots[index].invalid.clone()),
            None => (self.global, self.open.clone()),
        }
    }

    fn read(&self, state: usize, context: Option<usize>) -> Option<i32> {
        let (view, invalid) = self.view(context);
        let state = &self.states[state];
        readable(state, view, &invalid).map(|index| state.records[index].value)
    }

    fn new_state(&mut self, value: i32, policy: Policy, context: Option<usize>) {
        let (view, _) = self.view(context);
        let mut records = vec![ModelRecord { id: view, value }];
        if context.is_some() {
            records.push(ModelRecord { id: PREEXISTING, value });
        }
        self.states.push(ModelState { records, policy });
    }

    /// A write, the caller having checked it can read the state.
    fn write(&mut self, state: usize, value: i32, context: Option<usize>) {
        let (view, invalid) = self.view(context);
        let model = &mut self.states[state];
        let index = readable(model, view, &invalid).expect("checked");
        let current = model.records[index].value;
        if model.policy != Policy::Never && current == value {
            return;
        }
        if model.records[index].id == view {
            model.records[index].value = value;
            return;
        }
        model.records.push(ModelRecord { id: view, value });
        match context {
            Some(index) => {
                self.snapshots[index].modified.insert(state);
            }
            None => {
                self.global_modified.insert(state);
            }
        }
    }

    /// The global snapshot advances, telling what changed, as `send(always)`.
    fn send(&mut self, always: bool) {
        if !self.global_modified.is_empty() {
            self.heard.push(std::mem::take(&mut self.global_modified));
        } else if !always {
            return;
        }
        self.global = self.take_id();
    }

    fn take_root(&mut self, kind: Kind) -> usize {
        let id = self.take_id();
        let invalid = self.open.clone();
        self.open.insert(id);
        self.snapshots.push(ModelSnapshot {
            id,
            invalid,
            previous: BTreeSet::new(),
            modified: BTreeSet::new(),
            parent: None,
            kind,
            nested: 1,
            applied: false,
            disposed: false,
            deactivated: false,
        });
        self.send(true);
        self.snapshots.len() - 1
    }

    /// `advance`'s tail.
    fn new_id(&mut self, index: usize, previous: u64) {
        let id = self.take_id();
        self.open.insert(id);
        let snapshot = &mut self.snapshots[index];
        snapshot.id = id;
        snapshot.invalid.extend(previous + 1..id);
    }

    fn advance(&mut self, index: usize) {
        let previous = self.snapshots[index].id;
        self.snapshots[index].previous.insert(previous);
        if self.snapshots[index].active() {
            self.new_id(index, previous);
        }
    }

    fn take_nested(&mut self, parent: usize, kind: Kind) -> usize {
        let parent_id = self.snapshots[parent].id;
        if self.snapshots[parent].read_only() {
            let owner = match self.snapshots[parent].kind {
                Kind::ReadOnly => parent,
                _ => self.snapshots[parent].parent.expect("a parent"),
            };
            self.snapshots[owner].nested += 1;
            let invalid = self.snapshots[parent].invalid.clone();
            self.snapshots.push(ModelSnapshot {
                id: parent_id,
                invalid,
                previous: BTreeSet::new(),
                modified: BTreeSet::new(),
                parent: Some(owner),
                kind: Kind::NestedReadOnly,
                nested: 1,
                applied: false,
                disposed: false,
                deactivated: false,
            });
            return self.snapshots.len() - 1;
        }
        self.snapshots[parent].previous.insert(parent_id);
        let id = self.take_id();
        self.open.insert(id);
        let mut invalid = self.snapshots[parent].invalid.clone();
        if kind == Kind::NestedMutable {
            self.snapshots[parent].invalid.insert(id);
        }
        invalid.extend(parent_id + 1..id);
        self.snapshots[parent].nested += 1;
        self.snapshots.push(ModelSnapshot {
            id,
            invalid,
            previous: BTreeSet::new(),
            modified: BTreeSet::new(),
            parent: Some(parent),
            kind,
            nested: 1,
            applied: false,
            disposed: false,
            deactivated: false,
        });
        let child = self.snapshots.len() - 1;
        if self.snapshots[parent].active() {
            self.new_id(parent, parent_id);
            if kind == Kind::NestedMutable {
                let mut ancestor = self.snapshots[parent].parent;
                while let Some(index) = ancestor {
                    if !self.snapshots[index].read_only() && self.snapshots[index].active() {
                        let previous = self.snapshots[index].id;
                        self.snapshots[index].previous.insert(previous);
                        self.new_id(index, previous);
                    }
                    ancestor = self.snapshots[index].parent;
                }
                self.global = self.take_id();
            }
        }
        child
    }

    fn inner_apply(&mut self, index: usize, current_view: u64, current: BTreeSet<u64>) -> bool {
        let (id, invalid, previous, modified) = {
            let snapshot = &self.snapshots[index];
            (
                snapshot.id,
                snapshot.invalid.clone(),
                snapshot.previous.clone(),
                snapshot.modified.clone(),
            )
        };
        let mut start = invalid.clone();
        start.insert(id);
        start.extend(previous.iter().copied());
        let mut merged = Vec::new();
        let mut stands = Vec::new();
        for &state in &modified {
            let model = &self.states[state];
            let Some(now) = readable(model, current_view, &current) else { continue };
            let Some(before) = readable(model, id, &start) else { continue };
            if model.records[before].id == PREEXISTING || now == before {
                continue;
            }
            let applied = readable(model, id, &invalid).expect("its own write");
            let (before, now, applied) = (
                model.records[before].value,
                model.records[now].value,
                model.records[applied].value,
            );
            if model.policy != Policy::Never && now == applied {
                stands.push(state);
            } else if model.policy == Policy::Add {
                merged.push((state, now + applied - before));
            } else {
                return false;
            }
        }
        let alone = self.snapshots[index].nested == 1;
        for &state in &stands {
            if alone {
                self.states[state]
                    .records
                    .retain(|record| record.id != id && !previous.contains(&record.id));
            }
            self.snapshots[index].modified.remove(&state);
        }
        if !merged.is_empty() {
            self.advance(index);
            let id = self.snapshots[index].id;
            for (state, value) in merged {
                self.states[state].records.push(ModelRecord { id, value });
            }
        }
        true
    }

    fn close(&mut self, index: usize) {
        let id = self.snapshots[index].id;
        let previous = self.snapshots[index].previous.clone();
        self.open.remove(&id);
        for id in previous {
            self.open.remove(&id);
        }
    }

    fn apply(&mut self, index: usize) -> bool {
        if self.snapshots[index].kind == Kind::NestedMutable {
            return self.apply_nested(index);
        }
        let (next, open) = (self.next, self.open.clone());
        if !self.inner_apply(index, next, open) {
            return false;
        }
        self.close(index);
        self.snapshots[index].applied = true;
        let modified = std::mem::take(&mut self.snapshots[index].modified);
        if !self.global_modified.is_empty() {
            self.heard.push(std::mem::take(&mut self.global_modified));
        }
        if !modified.is_empty() {
            self.heard.push(modified);
        }
        self.global = self.take_id();
        true
    }

    fn apply_nested(&mut self, index: usize) -> bool {
        let parent = self.snapshots[index].parent.expect("a parent");
        if !self.snapshots[parent].active() {
            return false;
        }
        if self.snapshots[index].modified.is_empty() {
            self.close(index);
        } else {
            let (view, invalid) = self.view(Some(parent));
            if !self.inner_apply(index, view, invalid) {
                return false;
            }
            let modified = std::mem::take(&mut self.snapshots[index].modified);
            self.snapshots[parent].modified.extend(modified);
        }
        let id = self.snapshots[index].id;
        if self.snapshots[parent].id < id {
            self.advance(parent);
        }
        let previous = self.snapshots[index].previous.clone();
        let snapshot = &mut self.snapshots[parent];
        snapshot.invalid.remove(&id);
        for id in &previous {
            snapshot.invalid.remove(id);
        }
        snapshot.previous.insert(id);
        snapshot.previous.extend(previous);
        self.snapshots[index].applied = true;
        self.deactivate(index);
        true
    }

    fn deactivate(&mut self, index: usize) {
        if !std::mem::replace(&mut self.snapshots[index].deactivated, true)
            && let Some(parent) = self.snapshots[index].parent
        {
            self.nested_deactivated(parent);
        }
    }

    fn nested_deactivated(&mut self, index: usize) {
        self.snapshots[index].nested = self.snapshots[index].nested.saturating_sub(1);
        if self.snapshots[index].nested != 0 {
            return;
        }
        match self.snapshots[index].kind {
            Kind::ReadOnly => self.close_read_only(index),
            Kind::NestedReadOnly => {}
            Kind::Mutable | Kind::NestedMutable => {
                if !self.snapshots[index].applied {
                    let (id, previous, modified) = {
                        let snapshot = &mut self.snapshots[index];
                        (
                            snapshot.id,
                            snapshot.previous.clone(),
                            std::mem::take(&mut snapshot.modified),
                        )
                    };
                    for state in modified {
                        self.states[state]
                            .records
                            .retain(|record| record.id != id && !previous.contains(&record.id));
                    }
                    self.close(index);
                }
                if self.snapshots[index].kind == Kind::NestedMutable {
                    self.deactivate(index);
                }
            }
        }
    }

    /// A read-only snapshot's id closes with the last one sharing it.
    fn close_read_only(&mut self, index: usize) {
        let id = self.snapshots[index].id;
        let shared = self
            .snapshots
            .iter()
            .any(|other| other.read_only() && other.id == id && !other.disposed);
        if !shared {
            self.open.remove(&id);
        }
    }

    fn dispose(&mut self, index: usize) {
        if self.snapshots[index].disposed {
            return;
        }
        self.snapshots[index].disposed = true;
        match self.snapshots[index].kind {
            Kind::ReadOnly => self.nested_deactivated(index),
            Kind::NestedReadOnly => {
                let parent = self.snapshots[index].parent.expect("a parent");
                if self.snapshots[index].id != self.snapshots[parent].id {
                    self.close_read_only(index);
                }
                self.nested_deactivated(parent);
            }
            Kind::Mutable => self.nested_deactivated(index),
            Kind::NestedMutable => {
                self.nested_deactivated(index);
                self.deactivate(index);
            }
        }
    }

    fn notify_initialized(&mut self, context: Option<usize>) {
        match context {
            None => self.send(true),
            Some(index) => {
                if !self.snapshots[index].read_only() && self.snapshots[index].active() {
                    self.advance(index);
                }
            }
        }
    }

    /// Who reads may go through: one disposed or applied has given up its pin.
    fn usable(&self, index: usize) -> bool {
        self.snapshots[index].active()
    }
}

/// A real snapshot, read-only or mutable.
enum Real {
    ReadOnly(Snapshot),
    Mutable(MutableSnapshot),
}

impl Real {
    fn snapshot(&self) -> &Snapshot {
        match self {
            Real::ReadOnly(snapshot) => snapshot,
            Real::Mutable(snapshot) => snapshot,
        }
    }
}

fn in_context<R>(reals: &[Real], context: Option<usize>, block: impl FnOnce() -> R) -> R {
    match context {
        Some(index) => reals[index].snapshot().enter(block),
        None => block(),
    }
}

fn made(policy: Policy, value: i32) -> OwnedState<i32> {
    match policy {
        Policy::Structural => mutable_state_of(value),
        Policy::Never => mutable_state_of_with(value, never_equal_policy()),
        Policy::Add => mutable_state_of_with(value, Add),
    }
}

fn fuzz(seed: u64, operations: usize) {
    let mut random = Random(seed);
    let mut model = Model::new();
    let mut reals: Vec<Real> = Vec::new();
    let mut states: Vec<OwnedState<i32>> = Vec::new();
    let heard: Rc<RefCell<Vec<HashSet<StateObjectId>>>> = Rc::default();
    let log = heard.clone();
    let _observer = Snapshot::register_apply_observer(move |changed, _| {
        log.borrow_mut().push(changed.iter().collect());
    });
    let check_heard = |model: &mut Model, states: &[OwnedState<i32>], at: usize| {
        let expected: Vec<HashSet<StateObjectId>> = model
            .heard
            .drain(..)
            .map(|set| set.into_iter().map(|index| states[index].state_object_id()).collect())
            .collect();
        assert_eq!(*heard.borrow(), expected, "seed {seed}, operation {at}: what was heard");
        heard.borrow_mut().clear();
    };
    for policy in [Policy::Structural, Policy::Add, Policy::Never] {
        model.new_state(0, policy, None);
        states.push(made(policy, 0));
    }
    for at in 0..operations {
        let usable: Vec<usize> = (0..reals.len()).filter(|&index| model.usable(index)).collect();
        let context = if usable.is_empty() || random.next(3) == 0 {
            None
        } else {
            Some(usable[random.next(usable.len())])
        };
        let writable = context.is_none_or(|index| !model.snapshots[index].read_only());
        let state = random.next(model.states.len());
        let value = random.next(4) as i32;
        match random.next(16) {
            0..=4 => {
                // A read: the value, or Compose's error if it sees no record.
                let expected = model.read(state, context);
                let handle = states[state].handle();
                let got =
                    catch_unwind(AssertUnwindSafe(|| in_context(&reals, context, || handle.get())))
                        .ok();
                assert_eq!(got, expected, "seed {seed}, operation {at}: read of state {state}");
            }
            5..=8 if writable && model.read(state, context).is_some() => {
                model.write(state, value, context);
                let handle = states[state].handle();
                in_context(&reals, context, || handle.set(value));
            }
            9 if model.states.len() < 6 => {
                let policy = [Policy::Structural, Policy::Add, Policy::Never][random.next(3)];
                model.new_state(value, policy, context);
                states.push(in_context(&reals, context, || made(policy, value)));
            }
            10 if reals.len() < 40 => {
                // A snapshot of a context.
                match (context, random.next(2) == 0 && writable) {
                    (None, true) => {
                        model.take_root(Kind::Mutable);
                        reals.push(Real::Mutable(Snapshot::take_mutable_snapshot(None, None)));
                    }
                    (None, false) => {
                        model.take_root(Kind::ReadOnly);
                        reals.push(Real::ReadOnly(Snapshot::take_snapshot(None)));
                    }
                    (Some(index), true) => {
                        model.take_nested(index, Kind::NestedMutable);
                        let parent = reals[index].snapshot().as_mutable().expect("writable");
                        reals.push(Real::Mutable(parent.take_nested_mutable_snapshot(None, None)));
                    }
                    (Some(index), false) => {
                        model.take_nested(index, Kind::NestedReadOnly);
                        reals.push(Real::ReadOnly(
                            reals[index].snapshot().take_nested_snapshot(None),
                        ));
                    }
                }
            }
            11 => {
                if let Some(index) = context
                    && let Real::Mutable(snapshot) = &reals[index]
                {
                    let expected = model.apply(index);
                    assert_eq!(
                        snapshot.apply().succeeded(),
                        expected,
                        "seed {seed}, operation {at}: apply of {index}"
                    );
                }
            }
            12 => {
                if let Some(index) = context {
                    model.dispose(index);
                    reals[index].snapshot().dispose();
                }
            }
            13 => {
                model.send(false);
                Snapshot::send_apply_notifications();
            }
            14 => {
                model.notify_initialized(context);
                in_context(&reals, context, Snapshot::notify_objects_initialized);
            }
            _ => {}
        }
        check_heard(&mut model, &states, at);
    }
    // All gone: a read of each as the model says, and one record each, on the fast path again.
    for (index, real) in reals.iter().enumerate() {
        model.dispose(index);
        real.snapshot().dispose();
    }
    model.send(false);
    Snapshot::send_apply_notifications();
    check_heard(&mut model, &states, operations);
    assert_eq!(Snapshot::open_snapshot_count(), 1, "seed {seed}: left open");
    for (index, state) in states.iter().enumerate() {
        assert_eq!(Some(state.get()), model.read(index, None), "seed {seed}: state {index}");
        assert_eq!(state_records(state.handle()).0, 1, "seed {seed}: records of state {index}");
    }
}

#[test]
fn snapshots_agree_with_a_model_of_their_rules() {
    for seed in 0..400 {
        fuzz(seed, 150);
    }
}

/// In any snapshot, at any point, a derived state reads as its calculation would; nothing's left
/// when they're all gone.
#[test]
fn derived_states_read_as_their_calculation_in_any_snapshot() {
    for seed in 0..60 {
        let mut random = Random(seed);
        let states: Vec<_> = (0..4).map(|_| mutable_state_of(0i32)).collect();
        let handles: Vec<_> = states.iter().map(|state| state.handle()).collect();
        let sum = handles.clone();
        let inner =
            derived_state_of(move || sum.iter().take(2).map(|state| state.get()).sum::<i32>());
        let read_inner = inner.as_state();
        let rest = handles.clone();
        let outer = derived_state_of(move || {
            read_inner.get() * 10 + rest.iter().skip(2).map(|state| state.get()).sum::<i32>()
        });
        let expected = |handles: &[MutableState<i32>]| {
            handles.iter().take(2).map(|state| state.get()).sum::<i32>() * 10
                + handles.iter().skip(2).map(|state| state.get()).sum::<i32>()
        };
        let mut reals: Vec<Real> = Vec::new();
        for at in 0..200 {
            let context = if reals.is_empty() || random.next(3) == 0 {
                None
            } else {
                Some(random.next(reals.len()))
            };
            let writable = !matches!(context.map(|index| &reals[index]), Some(Real::ReadOnly(_)));
            let (state, value) = (handles[random.next(4)], random.next(5) as i32);
            match random.next(10) {
                0..=3 => {
                    in_context(&reals, context, || {
                        assert_eq!(outer.get(), expected(&handles), "seed {seed}, operation {at}");
                    });
                }
                4..=6 if writable => in_context(&reals, context, || state.set(value)),
                7 if reals.len() < 12 => {
                    if random.next(2) == 0 {
                        reals.push(Real::Mutable(Snapshot::take_mutable_snapshot(None, None)));
                    } else {
                        reals.push(Real::ReadOnly(Snapshot::take_snapshot(None)));
                    }
                }
                8 => {
                    if let Some(index) = context {
                        let real = reals.swap_remove(index);
                        if let Real::Mutable(snapshot) = &real
                            && random.next(2) == 0
                        {
                            let _ = snapshot.apply();
                        }
                    }
                }
                _ => Snapshot::send_apply_notifications(),
            }
        }
        reals.clear();
        Snapshot::send_apply_notifications();
        assert_eq!(outer.get(), expected(&handles), "seed {seed}");
        assert_eq!(Snapshot::open_snapshot_count(), 1, "seed {seed}");
        assert_eq!(state_records(handles[0]).0, 1, "seed {seed}");
    }
}
