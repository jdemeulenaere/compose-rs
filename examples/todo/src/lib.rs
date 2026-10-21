//! The todo app, as in sketches/todo.rs: per-item state in plain data (a list of `Todo`s whose
//! `done` is state), filters, and rows that skip, so toggling one of a thousand items
//! recomposes one row.

#![allow(non_snake_case)]

use std::rc::Rc;

use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ---- Model ----------------------------------------------------------------------------------

/// One item. `done` is state owned by the Todo: dropping the Todo frees it (deferred to the end
/// of the next pass, rule R2), so removing an item never leaks.
#[derive(ComposeArg)] // field-wise `same`: id by value, title by pointer, done by handle
pub struct Todo {
    pub id: u64,
    pub title: SharedString,
    pub done: OwnedState<bool>,
}

#[derive(Clone, Copy, PartialEq, Debug, ComposeArg)]
pub enum Filter {
    All,
    Active,
    Done,
}

impl Filter {
    fn shows(self, done: bool) -> bool {
        match self {
            Filter::All => true,
            Filter::Active => !done,
            Filter::Done => done,
        }
    }
}

// ---- Screen ---------------------------------------------------------------------------------

#[composable]
pub fn TodoApp() {
    TodoAppWith(0);
}

/// The app, starting with `initial_items` items: the 1,000-item screen of the roadmap.
#[composable]
pub fn TodoAppWith(initial_items: u64) {
    let todos = remember(|| {
        mutable_state_list_from((0..initial_items).map(|id| {
            Rc::new(Todo { id, title: format!("Item {id}").into(), done: mutable_state_of(false) })
        }))
    });
    let draft = remember(|| mutable_state_of(String::new()));
    let filter = remember(|| mutable_state_of(Filter::All));
    let next_id = remember(|| mutable_state_of(initial_items));

    Column!(modifier = Modifier.fill_max_size().padding(16.dp()), {
        Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, |row| {
            TextField!(
                value = draft.get(),
                on_value_change = move |text: String| draft.set(text),
                modifier = Modifier.weight(row, 1.0).test_tag("draft"),
                single_line = true,
            );
            Button!(
                on_click = move || {
                    let title = draft.get();
                    if title.is_empty() {
                        return;
                    }
                    let id = next_id.get();
                    next_id.set(id + 1);
                    todos.edit(|list| {
                        list.push(Rc::new(Todo {
                            id,
                            title: title.into(),
                            done: mutable_state_of(false),
                        }))
                    });
                    draft.set(String::new());
                },
                {
                    Text!("Add");
                }
            );
        });

        FilterRow!(selected = filter.get(), on_select = move |f: Filter| filter.set(f));

        // Reading `todos` registers this scope: adding or removing an item re-runs the loop, and
        // every unchanged row then skips, because its Rc<Todo> is the kept one.
        todos.with(|list| {
            let selected = filter.get();
            for todo in list {
                // Under `All` the loop never reads `done`, so toggling an item doesn't re-run
                // this scope: only that row recomposes, as it reads its own `done`.
                if selected == Filter::All || selected.shows(todo.done.get()) {
                    key(todo.id, || {
                        let id = todo.id;
                        TodoRow!(
                            todo = todo.clone(),
                            on_remove = move || { todos.edit(|list| list.retain(|t| t.id != id)) }
                        );
                    });
                }
            }
        });
    });
}

/// A row: restartable and skippable, as every parameter is `ComposeArg` (an `Rc` and a memoized
/// callback).
#[composable]
fn TodoRow(todo: Rc<Todo>, on_remove: impl Fn() + 'static) {
    Row!(
        modifier = Modifier.fill_max_width().padding(4.dp()),
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        |row| {
            let done = todo.done.handle(); // Copy handle for the 'static handler
            Checkbox!(checked = done.get(), on_checked_change = move |checked| done.set(checked));
            Text!(todo.title.clone(), modifier = Modifier.weight(row, 1.0));
            Button!(on_click = on_remove, {
                Text!("Remove");
            });
        }
    );
}

#[composable]
fn FilterRow(selected: Filter, on_select: impl Fn(Filter) + 'static) {
    Row!(horizontal_arrangement = Arrangement::spaced_by(8.dp()), |_row| {
        for filter in [Filter::All, Filter::Active, Filter::Done] {
            let on_select = on_select.clone();
            Button!(on_click = move || on_select(filter), enabled = filter != selected, {
                Text!("{:?}", filter);
            });
        }
    });
}
