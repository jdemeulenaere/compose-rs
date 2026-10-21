//! Phase C's bar (docs/10-roadmap.md): the counter and todo bodies of sketches/ compile as
//! written, against stand-ins for the toolkit (phase E) with Compose's parameter names and
//! defaults, and toggling one of 1,000 todos recomposes a constant number of composables.

#![allow(non_snake_case, non_upper_case_globals, clippy::redundant_closure)]

use std::fmt;
use std::rc::Rc;

use compose::runtime::*;
use toolkit::*;

// ---- Stand-ins for compose::ui, foundation::layout and material3 ----------------------------

mod toolkit {
    use std::fmt;
    use std::rc::Rc;

    use compose::runtime::*;

    pub type SharedString = Rc<str>;

    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct Dp(pub f32);

    pub trait DpExt {
        fn dp(self) -> Dp;
    }

    impl DpExt for i32 {
        fn dp(self) -> Dp {
            Dp(self as f32)
        }
    }

    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct Modifier {
        pub padding: f32,
        pub weight: f32,
        pub fill: bool,
        pub tag: &'static str,
    }

    /// `Modifier` is a type and a value, as in Kotlin (D13).
    pub const Modifier: Modifier = Modifier { padding: 0.0, weight: 0.0, fill: false, tag: "" };

    impl Modifier {
        pub fn padding(self, padding: Dp) -> Modifier {
            Modifier { padding: padding.0, ..self }
        }
        pub fn fill_max_size(self) -> Modifier {
            Modifier { fill: true, ..self }
        }
        pub fn fill_max_width(self) -> Modifier {
            Modifier { fill: true, ..self }
        }
        pub fn weight(self, _scope: RowScope, weight: f32) -> Modifier {
            Modifier { weight, ..self }
        }
        pub fn test_tag(self, tag: &'static str) -> Modifier {
            Modifier { tag, ..self }
        }
    }

    impl ComposeArg for Modifier {
        fn same(&self, other: &Self) -> bool {
            self == other
        }
    }

    #[derive(Clone, Copy, PartialEq, Debug, ComposeArg)]
    pub enum Arrangement {
        Top,
        Start,
        SpacedBy(Spacing),
    }

    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct Spacing(pub f32);

    impl ComposeArg for Spacing {
        fn same(&self, other: &Self) -> bool {
            self == other
        }
    }

    impl Arrangement {
        pub fn spaced_by(space: Dp) -> Arrangement {
            Arrangement::SpacedBy(Spacing(space.0))
        }
    }

    #[derive(Clone, Copy, PartialEq, Debug, ComposeArg)]
    pub enum Alignment {
        Start,
        Top,
        CenterVertically,
    }

    impl Alignment {
        pub const START: Alignment = Alignment::Start;
        pub const TOP: Alignment = Alignment::Top;
        pub const CENTER_VERTICALLY: Alignment = Alignment::CenterVertically;
    }

    #[derive(Clone, Copy, PartialEq, Debug, ComposeArg)]
    pub struct ColumnScope;

    #[derive(Clone, Copy, PartialEq, Debug, ComposeArg)]
    pub struct RowScope;

    /// A node of the stand-in UI tree: a label and the handlers a test can trigger.
    #[derive(Default)]
    pub struct UiNode {
        pub label: String,
        pub tag: &'static str,
        pub enabled: bool,
        pub on_click: Option<Callback<dyn Fn()>>,
        pub on_toggle: Option<Callback<dyn Fn(bool)>>,
        pub on_text: Option<Callback<dyn Fn(String)>>,
    }

    impl fmt::Debug for UiNode {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(&self.label)?;
            if !self.enabled {
                formatter.write_str(" (disabled)")?;
            }
            Ok(())
        }
    }

    pub type Tree = NodeTree<UiNode>;

    /// Emits a node and updates it: a hand-written primitive, as Compose's `ComposeNode`.
    pub fn Node(
        label: String,
        modifier: Modifier,
        update: impl FnOnce(&mut UiNode),
        content: impl FnOnce(),
    ) {
        let composer = current();
        let node = composer.start_node(key_of("sketches::Node"), UiNode::default);
        composer.with_applier(|tree: &mut Tree| {
            let value = tree.get_mut(node).expect("live node");
            value.label = label;
            value.tag = modifier.tag;
            value.enabled = true;
            update(value);
        });
        content();
        composer.end_group();
    }

    #[composable]
    pub fn Column(
        #[default(Modifier)] modifier: Modifier,
        #[default(Arrangement::Top)] vertical_arrangement: Arrangement,
        #[default(Alignment::START)] horizontal_alignment: Alignment,
        content: impl Fn(ColumnScope) + 'static,
    ) {
        let _ = (vertical_arrangement, horizontal_alignment);
        Node("Column".into(), modifier, |_| {}, || content(ColumnScope));
    }

    #[composable]
    pub fn Row(
        #[default(Modifier)] modifier: Modifier,
        #[default(Arrangement::Start)] horizontal_arrangement: Arrangement,
        #[default(Alignment::TOP)] vertical_alignment: Alignment,
        content: impl Fn(RowScope) + 'static,
    ) {
        let _ = (horizontal_arrangement, vertical_alignment);
        Node("Row".into(), modifier, |_| {}, || content(RowScope));
    }

    #[composable]
    pub fn Button(
        on_click: impl Fn() + 'static,
        #[default(Modifier)] modifier: Modifier,
        #[default(true)] enabled: bool,
        content: impl Fn() + 'static,
    ) {
        Node(
            "Button".into(),
            modifier,
            |node| {
                node.enabled = enabled;
                node.on_click = Some(on_click.clone());
            },
            || content(),
        );
    }

    #[composable]
    pub fn Text(#[format] text: SharedString, #[default(Modifier)] modifier: Modifier) {
        Node(text.to_string(), modifier, |_| {}, || {});
    }

    #[composable]
    pub fn TextField(
        value: String,
        on_value_change: impl Fn(String) + 'static,
        #[default(Modifier)] modifier: Modifier,
    ) {
        Node(
            format!("TextField({value})"),
            modifier,
            |node| node.on_text = Some(on_value_change.clone()),
            || {},
        );
    }

    #[composable]
    pub fn Checkbox(
        checked: bool,
        on_checked_change: impl Fn(bool) + 'static,
        #[default(Modifier)] modifier: Modifier,
    ) {
        Node(
            format!("Checkbox({checked})"),
            modifier,
            |node| {
                node.on_toggle = Some(on_checked_change.clone());
                node.enabled = true;
            },
            || {},
        );
    }
}

// ---- A tiny test host ---------------------------------------------------------------------

/// Finds nodes depth-first.
fn find_all(composition: &Composition, matches: impl Fn(&UiNode) -> bool) -> Vec<NodeRef> {
    composition.with_applier(|tree: &mut Tree| {
        let mut found = Vec::new();
        let mut stack: Vec<NodeRef> = tree.children(tree.root()).collect();
        stack.reverse();
        while let Some(node) = stack.pop() {
            if tree.get(node).is_some_and(&matches) {
                found.push(node);
            }
            let mut children: Vec<NodeRef> = tree.children(node).collect();
            children.reverse();
            stack.extend(children);
        }
        found
    })
}

/// Clicks the first button (in document order) whose content's text starts with `text`, then
/// recomposes.
fn click(composition: &Composition, text: &str) {
    let buttons = find_all(composition, |node| node.label == "Button");
    let handler = composition.with_applier(|tree: &mut Tree| {
        for button in buttons {
            let has_text = tree
                .children(button)
                .any(|child| tree.get(child).is_some_and(|child| child.label.starts_with(text)));
            if has_text {
                let value = tree.get(button).expect("live node");
                assert!(value.enabled, "clicking a disabled button");
                return value.on_click.clone().expect("a click handler");
            }
        }
        panic!("no button with text {text:?}");
    });
    handler();
    composition.recompose();
}

fn tree(composition: &Composition) -> String {
    composition.with_applier(|tree: &mut Tree| tree.dump())
}

// ---- sketches/counter.rs, verbatim -----------------------------------------------------------

#[composable]
fn Counter() {
    // remember keeps the OwnedState in this group's slot and returns its Copy handle.
    let count = remember(|| mutable_state_of(0));

    Column!(
        modifier = Modifier.padding(16.dp()),
        vertical_arrangement = Arrangement::spaced_by(8.dp()),
        {
            // `move` closures capture the Copy handle; the macro memoizes each lambda on its
            // captures (count), so Button receives the same Callback every pass and can skip.
            Button!(on_click = move || count.update(|c| c + 1), {
                Text!("Count: {}", count.get()); // a read: registers Button's restart scope
            });
            Button!(on_click = move || count.set(0), enabled = count.get() > 0, {
                Text!("Reset");
            });
        }
    );
}

#[test]
fn counting_to_three() {
    let composition = Composition::new(Tree::new());
    composition.set_content(|| Counter());
    assert_eq!(
        tree(&composition),
        "Column\n  Button\n    Count: 0\n  Button (disabled)\n    Reset\n"
    );
    for _ in 0..3 {
        click(&composition, "Count");
    }
    assert_eq!(tree(&composition), "Column\n  Button\n    Count: 3\n  Button\n    Reset\n");
    click(&composition, "Reset");
    assert!(tree(&composition).contains("Count: 0"));
    composition.validate();
}

#[test]
fn a_click_recomposes_only_what_reads_the_count() {
    let composition = Composition::new(Tree::new());
    composition.set_content(|| Counter());
    click(&composition, "Count");
    let before = bodies_run();
    click(&composition, "Count");
    // The count is read in both buttons' content, which registers each Button's scope (D10):
    // two Buttons and the Text whose argument changed run. Column and Counter skip, their
    // lambdas being memoized on the same `count` handle.
    assert_eq!(bodies_run() - before, 3);
}

// ---- sketches/todo.rs, verbatim but for `mutable_state_list_of` ----------------------------

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

impl fmt::Debug for Filter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Filter::All => "All",
            Filter::Active => "Active",
            Filter::Done => "Done",
        };
        formatter.write_str(name)
    }
}

#[composable]
pub fn TodoApp() {
    let todos = remember(|| mutable_state_list_of::<Rc<Todo>>());
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

impl Filter {
    fn shows(self, done: bool) -> bool {
        match self {
            Filter::All => true,
            Filter::Active => !done,
            Filter::Done => done,
        }
    }
}

// ---- Todo checks ----------------------------------------------------------------------------

fn type_into_draft(composition: &Composition, text: &str) {
    let field = find_all(composition, |node| node.tag == "draft");
    let handler = composition.with_applier(|tree: &mut Tree| {
        tree.get(field[0]).and_then(|node| node.on_text.clone()).expect("a text field")
    });
    handler(text.to_string());
    composition.recompose();
}

fn add(composition: &Composition, title: &str) {
    type_into_draft(composition, title);
    click(composition, "Add");
}

fn toggle(composition: &Composition, index: usize) {
    let boxes = find_all(composition, |node| node.label.starts_with("Checkbox"));
    let (handler, checked) = composition.with_applier(|tree: &mut Tree| {
        let node = tree.get(boxes[index]).expect("a checkbox");
        (node.on_toggle.clone().expect("a toggle handler"), node.label == "Checkbox(true)")
    });
    handler(!checked);
    composition.recompose();
}

fn rows(composition: &Composition) -> Vec<String> {
    let rows = find_all(composition, |node| node.label.starts_with("Checkbox"));
    composition.with_applier(|tree: &mut Tree| {
        rows.iter()
            .map(|&checkbox| {
                let row = tree.parent(checkbox).expect("a row");
                let labels: Vec<String> = tree
                    .children(row)
                    .filter_map(|child| tree.get(child).map(|node| node.label.clone()))
                    .collect();
                labels.join(" ")
            })
            .collect()
    })
}

#[test]
fn adding_toggling_filtering_and_removing_todos() {
    let composition = Composition::new(Tree::new());
    composition.set_content(|| TodoApp());
    for title in ["milk", "eggs", "bread"] {
        add(&composition, title);
    }
    assert_eq!(
        rows(&composition),
        [
            "Checkbox(false) milk Button",
            "Checkbox(false) eggs Button",
            "Checkbox(false) bread Button"
        ]
    );
    toggle(&composition, 1);
    assert_eq!(rows(&composition)[1], "Checkbox(true) eggs Button");
    click(&composition, "Done");
    assert_eq!(rows(&composition), ["Checkbox(true) eggs Button"]);
    click(&composition, "All");
    click(&composition, "Remove"); // the first row's
    assert_eq!(rows(&composition), ["Checkbox(true) eggs Button", "Checkbox(false) bread Button"]);
    composition.validate();
}

/// The bodies a toggle runs, with `count` todos.
fn bodies_for_a_toggle(count: usize) -> u64 {
    let composition = Composition::new(Tree::new());
    composition.set_content(|| TodoApp());
    for index in 0..count {
        add(&composition, &format!("item {index}"));
    }
    toggle(&composition, count / 2);
    let before = bodies_run();
    toggle(&composition, count / 2);
    bodies_run() - before
}

#[test]
fn toggling_one_of_a_thousand_todos_recomposes_one_row() {
    let few = bodies_for_a_toggle(3);
    let many = bodies_for_a_toggle(if cfg!(miri) { 20 } else { 1000 });
    // The toggled row's Row (whose content reads `done`) and its Checkbox: nothing else, however
    // many todos there are.
    assert_eq!(few, 2);
    assert_eq!(many, few);
}

#[test]
fn adding_a_todo_skips_the_existing_rows() {
    let composition = Composition::new(Tree::new());
    composition.set_content(|| TodoApp());
    for index in 0..50 {
        add(&composition, &format!("item {index}"));
    }
    type_into_draft(&composition, "one more");
    let before = bodies_run();
    click(&composition, "Add");
    let ran = bodies_run() - before;
    // The list scope re-runs, the new row composes (TodoRow, Row, Checkbox, Text, Button, Text),
    // and the text field updates; the 50 existing rows skip.
    assert!(ran < 20, "{ran} bodies ran for one added todo");
}
