//! The access window: how a [`LayoutCoordinates`](crate::LayoutCoordinates) held by application
//! code answers while its tree is in a pass. The crate's only `unsafe`, its tests under Miri in
//! both borrow models (docs/09-testing.md, "Miri"; D1211, D1214).
//!
//! A pass holds `&mut UiTree`, and calls application code (a measure policy, a placement block, a
//! node's callback, a draw) that may ask a handle where it is. So a scope passed to application
//! code holds a [`TreeAccess`] instead of a `&mut UiTree`: a raw pointer. While the application
//! code runs the same pointer is in the tree's [`TreeWindow`], and a query derives a shared
//! reference from it. What makes that sound is enforced, not audited:
//!
//! - **Open:** [`TreeAccess::open`] around application code a pass calls. Its guard borrows the
//!   tree for the access's lifetime, so nothing else uses the tree until it's dropped, and
//!   nothing derived from the pointer is alive at that moment: the access is all a site has.
//! - **Entered:** the only way to a `&mut UiTree` from an access is [`TreeAccess::enter`], whose
//!   guard closes the window while it lives. So whatever a scope's method reaches, application
//!   code in it finds the tree busy (its answer is nothing), or opens the window again with a
//!   pointer of its own from a site inside: there's no way to query under a live `&mut`.
//! - **Reads:** [`TreeAccess`]'s `Deref` hands out `&UiTree`, which a query's `&UiTree` may
//!   overlap. A query runs no application code and writes nothing of the tree but `Cell`s and,
//!   for `LayoutCoordinates::alignment_line`, a push to a `RefCell` held for that push alone
//!   (`coordinates.rs`), so reads nest.
//!
//! A window holds the pointer of the innermost site, as each guard puts back what it found, a
//! panic included.

use std::cell::Cell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

use crate::tree::UiTree;

/// Where a tree's pass publishes its pointer for queries. It's in the tree's requests, which its
/// handles reach through the thread's registry, not through the tree: `open` costs a store.
#[derive(Default)]
pub(crate) struct TreeWindow(Cell<Option<NonNull<UiTree>>>);

impl TreeWindow {
    /// Runs `read` with the tree if a pass opened the window. Reads nest.
    pub(crate) fn read<R>(&self, read: impl FnOnce(&UiTree) -> R) -> Option<R> {
        let tree = self.0.get()?;
        // SAFETY: the window holds a pointer only while a site's guard lives (`TreeAccess::open`
        // or `open_to_read`), on this thread (the requests are `!Sync`), with no `&mut` derived
        // from it alive (`TreeAccess::enter` closes the window). Its lifetime is that of the
        // reference it came from, which the guard borrows.
        Some(read(unsafe { tree.as_ref() }))
    }

    /// Opens the window to read `tree` until what it returns is dropped: for application code a
    /// pass runs holding only shared references into the tree (a graphics layer's block, its
    /// node in hand).
    #[inline]
    #[must_use]
    pub(crate) fn open_to_read<'t>(&'t self, tree: &'t UiTree) -> ReadGuard<'t> {
        ReadGuard { window: self, previous: self.0.replace(Some(NonNull::from(tree))) }
    }
}

/// What `open_to_read` returns.
pub(crate) struct ReadGuard<'t> {
    window: &'t TreeWindow,
    previous: Option<NonNull<UiTree>>,
}

impl Drop for ReadGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        self.window.0.set(self.previous);
    }
}

/// A `&'a mut UiTree` that application code may be called under: see the module's documentation.
pub(crate) struct TreeAccess<'a> {
    tree: NonNull<UiTree>,
    _tree: PhantomData<&'a mut UiTree>,
}

impl<'a> TreeAccess<'a> {
    #[inline]
    pub(crate) fn new(tree: &'a mut UiTree) -> TreeAccess<'a> {
        TreeAccess { tree: NonNull::from(tree), _tree: PhantomData }
    }

    /// This access again, for as long as the borrow.
    #[inline]
    pub(crate) fn reborrow(&mut self) -> TreeAccess<'_> {
        TreeAccess { tree: self.tree, _tree: PhantomData }
    }

    /// Opens the window until what it returns is dropped, for application code a pass calls. It
    /// borrows the tree all that time.
    #[inline]
    #[must_use]
    pub(crate) fn open(&self) -> WindowGuard<'a> {
        let window = &self.deref().focus_requests.window;
        WindowGuard {
            tree: self.tree,
            previous: window.0.replace(Some(self.tree)),
            _tree: PhantomData,
        }
    }

    /// The tree to change or to do what may run other code: the window's closed meanwhile.
    #[inline]
    pub(crate) fn enter(&mut self) -> Entered<'_> {
        let window = &self.deref().focus_requests.window;
        Entered { tree: self.tree, previous: window.0.take(), _tree: PhantomData }
    }

    /// What `find` finds in the tree, for as long as the caller wants it: the window's closed
    /// meanwhile.
    pub(crate) fn hold<'s, T: ?Sized>(
        &'s mut self,
        find: impl FnOnce(&'s mut UiTree) -> Option<&'s mut T>,
    ) -> Option<TreeRef<'s, T>> {
        let mut tree = self.tree;
        let entered = self.enter();
        // SAFETY: as `Entered::deref_mut`, for `'s`: `entered` lives as long as what's found, in
        // one value, and borrows this access meanwhile.
        let value = find(unsafe { tree.as_mut() })?;
        Some(TreeRef { value, _entered: entered })
    }
}

impl Deref for TreeAccess<'_> {
    type Target = UiTree;

    #[inline]
    fn deref(&self) -> &UiTree {
        // SAFETY: from the `&'a mut UiTree` this access took, which nothing else uses for `'a`.
        // Any `&mut` from it is through `enter`, which borrows this access mutably.
        unsafe { self.tree.as_ref() }
    }
}

/// What `open` returns: puts the window back when dropped.
pub(crate) struct WindowGuard<'a> {
    tree: NonNull<UiTree>,
    previous: Option<NonNull<UiTree>>,
    _tree: PhantomData<&'a mut UiTree>,
}

impl Drop for WindowGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: the guard borrows the tree for `'a`, so what it points to is alive and nothing
        // but its access has used it; nothing derived from it is alive as it ends.
        unsafe { self.tree.as_ref() }.focus_requests.window.0.set(self.previous);
    }
}

/// The tree, its window closed, as `TreeAccess::enter` gives it.
pub(crate) struct Entered<'s> {
    tree: NonNull<UiTree>,
    previous: Option<NonNull<UiTree>>,
    _tree: PhantomData<&'s mut UiTree>,
}

impl Deref for Entered<'_> {
    type Target = UiTree;

    #[inline]
    fn deref(&self) -> &UiTree {
        // SAFETY: the window's closed, so nothing else derives from the pointer while this lives,
        // and this borrows the access mutably.
        unsafe { self.tree.as_ref() }
    }
}

impl DerefMut for Entered<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut UiTree {
        // SAFETY: as `deref`.
        unsafe { self.tree.as_mut() }
    }
}

impl Drop for Entered<'_> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: as `WindowGuard::drop`: nothing from `deref_mut` outlives `self`.
        unsafe { self.tree.as_ref() }.focus_requests.window.0.set(self.previous);
    }
}

/// Something in the tree a node has in hand (an ancestor it found): what asks where something is
/// meanwhile finds the tree busy, as this has part of it. To ask, copy the handle out, drop this,
/// then ask.
pub struct TreeRef<'s, T: ?Sized> {
    value: &'s mut T,
    _entered: Entered<'s>,
}

impl<T: ?Sized> Deref for TreeRef<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.value
    }
}

impl<T: ?Sized> DerefMut for TreeRef<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

// Each runs a real tree's passes with application code asking coordinates in the middle of them;
// the rest of the boundary is in the suite's `coordinates_in_passes`.
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

    /// Asked with nothing open while the tree's borrowed: zeros, no panic.
    #[test]
    fn a_tree_busy_answers_nothing() {
        let kept: Kept = Rc::default();
        let k = kept.clone();
        let host = host(move || leaf(at(3, 4).then(keep(&k))));
        let coordinates = kept.get().expect("positioned");
        assert_eq!(host.with_tree(|_| coordinates.position_in_root()), Offset::ZERO);
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

    /// A measurer's `&mut self`, a query inside its `layout`, the concrete one's: what's asked
    /// reads no part of the measurer, as it's behind its own cell.
    #[test]
    fn a_query_inside_a_measurer_s_layout() {
        use crate::fixed_text::FixedTextMeasurer;
        use crate::text::{ParagraphHandle, ParagraphMetrics, TextMeasurer, TextPosition};
        struct Asking(FixedTextMeasurer, Kept, Rc<Cell<Offset>>, u32);
        impl TextMeasurer for Asking {
            fn layout(
                &mut self,
                text: &str,
                style: &crate::TextStyle,
                options: crate::text::TextLayoutOptions,
                constraints: crate::constraints::Constraints,
                density: &crate::unit::Density,
            ) -> ParagraphHandle {
                self.3 += 1;
                if let Some(coordinates) = self.1.get() {
                    self.2.set(coordinates.position_in_root());
                }
                self.0.layout(text, style, options, constraints, density)
            }
            fn layout_annotated(
                &mut self,
                text: &crate::annotated::AnnotatedString,
                style: &crate::TextStyle,
                options: crate::text::TextLayoutOptions,
                constraints: crate::constraints::Constraints,
                density: &crate::unit::Density,
            ) -> ParagraphHandle {
                self.0.layout_annotated(text, style, options, constraints, density)
            }
            fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics {
                self.0.metrics(paragraph)
            }
            fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition {
                self.0.hit_test(paragraph, position)
            }
            fn cursor_rect(&self, paragraph: ParagraphHandle, offset: usize) -> crate::unit::Rect {
                self.0.cursor_rect(paragraph, offset)
            }
            fn text(&self, paragraph: ParagraphHandle) -> &str {
                self.0.text(paragraph)
            }
            fn release(&mut self, paragraph: ParagraphHandle) {
                self.0.release(paragraph)
            }
        }
        let kept: Kept = Rc::default();
        let again = mutable_state_of(0);
        let seen = Rc::new(Cell::new(Offset::ZERO));
        let (k, a) = (kept.clone(), again.handle());
        let measurer = Asking(FixedTextMeasurer::new(), kept.clone(), seen.clone(), 0);
        let mut host = UiHost::new(
            crate::unit::IntSize::new(100, 100),
            crate::unit::Density::new(1.0),
            Box::new(measurer),
        );
        host.set_content(move || {
            let k = k.clone();
            Layout(
                at(3, 4).then(keep(&k)),
                measure_policy(move |scope, _, _| {
                    let text = crate::TextStyle::default();
                    let round = a.get() as f32;
                    let paragraph = scope.text_measurer().layout(
                        "text",
                        &text,
                        Default::default(),
                        crate::constraints::Constraints::fixed(50 + round as i32, 50),
                        &crate::unit::Density::new(1.0),
                    );
                    scope.text_measurer().release(paragraph);
                    scope.layout(5, 5)
                }),
                compose_runtime::callback(|| {}),
            );
        });
        host.frame();
        again.set(1);
        host.frame();
        assert_eq!(seen.get(), Offset::new(3.0, 4.0));
    }
}
