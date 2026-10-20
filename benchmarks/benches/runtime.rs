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
    c.bench_function("state get outside composition", |b| {
        b.iter(|| black_box(state.get()))
    });
}

criterion_group!(
    benches,
    counter_click,
    toggle_one_row,
    skipped_composables,
    initial_composition,
    keyed_reorders,
    state_reads
);
criterion_main!(benches);
