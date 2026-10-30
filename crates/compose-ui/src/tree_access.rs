//! The access window: how a [`LayoutCoordinates`](crate::LayoutCoordinates) held by application
//! code answers while its tree is in a pass. The crate's only `unsafe`, its tests running under
//! Miri (`cargo +nightly miri test -p compose-ui --lib tree_access`), as the design's answers ask
//! (docs/parity/design/placement-and-lookahead.md §4.2, D1211).
//!
//! A pass holds `&mut UiTree`, and calls application code (a measure policy, a placement block, a
//! node's callback, a draw) that may ask a handle where it is. So a scope passed to application
//! code holds a [`TreeAccess`] instead of a `&mut UiTree`: a raw pointer, from which each of the
//! scope's own methods derives a reference that ends as the method does. While the application
//! code runs, the same pointer is in the tree's [`TreeWindow`], and a query derives its reference
//! from it. So two references to the tree are never used alternately:
//!
//! - **Open:** `open` around application code that a pass calls, with nothing derived from the
//!   pointer alive. What a site borrowed from the tree before (a chain entry) it must have put
//!   down: a query may read any of it.
//! - **Shut:** `shut` around what a scope does that may reach other application code: there, a
//!   reference derived from the pointer is alive up the stack. Closed, a query there is busy: an
//!   answer of nothing, never undefined behaviour.
//!
//! A window always holds the pointer of the innermost open site, as each `open` and `shut` puts
//! back what it found when it ends, a panic included.

use std::cell::Cell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

use crate::tree::UiTree;

/// Where a tree's pass publishes its pointer for queries. It's in the tree's requests, which its
/// handles reach through the thread's registry, not through the tree.
#[derive(Default)]
pub(crate) struct TreeWindow {
    tree: Cell<Option<NonNull<UiTree>>>,
    /// The pointer is from a shared reference, with others alive: only `read` may use it.
    read_only: Cell<bool>,
}

impl TreeWindow {
    /// Closes the window, for what moves the tree away from where its pointer points: what it
    /// held, for `reopen`.
    pub(crate) fn close(&self) -> Option<NonNull<UiTree>> {
        self.tree.take()
    }

    pub(crate) fn reopen(&self, value: Option<NonNull<UiTree>>) {
        self.tree.set(value);
    }

    /// Opens the window to read `tree` until what it returns is dropped: for application code
    /// run with shared references into the tree alive (a graphics layer's block, its node in
    /// hand). A query needing the tree alone finds it busy.
    #[inline]
    #[must_use]
    pub(crate) fn open_to_read<'t>(&'t self, tree: &'t UiTree) -> ReadGuard<'t> {
        ReadGuard {
            window: self,
            previous: self.tree.replace(Some(NonNull::from(tree))),
            was_read_only: self.read_only.replace(true),
        }
    }

    /// Runs `read` with the tree if a pass opened the window. Reads nest (a query running a
    /// graphics layer's block, which asks another).
    pub(crate) fn read<R>(&self, read: impl FnOnce(&UiTree) -> R) -> Option<R> {
        let tree = self.tree.get()?;
        // SAFETY: the window holds a pointer only between `TreeAccess::open` and the end of what
        // it returned, on this thread (the requests are `!Sync`), with no reference derived from
        // it alive but other reads: see the module's documentation. The pointer's lifetime is
        // that of the `&mut UiTree` it came from, alive up the stack.
        Some(read(unsafe { tree.as_ref() }))
    }

    /// Runs `query` with the tree if a pass opened the window, closed meanwhile: what it runs (a
    /// graphics layer's block, a modifier node's clip) finds the tree busy.
    pub(crate) fn exclusive<R>(&self, query: impl FnOnce(&mut UiTree) -> R) -> Option<R> {
        if self.read_only.get() {
            return None;
        }
        let mut tree = self.tree.get()?;
        let _restore = Restore { window: self, value: self.tree.take() };
        // SAFETY: as in `read`, and with the window closed no other reference can be derived from
        // the pointer until this one is gone.
        Some(query(unsafe { tree.as_mut() }))
    }
}

/// Puts a window back as it was.
struct Restore<'w> {
    window: &'w TreeWindow,
    value: Option<NonNull<UiTree>>,
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        self.window.tree.set(self.value);
    }
}

/// What `open_to_read` returns.
pub(crate) struct ReadGuard<'t> {
    window: &'t TreeWindow,
    previous: Option<NonNull<UiTree>>,
    was_read_only: bool,
}

impl Drop for ReadGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        self.window.tree.set(self.previous);
        self.window.read_only.set(self.was_read_only);
    }
}

/// A `&'a mut UiTree` that application code may be called under: see the module's documentation.
pub(crate) struct TreeAccess<'a> {
    tree: NonNull<UiTree>,
    _tree: PhantomData<&'a mut UiTree>,
}

impl<'a> TreeAccess<'a> {
    pub(crate) fn new(tree: &'a mut UiTree) -> TreeAccess<'a> {
        TreeAccess { tree: NonNull::from(tree), _tree: PhantomData }
    }

    /// This access again, for as long as the borrow.
    pub(crate) fn reborrow(&mut self) -> TreeAccess<'_> {
        TreeAccess { tree: self.tree, _tree: PhantomData }
    }

    /// Opens the window until what it returns is dropped: for application code a pass calls.
    #[inline]
    #[must_use]
    pub(crate) fn open(&self) -> WindowGuard {
        self.set_window(Some(self.tree))
    }

    /// Closes the window until what it returns is dropped: for what a scope does that may reach
    /// other application code.
    #[inline]
    #[must_use]
    pub(crate) fn shut(&self) -> WindowGuard {
        self.set_window(None)
    }

    #[inline]
    fn set_window(&self, value: Option<NonNull<UiTree>>) -> WindowGuard {
        let window = &self.deref().focus_requests.window;
        WindowGuard { tree: self.tree, previous: window.tree.replace(value) }
    }
}

impl Deref for TreeAccess<'_> {
    type Target = UiTree;

    #[inline]
    fn deref(&self) -> &UiTree {
        // SAFETY: from the `&'a mut UiTree` this access took, which nothing else uses for `'a`:
        // only what this access gives out, and each ends before the next (the borrow checker's
        // for this access; for a window's, the module's documentation).
        unsafe { self.tree.as_ref() }
    }
}

impl DerefMut for TreeAccess<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut UiTree {
        // SAFETY: as in `deref`.
        unsafe { self.tree.as_mut() }
    }
}

/// What `open` and `shut` return: puts the window back when dropped.
pub(crate) struct WindowGuard {
    tree: NonNull<UiTree>,
    previous: Option<NonNull<UiTree>>,
}

impl Drop for WindowGuard {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: the guard is dropped in the scope that made it, with the tree's `&mut` still
        // alive up the stack and nothing derived from its pointer alive: a site's guard ends
        // after the application code it wraps, a `shut`'s after the scope's call.
        unsafe { self.tree.as_ref() }.focus_requests.window.tree.set(self.previous);
    }
}

// Under Miri: `cargo +nightly miri test -p compose-ui --lib tree_access`. Each runs a real tree's
// passes with application code asking coordinates in the middle of them.
#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use compose_runtime::mutable_state_of;

    use crate::coordinates::LayoutCoordinates;
    use crate::host::UiHost;
    use crate::layout::{Layout, Measurable, measure_policy};
    use crate::modifier::Modifier;
    use crate::unit::Offset;

    fn host(content: impl Fn() + 'static) -> UiHost {
        let mut host = UiHost::headless(100, 100);
        host.set_content(content);
        host.frame();
        host
    }

    type Kept = Rc<Cell<Option<LayoutCoordinates>>>;

    fn keep(kept: &Kept) -> Modifier {
        let kept = kept.clone();
        Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
            kept.set(Some(coordinates));
        })
    }

    fn at(x: i32, y: i32) -> Modifier {
        Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            scope.place(&placeable, x, y);
            scope.layout(placeable.width.max(1), placeable.height.max(1))
        })
    }

    fn leaf(modifier: Modifier) {
        Layout(
            modifier,
            measure_policy(|scope, _, _| scope.layout(5, 5)),
            compose_runtime::callback(|| {}),
        );
    }

    /// A query in a placement block, between what the block does to the tree.
    #[test]
    fn a_block_asks_between_placing_and_measuring() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (first, second) = (seen.clone(), seen.clone());
        let policy = measure_policy(move |scope, measurables, _| {
            let child: Measurable = measurables[0];
            let (first, second) = (first.clone(), second.clone());
            scope.layout_with(50, 50, move |placement| {
                let coordinates = placement.coordinates().expect("placing");
                first.borrow_mut().push(coordinates.position_in_root());
                let constraints = crate::constraints::Constraints::fixed(5, 5);
                let placeable = placement.measure(child, constraints);
                assert_eq!(coordinates.size(), crate::unit::IntSize::new(50, 50));
                placement.place(&placeable, 1, 2);
                second.borrow_mut().push(coordinates.position_in_root());
                let bounds = coordinates.bounds_in_root();
                second.borrow_mut().push(Offset::new(bounds.left, bounds.top));
            })
        });
        let _host = host(move || {
            Layout(at(3, 4), policy.clone(), compose_runtime::callback(|| leaf(Modifier)));
        });
        assert_eq!(seen.borrow()[0], Offset::new(3.0, 4.0));
        assert_eq!(seen.borrow()[2], Offset::new(3.0, 4.0));
    }

    /// A query in a child's measuring code, under a block, under another measurement.
    #[test]
    fn measuring_code_asks_while_its_parents_are_on_the_stack() {
        let kept: Kept = Rc::default();
        let again = mutable_state_of(0);
        let (a, k, seen) = (again.handle(), kept.clone(), Rc::new(Cell::new(Offset::ZERO)));
        let (s, k2) = (seen.clone(), kept.clone());
        let child = measure_policy(move |scope, _, _| {
            if let Some(coordinates) = k2.get() {
                s.set(coordinates.position_in_root());
            }
            scope.layout(5, 5)
        });
        // New constraints in the block: the child's measured under it.
        let policy = measure_policy(move |scope, measurables, _| {
            let child = measurables[0];
            scope.layout_with(50, 50, move |placement| {
                let size = 5 + a.get();
                let constraints = crate::constraints::Constraints::fixed(size, size);
                let placeable = placement.measure(child, constraints);
                placement.place(&placeable, 0, 0);
            })
        });
        let mut host = host(move || {
            let (k, child) = (k.clone(), child.clone());
            Layout(
                at(3, 4).then(keep(&k)),
                policy.clone(),
                compose_runtime::callback(move || {
                    Layout(Modifier, child.clone(), compose_runtime::callback(|| {}));
                }),
            );
        });
        again.set(1);
        host.frame();
        assert_eq!(seen.get(), Offset::new(3.0, 4.0));
    }

    /// A query in a draw, in a drawing's content, and in a layer's block, kept across a frame.
    #[test]
    fn drawing_and_layers_ask() {
        let kept: Kept = Rc::default();
        let again = mutable_state_of(0);
        let seen: Rc<RefCell<Vec<Offset>>> = Rc::default();
        let (k, s, a) = (kept.clone(), seen.clone(), again.handle());
        let mut host = host(move || {
            let (s1, s2, s3, k1, k2, k3) =
                (s.clone(), s.clone(), s.clone(), k.clone(), k.clone(), k.clone());
            a.get();
            let note = |seen: &Rc<RefCell<Vec<Offset>>>, kept: &Kept| {
                if let Some(coordinates) = kept.get() {
                    seen.borrow_mut().push(coordinates.position_in_root());
                }
            };
            leaf(
                at(3, 4)
                    .then(keep(&k))
                    .draw_behind(move |_| note(&s1, &k1))
                    .graphics_layer(move |_| note(&s2, &k2))
                    .draw_with_content(move |scope| {
                        note(&s3, &k3);
                        scope.draw_content();
                        note(&s3, &k3);
                    }),
            );
        });
        again.set(1);
        host.frame();
        assert!(seen.borrow().len() >= 4, "{:?}", seen.borrow());
        assert!(seen.borrow().iter().all(|at| *at == Offset::new(3.0, 4.0)));
    }

    /// A query in a node's callback that then measures through it, in `on_placed` of a node told
    /// in the tree's pass.
    #[test]
    fn a_node_told_it_was_placed_asks() {
        use crate::layout_callbacks::LayoutAwareModifierNode;
        use crate::modifier::{ModifierNode, ModifierNodeElement};
        use crate::node_context::NodeContext;

        #[derive(Clone)]
        struct Asking(Rc<Cell<Offset>>);
        impl ModifierNode for Asking {
            fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
                Some(self)
            }
        }
        impl LayoutAwareModifierNode for Asking {
            fn on_placed(&mut self, coordinates: LayoutCoordinates, context: &mut NodeContext<'_>) {
                self.0.set(coordinates.position_in_root());
                context.invalidate_draw();
                let bounds = coordinates.bounds_in_root();
                self.0.set(coordinates.position_in_root() + Offset::new(bounds.left, bounds.top));
            }
        }
        struct Element(Rc<Cell<Offset>>);
        impl ModifierNodeElement for Element {
            type Node = Asking;
            fn create(&self) -> Asking {
                Asking(self.0.clone())
            }
            fn update(&self, _: &mut Asking) {}
            fn same(&self, _: &Self) -> bool {
                true
            }
        }
        let seen = Rc::new(Cell::new(Offset::ZERO));
        let s = seen.clone();
        let _host = host(move || leaf(at(3, 4).then_custom(Element(s.clone()))));
        assert_eq!(seen.get(), Offset::new(6.0, 8.0));
    }

    /// Asked with nothing open while the tree's borrowed: nothing, a debug build saying so.
    #[test]
    fn a_tree_busy_answers_nothing() {
        let kept: Kept = Rc::default();
        let k = kept.clone();
        let host = host(move || leaf(at(3, 4).then(keep(&k))));
        let coordinates = kept.get().expect("positioned");
        let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            host.with_tree(|_| coordinates.position_in_root())
        }));
        if cfg!(debug_assertions) {
            assert!(answer.is_err());
        } else {
            assert_eq!(answer.ok(), Some(Offset::ZERO));
        }
        assert_eq!(coordinates.position_in_root(), Offset::new(3.0, 4.0), "free again");
    }

    /// A window opened in a panic's way is put back.
    #[test]
    fn a_panic_puts_the_window_back() {
        let panics = mutable_state_of(false);
        let kept: Kept = Rc::default();
        let (k, p) = (kept.clone(), panics.handle());
        let policy = measure_policy(move |scope, _, _| {
            assert!(!p.get(), "asked to");
            scope.layout(5, 5)
        });
        let mut host = host(move || {
            Layout(at(3, 4).then(keep(&k)), policy.clone(), compose_runtime::callback(|| {}));
        });
        panics.set(true);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            host.frame();
        }));
        assert!(result.is_err());
        // A kept pointer would answer from a frame long gone.
        host.with_tree(|tree| assert!(tree.focus_requests.window.read(|_| ()).is_none()));
    }
}
