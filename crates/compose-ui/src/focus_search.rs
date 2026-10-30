//! Focus searches over the targets' tree, as Compose's (`focus/FocusTraversal.kt`,
//! `OneDimensionalFocusSearch.kt`, `TwoDimensionalFocusSearch.kt`, `FocusOwnerImpl.moveFocus`):
//! the active target's custom destination first, then a search from the root.

use std::cmp::Ordering;

use smallvec::SmallVec;

use crate::alignment::LayoutDirection;
use crate::focus::{FocusPlace, FocusState, FocusTargetRef, MOST_BEYOND_STEPS};
use crate::focus_manager::FocusDirection;
use crate::focus_requester::FocusRequester;
use crate::input::Action;
use crate::node_chain::NodeKinds;
use crate::tree::{NodeId, UiTree};
use crate::unit::Rect;

/// What a search calls with what it found: whether it took the focus (the search goes on if not).
pub(crate) type OnFound<'a> = &'a mut dyn FnMut(&mut UiTree, FocusTargetRef) -> bool;

/// A child with what it's sorted by: whether a search looks at it, its path from the root.
type KeyedChild = (FocusTargetRef, bool, SmallVec<[NodeId; 16]>);

/// What `custom_focus_search` says.
enum CustomSearch {
    Default,
    Cancel,
    Redirect,
    Requester(FocusRequester),
}

impl UiTree {
    /// Moves the focus in `direction`, as `FocusOwner.moveFocus`. Returns whether it moved, and
    /// the actions focus listeners queued.
    ///
    /// A search from inside a lazy list that finds nothing more inside it asks the list for the
    /// next item beyond its bounds, and is made again once it's laid out (at the next layout, in
    /// the frame), as Compose's `BeyondBoundsLayout`; moves asked meanwhile wait their turn
    /// (D477).
    pub fn move_focus_in(&mut self, direction: FocusDirection) -> (bool, Vec<Action>) {
        let moved = self.move_focus_with(direction, true);
        (moved, self.take_pending_actions())
    }

    /// `move_focus_in` that doesn't wrap around, as `moveFocus(direction,
    /// wrapAroundForOneDimensionalFocus = false)`, internal in Compose. For tests.
    #[doc(hidden)]
    pub fn move_focus_without_wrapping(
        &mut self,
        direction: FocusDirection,
    ) -> (bool, Vec<Action>) {
        let moved = self.move_focus_with(direction, false);
        (moved, self.take_pending_actions())
    }

    /// Moves focus to the next (or previous) target, wrapping around, as Tab does.
    pub fn move_focus(&mut self, forward: bool) -> Vec<Action> {
        let direction = if forward { FocusDirection::Next } else { FocusDirection::Previous };
        self.move_focus_in(direction).1
    }

    pub(crate) fn move_focus_with(&mut self, direction: FocusDirection, wrap: bool) -> bool {
        if self.has_beyond_bounds_moves() {
            if self.beyond_search.window == self.focus.window {
                self.beyond_search.moves.push_back(direction);
                return true;
            }
            // Another window's move: it wins, as a focus change from outside does (D495). Each
            // window could wait on its own, were there a search per window.
            self.beyond_search.moves.clear();
            self.end_beyond_bounds();
        }
        self.focus.move_stopped = false;
        let moved = self.search_and_move_focus(direction, wrap);
        if !self.focus.search_waiting {
            self.end_beyond_bounds();
        }
        self.focus.search_waiting = false;
        moved
    }

    /// The body of `moveFocus`.
    fn search_and_move_focus(&mut self, direction: FocusDirection, wrap: bool) -> bool {
        let mut request_success = false;
        let active_before = self.focus.active;
        let found = self.focus_search(direction, None, &mut |tree, target| {
            request_success = tree.request_focus_target(target, direction);
            request_success
        });
        if self.focus.search_waiting {
            // It goes on as the list is laid out, and it's expected to find something.
            return true;
        }
        if found == Some(true) && active_before != self.focus.active {
            // A request during the search took the focus.
            return true;
        }
        // Cancelled.
        let Some(found) = found else {
            self.focus.move_stopped = true;
            return false;
        };
        if found && request_success {
            return true;
        }
        if is_one_dimensional(direction) && wrap {
            // Wraps around: clears the focus and takes it as initially.
            if !self.clear_focus_with(false, true, direction) {
                self.focus.move_stopped = true;
                return false;
            }
            return self.take_focus(direction);
        }
        false
    }

    /// Whether `target` is the own target of the modal overlay showing: a window's root, which
    /// doesn't take the focus by a search (D1269).
    fn is_modal_root(&self, target: FocusTargetRef) -> bool {
        self.modal_overlay() == Some(target.node)
            && self.target_index(target).is_some_and(|index| index < self.root_branch().1)
    }

    /// As `FocusOwner.takeFocus`.
    fn take_focus(&mut self, direction: FocusDirection) -> bool {
        self.focus_search(direction, None, &mut |tree, target| {
            tree.request_focus_target(target, direction)
        })
        .unwrap_or(false)
    }

    /// As `FocusOwner.focusSearch`: `None` if it was cancelled.
    pub(crate) fn focus_search(
        &mut self,
        direction: FocusDirection,
        focused_rect: Option<Rect>,
        on_found: OnFound<'_>,
    ) -> Option<bool> {
        let source = self.focus.active;
        if let Some(source) = source {
            match self.custom_focus_search(source, direction) {
                CustomSearch::Cancel => return None,
                CustomSearch::Redirect => {
                    let active = self.focus.active?;
                    return Some(on_found(self, active));
                }
                CustomSearch::Default => {}
                CustomSearch::Requester(requester) => {
                    return Some(requester.find_focus_target_on_tree(self, on_found));
                }
            }
        }
        let root_direction = self.node(self.root_id()).layout_direction;
        let mut guard = |tree: &mut UiTree, target: FocusTargetRef| -> bool {
            if Some(target) == source { false } else { on_found(tree, target) }
        };
        self.root_focus_search(direction, root_direction, focused_rect, &mut guard)
    }

    /// As `customFocusSearch`.
    fn custom_focus_search(
        &mut self,
        target: FocusTargetRef,
        direction: FocusDirection,
    ) -> CustomSearch {
        let layout_direction = self.node(self.root_id()).layout_direction;
        let properties = self.fetch_focus_properties(target);
        let (relative, physical) = match (direction, layout_direction) {
            (FocusDirection::Next, _) => (FocusRequester::DEFAULT, properties.next),
            (FocusDirection::Previous, _) => (FocusRequester::DEFAULT, properties.previous),
            (FocusDirection::Up, _) => (FocusRequester::DEFAULT, properties.up),
            (FocusDirection::Down, _) => (FocusRequester::DEFAULT, properties.down),
            (FocusDirection::Left, LayoutDirection::Ltr) => (properties.start, properties.left),
            (FocusDirection::Left, LayoutDirection::Rtl) => (properties.end, properties.left),
            (FocusDirection::Right, LayoutDirection::Ltr) => (properties.end, properties.right),
            (FocusDirection::Right, LayoutDirection::Rtl) => (properties.start, properties.right),
            (FocusDirection::Enter | FocusDirection::Exit, _) => {
                let block = if direction == FocusDirection::Enter {
                    &properties.on_enter
                } else {
                    &properties.on_exit
                };
                let Some(block) = block else { return CustomSearch::Default };
                let active_before = self.focus.active;
                let canceled = self.run_enter_exit_block(block, direction);
                return if canceled {
                    CustomSearch::Cancel
                } else if active_before != self.focus.active {
                    CustomSearch::Redirect
                } else {
                    CustomSearch::Default
                };
            }
        };
        let requester = if relative != FocusRequester::DEFAULT { relative } else { physical };
        if requester == FocusRequester::DEFAULT {
            CustomSearch::Default
        } else if requester == FocusRequester::CANCEL {
            CustomSearch::Cancel
        } else {
            CustomSearch::Requester(requester)
        }
    }

    /// `rootFocusNode.focusSearch`.
    fn root_focus_search(
        &mut self,
        direction: FocusDirection,
        layout_direction: LayoutDirection,
        focused_rect: Option<Rect>,
        on_found: OnFound<'_>,
    ) -> Option<bool> {
        match direction {
            FocusDirection::Next => Some(self.forward_focus_search(None, on_found)),
            FocusDirection::Previous => Some(self.backward_focus_search(None, on_found)),
            FocusDirection::Left
            | FocusDirection::Right
            | FocusDirection::Up
            | FocusDirection::Down => {
                Some(self.two_dimensional_focus_search(None, direction, focused_rect, on_found))
            }
            FocusDirection::Enter => {
                // Among the children of the active target.
                let direction = match layout_direction {
                    LayoutDirection::Rtl => FocusDirection::Left,
                    LayoutDirection::Ltr => FocusDirection::Right,
                };
                let active = self.focus.active?;
                Some(self.two_dimensional_focus_search(
                    Some(active),
                    direction,
                    focused_rect,
                    on_found,
                ))
            }
            FocusDirection::Exit => {
                let active = self.focus.active?;
                for ancestor in self.targets_above(active) {
                    if !self.is_modal_root(ancestor)
                        && self.fetch_focus_properties(ancestor).can_focus
                    {
                        return Some(on_found(self, ancestor));
                    }
                }
                Some(false)
            }
        }
    }

    // ---- One dimension -------------------------------------------------------------------------

    fn forward_focus_search(&mut self, place: FocusPlace, on_found: OnFound<'_>) -> bool {
        match self.focus_state_at(place) {
            FocusState::ActiveParent => {
                let Some(child) = self.active_child(place) else { return false };
                self.forward_focus_search(Some(child), on_found)
                    || self.generate_and_search_children_1d(
                        place,
                        child,
                        FocusDirection::Next,
                        on_found,
                    )
            }
            FocusState::Active | FocusState::Captured => {
                self.pick_child_for_forward_search(place, on_found)
            }
            FocusState::Inactive => match place {
                Some(target) if self.fetch_focus_properties(target).can_focus => {
                    on_found(self, target)
                }
                _ => self.pick_child_for_forward_search(place, on_found),
            },
        }
    }

    fn backward_focus_search(&mut self, place: FocusPlace, on_found: OnFound<'_>) -> bool {
        match self.focus_state_at(place) {
            FocusState::ActiveParent => {
                let Some(child) = self.active_child(place) else { return false };
                // Unlike a forward search, children before the parent.
                match self.focus_state_of(child) {
                    FocusState::ActiveParent => {
                        self.backward_focus_search(Some(child), on_found)
                            || self.generate_and_search_children_1d(
                                place,
                                child,
                                FocusDirection::Previous,
                                on_found,
                            )
                            || (!self.is_modal_root(child)
                                && self.fetch_focus_properties(child).can_focus
                                && on_found(self, child))
                    }
                    // It has the focus: its children were visited, only its siblings are left.
                    FocusState::Active | FocusState::Captured => self
                        .generate_and_search_children_1d(
                            place,
                            child,
                            FocusDirection::Previous,
                            on_found,
                        ),
                    FocusState::Inactive => false,
                }
            }
            // A search from the root meeting an active target that isn't a parent: an initial
            // focus. One of its children.
            FocusState::Active | FocusState::Captured => {
                self.pick_child_for_backward_search(place, on_found)
            }
            FocusState::Inactive => {
                self.pick_child_for_backward_search(place, on_found)
                    || match place {
                        Some(target) if self.fetch_focus_properties(target).can_focus => {
                            on_found(self, target)
                        }
                        _ => false,
                    }
            }
        }
    }

    /// Among `place`'s children; finding nothing, generating more beyond its bounds.
    fn generate_and_search_children_1d(
        &mut self,
        place: FocusPlace,
        focused_item: FocusTargetRef,
        direction: FocusDirection,
        on_found: OnFound<'_>,
    ) -> bool {
        self.search_children_1d(place, focused_item, direction, on_found)
            || self.search_beyond_bounds(place, direction)
    }

    /// The next sibling of `focused_item` that takes the focus; a backward search ending there
    /// moves to the parent, unless it's deactivated or the root.
    fn search_children_1d(
        &mut self,
        place: FocusPlace,
        focused_item: FocusTargetRef,
        direction: FocusDirection,
        on_found: OnFound<'_>,
    ) -> bool {
        let children = self.sorted_focus_children(place);
        let at = children.iter().position(|child| *child == focused_item);
        match direction {
            FocusDirection::Next => {
                if let Some(at) = at {
                    for &child in &children[at + 1..] {
                        if self.is_eligible_for_focus_search(child)
                            && self.forward_focus_search(Some(child), on_found)
                        {
                            return true;
                        }
                    }
                }
                false
            }
            _ => {
                if let Some(at) = at {
                    for &child in children[..at].iter().rev() {
                        if self.is_eligible_for_focus_search(child)
                            && self.backward_focus_search(Some(child), on_found)
                        {
                            return true;
                        }
                    }
                }
                match place {
                    Some(target)
                        if !self.is_modal_root(target)
                            && self.fetch_focus_properties(target).can_focus =>
                    {
                        on_found(self, target)
                    }
                    _ => false,
                }
            }
        }
    }

    fn pick_child_for_forward_search(&mut self, place: FocusPlace, on_found: OnFound<'_>) -> bool {
        for child in self.sorted_focus_children(place) {
            if self.is_eligible_for_focus_search(child)
                && self.forward_focus_search(Some(child), on_found)
            {
                return true;
            }
        }
        false
    }

    fn pick_child_for_backward_search(&mut self, place: FocusPlace, on_found: OnFound<'_>) -> bool {
        for child in self.sorted_focus_children(place).into_iter().rev() {
            if self.is_eligible_for_focus_search(child)
                && self.backward_focus_search(Some(child), on_found)
            {
                return true;
            }
        }
        false
    }

    /// `place`'s children by the placement order of their layout ancestors under their common
    /// ancestor, as `FocusableChildrenComparator`, those a search won't look at last.
    fn sorted_focus_children(&mut self, place: FocusPlace) -> SmallVec<[FocusTargetRef; 8]> {
        let children = self.focus_children(place);
        if children.len() < 2 {
            return children;
        }
        let mut keyed: SmallVec<[KeyedChild; 8]> = children
            .into_iter()
            .map(|child| {
                let eligible = self.is_eligible_for_focus_search(child);
                let mut path: SmallVec<[NodeId; 16]> = SmallVec::new();
                let mut current = Some(child.node);
                while let Some(id) = current {
                    path.push(id);
                    current = self.get(id).and_then(|value| value.parent);
                }
                path.reverse();
                (child, eligible, path)
            })
            .collect();
        keyed.sort_by(|first, second| match (first.1, second.1) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => Ordering::Equal,
            (true, true) => {
                for (one, other) in first.2.iter().zip(second.2.iter()) {
                    if one != other {
                        return self.node(*one).place_order.cmp(&self.node(*other).place_order);
                    }
                }
                Ordering::Equal
            }
        });
        keyed.into_iter().map(|(child, _, _)| child).collect()
    }

    // ---- Two dimensions ------------------------------------------------------------------------

    /// As `twoDimensionalFocusSearch`.
    fn two_dimensional_focus_search(
        &mut self,
        place: FocusPlace,
        direction: FocusDirection,
        previously_focused_rect: Option<Rect>,
        on_found: OnFound<'_>,
    ) -> bool {
        match self.focus_state_at(place) {
            FocusState::Inactive => {
                if let Some(target) = place
                    && self.fetch_focus_properties(target).can_focus
                {
                    return on_found(self, target);
                }
                match previously_focused_rect {
                    None => {
                        self.find_child_corresponding_to_focus_enter(place, direction, on_found)
                    }
                    Some(rect) => self.search_children_2d(place, rect, direction, on_found),
                }
            }
            FocusState::ActiveParent => {
                let Some(child) = self.active_child(place) else { return false };
                // Only among siblings: entering children takes `Enter`. It delegates to a child
                // that has the focus below it, and skips one that is focused.
                match self.focus_state_of(child) {
                    FocusState::ActiveParent => {
                        if self.two_dimensional_focus_search(
                            Some(child),
                            direction,
                            previously_focused_rect,
                            on_found,
                        ) {
                            return true;
                        }
                        let rect = match previously_focused_rect {
                            Some(rect) => rect,
                            None => match self.focus.active {
                                Some(active) => self.focus_rect_of(active),
                                None => return false,
                            },
                        };
                        self.generate_and_search_children_2d(place, rect, direction, on_found)
                    }
                    FocusState::Active | FocusState::Captured => {
                        let rect = match previously_focused_rect {
                            Some(rect) => rect,
                            None => self.focus_rect_of(child),
                        };
                        self.generate_and_search_children_2d(place, rect, direction, on_found)
                    }
                    FocusState::Inactive => false,
                }
            }
            // The search starts at the root: no parent on the way had the focus: an initial focus.
            FocusState::Active | FocusState::Captured => {
                self.find_child_corresponding_to_focus_enter(place, direction, on_found)
            }
        }
    }

    /// The child an `Enter` picks, as `findChildCorrespondingToFocusEnter`: the only one, or the
    /// best from the corner of `place` that `direction` starts at.
    pub(crate) fn find_child_corresponding_to_focus_enter(
        &mut self,
        place: FocusPlace,
        direction: FocusDirection,
        on_found: OnFound<'_>,
    ) -> bool {
        let mut children: SmallVec<[FocusTargetRef; 8]> = SmallVec::new();
        self.collect_accessible_children(place, &mut children);
        if children.len() <= 1 {
            return match children.first() {
                Some(&child) => on_found(self, child),
                None => false,
            };
        }
        // `Enter` goes `Right`; an implicit enter goes the way that triggered it.
        let requested = match direction {
            FocusDirection::Enter => FocusDirection::Right,
            other => other,
        };
        let rect = match place {
            Some(target) => self.focus_rect_of(target),
            None => self.root_focus_rect(),
        };
        let start = match requested {
            FocusDirection::Right | FocusDirection::Down => {
                Rect::new(rect.left, rect.top, rect.left, rect.top)
            }
            _ => Rect::new(rect.right, rect.bottom, rect.right, rect.bottom),
        };
        match self.find_best_candidate(&children, start, requested) {
            Some(child) => on_found(self, child),
            None => false,
        }
    }

    /// The rectangle of the root target: the window, or a modal overlay.
    fn root_focus_rect(&self) -> Rect {
        match self.modal_overlay() {
            Some(modal) => self.bounds_in_root(modal),
            None => self.bounds_in_root(self.root_id()),
        }
    }

    fn generate_and_search_children_2d(
        &mut self,
        place: FocusPlace,
        focused_item: Rect,
        direction: FocusDirection,
        on_found: OnFound<'_>,
    ) -> bool {
        self.search_children_2d(place, focused_item, direction, on_found)
            || self.search_beyond_bounds(place, direction)
    }

    fn search_children_2d(
        &mut self,
        place: FocusPlace,
        focused_item: Rect,
        direction: FocusDirection,
        on_found: OnFound<'_>,
    ) -> bool {
        let mut children = self.focus_children(place);
        while !children.is_empty() {
            let Some(next) = self.find_best_candidate(&children, focused_item, direction) else {
                return false;
            };
            if self.fetch_focus_properties(next).can_focus {
                return on_found(self, next);
            }
            // Deactivated: among its children, then without it.
            if self.generate_and_search_children_2d(Some(next), focused_item, direction, on_found) {
                return true;
            }
            children.retain(|child| *child != next);
        }
        false
    }

    fn collect_accessible_children(
        &mut self,
        place: FocusPlace,
        children: &mut SmallVec<[FocusTargetRef; 8]>,
    ) {
        for child in self.focus_children(place) {
            if self.fetch_focus_properties(child).can_focus {
                children.push(child);
            } else {
                self.collect_accessible_children(Some(child), children);
            }
        }
    }

    /// As `findBestCandidate`.
    fn find_best_candidate(
        &mut self,
        candidates: &[FocusTargetRef],
        focus_rect: Rect,
        direction: FocusDirection,
    ) -> Option<FocusTargetRef> {
        // An impossible rectangle as the first best.
        let (width, height) =
            (focus_rect.right - focus_rect.left, focus_rect.bottom - focus_rect.top);
        let translate = |rect: &Rect, x: f32, y: f32| {
            Rect::new(rect.left + x, rect.top + y, rect.right + x, rect.bottom + y)
        };
        let mut best = match direction {
            FocusDirection::Left => translate(&focus_rect, width + 1.0, 0.0),
            FocusDirection::Right => translate(&focus_rect, -(width + 1.0), 0.0),
            FocusDirection::Up => translate(&focus_rect, 0.0, height + 1.0),
            _ => translate(&focus_rect, 0.0, -(height + 1.0)),
        };
        let mut result = None;
        for &candidate in candidates {
            if self.is_eligible_for_focus_search(candidate) {
                let rect = self.focus_rect_of(candidate);
                if is_better_candidate(&rect, &best, &focus_rect, direction) {
                    best = rect;
                    result = Some(candidate);
                }
            }
        }
        result
    }

    // ---- Beyond bounds -------------------------------------------------------------------------

    /// As `searchBeyondBounds`: only a target with a beyond-bounds container of its own (a lazy
    /// list's), not those inside its items, whose nearest container is their parent's too. Says
    /// whether the search waits for the container: it unwinds as if found, and is made again once
    /// that's laid out (D477).
    fn search_beyond_bounds(&mut self, place: FocusPlace, direction: FocusDirection) -> bool {
        if !matches!(
            direction,
            FocusDirection::Next
                | FocusDirection::Previous
                | FocusDirection::Up
                | FocusDirection::Down
                | FocusDirection::Left
                | FocusDirection::Right
        ) {
            return false;
        }
        let Some(target) = place else { return false };
        let Some(index) = self.target_index(target) else { return false };
        let Some(container) = self.beyond_bounds_container_at(target.node, index) else {
            return false;
        };
        if let Some(parent) = self.focus_parent(target)
            && let Some(parent_index) = self.target_index(parent)
            && self.beyond_bounds_container_at(parent.node, parent_index) == Some(container)
        {
            return false;
        }
        if self.lay_out_beyond(container.0, container.1, direction) {
            self.focus.search_waiting = true;
            return true;
        }
        false
    }

    /// The nearest node above entry `before` of `node` laying out beyond its bounds for focus
    /// searches, and its index in its chain, as `findNearestBeyondBoundsLayoutAncestor`.
    fn beyond_bounds_container_at(&self, node: NodeId, before: usize) -> Option<(NodeId, usize)> {
        let mut current = Some(node);
        let mut end = before;
        while let Some(id) = current {
            let value = self.get(id)?;
            if let Some(index) = value.chain[..end.min(value.chain.len())]
                .iter()
                .rposition(|entry| entry.kinds.any(NodeKinds::BEYOND_BOUNDS_LAYOUT))
            {
                return Some((id, index));
            }
            current = self.focus_parent_of(id);
            end = usize::MAX;
        }
        None
    }

    /// Asks `container` (its modifier node `index`) to lay out one more item beyond its bounds
    /// going `direction`, for the search: the move waits for it, made again at the next layout
    /// (D477). Returns whether it waits (`false`: nothing more that way, or the search laid out
    /// `MOST_BEYOND_STEPS` items already).
    pub(crate) fn lay_out_beyond(
        &mut self,
        container: NodeId,
        index: usize,
        direction: FocusDirection,
    ) -> bool {
        // A container that had no more that way has none for the rest of the search (D486).
        if self.beyond_search.exhausted.contains(&(container, index))
            || self.beyond_search.steps >= MOST_BEYOND_STEPS
        {
            return false;
        }
        if let Some(active) = self.beyond_search.container
            && active != (container, index)
        {
            // Another container's turn: the items the last one laid out go (D483); the count
            // goes on, so the search ends within `MOST_BEYOND_STEPS` items whatever the tree.
            self.beyond_search.container = None;
            if let Some(value) = self.get_mut(active.0)
                && active.1 < value.chain.len()
                && let Some(layout) = value.chain[active.1].beyond_bounds()
            {
                layout.end_beyond_bounds();
            }
        }
        let layout_direction = self.node(container).layout_direction;
        let more = self.get_mut(container).is_some_and(|value| {
            index < value.chain.len()
                && value.chain[index]
                    .beyond_bounds()
                    .is_some_and(|layout| layout.lay_out_beyond_bounds(direction, layout_direction))
        });
        if !more {
            self.beyond_search.exhausted.push((container, index));
            return false;
        }
        self.beyond_search.container = Some((container, index));
        self.beyond_search.steps += 1;
        self.beyond_search.moves.push_back(direction);
        self.beyond_search.focused = self.focus.active;
        self.beyond_search.window = self.focus.window;
        true
    }

    /// The search is over: what was laid out beyond the bounds for it goes.
    pub(crate) fn end_beyond_bounds(&mut self) {
        self.beyond_search.steps = 0;
        self.beyond_search.exhausted.clear();
        if let Some((container, index)) = self.beyond_search.container.take()
            && let Some(value) = self.get_mut(container)
            && index < value.chain.len()
            && let Some(layout) = value.chain[index].beyond_bounds()
        {
            layout.end_beyond_bounds();
        }
    }

    /// Whether focus moves wait for content laid out beyond a container's bounds.
    pub fn has_beyond_bounds_moves(&self) -> bool {
        !self.beyond_search.moves.is_empty()
    }

    /// Makes the focus moves that waited for content beyond a container's bounds, now laid
    /// out (each may wait again, for one more item).
    pub fn run_beyond_bounds_moves(&mut self) -> Vec<Action> {
        let moves = std::mem::take(&mut self.beyond_search.moves);
        let window = self.beyond_search.window;
        // In the window it started in, whichever has the keys now.
        let ended = self.in_window(window, |tree| {
            // The focus moved meanwhile (a click, a `FocusRequester`): that wins, the search and
            // the moves waiting go (D495).
            if !moves.is_empty() && tree.focus.active != tree.beyond_search.focused {
                tree.end_beyond_bounds();
                return true;
            }
            for direction in moves {
                tree.move_focus_with(direction, true);
            }
            false
        });
        if ended {
            return Vec::new();
        }
        self.take_pending_actions()
    }
}

/// Whether a search in `direction` wraps around, as `is1dFocusSearch`.
fn is_one_dimensional(direction: FocusDirection) -> bool {
    matches!(direction, FocusDirection::Next | FocusDirection::Previous)
}

/// Whether `rect` is beyond `source` in `direction`, as Compose's `isCandidate`.
fn is_candidate(source: &Rect, rect: &Rect, direction: FocusDirection) -> bool {
    match direction {
        FocusDirection::Left => {
            (source.right > rect.right || source.left >= rect.right) && source.left > rect.left
        }
        FocusDirection::Right => {
            (source.left < rect.left || source.right <= rect.left) && source.right < rect.right
        }
        FocusDirection::Up => {
            (source.bottom > rect.bottom || source.top >= rect.bottom) && source.top > rect.top
        }
        FocusDirection::Down => {
            (source.top < rect.top || source.bottom <= rect.top) && source.bottom < rect.bottom
        }
        _ => false,
    }
}

fn horizontal(direction: FocusDirection) -> bool {
    matches!(direction, FocusDirection::Left | FocusDirection::Right)
}

/// How far `rect` is from `source` along `direction` (0 when they overlap).
fn major_axis_distance(source: &Rect, rect: &Rect, direction: FocusDirection) -> f32 {
    let distance = match direction {
        FocusDirection::Left => source.left - rect.right,
        FocusDirection::Right => rect.left - source.right,
        FocusDirection::Up => source.top - rect.bottom,
        _ => rect.top - source.bottom,
    };
    distance.max(0.0)
}

/// As Compose's `weightedDistance`: the distance along the direction counts 13 times more.
fn weighted_distance(source: &Rect, rect: &Rect, direction: FocusDirection) -> i64 {
    let major = major_axis_distance(source, rect, direction) as i64;
    let minor = if horizontal(direction) {
        (source.top + (source.bottom - source.top) / 2.0)
            - (rect.top + (rect.bottom - rect.top) / 2.0)
    } else {
        (source.left + (source.right - source.left) / 2.0)
            - (rect.left + (rect.right - rect.left) / 2.0)
    } as i64;
    13 * major * major + minor * minor
}

/// Whether `first` beats `second` by being in `source`'s beam, as Compose's `beamBeats`.
fn beam_beats(source: &Rect, first: &Rect, second: &Rect, direction: FocusDirection) -> bool {
    let in_beam = |rect: &Rect| {
        if horizontal(direction) {
            rect.bottom > source.top && rect.top < source.bottom
        } else {
            rect.right > source.left && rect.left < source.right
        }
    };
    if in_beam(second) || !in_beam(first) {
        return false;
    }
    let in_direction = match direction {
        FocusDirection::Left => source.left >= second.right,
        FocusDirection::Right => source.right <= second.left,
        FocusDirection::Up => source.top >= second.bottom,
        _ => source.bottom <= second.top,
    };
    if !in_direction || horizontal(direction) {
        return true;
    }
    // Vertically, beams beat only up to a point: unless `second` is completely closer.
    let far_edge = match direction {
        FocusDirection::Up => source.top - second.top,
        _ => second.bottom - source.bottom,
    }
    .max(1.0);
    major_axis_distance(source, first, direction) < far_edge
}

/// Whether `proposed` is better than `current` for a search from `focused` in `direction`, as
/// Compose's `isBetterCandidate`.
pub(crate) fn is_better_candidate(
    proposed: &Rect,
    current: &Rect,
    focused: &Rect,
    direction: FocusDirection,
) -> bool {
    if !is_candidate(focused, proposed, direction) {
        false
    } else if !is_candidate(focused, current, direction)
        || beam_beats(focused, proposed, current, direction)
    {
        true
    } else if beam_beats(focused, current, proposed, direction) {
        false
    } else {
        weighted_distance(focused, proposed, direction)
            < weighted_distance(focused, current, direction)
    }
}
