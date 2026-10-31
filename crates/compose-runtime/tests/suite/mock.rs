//! The shared runtime test kit, as Compose's `runtime-test-utils` (`androidx.compose.runtime.mock`):
//! mock views with a name and attributes, their applier, the emitters Compose's runtime tests
//! compose them with (`Linear`, `Text`, `Edit`, `SelectBox`, `Repeated`, `Wrap`), an
//! expected-tree builder (`validate { Linear { Text("a") } }`) and a composition driver
//! ([`CompositionTest`]: `compose`, `advance`, `expect_changes`, …) that also turns the UI
//! thread's executor, so effects run without a host.
//!
//! The ported Compose tests (D710) build on it; each module of the suite uses what it needs.

#![allow(non_snake_case, dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::hash::Hash;
use std::rc::Rc;
use std::time::Duration;

use compose_runtime::platform::*;
use compose_runtime::*;

// ---- Views ---------------------------------------------------------------------------------

/// A mock view, as Compose's `mock.View`: a name, string attributes, and the lifecycle a node
/// sees (`ComposeNodeLifecycleCallback`: reused, deactivated, released).
pub struct View {
    /// Unique on the thread: which view instance this is, as the mock views' identity.
    pub id: u64,
    pub name: &'static str,
    pub attributes: BTreeMap<&'static str, String>,
    /// Not deactivated (`Applier::deactivate`), or reused since.
    pub active: bool,
    /// What the node's lifecycle callbacks call, as `Linear(onReuse, onDeactivate, onRelease)`.
    pub callbacks: Option<Rc<ViewCallbacks>>,
    /// Being reused in paused content, not applied yet (D591).
    reusing_paused: bool,
}

/// A view's lifecycle callbacks.
#[derive(Default)]
pub struct ViewCallbacks {
    pub on_reuse: Option<Box<dyn Fn()>>,
    pub on_deactivate: Option<Box<dyn Fn()>>,
    pub on_release: Option<Box<dyn Fn()>>,
}

thread_local! {
    static NEXT_VIEW_ID: Cell<u64> = const { Cell::new(1) };
}

impl View {
    pub fn new(name: &'static str) -> View {
        let id = NEXT_VIEW_ID.with(|next| next.replace(next.get() + 1));
        View {
            id,
            name,
            attributes: BTreeMap::new(),
            active: true,
            reusing_paused: false,
            callbacks: None,
        }
    }

    /// Sets an attribute, as `View.attribute(name, value)`. A node is modified only while it's
    /// active, as Compose's mock checks.
    pub fn set_attribute(&mut self, name: &'static str, value: impl Into<String>) {
        assert!(
            self.active || self.reusing_paused,
            "node {self:?} modified while it wasn't active"
        );
        self.attributes.insert(name, value.into());
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }
}

impl fmt::Debug for View {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "<{}", self.name)?;
        for (name, value) in &self.attributes {
            write!(formatter, " {name}='{value}'")?;
        }
        formatter.write_str(">")
    }
}

/// The mock views' applier, as Compose's `ViewApplier`: a [`NodeTree`] of [`View`]s that counts
/// the changes applied to it, for [`CompositionTest::advance`].
pub struct ViewApplier {
    pub tree: NodeTree<View>,
    /// Insertions, removals, moves, reuses, deactivations, releases and attribute updates.
    pub changes: u64,
    pub begin_changes: u32,
    pub end_changes: u32,
    /// Told "begin" and "end" as the changes do, if a test wants their order.
    pub change_log: Option<Rc<RefCell<Vec<String>>>>,
    /// Set by a test: a release panics.
    pub panic_in_release: Rc<Cell<bool>>,
}

impl ViewApplier {
    pub fn new() -> ViewApplier {
        ViewApplier {
            tree: NodeTree::new(),
            changes: 0,
            begin_changes: 0,
            end_changes: 0,
            change_log: None,
            panic_in_release: Rc::default(),
        }
    }

    /// Updates `node`, counting a change.
    pub fn update(&mut self, node: NodeRef, update: impl FnOnce(&mut View)) {
        self.changes += 1;
        update(self.tree.get_mut(node).expect("a live view"));
    }

    /// The views below the root.
    pub fn roots(&self) -> Vec<NodeRef> {
        self.tree.children(self.tree.root()).collect()
    }

    fn callbacks(&self, node: NodeRef) -> Option<Rc<ViewCallbacks>> {
        self.tree.get(node).and_then(|view| view.callbacks.clone())
    }
}

impl Default for ViewApplier {
    fn default() -> Self {
        ViewApplier::new()
    }
}

thread_local! {
    /// How many placeholders the thread's lends made.
    pub static PLACEHOLDERS: Cell<u32> = const { Cell::new(0) };
}

impl compose_runtime::platform::LendableApplier for ViewApplier {
    fn placeholder() -> ViewApplier {
        PLACEHOLDERS.with(|made| made.set(made.get() + 1));
        ViewApplier::new()
    }
}

impl Applier for ViewApplier {
    type Node = View;

    fn root(&self) -> NodeRef {
        self.tree.root()
    }

    fn contains(&self, node: NodeRef) -> bool {
        self.tree.contains(node)
    }

    fn insert(&mut self, parent: NodeRef, index: usize, node: View) -> NodeRef {
        self.changes += 1;
        Applier::insert(&mut self.tree, parent, index, node)
    }

    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        self.changes += 1;
        Applier::remove(&mut self.tree, parent, index, count)
    }

    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        self.changes += 1;
        Applier::move_nodes(&mut self.tree, parent, from, to, count)
    }

    fn release(&mut self, node: NodeRef) {
        assert!(!self.panic_in_release.get(), "in a release");
        self.changes += 1;
        // The node and its subtree: each view hears it, as Compose's `onRelease`.
        let mut stack = vec![node];
        let mut callbacks = Vec::new();
        while let Some(node) = stack.pop() {
            if let Some(view_callbacks) = self.callbacks(node) {
                callbacks.push(view_callbacks);
            }
            stack.extend(self.tree.children(node));
        }
        Applier::release(&mut self.tree, node);
        for callbacks in callbacks {
            if let Some(on_release) = &callbacks.on_release {
                on_release();
            }
        }
    }

    fn reuse(&mut self, node: NodeRef) {
        self.changes += 1;
        let view = self.tree.get_mut(node).expect("a live view");
        // A node can be reused without being deactivated: active from here on.
        view.active = true;
        view.reusing_paused = false;
        if let Some(on_reuse) = view.callbacks.clone().as_ref().and_then(|c| c.on_reuse.as_ref()) {
            on_reuse();
        }
    }

    /// Left to `apply`, as by default; paused content updates a node before that (D591), which
    /// the check of `set_attribute` lets it.
    fn reuse_paused(&mut self, node: NodeRef) -> bool {
        self.tree.get_mut(node).expect("a live view").reusing_paused = true;
        false
    }

    fn deactivate(&mut self, node: NodeRef) {
        self.changes += 1;
        let view = self.tree.get_mut(node).expect("a live view");
        assert!(view.active, "node {view:?} deactivated while it was inactive");
        view.active = false;
        if let Some(on_deactivate) =
            view.callbacks.clone().as_ref().and_then(|c| c.on_deactivate.as_ref())
        {
            on_deactivate();
        }
    }

    fn node_mut(&mut self, node: NodeRef) -> &mut View {
        // What an `Updater` changes.
        self.changes += 1;
        self.tree.get_mut(node).expect("a live view")
    }

    fn create(&mut self, node: View) -> NodeRef {
        self.changes += 1;
        Applier::create(&mut self.tree, node)
    }

    fn attach(&mut self, parent: NodeRef, index: usize, node: NodeRef) {
        self.changes += 1;
        Applier::attach(&mut self.tree, parent, index, node)
    }

    fn on_begin_changes(&mut self) {
        self.begin_changes += 1;
        if let Some(log) = &self.change_log {
            log.borrow_mut().push("begin".to_string());
        }
    }

    fn on_end_changes(&mut self) {
        self.end_changes += 1;
        if let Some(log) = &self.change_log {
            log.borrow_mut().push("end".to_string());
        }
    }
}

// ---- Emitters ------------------------------------------------------------------------------

/// A view named `name`, for a node factory.
pub fn view(name: &'static str) -> impl FnOnce() -> View {
    move || View::new(name)
}

/// Sets attribute `name` of a view to `value`, as `set(value) { attribute = it }`.
pub fn set_attribute(name: &'static str) -> impl FnOnce(&mut View, &String) {
    move |view, value| view.set_attribute(name, value.clone())
}

/// A `linear` view around `content`, as the mock's `Linear` (a `ReusableComposeNode`).
#[composable]
pub fn Linear(content: impl Fn() + 'static) {
    ReusableComposeNodeWithContent::<ViewApplier>(view("linear"), |_| {}, || content());
}

/// A callback a mock view calls: compared by identity.
pub type Action = Rc<dyn Fn()>;

/// An action that does nothing, a fresh one each call.
pub fn nothing() -> Action {
    Rc::new(|| {})
}

/// A `linear` view whose lifecycle calls the latest `on_reuse`, `on_deactivate` and
/// `on_release`, and whose update calls `on_set` when it changes, as the mock's
/// `Linear(onReuse, onDeactivate, onRelease, onSet, content)`.
#[composable(non_restartable)]
pub fn LinearWithCallbacks(
    on_reuse: Action,
    on_deactivate: Action,
    on_release: Action,
    on_set: Action,
    content: impl FnOnce(),
) {
    // `rememberUpdatedState`s the view keeps: after a reuse forgot them, it calls the last
    // callbacks they held (Kotlin's states outlive their slots; a forgotten one here is freed).
    let on_reuse = updated(on_reuse);
    let on_deactivate = updated(on_deactivate);
    let on_release = updated(on_release);
    let factory = move || {
        let mut view = View::new("linear");
        view.callbacks = Some(Rc::new(ViewCallbacks {
            on_reuse: Some(Box::new(move || (on_reuse.borrow().clone())())),
            on_deactivate: Some(Box::new(move || (on_deactivate.borrow().clone())())),
            on_release: Some(Box::new(move || (on_release.borrow().clone())())),
        }));
        view
    };
    ReusableComposeNodeWithContent::<ViewApplier>(
        factory,
        |updater| updater.set(on_set, |_, on_set| on_set()),
        content,
    );
}

/// A remembered cell holding the latest `action`, as `rememberUpdatedState`.
fn updated(action: Action) -> Rc<RefCell<Action>> {
    let cell = remember(|| Rc::new(RefCell::new(action.clone())));
    *cell.borrow_mut() = action;
    cell
}

/// A `linear` view that content being reused replaces, as the mock's `NonReusableLinear`.
#[composable]
pub fn NonReusableLinear(content: impl Fn() + 'static) {
    ComposeNodeWithContent::<ViewApplier>(view("linear"), |_| {}, || content());
}

/// A `text` view showing `value`, as the mock's `Text` (non-restartable, a reusable node).
#[composable(non_restartable)]
pub fn Text(value: &str) {
    ReusableComposeNode::<ViewApplier>(view("text"), |updater| {
        updater.set(value.to_string(), set_attribute("text"))
    });
}

/// A `text` view that content being reused replaces, as the mock's `NonReusableText`.
#[composable]
pub fn NonReusableText(value: String) {
    ComposeNode::<ViewApplier>(view("text"), |updater| updater.set(value, set_attribute("text")));
}

/// An `edit` view with `value`, as the mock's `Edit`.
#[composable]
pub fn Edit(value: String) {
    ReusableComposeNode::<ViewApplier>(view("edit"), |updater| {
        updater.set(value, set_attribute("value"))
    });
}

/// `content` in a `box` view if `selected`, as is otherwise, as the mock's `SelectBox`.
#[composable]
pub fn SelectBox(selected: bool, content: impl Fn() + 'static) {
    if selected {
        ReusableComposeNodeWithContent::<ViewApplier>(view("box"), |_| {}, || content());
    } else {
        content();
    }
}

/// Calls `content`: a restartable composable around content, as `Wrap` in Compose's tests.
#[composable]
pub fn Wrap(content: impl Fn() + 'static) {
    content();
}

/// `block` for each item, each in a `key` group of the item's key, as the mock's
/// `Repeated(of) { … }` (its items are their own keys there).
pub fn Repeated<T, K: ComposeArg + Hash>(
    of: impl IntoIterator<Item = T>,
    key_of_item: impl Fn(&T) -> K,
    block: impl Fn(T),
) {
    for item in of {
        key(key_of_item(&item), || block(item));
    }
}

// ---- The contact model ---------------------------------------------------------------------

/// A contact, as the mock's `Contact`: compared by identity, as a Kotlin class.
#[derive(Debug)]
pub struct Contact {
    pub name: String,
    pub email: String,
}

pub type ContactRef = Rc<Contact>;

pub fn contact_of(name: &str, email: &str) -> ContactRef {
    Rc::new(Contact { name: name.into(), email: email.into() })
}

/// The identity of a contact, for `key`.
pub fn identity<T>(item: &Rc<T>) -> usize {
    Rc::as_ptr(item) as usize
}

/// The mock's `ContactModel`, shared by the tests and their composables. Its data is state:
/// Kotlin re-runs a composable lambda whenever the composable creating it recomposes, where
/// compose-rs memoizes it on its captures (docs/03), so a model a lambda reads must be
/// observable for a change to reach it.
#[derive(Clone)]
pub struct ContactModel(pub Rc<OwnedState<ContactModelData>>);

#[derive(Clone)]
pub struct ContactModelData {
    pub filter: String,
    pub contacts: Vec<ContactRef>,
    pub selected: Option<ContactRef>,
}

impl ComposeArg for ContactModel {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ContactModel {
    pub fn new(contacts: Vec<ContactRef>) -> ContactModel {
        ContactModel(Rc::new(mutable_state_of_with(
            ContactModelData { filter: String::new(), contacts, selected: None },
            NeverEqualPolicy,
        )))
    }

    pub fn filtered(&self) -> Vec<ContactRef> {
        self.0.with(|data| {
            data.contacts.iter().filter(|c| c.name.contains(&data.filter)).cloned().collect()
        })
    }

    pub fn filter(&self) -> String {
        self.0.with(|data| data.filter.clone())
    }

    pub fn set_filter(&self, filter: &str) {
        self.0.edit(|data| data.filter = filter.into());
    }

    pub fn selected(&self) -> Option<ContactRef> {
        self.0.with(|data| data.selected.clone())
    }

    pub fn add(&self, contact: ContactRef, after: Option<&ContactRef>) {
        self.0.edit(|data| match after {
            None => data.contacts.push(contact),
            Some(after) => {
                let index = find(&data.contacts, after);
                data.contacts.insert(index + 1, contact);
            }
        });
    }

    pub fn move_after(&self, contact: &ContactRef, after: Option<&ContactRef>) {
        self.0.edit(|data| {
            let index = find(&data.contacts, contact);
            let moved = data.contacts.remove(index);
            match after {
                None => data.contacts.insert(0, moved),
                Some(after) => {
                    let index = find(&data.contacts, after);
                    data.contacts.insert(index + 1, moved);
                }
            }
        });
    }
}

fn find(list: &[ContactRef], contact: &ContactRef) -> usize {
    list.iter().position(|c| Rc::ptr_eq(c, contact)).expect("contact not found")
}

/// Bob, Jon and Steve, as Compose's tests' `bob`, `jon` and `steve`.
pub struct Contacts {
    pub bob: ContactRef,
    pub jon: ContactRef,
    pub steve: ContactRef,
}

pub fn contacts() -> Contacts {
    Contacts {
        bob: contact_of("Bob Smith", "bob@smith.com"),
        jon: contact_of("Jon Alberton", "jon@alberton.com"),
        steve: contact_of("Steve Roberson", "steverob@somemail.com"),
    }
}

/// `<linear><text Name: …/><text email: …/></linear>`, as the mock's `contact`.
#[composable]
pub fn contact(contact: ContactRef) {
    Linear(callback(move || {
        Text(&format!("Name: {}", contact.name));
        Text(&format!("email: {}", contact.email));
    }));
}

/// The contacts, each in a select box, as the mock's `contacts`.
#[composable(non_restartable)]
pub fn contacts_view(contacts: Vec<ContactRef>, selected: Option<ContactRef>) {
    let contacts = Rc::new(contacts);
    Linear(callback(move || {
        let selected = selected.clone();
        Repeated(contacts.iter().cloned(), identity, move |c| {
            let is_selected = selected.as_ref().is_some_and(|s| Rc::ptr_eq(s, &c));
            SelectBox(is_selected, callback(move || self::contact(c.clone())));
        });
    }));
}

/// The contact selector, as the mock's `SelectContact` (`@NonSkippableComposable`).
#[composable(non_restartable)]
pub fn SelectContact(model: ContactModel) {
    Linear(callback(move || {
        let filter = model.filter();
        Linear(callback(move || {
            Text("Filter:");
            Edit(filter.clone());
        }));
        let model = model.clone();
        Linear(callback(move || {
            Text("Contacts:");
            contacts_view(model.filtered(), model.selected());
        }));
    }));
}

// ---- Expected trees ------------------------------------------------------------------------

/// A view a test expects, as Compose's `MockViewValidator` checks one.
#[derive(Clone)]
enum ExpectedView {
    /// Any view, unchecked (`skip()`).
    Any,
    View {
        name: &'static str,
        attributes: Vec<(&'static str, String)>,
        children: Vec<ExpectedView>,
    },
}

/// The expected tree a `validate` block builds, as Compose's `MockViewValidator`: each method
/// adds the views it names at the current level.
#[derive(Default)]
pub struct Expected {
    views: Vec<ExpectedView>,
}

impl Expected {
    /// A view named `name` with these attributes (others are ignored), and `children` inside.
    pub fn view(
        &mut self,
        name: &'static str,
        attributes: &[(&'static str, &str)],
        children: impl FnOnce(&mut Expected),
    ) {
        let mut inner = Expected::default();
        children(&mut inner);
        self.views.push(ExpectedView::View {
            name,
            attributes: attributes.iter().map(|(n, v)| (*n, v.to_string())).collect(),
            children: inner.views,
        });
    }

    /// A `linear` view around `children`.
    pub fn linear(&mut self, children: impl FnOnce(&mut Expected)) {
        self.view("linear", &[], children);
    }

    /// An empty `linear` view.
    pub fn empty_linear(&mut self) {
        self.view("linear", &[], |_| {});
    }

    /// A `box` view around `children`.
    pub fn select_box(&mut self, selected: bool, children: impl FnOnce(&mut Expected)) {
        if selected {
            self.view("box", &[], children);
        } else {
            children(self);
        }
    }

    pub fn text(&mut self, value: &str) {
        self.view("text", &[("text", value)], |_| {});
    }

    pub fn edit(&mut self, value: &str) {
        self.view("edit", &[("value", value)], |_| {});
    }

    /// `count` views, whatever they are.
    pub fn skip(&mut self, count: usize) {
        self.views.extend(std::iter::repeat_n(ExpectedView::Any, count));
    }

    pub fn contact(&mut self, contact: &Contact) {
        self.linear(|v| {
            v.text(&format!("Name: {}", contact.name));
            v.text(&format!("email: {}", contact.email));
        });
    }

    pub fn contacts(&mut self, contacts: &[ContactRef], selected: Option<&ContactRef>) {
        self.linear(|v| {
            for c in contacts {
                let is_selected = selected.is_some_and(|s| Rc::ptr_eq(s, c));
                v.select_box(is_selected, |v| v.contact(c));
            }
        });
    }

    pub fn select_contact(&mut self, model: &ContactModel) {
        self.linear(|v| {
            v.linear(|v| {
                v.text("Filter:");
                v.edit(&model.filter());
            });
            v.linear(|v| {
                v.text("Contacts:");
                v.contacts(&model.filtered(), model.selected().as_ref());
            });
        });
    }
}

/// Renders expected views in the format of [`NodeTree::dump`] of [`View`]s, `?` for any view.
fn render(views: &[ExpectedView], depth: usize, out: &mut String) {
    for view in views {
        out.push_str(&"  ".repeat(depth));
        match view {
            ExpectedView::Any => out.push_str("?\n"),
            ExpectedView::View { name, attributes, children } => {
                out.push('<');
                out.push_str(name);
                for (name, value) in attributes {
                    out.push_str(&format!(" {name}='{value}'"));
                }
                out.push_str(">\n");
                render(children, depth + 1, out);
            }
        }
    }
}

/// Whether the views under `parent` match `expected`.
fn matches(tree: &NodeTree<View>, parent: NodeRef, expected: &[ExpectedView]) -> bool {
    let children: Vec<NodeRef> = tree.children(parent).collect();
    children.len() == expected.len()
        && children.iter().zip(expected).all(|(&node, expected)| match expected {
            ExpectedView::Any => true,
            ExpectedView::View { name, attributes, children } => {
                let view = tree.get(node).expect("a live view");
                view.name == *name
                    && attributes.iter().all(|(n, v)| view.attribute(n) == Some(v.as_str()))
                    && matches(tree, node, children)
            }
        })
}

// ---- Logs ----------------------------------------------------------------------------------

/// A shared log, as the `mutableListOf<String>()` Compose's tests record into: composables and
/// effects push, the test takes. Compared by identity.
#[derive(Clone, Default)]
pub struct Log(Rc<RefCell<Vec<String>>>);

impl Log {
    pub fn new() -> Log {
        Log::default()
    }

    pub fn push(&self, entry: impl Into<String>) {
        self.0.borrow_mut().push(entry.into());
    }

    /// The entries so far, emptying the log.
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.borrow_mut())
    }

    /// The entries so far.
    pub fn entries(&self) -> Vec<String> {
        self.0.borrow().clone()
    }

    /// The entries joined with ", ", as Kotlin's `joinToString()`.
    pub fn joined(&self) -> String {
        self.0.borrow().join(", ")
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}

impl ComposeArg for Log {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A shared counter, as a captured `var count = 0` in Compose's tests.
#[derive(Clone, Default)]
pub struct Counter(Rc<Cell<i64>>);

impl Counter {
    pub fn new() -> Counter {
        Counter::default()
    }

    pub fn increment(&self) -> i64 {
        self.0.set(self.0.get() + 1);
        self.0.get()
    }

    pub fn get(&self) -> i64 {
        self.0.get()
    }

    pub fn set(&self, value: i64) {
        self.0.set(value);
    }
}

impl ComposeArg for Counter {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A recompose scope a composable hands out, as a captured `var scope: RecomposeScope?` that
/// Compose's tests set to `currentRecomposeScope` and invalidate later.
#[derive(Clone, Default)]
pub struct ScopeHolder(pub Rc<Cell<Option<RecomposeScope>>>);

impl ScopeHolder {
    pub fn new() -> ScopeHolder {
        ScopeHolder::default()
    }

    /// Keeps the scope of the composable composing now.
    pub fn capture(&self) {
        self.0.set(Some(current_recompose_scope()));
    }

    /// Invalidates the scope kept last, if any.
    pub fn invalidate(&self) {
        if let Some(scope) = self.0.get() {
            scope.invalidate();
        }
    }

    pub fn get(&self) -> Option<RecomposeScope> {
        self.0.get()
    }
}

impl ComposeArg for ScopeHolder {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A key that is never the same as itself, as the `NeverEqualObject` of Compose's tests: an
/// effect keyed on it runs again at each pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct NeverEqual;

impl ComposeArg for NeverEqual {
    fn same(&self, _: &Self) -> bool {
        false
    }
}

// ---- Channels ------------------------------------------------------------------------------

/// A test channel between the test and its tasks, as the `Channel`s and `CompletableDeferred`s
/// Compose's effect tests use: an unbounded queue whose receiver awaits the next value.
pub struct Channel<T> {
    inner: Rc<ChannelInner<T>>,
}

struct ChannelInner<T> {
    queue: RefCell<std::collections::VecDeque<T>>,
    waiting: RefCell<Vec<std::task::Waker>>,
    /// Keep only the latest value, as `Channel.CONFLATED`.
    conflated: bool,
}

impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self {
        Channel { inner: self.inner.clone() }
    }
}

impl<T: 'static> ComposeArg for Channel<T> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<T> Default for Channel<T> {
    fn default() -> Self {
        Channel::new()
    }
}

impl<T> Channel<T> {
    pub fn new() -> Channel<T> {
        Channel::with_conflation(false)
    }

    /// A channel keeping only its latest value, as `Channel(Channel.CONFLATED)`.
    pub fn conflated() -> Channel<T> {
        Channel::with_conflation(true)
    }

    fn with_conflation(conflated: bool) -> Channel<T> {
        Channel {
            inner: Rc::new(ChannelInner {
                queue: RefCell::new(Default::default()),
                waiting: RefCell::new(Vec::new()),
                conflated,
            }),
        }
    }

    /// Sends `value`, waking the receivers, as `trySend`.
    pub fn send(&self, value: T) {
        {
            let mut queue = self.inner.queue.borrow_mut();
            if self.inner.conflated {
                queue.clear();
            }
            queue.push_back(value);
        }
        let waiting = std::mem::take(&mut *self.inner.waiting.borrow_mut());
        for waker in waiting {
            waker.wake();
        }
    }

    /// The next value if one is there, as `tryReceive`.
    pub fn try_receive(&self) -> Option<T> {
        self.inner.queue.borrow_mut().pop_front()
    }

    /// Waits for the next value, as `receive()`.
    pub fn receive(&self) -> impl std::future::Future<Output = T> + 'static
    where
        T: 'static,
    {
        let inner = self.inner.clone();
        std::future::poll_fn(move |context| match inner.queue.borrow_mut().pop_front() {
            Some(value) => std::task::Poll::Ready(value),
            None => {
                inner.waiting.borrow_mut().push(context.waker().clone());
                std::task::Poll::Pending
            }
        })
    }
}

// ---- The composition driver ----------------------------------------------------------------

/// The time between two frames of the test clock, as `TestMonotonicFrameClock`'s.
pub const FRAME: Duration = Duration::from_nanos(16_666_667);

/// How far one `advance` moves the clock at most, as `compositionTest`'s `advanceTimeBy(5_000)`.
const ADVANCE_BUDGET: Duration = Duration::from_secs(5);

/// What a `validate` block is.
type Validator = Rc<dyn Fn(&mut Expected)>;

/// A composition of mock views and the clock and tasks around it, as Compose's
/// `compositionTest { … }` scope. The thread's clock is manual while it lives.
pub struct CompositionTest {
    composition: Option<Composition>,
    validator: Option<Validator>,
}

/// Starts a composition test on this thread, as `compositionTest { … }`.
pub fn composition_test() -> CompositionTest {
    set_manual_clock(0);
    CompositionTest { composition: None, validator: None }
}

impl CompositionTest {
    /// Composes `content`, once per test, as `compose { … }`.
    pub fn compose(&mut self, content: impl Fn() + 'static) {
        assert!(self.composition.is_none(), "compose should only be called once");
        let composition = Composition::new(ViewApplier::new());
        composition.set_content(content);
        self.composition = Some(composition);
    }

    pub fn composition(&self) -> &Composition {
        self.composition.as_ref().expect("compose wasn't called")
    }

    /// Runs `f` with the mock views' applier.
    pub fn with_views<R>(&self, f: impl FnOnce(&mut ViewApplier) -> R) -> R {
        self.composition().with_applier(f)
    }

    /// The ids of the views, depth first, as `root.flatten()`.
    pub fn flatten(&self) -> Vec<u64> {
        self.with_views(|views| {
            let mut ids = Vec::new();
            let mut stack: Vec<NodeRef> = views.roots().into_iter().rev().collect();
            while let Some(node) = stack.pop() {
                ids.push(views.tree.get(node).expect("a live view").id);
                let children: Vec<NodeRef> = views.tree.children(node).collect();
                stack.extend(children.into_iter().rev());
            }
            ids
        })
    }

    /// The id of the first text view whose text starts with `prefix`, as `findTextWith`.
    #[track_caller]
    pub fn find_text_with(&self, prefix: &str) -> u64 {
        let found = self.with_views(|views| {
            let mut stack = views.roots();
            while let Some(node) = stack.pop() {
                let view = views.tree.get(node).expect("a live view");
                if view.name == "text"
                    && view.attribute("text").is_some_and(|text| text.starts_with(prefix))
                {
                    return Some(view.id);
                }
                stack.extend(views.tree.children(node));
            }
            None
        });
        found.unwrap_or_else(|| panic!("no text starting with {prefix:?}"))
    }

    /// The views, as an indented dump.
    pub fn dump(&self) -> String {
        self.with_views(|views| views.tree.dump())
    }

    /// The changes passes made so far: to the views, and the slots written and side effects
    /// recorded (what Compose's change list holds).
    pub fn changes(&self) -> u64 {
        self.with_views(|views| views.changes) + recorded_changes()
    }

    /// Checks the views against what `expected` builds, and keeps it for [`revalidate`], as
    /// `validate { … }`.
    ///
    /// [`revalidate`]: CompositionTest::revalidate
    #[track_caller]
    pub fn validate(&mut self, expected: impl Fn(&mut Expected) + 'static) {
        let expected: Validator = Rc::new(expected);
        self.check(&*expected);
        self.validator = Some(expected);
    }

    /// Checks the views against the last `validate`'s expectation again, as `revalidate()`.
    #[track_caller]
    pub fn revalidate(&self) {
        let expected = self.validator.clone().expect("validate was not called");
        self.check(&*expected);
    }

    #[track_caller]
    fn check(&self, expected: &dyn Fn(&mut Expected)) {
        let mut built = Expected::default();
        expected(&mut built);
        let (matched, actual) = self.with_views(|views| {
            let root = views.tree.root();
            (matches(&views.tree, root, &built.views), views.tree.dump())
        });
        if !matched {
            let mut wanted = String::new();
            render(&built.views, 0, &mut wanted);
            panic!("the views don't match\nexpected:\n{wanted}actual:\n{actual}");
        }
    }

    /// Runs what's pending, as `advance()`: tasks, frames (each one's awaiters, then a pass) and
    /// timers, for up to five seconds of the test clock. Returns whether a pass changed the
    /// views. Panics if work is still pending then (an endless recomposition).
    pub fn advance(&mut self) -> bool {
        self.advance_count() != 0
    }

    /// Like [`advance`](CompositionTest::advance), leaving pending work pending, as
    /// `advance(ignorePendingWork = true)`.
    pub fn advance_ignoring_pending_work(&mut self) -> bool {
        self.run_frames(false) != 0
    }

    /// Like [`advance`](CompositionTest::advance), returning the number of frames whose pass
    /// changed the views, as `advanceCount()`.
    pub fn advance_count(&mut self) -> u64 {
        self.run_frames(true)
    }

    fn run_frames(&mut self, check_pending: bool) -> u64 {
        let deadline = now_nanos() + ADVANCE_BUDGET.as_nanos() as u64;
        let mut changed_frames = 0;
        loop {
            run_ready();
            let composition = self.composition();
            if composition.has_invalidations() || has_frame_awaiters() {
                if now_nanos() >= deadline {
                    break;
                }
                let before = self.changes();
                advance_clock(FRAME);
                dispatch_frame();
                self.composition().recompose();
                run_ready();
                if self.changes() != before {
                    changed_frames += 1;
                }
                continue;
            }
            match next_timer_deadline() {
                Some(next) if next <= deadline => {
                    let now = now_nanos();
                    advance_clock(Duration::from_nanos(next.saturating_sub(now)));
                    fire_timers();
                }
                _ => break,
            }
        }
        if check_pending {
            assert!(
                !self.composition().has_invalidations(),
                "potentially infinite recomposition, still recomposing after advancing"
            );
        }
        changed_frames
    }

    /// Moves the clock forward by `duration`, running the tasks, timers and frame awaiters due,
    /// as `advanceTimeBy`. It recomposes nothing: Compose's test scheduler sends no apply
    /// notifications, so its recomposer waits for the next `advance`.
    pub fn advance_time_by(&mut self, duration: Duration) {
        let end = now_nanos() + duration.as_nanos() as u64;
        loop {
            run_ready();
            let next_frame = has_frame_awaiters().then(|| now_nanos() + FRAME.as_nanos() as u64);
            let next = [next_frame, next_timer_deadline()].into_iter().flatten().min();
            match next {
                Some(next) if next <= end => {
                    advance_clock(Duration::from_nanos(next.saturating_sub(now_nanos())));
                    fire_timers();
                    if Some(next) == next_frame {
                        dispatch_frame();
                    }
                }
                _ => {
                    advance_clock(Duration::from_nanos(end.saturating_sub(now_nanos())));
                    fire_timers();
                    run_ready();
                    return;
                }
            }
        }
    }

    /// Asserts the views changed, as `expectChanges()`.
    #[track_caller]
    pub fn expect_changes(&mut self) {
        assert!(self.advance(), "expected changes but none were found");
    }

    /// Asserts the views didn't change, as `expectNoChanges()`.
    #[track_caller]
    pub fn expect_no_changes(&mut self) {
        assert!(!self.advance(), "expected no changes but changes occurred");
    }

    /// Checks the slot table, as `verifyConsistent()`.
    pub fn verify_consistent(&self) {
        self.composition().validate();
    }

    /// Disposes the composition, as `composition.dispose()`.
    pub fn dispose(&mut self) {
        self.composition = None;
    }
}

impl Drop for CompositionTest {
    fn drop(&mut self) {
        // Disposed before the thread's tasks: a test that failed leaves nothing composing.
        self.composition = None;
    }
}
