// Sketch, not compiled: the todo app, with per-item state in plain data (a plain list of Todo
// whose `done` is state) and rows that skip.

use std::rc::Rc;

use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ---- Model ----------------------------------------------------------------------------------

/// One item. `done` is state owned by the Todo: dropping the Todo frees it (deferred to the end
/// of the next pass, rule R2), so removing an item never leaks and never leaves a dangling
/// handle in a row that is about to be removed.
#[derive(ComposeArg)] // field-wise `same`: id by value, title by pointer, done by handle
pub struct Todo {
    pub id: u64,
    pub title: SharedString,
    pub done: OwnedState<bool>,
}

#[derive(Clone, Copy, PartialEq, ComposeArg)]
pub enum Filter {
    All,
    Active,
    Done,
}

// ---- Screen ---------------------------------------------------------------------------------

#[composable]
pub fn TodoApp() {
    let todos = remember(|| mutable_state_of(Vec::<Rc<Todo>>::new()));
    let draft = remember(|| mutable_state_of(String::new()));
    let filter = remember(|| mutable_state_of(Filter::All));
    let next_id = remember(|| mutable_state_of(0u64));

    Column!(modifier = Modifier.fill_max_size().padding(16.dp()), {
        Row!(|row| {
            TextField!(
                value = draft.get(),
                on_value_change = move |text: String| draft.set(text),
                modifier = Modifier.weight(row, 1.0).test_tag("draft"), // scope token: Row content only
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
                        list.push(Rc::new(Todo { id, title: title.into(), done: mutable_state_of(false) }))
                    });
                    draft.set(String::new());
                },
                { Text!("Add"); }
            );
        });

        // Compose's stateless pattern, value plus event lambda: a Callback is an Rc, so it can be
        // shared freely between handlers.
        FilterRow!(selected = filter.get(), on_select = move |f: Filter| filter.set(f));

        // Reading `todos` registers this scope: adding or removing an item re-runs the loop, and
        // every unchanged row then SKIPS, because its Rc<Todo> is ptr_eq to the kept one.
        // `with` and `key` take borrowing closures that run at once: the macro leaves them alone
        // and memoizes only the `move` closures that are stored (on_remove).
        todos.with(|list| {
            let selected = filter.get();
            for todo in list {
                // Under `All` the loop never reads `done`, so toggling an item does not even
                // re-run this scope: only that row recomposes, as it reads its own `done`.
                if selected == Filter::All || selected.shows(todo.done.get()) {
                    key(todo.id, || {
                        let id = todo.id;
                        TodoRow!(todo = todo.clone(), on_remove = move || {
                            todos.edit(|list| list.retain(|t| t.id != id))
                        });
                    });
                }
            }
        });
    });
}

/// A row: restartable and skippable, because every parameter is ComposeArg (Rc identity and a
/// memoized Callback). The macro turns `impl Fn` parameters into Callbacks compared by identity.
#[composable]
fn TodoRow(todo: Rc<Todo>, on_remove: impl Fn() + 'static) {
    Row!(
        modifier = Modifier.fill_max_width().padding(4.dp()),
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        |row| {
            let done = todo.done.handle(); // Copy handle for the 'static handler
            Checkbox!(checked = done.get(), on_checked_change = move |checked| done.set(checked));
            Text!(todo.title.clone(), modifier = Modifier.weight(row, 1.0));
            Button!(on_click = on_remove, { Text!("Remove"); });
        }
    );
}

#[composable]
fn FilterRow(selected: Filter, on_select: impl Fn(Filter) + 'static) {
    Row!(horizontal_arrangement = Arrangement::spaced_by(8.dp()), |_row| {
        for filter in [Filter::All, Filter::Active, Filter::Done] {
            let on_select = on_select.clone();
            Button!(
                on_click = move || on_select(filter),
                enabled = filter != selected,
                { Text!("{:?}", filter); }
            );
        }
    });
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

fn main() {
    compose::desktop::run_app(
        WindowOptions { title: "Todo".into(), size: (480, 640).into(), ..Default::default() },
        || TodoApp(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose::test::*;

    #[test]
    fn toggling_one_item_recomposes_one_row() {
        let mut ui = compose_test(|| TodoApp());
        for title in ["milk", "eggs", "bread"] {
            ui.on_node_with_tag("draft").perform_text_input(title);
            ui.on_node_with_text("Add").perform_click();
        }
        let before = ui.recomposition_counts();
        ui.on_all_nodes_with_role(Role::Checkbox)[1].perform_click();
        let recomposed = ui.recomposition_counts().since(&before);
        assert_eq!(recomposed.scopes_named("TodoRow"), 1);
        assert_eq!(recomposed.scopes_named("TodoApp"), 0);
    }
}
