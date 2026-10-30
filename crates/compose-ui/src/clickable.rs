//! The clickable family's node, as Compose's `AbstractClickableNode`, `ClickableNode` and
//! `CombinedClickableNode`: `clickable`, `toggleable`, `tri_state_toggleable`, `selectable` and
//! `combined_clickable`. It recognizes clicks (and long and double clicks), emits its presses,
//! hover and focus into its interaction source (its own, made when first needed, if none is
//! given), and hosts its indication's node.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose_runtime::{Callback, CoroutineScope, Job};
use smallvec::SmallVec;

use crate::draw::DrawScope;
use crate::indication::IndicationNodeFactory;
use crate::input::{
    EventContext, Key, KeyEvent, KeyEventType, PointerEvent, PointerEventPass, PointerEventType,
    PointerId,
};
use crate::interaction::{
    Emitter, Focus, FocusInteraction, HoverEnter, HoverInteraction, MutableInteractionSource,
    Press, PressInteraction,
};
use crate::modifier::{
    DrawModifierNode, FocusTargetModifierNode, KeyInputModifierNode, ModifierNode,
    ModifierNodeElement, PointerInputModifierNode, SemanticsModifierNode,
};
use crate::semantics::{Role, SemanticsConfiguration, ToggleableState};
use crate::text::SharedString;
use crate::unit::{IntSize, Offset};

/// How long a press inside something waiting for a gesture (a scroll's slop) waits before it
/// shows, as Compose's `TapIndicationDelay` (Android's tap timeout): a touch that becomes a
/// scroll shows no press.
pub const TAP_INDICATION_DELAY: Duration = Duration::from_millis(100);

/// What a click does.
#[derive(Clone)]
pub(crate) enum ClickAction {
    /// `clickable`, `selectable`, `combined_clickable`: calls back.
    Click(Callback<dyn Fn()>),
    /// `toggleable`: calls back with the value toggled.
    Toggle(bool, Callback<dyn Fn(bool)>),
    /// `tri_state_toggleable`: its state in the semantics, a click calls back.
    TriState(ToggleableState, Callback<dyn Fn()>),
}

impl ClickAction {
    fn same(&self, other: &ClickAction) -> bool {
        match (self, other) {
            (ClickAction::Click(a), ClickAction::Click(b)) => a.ptr_eq(b),
            (ClickAction::Toggle(a, c), ClickAction::Toggle(b, d)) => a == b && c.ptr_eq(d),
            (ClickAction::TriState(a, c), ClickAction::TriState(b, d)) => a == b && c.ptr_eq(d),
            _ => false,
        }
    }

    /// Clicks, after dispatch.
    fn fire(&self, context: &mut EventContext<'_>) {
        match self {
            ClickAction::Click(callback) | ClickAction::TriState(_, callback) => {
                context.call(callback)
            }
            ClickAction::Toggle(value, callback) => context.call_with_bool(callback, !value),
        }
    }

    /// Clicks now (a timer's click).
    fn fire_now(&self) {
        match self {
            ClickAction::Click(callback) | ClickAction::TriState(_, callback) => callback(),
            ClickAction::Toggle(value, callback) => callback(!value),
        }
    }
}

/// The clickable family's data (an element's).
#[derive(Clone)]
pub(crate) struct Clickable {
    pub(crate) enabled: bool,
    pub(crate) role: Option<Role>,
    /// Boxed: rare, and the element stays as small as the others (D1052).
    pub(crate) on_click_label: Option<Rc<SharedString>>,
    /// The indication's factory: the one given, or with `use_local_indication`,
    /// `LOCAL_INDICATION`'s where the node is (set as the layout node takes its modifier,
    /// [`resolve_local_indications`]: none until then).
    pub(crate) indication: Option<Rc<dyn IndicationNodeFactory>>,
    /// The indication is `Indication::LOCAL`, as Compose's `useLocalIndication` (D1056).
    pub(crate) use_local_indication: bool,
    /// The source given, if any.
    pub(crate) interaction_source: Option<MutableInteractionSource>,
    pub(crate) action: ClickAction,
}

impl Clickable {
    pub(crate) fn same(&self, other: &Clickable) -> bool {
        self.enabled == other.enabled
            && self.use_local_indication == other.use_local_indication
            && self.role == other.role
            && self.on_click_label == other.on_click_label
            && crate::indication::same_factory(&self.indication, &other.indication)
            && self.interaction_source == other.interaction_source
            && self.action.same(&other.action)
    }
}

/// `combined_clickable`'s data beyond a clickable's.
#[derive(Clone)]
pub(crate) struct Combined {
    pub(crate) on_long_click: Option<Callback<dyn Fn()>>,
    pub(crate) on_double_click: Option<Callback<dyn Fn()>>,
    pub(crate) on_long_click_label: Option<SharedString>,
    /// What a long click vibrates with (`LOCAL_HAPTIC_FEEDBACK`'s where it was made), if
    /// `haptic_feedback_enabled`.
    pub(crate) haptic_feedback: Option<Rc<dyn crate::platform_services::HapticFeedback>>,
}

impl Combined {
    fn same(&self, other: &Combined) -> bool {
        let same_callback =
            |a: &Option<Callback<dyn Fn()>>, b: &Option<Callback<dyn Fn()>>| match (a, b) {
                (Some(a), Some(b)) => a.ptr_eq(b),
                (None, None) => true,
                _ => false,
            };
        same_callback(&self.on_long_click, &other.on_long_click)
            && same_callback(&self.on_double_click, &other.on_double_click)
            && self.on_long_click_label == other.on_long_click_label
            && match (&self.haptic_feedback, &other.haptic_feedback) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

/// `combined_clickable`'s element.
pub(crate) struct CombinedClickableElement {
    pub(crate) clickable: Clickable,
    pub(crate) combined: Combined,
}

impl ModifierNodeElement for CombinedClickableElement {
    type Node = ClickableNode;

    fn create(&self) -> ClickableNode {
        ClickableNode::new(self.clickable.clone(), Some(self.combined.clone()))
    }

    fn update(&self, node: &mut ClickableNode) {
        node.update(self.clickable.clone(), Some(self.combined.clone()));
    }

    fn same(&self, other: &Self) -> bool {
        self.clickable.same(&other.clickable) && self.combined.same(&other.combined)
    }

    fn name(&self) -> &'static str {
        "combined_clickable"
    }
}

/// What the node's timers (a delayed press, a long press, a double click's wait) share with it.
#[derive(Default)]
struct Shared {
    /// The source interactions go to: the one given, or the node's own once made.
    source: Option<MutableInteractionSource>,
    /// Its emissions, in order (D1057).
    emitter: Emitter,
    /// The node's handle while it's attached, for its coroutine scope and `run_lifecycle`.
    handle: Option<crate::node_context::NodeHandle>,
    /// The node's coroutine scope, once asked for, while it's attached: its timers' and
    /// emissions' tasks, cancelled when it's detached.
    scope: std::cell::Cell<Option<CoroutineScope>>,
    /// The pointer press emitted and not yet released.
    press: Option<Press>,
    /// A press waiting `TAP_INDICATION_DELAY` to be emitted.
    delay_job: Option<Job>,
    /// The click callback, for a click a timer makes (a double click's wait).
    action: Option<ClickAction>,
    on_long_click: Option<Callback<dyn Fn()>>,
    haptic_feedback: Option<Rc<dyn crate::platform_services::HapticFeedback>>,
    long_press_job: Option<Job>,
    long_press_triggered: bool,
    tap_job: Option<Job>,
    /// Keys held for a long click, with their timers.
    long_key_jobs: SmallVec<[(Key, Job); 1]>,
    /// Keys clicked once, waiting for a second click: the wait, and whether the minimum time
    /// passed.
    double_key_clicks: SmallVec<[(Key, Job, bool); 1]>,
}

impl Shared {
    /// The node's coroutine scope, while it's attached.
    fn scope(&self) -> Option<CoroutineScope> {
        if self.scope.get().is_none() {
            self.scope.set(self.handle.and_then(|handle| handle.coroutine_scope()));
        }
        self.scope.get()
    }

    /// The node leaves (reset or detached): what it queued goes as its scope is cancelled
    /// (D1061), and from now on it emits at once.
    fn leave(&mut self) {
        self.emitter.drop_queued();
        self.scope.set(None);
        self.handle = None;
    }

    /// Emits at once (after what's queued), as Compose's `tryEmit` where a node ends its
    /// interactions.
    fn emit(&self, interaction: impl Into<crate::interaction::Interaction>) {
        if let Some(source) = &self.source {
            self.emitter.emit_now(source, interaction);
        }
    }

    /// Emits from the node's task, as Compose's `coroutineScope.launch { emit(…) }`.
    fn emit_soon(&self, interaction: impl Into<crate::interaction::Interaction>) {
        if let Some(source) = &self.source {
            self.emitter.emit_soon(self.scope(), source, interaction);
        }
    }
}

/// Sets `LOCAL_INDICATION`'s factory, where the composition is now (the layout node taking
/// `modifier`, `emit_layout_node`), on its clickables using the local indication: Compose's
/// node reads it where it is (`currentValueOf(LocalIndication)`), so a clickable made outside
/// a provider and used inside it shows the provider's (D1056). Read once, only if one does.
/// (With node-level composition locals, MN-8, the node reads it itself.)
pub(crate) fn resolve_local_indications(modifier: &mut crate::modifier::Modifier) {
    let mut local: Option<Option<Rc<dyn IndicationNodeFactory>>> = None;
    let mut resolved = || local.get_or_insert_with(crate::indication::local_factory).clone();
    for element in modifier.elements_mut() {
        match element {
            crate::modifier::Element::Clickable(clickable) if clickable.use_local_indication => {
                clickable.indication = resolved();
            }
            crate::modifier::Element::Custom(custom) => {
                let factory = match custom.downcast_ref::<CombinedClickableElement>() {
                    Some(combined) if combined.clickable.use_local_indication => {
                        let factory = resolved();
                        if crate::indication::same_factory(&combined.clickable.indication, &factory)
                        {
                            continue;
                        }
                        factory
                    }
                    _ => continue,
                };
                if Rc::get_mut(custom).is_none() {
                    // Shared (a modifier kept and used again): a copy of its own.
                    let combined = custom
                        .downcast_ref::<CombinedClickableElement>()
                        .expect("a combined clickable");
                    *custom = Rc::new(CombinedClickableElement {
                        clickable: combined.clickable.clone(),
                        combined: combined.combined.clone(),
                    });
                }
                let element = Rc::get_mut(custom).expect("its own");
                if let Some(combined) =
                    (element as &mut dyn std::any::Any).downcast_mut::<CombinedClickableElement>()
                {
                    combined.clickable.indication = factory;
                }
            }
            _ => {}
        }
    }
}

fn cancel(job: &mut Option<Job>) {
    if let Some(job) = job.take() {
        job.cancel();
    }
}

/// The press in progress: its pointer, where it went down (in the node), when.
#[derive(Clone, Copy)]
struct Down {
    pointer: PointerId,
    position: Offset,
}

/// The clickable family's node.
pub(crate) struct ClickableNode {
    spec: Clickable,
    combined: Option<Combined>,
    shared: Rc<RefCell<Shared>>,
    /// The indication's node, made when first needed (with its own source) or as the node is
    /// attached (with a source given), as Compose's; attached with it, as Compose's delegate.
    indication: Option<Box<dyn ModifierNode>>,
    indication_draws_content: bool,
    /// Whether `indication` is attached: one made or replaced where the node has no context
    /// (an event, an update) is attached by `run_lifecycle`, from a task.
    indication_attached: bool,
    /// Indication nodes replaced or forgotten while attached, detached by `run_lifecycle`.
    retired: Vec<Box<dyn ModifierNode>>,
    /// A task runs `run_lifecycle` soon.
    lifecycle_scheduled: bool,
    down: Option<Down>,
    hover: Option<HoverEnter>,
    /// The keys held, each with its press.
    key_presses: SmallVec<[(Key, Press); 1]>,
    focus: Option<Focus>,
    focused: bool,
    /// The center of the node, where a key's press is.
    center: Offset,
    // Combined clickable's double clicks.
    is_second_tap: bool,
    first_tap_up_nanos: Option<u64>,
    ignore_next_up: bool,
}

impl ClickableNode {
    pub(crate) fn new(spec: Clickable, combined: Option<Combined>) -> ClickableNode {
        let shared = Shared {
            source: spec.interaction_source.clone(),
            action: Some(spec.action.clone()),
            on_long_click: combined.as_ref().and_then(|combined| combined.on_long_click.clone()),
            haptic_feedback: combined.as_ref().and_then(|c| c.haptic_feedback.clone()),
            ..Shared::default()
        };
        ClickableNode {
            spec,
            combined,
            shared: Rc::new(RefCell::new(shared)),
            indication: None,
            indication_draws_content: false,
            indication_attached: false,
            retired: Vec::new(),
            lifecycle_scheduled: false,
            down: None,
            hover: None,
            key_presses: SmallVec::new(),
            focus: None,
            focused: false,
            center: Offset::ZERO,
            is_second_tap: false,
            first_tap_up_nanos: None,
            ignore_next_up: false,
        }
    }

    /// Updates to a new element's data, as Compose's `updateCommon`.
    pub(crate) fn update(&mut self, spec: Clickable, combined: Option<Combined>) {
        let mut indication_dirty = false;
        if self.spec.interaction_source != spec.interaction_source {
            // The old source's interactions end, its focus too (as `FocusableNode.update`).
            self.dispose_interactions();
            self.dispose_focus();
            self.shared.borrow_mut().source = spec.interaction_source.clone();
            indication_dirty = true;
        }
        if !crate::indication::same_factory(&self.spec.indication, &spec.indication) {
            indication_dirty = true;
        }
        if self.spec.enabled != spec.enabled && !spec.enabled {
            self.dispose_interactions();
            self.cancel_input();
        }
        let (long_changed, double_changed) = match (&self.combined, &combined) {
            (Some(old), Some(new)) => (
                old.on_long_click.is_some() != new.on_long_click.is_some(),
                old.on_double_click.is_some() != new.on_double_click.is_some(),
            ),
            _ => (false, false),
        };
        // Adding or removing a long click cancels the press, as Compose's.
        if long_changed {
            self.dispose_interactions();
            self.cancel_input();
        } else if double_changed {
            self.cancel_input();
        }
        {
            let mut shared = self.shared.borrow_mut();
            shared.action = Some(spec.action.clone());
            shared.on_long_click =
                combined.as_ref().and_then(|combined| combined.on_long_click.clone());
            shared.haptic_feedback = combined.as_ref().and_then(|c| c.haptic_feedback.clone());
        }
        let given_before = self.spec.interaction_source.is_some();
        self.spec = spec;
        self.combined = combined;
        let given_now = self.spec.interaction_source.is_some();
        if given_before != given_now && !given_now {
            // No source given any more: the node's own is made when needed.
            indication_dirty = true;
        }
        if indication_dirty {
            self.recreate_indication();
        }
    }

    /// Makes the indication's node (and, without a source given, the node's own source) if
    /// there's an indication and it isn't made yet, as `initializeIndicationAndInteractionSource
    /// IfNeeded`. Returns whether it made it.
    fn initialize_indication(&mut self) -> bool {
        if self.indication.is_some() {
            return false;
        }
        let Some(factory) = self.spec.indication.clone() else { return false };
        let source = {
            let mut shared = self.shared.borrow_mut();
            shared.source.get_or_insert_with(MutableInteractionSource::new).clone()
        };
        let mut node = factory.create(source.as_source().clone());
        self.indication_draws_content = node.as_draw().is_some_and(|draw| draw.draws_content());
        self.indication = Some(node);
        self.indication_attached = false;
        self.schedule_lifecycle();
        true
    }

    /// The indication changed: its node is made again if it was made (or is made eagerly).
    fn recreate_indication(&mut self) {
        let made = self.forget_indication();
        if made || self.spec.interaction_source.is_some() {
            self.initialize_indication();
        }
    }

    /// The indication's node forgotten, as Compose's `undelegate`: detached soon if it was
    /// attached (`run_lifecycle`). Whether there was one.
    fn forget_indication(&mut self) -> bool {
        let Some(node) = self.indication.take() else { return false };
        if std::mem::take(&mut self.indication_attached) {
            self.retired.push(node);
            self.schedule_lifecycle();
        }
        true
    }

    /// Asks a task to run `run_lifecycle` with the node's context, where it has none (an
    /// event, an element's update): the tree is free then (MN-5's delegates will make it at
    /// once).
    fn schedule_lifecycle(&mut self) {
        let (handle, scope) = {
            let shared = self.shared.borrow();
            let (Some(handle), Some(scope)) = (shared.handle, shared.scope()) else { return };
            (handle, scope)
        };
        if std::mem::replace(&mut self.lifecycle_scheduled, true) {
            return;
        }
        scope.launch(async move {
            handle.with_node(|node: &mut ClickableNode, context| {
                node.lifecycle_scheduled = false;
                node.run_lifecycle(context);
            });
        });
    }

    /// Detaches the indication nodes forgotten, and attaches the one made, as Compose's
    /// `undelegate` and `delegate` while the node is attached.
    fn run_lifecycle(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        for mut node in self.retired.drain(..) {
            node.on_detach(context);
        }
        if context.is_attached()
            && !self.indication_attached
            && let Some(node) = &mut self.indication
        {
            node.on_attach(context);
            self.indication_attached = true;
        }
    }

    /// Makes the indication if needed; if it was just made, the node draws again to show it.
    fn initialize_indication_in(&mut self, context: &mut EventContext<'_>) {
        if self.initialize_indication() {
            context.invalidate_draw();
        }
    }

    /// Ends the interactions under way, as Compose's `disposeInteractions`: the presses
    /// cancelled, the hover exited.
    fn dispose_interactions(&mut self) {
        let mut shared = self.shared.borrow_mut();
        if let Some(press) = shared.press.take() {
            shared.emit(PressInteraction::Cancel(press));
        }
        cancel(&mut shared.delay_job);
        if let Some(enter) = self.hover.take() {
            shared.emit(HoverInteraction::Exit(enter));
        }
        for (_, press) in self.key_presses.drain(..) {
            shared.emit(PressInteraction::Cancel(press));
        }
    }

    /// The focus interaction ends, as `FocusableNode.disposeInteractionSource`.
    fn dispose_focus(&mut self) {
        if let Some(focus) = self.focus.take() {
            self.shared.borrow().emit(FocusInteraction::Unfocus(focus));
        }
    }

    /// Forgets the pointer gesture under way (its press cancelled), as `cancelInput`.
    fn cancel_input(&mut self) {
        self.down = None;
        self.is_second_tap = false;
        self.first_tap_up_nanos = None;
        self.ignore_next_up = false;
        {
            let mut shared = self.shared.borrow_mut();
            cancel(&mut shared.long_press_job);
            cancel(&mut shared.tap_job);
            shared.long_press_triggered = false;
        }
        self.press_cancelled();
    }

    fn reset_key_state(&mut self) {
        let mut shared = self.shared.borrow_mut();
        for (_, job) in shared.long_key_jobs.drain(..) {
            job.cancel();
        }
        for (_, job, _) in shared.double_key_clicks.drain(..) {
            job.cancel();
        }
    }

    // ---- Press interactions --------------------------------------------------------------

    /// The pointer went down at `position`: its press is emitted, after `TAP_INDICATION_DELAY`
    /// if something around waits for a gesture, as `handlePressInteractionStart`.
    fn press_started(&mut self, position: Offset, delay: bool) {
        let mut shared = self.shared.borrow_mut();
        let Some(source) = shared.source.clone() else { return };
        let press = Press::new(position);
        let scope = shared.scope();
        if delay && let Some(scope) = scope {
            let shared_handle = Rc::downgrade(&self.shared);
            let job = scope.launch(async move {
                compose_runtime::delay(TAP_INDICATION_DELAY).await;
                if let Some(shared) = shared_handle.upgrade() {
                    let mut shared = shared.borrow_mut();
                    shared.emitter.emit_now(&source, PressInteraction::Press(press));
                    shared.press = Some(press);
                    shared.delay_job = None;
                }
            });
            shared.delay_job = Some(job);
        } else {
            shared.press = Some(press);
            shared.emitter.emit_soon(scope, &source, PressInteraction::Press(press));
        }
    }

    /// The press released (a click): a press still waiting is emitted at once, then released,
    /// as `handlePressInteractionRelease`.
    fn press_released(&mut self, position: Offset) {
        let mut shared = self.shared.borrow_mut();
        let Some(source) = shared.source.clone() else { return };
        if let Some(job) = shared.delay_job.take()
            && job.is_active()
        {
            job.cancel();
            let press = Press::new(position);
            let scope = shared.scope();
            shared.emitter.emit_soon(scope, &source, PressInteraction::Press(press));
            shared.emitter.emit_soon(scope, &source, PressInteraction::Release(press));
        } else if let Some(press) = shared.press {
            shared.emitter.emit_soon(shared.scope(), &source, PressInteraction::Release(press));
        }
        shared.press = None;
    }

    /// The press cancelled: a press still waiting is never emitted, as
    /// `handlePressInteractionCancel`.
    fn press_cancelled(&mut self) {
        let mut shared = self.shared.borrow_mut();
        if let Some(job) = shared.delay_job.take()
            && job.is_active()
        {
            job.cancel();
        } else if let Some(press) = shared.press {
            shared.emit_soon(PressInteraction::Cancel(press));
        }
        shared.press = None;
    }

    // ---- Pointer -------------------------------------------------------------------------

    /// Whether `event`'s position is outside the bounds, extended to the minimum touch target
    /// around a smaller node, as `isOutOfBounds(size, extendedTouchPadding)`, the target's
    /// `LOCAL_VIEW_CONFIGURATION`'s `minimum_touch_target_size`.
    fn out_of_bounds(event: &PointerEvent, bounds: IntSize, context: &EventContext<'_>) -> bool {
        let minimum = context.view_configuration().minimum_touch_target_size;
        let density = context.density();
        let horizontal = ((density.to_px(minimum.width) - bounds.width as f32) / 2.0).max(0.0);
        let vertical = ((density.to_px(minimum.height) - bounds.height as f32) / 2.0).max(0.0);
        let position = event.position;
        position.x < -horizontal
            || position.y < -vertical
            || position.x > bounds.width as f32 + horizontal
            || position.y > bounds.height as f32 + vertical
    }

    fn handle_down(&mut self, event: &mut PointerEvent, context: &mut EventContext<'_>) {
        event.consume();
        self.down = Some(Down { pointer: event.pointer, position: event.position });
        if !self.spec.enabled {
            return;
        }
        let configuration = context.view_configuration();
        if self.combined.is_some() {
            let tap_waiting = self.shared.borrow().tap_job.is_some_and(|job| job.is_active());
            if tap_waiting {
                let now = event_nanos(event);
                let min_time = configuration.double_tap_min_time_millis * 1_000_000;
                let soon =
                    self.first_tap_up_nanos.is_some_and(|up| now.saturating_sub(up) < min_time);
                if soon {
                    // Too soon for a double click: ignored, no press, no long press.
                    self.ignore_next_up = true;
                    return;
                }
                self.is_second_tap = true;
                cancel(&mut self.shared.borrow_mut().tap_job);
            }
            self.shared.borrow_mut().long_press_triggered = false;
        }
        self.press_started(event.position, context.has_waiting_parent());
        let long = self.combined.as_ref().is_some_and(|combined| combined.on_long_click.is_some());
        let scope = self.shared.borrow().scope();
        if long && let Some(scope) = scope {
            let timeout = Duration::from_millis(configuration.long_press_timeout_millis);
            let shared = Rc::downgrade(&self.shared);
            let job = scope.launch(async move {
                compose_runtime::delay(timeout).await;
                let Some(shared) = shared.upgrade() else { return };
                let (callback, haptics) = {
                    let shared = shared.borrow();
                    (shared.on_long_click.clone(), shared.haptic_feedback.clone())
                };
                if let Some(callback) = callback {
                    callback();
                }
                if let Some(haptics) = haptics {
                    haptics.perform_haptic_feedback(
                        crate::platform_services::HapticFeedbackType::LongPress,
                    );
                }
                let mut shared = shared.borrow_mut();
                shared.long_press_triggered = true;
                cancel(&mut shared.tap_job);
                shared.long_press_job = None;
            });
            self.shared.borrow_mut().long_press_job = Some(job);
        }
    }

    fn handle_up(&mut self, event: &PointerEvent, context: &mut EventContext<'_>) {
        let Some(down) = self.down.take() else { return };
        if self.spec.enabled && !self.ignore_next_up {
            self.press_released(down.position);
            match &self.combined {
                None => self.spec.action.fire(context),
                Some(combined) => {
                    self.first_tap_up_nanos = Some(event_nanos(event));
                    let long_triggered = self.shared.borrow().long_press_triggered;
                    if !long_triggered {
                        if self.is_second_tap {
                            if let Some(on_double_click) = &combined.on_double_click {
                                context.call(on_double_click);
                            }
                        } else if combined.on_double_click.is_some() {
                            let scope = self.shared.borrow().scope();
                            let timeout = context.view_configuration().double_tap_timeout_millis;
                            if let Some(scope) = scope {
                                let shared = Rc::downgrade(&self.shared);
                                let job = scope.launch(async move {
                                    compose_runtime::delay(Duration::from_millis(timeout)).await;
                                    let Some(shared) = shared.upgrade() else { return };
                                    let action = shared.borrow_mut().action.clone();
                                    shared.borrow_mut().tap_job = None;
                                    if let Some(action) = action {
                                        action.fire_now();
                                    }
                                });
                                self.shared.borrow_mut().tap_job = Some(job);
                            }
                        } else {
                            self.spec.action.fire(context);
                        }
                    }
                }
            }
        }
        self.ignore_next_up = false;
        self.is_second_tap = false;
        let mut shared = self.shared.borrow_mut();
        cancel(&mut shared.long_press_job);
        shared.long_press_triggered = false;
    }

    /// Hover enters or exits, when enabled, as `emitHoverEnter` and `emitHoverExit`: at once
    /// (Compose launches them), so a hover the frame's layout finds is published before drawing.
    fn hover_changed(&mut self, entered: bool) {
        if entered {
            if self.hover.is_none() {
                let enter = HoverEnter::new();
                self.shared.borrow().emit(HoverInteraction::Enter(enter));
                self.hover = Some(enter);
            }
        } else if let Some(enter) = self.hover.take() {
            self.shared.borrow().emit(HoverInteraction::Exit(enter));
        }
    }
}

/// When `event` happened, in the executor's clock.
fn event_nanos(event: &PointerEvent) -> u64 {
    if event.time_nanos != 0 { event.time_nanos } else { compose_runtime::platform::now_nanos() }
}

impl ModifierNode for ClickableNode {
    /// Attached, with a source given, its indication is made at once, as `onAttach`.
    fn on_attach(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        if self.spec.interaction_source.is_some() {
            self.initialize_indication();
        }
        self.run_lifecycle(context);
        self.shared.borrow_mut().handle = Some(context.handle());
    }

    /// Leaving ends its interactions, and forgets its own source and its indication (made
    /// again if it comes back), as `onDetach`; what it queued to emit is dropped but its
    /// fallbacks, as its scope's coroutines are cancelled (D1057).
    fn on_detach(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        self.shared.borrow_mut().leave();
        self.dispose_interactions();
        self.dispose_focus();
        self.cancel_input();
        self.reset_key_state();
        self.detach_indication(context);
        if self.spec.interaction_source.is_none() {
            self.shared.borrow_mut().source = None;
        }
        // Its task went with its coroutine scope.
        self.lifecycle_scheduled = false;
    }

    /// Reused or deactivated: its interactions are over and its indication goes, as
    /// `onReset`; it's detached next, and attached again when reused.
    fn on_reset(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        self.shared.borrow_mut().leave();
        self.dispose_interactions();
        self.dispose_focus();
        self.cancel_input();
        self.reset_key_state();
        self.focused = false;
        self.detach_indication(context);
        if self.spec.interaction_source.is_none() {
            self.shared.borrow_mut().source = None;
        }
    }

    /// Dropped for good: its indication nodes give back what they hold.
    fn on_release(&mut self, context: &mut crate::modifier::ReleaseContext<'_>) {
        for node in self.retired.iter_mut().chain(self.indication.as_mut()) {
            node.on_release(context);
        }
    }

    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    /// Its indication draws what its listener writes (whatever indication an update gives it:
    /// kinds are asked once).
    fn observes_reads(&self) -> bool {
        true
    }
}

impl ClickableNode {
    /// Detaches the indication's node (and those forgotten) now, with the node's context.
    fn detach_indication(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        if let Some(mut node) = self.indication.take()
            && std::mem::take(&mut self.indication_attached)
        {
            node.on_detach(context);
        }
        for mut node in self.retired.drain(..) {
            node.on_detach(context);
        }
    }
}

/// Draws its indication, as the node it delegates to.
impl DrawModifierNode for ClickableNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        if let Some(draw) = self.indication.as_mut().and_then(|node| node.as_draw()) {
            draw.draw(scope);
        }
    }
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        if let Some(draw) = self.indication.as_mut().and_then(|node| node.as_draw()) {
            draw.draw_after(scope);
        }
    }
    fn draws_content(&self) -> bool {
        self.indication.is_some() && self.indication_draws_content
    }
    fn draw_with_content(&mut self, scope: &mut crate::draw_modifiers::ContentDrawScope<'_, '_>) {
        if let Some(draw) = self.indication.as_mut().and_then(|node| node.as_draw()) {
            draw.draw_with_content(scope);
        }
    }
}

impl PointerInputModifierNode for ClickableNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        self.center = Offset::new(bounds.width as f32 / 2.0, bounds.height as f32 / 2.0);
        self.initialize_indication_in(context);
        if self.spec.enabled && pass == PointerEventPass::Main {
            match event.kind {
                PointerEventType::Enter => self.hover_changed(true),
                PointerEventType::Exit => self.hover_changed(false),
                _ => {}
            }
        }
        if matches!(event.kind, PointerEventType::Enter | PointerEventType::Exit) {
            return;
        }
        match pass {
            PointerEventPass::Main => match self.down {
                None => {
                    if event.kind == PointerEventType::Press && !event.consumed {
                        self.handle_down(event, context);
                    }
                }
                Some(down) if down.pointer == event.pointer => {
                    let long_triggered = self.shared.borrow().long_press_triggered;
                    if long_triggered {
                        // A long click fired: everything is consumed until the release.
                        event.consume();
                        if event.kind == PointerEventType::Release {
                            self.handle_up(event, context);
                        }
                        return;
                    }
                    if event.kind == PointerEventType::Release && !event.consumed {
                        event.consume();
                        self.handle_up(event, context);
                    } else if event.consumed
                        || event.kind == PointerEventType::Release
                        || Self::out_of_bounds(event, bounds, context)
                    {
                        self.cancel_input();
                    }
                }
                Some(_) => {}
            },
            PointerEventPass::Final => {
                // A move of its press that something took by the final pass (a drag around
                // it) cancels the press, as `checkForCancellation`.
                let long_triggered = self.shared.borrow().long_press_triggered;
                if let Some(down) = self.down
                    && down.pointer == event.pointer
                    && event.kind == PointerEventType::Move
                    && event.consumed
                    && !long_triggered
                {
                    self.cancel_input();
                }
            }
            PointerEventPass::Initial => {}
        }
    }

    /// The gesture ended without its events: the hover exits, the press is cancelled.
    fn on_cancel_pointer_input(&mut self) {
        if let Some(enter) = self.hover.take() {
            self.shared.borrow().emit(HoverInteraction::Exit(enter));
        }
        self.cancel_input();
    }
}

impl SemanticsModifierNode for ClickableNode {
    /// As Compose's clickable: merging its descendants, its role, its click (labelled
    /// `on_click_label`; called whether enabled or not, as Compose's: refusing a disabled node's
    /// actions is the platform bridges', D1255), its toggle state, a long click with its label
    /// (`combined_clickable`), the focus when enabled, else disabled.
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.is_merging_semantics_of_descendants = true;
        if let Some(role) = self.spec.role {
            config.set_role(role);
        }
        let action = self.spec.action.clone();
        config.on_click(self.spec.on_click_label.as_deref().cloned(), move || {
            action.fire_now();
            true
        });
        if self.spec.enabled {
            config.set_focused(self.focused);
            config.request_focus_of_node();
        } else {
            config.disabled();
        }
        match &self.spec.action {
            ClickAction::Toggle(value, _) => {
                config.set_toggleable_state(ToggleableState::from(*value))
            }
            ClickAction::TriState(state, _) => config.set_toggleable_state(*state),
            ClickAction::Click(_) => {}
        }
        if let Some(combined) = &self.combined
            && let Some(on_long_click) = combined.on_long_click.clone()
        {
            config.on_long_click(combined.on_long_click_label.clone(), move || {
                on_long_click();
                true
            });
        }
    }
}

impl FocusTargetModifierNode for ClickableNode {
    fn can_focus(&self) -> bool {
        self.spec.enabled
    }

    fn on_focus_changed(&mut self, focused: bool, context: &mut EventContext<'_>) {
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        if focused {
            self.initialize_indication_in(context);
            // As `FocusableNode.emitInteraction`.
            let shared = self.shared.borrow();
            if let Some(focus) = self.focus.take() {
                shared.emit_soon(FocusInteraction::Unfocus(focus));
            }
            let focus = Focus::new();
            shared.emit_soon(FocusInteraction::Focus(focus));
            drop(shared);
            self.focus = Some(focus);
        } else {
            let shared = self.shared.borrow();
            if let Some(focus) = self.focus.take() {
                shared.emit_soon(FocusInteraction::Unfocus(focus));
            }
            // The keys held are cancelled, as `onFocusChange(false)`.
            for (_, press) in self.key_presses.drain(..) {
                shared.emit_soon(PressInteraction::Cancel(press));
            }
            drop(shared);
            self.reset_key_state();
        }
        context.invalidate_draw();
    }
}

/// Whether `key` clicks, as Compose's `isEnter`.
fn clicks(key: Key) -> bool {
    matches!(key, Key::Enter | Key::NumPadEnter | Key::Space | Key::DirectionCenter)
}

impl KeyInputModifierNode for ClickableNode {
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        self.initialize_indication_in(context);
        if !self.spec.enabled || !clicks(event.key) {
            return false;
        }
        match event.kind {
            KeyEventType::KeyDown => {
                let mut handled = false;
                // A key already held is a repeat: no new press.
                if !self.key_presses.iter().any(|(key, _)| *key == event.key) {
                    let press = Press::new(self.center);
                    self.key_presses.push((event.key, press));
                    self.shared.borrow().emit_soon(PressInteraction::Press(press));
                    handled = true;
                }
                self.combined_key_down(event.key, context) || handled
            }
            KeyEventType::KeyUp => {
                let Some(index) = self.key_presses.iter().position(|(key, _)| *key == event.key)
                else {
                    return false;
                };
                let (_, press) = self.key_presses.remove(index);
                self.shared.borrow().emit_soon(PressInteraction::Release(press));
                self.combined_key_up(event.key, context);
                true
            }
        }
    }
}

impl ClickableNode {
    /// A clicking key went down: a combined clickable's long click starts waiting, and a second
    /// press may be a double click, as `CombinedClickableNode.onClickKeyDownEvent`.
    fn combined_key_down(&mut self, key: Key, context: &EventContext<'_>) -> bool {
        let Some(combined) = &self.combined else { return false };
        let mut handled = false;
        let mut shared = self.shared.borrow_mut();
        let scope = shared.scope();
        if combined.on_long_click.is_some()
            && !shared.long_key_jobs.iter().any(|(held, _)| *held == key)
            && let Some(scope) = scope
        {
            let timeout = context.view_configuration().long_press_timeout_millis;
            let weak = Rc::downgrade(&self.shared);
            let job = scope.launch(async move {
                compose_runtime::delay(Duration::from_millis(timeout)).await;
                let callback =
                    weak.upgrade().and_then(|shared| shared.borrow().on_long_click.clone());
                if let Some(callback) = callback {
                    callback();
                }
            });
            shared.long_key_jobs.push((key, job));
            handled = true;
        }
        if let Some(index) = shared.double_key_clicks.iter().position(|(held, ..)| *held == key) {
            let (_, job, min_time_passed) = shared.double_key_clicks[index];
            if job.is_active() {
                job.cancel();
                if !min_time_passed {
                    // Too soon: the first was a click of its own.
                    shared.double_key_clicks.remove(index);
                    let action = shared.action.clone();
                    drop(shared);
                    if let Some(action) = action {
                        action.fire_now();
                    }
                    return handled;
                }
            } else {
                shared.double_key_clicks.remove(index);
            }
        }
        handled
    }

    /// A clicking key went up: a click, unless it long-clicked, or a double click's first or
    /// second, as `onClickKeyUpEvent`.
    fn combined_key_up(&mut self, key: Key, context: &mut EventContext<'_>) {
        let Some(combined) = self.combined.clone() else {
            self.spec.action.fire(context);
            return;
        };
        let mut shared = self.shared.borrow_mut();
        let mut long_clicked = false;
        if let Some(index) = shared.long_key_jobs.iter().position(|(held, _)| *held == key) {
            let (_, job) = shared.long_key_jobs.remove(index);
            if job.is_active() {
                job.cancel();
            } else {
                long_clicked = true;
            }
        }
        match &combined.on_double_click {
            Some(on_double_click) => {
                match shared.double_key_clicks.iter().position(|(held, ..)| *held == key) {
                    None => {
                        let scope = shared.scope();
                        if !long_clicked && let Some(scope) = scope {
                            let configuration = context.view_configuration();
                            let min_time =
                                Duration::from_millis(configuration.double_tap_min_time_millis);
                            let timeout =
                                Duration::from_millis(configuration.double_tap_timeout_millis);
                            let weak = Rc::downgrade(&self.shared);
                            let job = scope.launch(async move {
                                compose_runtime::delay(min_time).await;
                                if let Some(shared) = weak.upgrade() {
                                    for entry in &mut shared.borrow_mut().double_key_clicks {
                                        if entry.0 == key {
                                            entry.2 = true;
                                        }
                                    }
                                }
                                compose_runtime::delay(timeout.saturating_sub(min_time)).await;
                                let action = weak.upgrade().and_then(|shared| {
                                    let mut shared = shared.borrow_mut();
                                    shared.double_key_clicks.retain(|entry| entry.0 != key);
                                    shared.action.clone()
                                });
                                if let Some(action) = action {
                                    action.fire_now();
                                }
                            });
                            shared.double_key_clicks.push((key, job, false));
                        }
                    }
                    Some(index) => {
                        if !long_clicked {
                            context.call(on_double_click);
                        }
                        shared.double_key_clicks.remove(index);
                    }
                }
            }
            None => {
                if !long_clicked {
                    drop(shared);
                    self.spec.action.fire(context);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::UiHost;
    use crate::modifier::{ClickableOptions, Modifier};
    use crate::tree::{NodeId, UiTree};

    /// The first layout node with a clickable, and its entry.
    fn clickable_entry(tree: &UiTree) -> (NodeId, usize) {
        let mut stack = vec![tree.root_id()];
        while let Some(id) = stack.pop() {
            let value = tree.node(id);
            if let Some(index) = value
                .chain
                .iter()
                .position(|entry| entry.node.downcast_ref::<ClickableNode>().is_some())
            {
                return (id, index);
            }
            stack.extend(tree.children_of(id));
        }
        panic!("no clickable");
    }

    /// A clickable's tasks are on its coroutine scope, made at its first event, not when it's
    /// attached (most clickables are never pressed), and gone with it.
    #[test]
    fn a_clickable_makes_its_coroutine_scope_at_its_first_event() {
        let scopes =
            |host: &UiHost| host.with_tree(|tree| tree.focus_requests.scopes.borrow().len());
        let source = MutableInteractionSource::new();
        let given = source.clone();
        let shown = compose_runtime::mutable_state_of(true);
        let is_shown = shown.handle();
        let mut host = UiHost::headless(100, 100);
        host.set_content(move || {
            if is_shown.get() {
                let options = ClickableOptions {
                    interaction_source: Some(given.clone()),
                    ..ClickableOptions::DEFAULT
                };
                crate::layout::emit_layout_node(
                    Modifier.clickable_with(options, || {}).fill_max_size(),
                    crate::layout::empty_measure_policy(),
                    || {},
                );
            }
        });
        host.frame();
        assert_eq!(scopes(&host), 0, "attached with a source and its indication: no scope");
        let at = Offset::new(5.0, 5.0);
        host.send_pointer(crate::input::PointerEvent::press(at));
        assert_eq!(scopes(&host), 1);
        host.frame();
        assert!(source.is_pressed());
        host.send_pointer(crate::input::PointerEvent::release(at));
        host.frame();
        assert!(!source.is_pressed());
        shown.set(false);
        host.frame();
        assert_eq!(scopes(&host), 0, "cancelled and gone when it's detached");
    }

    /// Detached, the clickable drops what it queued to emit but its fallbacks, as Compose's
    /// node scope is cancelled (D1057): a press queued and not yet sent never is.
    #[test]
    fn detaching_a_clickable_drops_what_it_queued() {
        let source = MutableInteractionSource::new();
        let given = source.clone();
        let host = UiHost::headless(100, 100);
        host.set_content(move || {
            let options = ClickableOptions {
                interaction_source: Some(given.clone()),
                indication: crate::indication::Indication::NONE,
                ..ClickableOptions::DEFAULT
            };
            crate::layout::emit_layout_node(
                Modifier.clickable_with(options, || {}),
                crate::layout::empty_measure_policy(),
                || {},
            );
        });
        let mut host = host;
        host.frame();
        let heard = Rc::new(RefCell::new(Vec::new()));
        let sink = heard.clone();
        let _listener =
            source.listen(move |interaction| sink.borrow_mut().push(interaction.clone()));
        let press = Press::new(Offset::ZERO);
        host.with_tree(|tree| {
            let (node, index) = clickable_entry(tree);
            tree.with_entry(node, index, |modifier_node, context| {
                let clickable = modifier_node.downcast_mut::<ClickableNode>().expect("clickable");
                clickable.shared.borrow().emit_soon(PressInteraction::Press(press));
                clickable.on_detach(context);
            });
        });
        compose_runtime::platform::run_ready();
        assert!(heard.borrow().is_empty(), "{:?}", heard.borrow());
        assert!(!source.is_pressed());
    }
}
