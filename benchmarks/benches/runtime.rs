//! Runtime benchmarks, with the targets of docs/09-testing.md.

use std::hint::black_box;

use compose_benchmarks::*;
use compose_runtime::*;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn counter_click(c: &mut Criterion) {
    let composition = Composition::new(Tree::new());
    composition.set_content(Counter);
    let count = counter_state();
    c.bench_function("counter click (write + recompose)", |b| {
        b.iter(|| {
            count.update(|count| count + 1);
            composition.recompose()
        })
    });
}

fn toggle_one_row(c: &mut Criterion) {
    let rows = todo_rows(1000);
    let handle = rows.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || TodoList(handle));
    let done: Vec<MutableState<bool>> =
        rows.with(|rows| rows.iter().map(|row| row.done.handle()).collect());
    let mut index = 0;
    c.bench_function("toggle 1 of 1000 rows (write + recompose)", |b| {
        b.iter(|| {
            index = (index + 37) % 1000;
            done[index].set(!done[index].get());
            composition.recompose()
        })
    });
}

fn skipped_composables(c: &mut Criterion) {
    let trigger = mutable_state_of(0u64);
    let handle = trigger.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || Skipper(handle, 1000));
    c.bench_function("parent recompose with 1000 skipping children", |b| {
        b.iter(|| {
            trigger.update(|value| value + 1);
            composition.recompose()
        })
    });
}

fn initial_composition(c: &mut Criterion) {
    let mut group = c.benchmark_group("initial composition of n rows");
    for count in [100u64, 1000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            b.iter_with_large_drop(|| {
                let rows = todo_rows(count);
                let handle = rows.handle();
                let composition = Composition::new(Tree::new());
                composition.set_content(move || TodoList(handle));
                (composition, rows)
            })
        });
    }
    group.finish();
}

fn keyed_reorders(c: &mut Criterion) {
    let mut group = c.benchmark_group("1000 keyed rows");
    let rows = todo_rows(1000);
    let handle = rows.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || TodoList(handle));
    group.bench_function("rotate by one", |b| {
        b.iter(|| {
            handle.edit(|rows| rows.rotate_left(1));
            composition.recompose()
        })
    });
    group.bench_function("reverse (worst case for matching)", |b| {
        b.iter(|| {
            handle.edit(|rows| rows.reverse());
            composition.recompose()
        })
    });
    group.finish();
}

fn state_reads(c: &mut Criterion) {
    let state = mutable_state_of(1u64);
    c.bench_function("state get outside composition", |b| b.iter(|| black_box(state.get())));
}

/// A derived state read with nothing changed since it was validated, read after a write of a
/// state nothing derives from (its dependency compared), and recomputed; and a write sent to a
/// derived state an observation reads (design §3.16).
fn derived_reads(c: &mut Criterion) {
    let state = mutable_state_of(1u64);
    let unrelated = mutable_state_of(0u64);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get() + 1);
    let read = derived.as_state();
    assert_eq!(read.get(), 2);
    c.bench_function("derived state read, valid", |b| b.iter(|| black_box(read.get())));
    let mut value = 0;
    c.bench_function("derived state read after an unrelated write", |b| {
        b.iter(|| {
            value += 1;
            unrelated.set(value);
            black_box(read.get())
        })
    });
    let mut value = 1;
    c.bench_function("derived state read, recomputed", |b| {
        b.iter(|| {
            value += 1;
            state.set(value);
            black_box(read.get())
        })
    });
}

/// A push on a state list, nothing sharing it: a `Vec` push, and what a write is (design §3.16).
fn state_list_push(c: &mut Criterion) {
    let list = mutable_state_list_of::<u64>();
    let mut value = 0;
    c.bench_function("state list push", |b| {
        b.iter(|| {
            value += 1;
            if list.with(Vec::len) == 1024 {
                list.clear();
            }
            list.push(black_box(value));
        })
    });
}

/// A host's tick with nothing to do: the executor's `run_ready` finding no task, no change to send
/// and no derived state to flush.
fn idle_tick(c: &mut Criterion) {
    c.bench_function("run_ready with nothing ready", |b| {
        b.iter(compose_runtime::platform::run_ready)
    });
}

/// A write outside composition, nothing reading the state: in the same snapshot as the last
/// (docs/parity/design/snapshots.md §3.16); and each write followed by the send a turn ends with,
/// which makes it a new record in a new snapshot and invalidates what read it (nothing here).
fn state_writes(c: &mut Criterion) {
    let state = mutable_state_of(0u64);
    let mut value = 0;
    c.bench_function("state set outside composition", |b| {
        b.iter(|| {
            value += 1;
            state.set(black_box(value));
        })
    });
    c.bench_function("state set, then the turn's send", |b| {
        b.iter(|| {
            value += 1;
            state.set(black_box(value));
            compose::runtime::snapshots::Snapshot::send_apply_notifications();
        })
    });
}

/// What composing a big screen does a lot of (docs/14, #5): reading a composition local whose
/// provider is dozens of groups up, and reading one state from thousands of scopes (every layout
/// node reads the layout direction).
fn many_readers(criterion: &mut Criterion) {
    let trigger = mutable_state_of(0u64);
    let handle = trigger.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        CompositionLocalProvider(LOCAL_VALUE.provides(7), || {
            Nest(60, &|| LocalReader(handle, 1000));
        })
    });
    criterion.bench_function("1000 local reads, the provider 60 groups up (recompose)", |bench| {
        bench.iter(|| {
            trigger.update(|value| value + 1);
            composition.recompose()
        })
    });

    let shared = mutable_state_of(0u64);
    let handle = shared.handle();
    criterion.bench_function("initial composition of 2000 readers of one state", |bench| {
        bench.iter_with_large_drop(|| {
            let composition = Composition::new(Tree::new());
            composition.set_content(move || SharedReaders(handle, 2000));
            composition
        })
    });
    let composition = Composition::new(Tree::new());
    composition.set_content(move || SharedReaders(handle, 2000));
    criterion.bench_function("write a state 2000 readers read (recompose all)", |bench| {
        bench.iter(|| {
            shared.update(|value| value + 1);
            composition.recompose()
        })
    });
}

criterion_group!(
    benches,
    counter_click,
    toggle_one_row,
    skipped_composables,
    initial_composition,
    keyed_reorders,
    state_reads,
    state_writes,
    idle_tick,
    derived_reads,
    state_list_push,
    many_readers
);
criterion_main!(benches);
