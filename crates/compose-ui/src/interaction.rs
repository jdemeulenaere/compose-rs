//! Interactions and their sources, as Compose's `androidx.compose.foundation.interaction`: a
//! component (a clickable, a focusable, a draggable) emits what happens to it, presses, focus,
//! hover and drags, into a [`MutableInteractionSource`], and what collects the source's
//! [`interactions`](InteractionSource::interactions) (an indication, `collect_is_pressed_as_state`,
//! an app) shows it.
//!
//! A collector is a `futures_core::Stream` (D1050), the trait the runtime's flows use
//! (`snapshot_flow`, docs/parity/design/snapshots.md): each has a queue of its own, bounded as
//! Compose's `MutableSharedFlow(extraBufferCapacity = 16, onBufferOverflow = DROP_OLDEST)`,
//! filled synchronously by `emit` and drained by the collector's task. compose-rs's own
//! indications listen instead (`listen`: called back from `emit`, no task), and a node emits
//! through its [`Emitter`], one ordered queue (D1057).

use std::any::Any;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Poll, Waker};

use compose_runtime::{ComposeArg, CoroutineScope, OwnedState, State};

/// A clickable's press, focus and hover, shared, as Compose's `MutableInteractionSource` with
/// `collectIsPressedAsState`, `collectIsFocusedAsState` and `collectIsHoveredAsState`. Remember
/// one and give it to `Modifier.clickable_with`.
#[derive(Clone)]
pub struct MutableInteractionSource {
    pressed: Rc<OwnedState<bool>>,
    focused: Rc<OwnedState<bool>>,
    hovered: Rc<OwnedState<bool>>,
    /// The drags going on: as Compose's `collectIsDraggedAsState`, which counts the starts not
    /// yet stopped or cancelled (two scrollables sharing the source, one still dragging).
    dragged: Rc<OwnedState<u32>>,
}

impl MutableInteractionSource {
    pub fn new() -> MutableInteractionSource {
        MutableInteractionSource {
            pressed: Rc::new(compose_runtime::mutable_state_of(false)),
            focused: Rc::new(compose_runtime::mutable_state_of(false)),
            hovered: Rc::new(compose_runtime::mutable_state_of(false)),
            dragged: Rc::new(compose_runtime::mutable_state_of(0)),
        }
    }

    fn state(&self, kind: Kind) -> &OwnedState<bool> {
        self.states[kind as usize].get_or_init(|| compose_runtime::mutable_state_of(self.is(kind)))
    }

    /// Whether a drag is going on (a scrollable's, a draggable's), as `collectIsDraggedAsState`
    /// (a state read).
    pub fn is_dragged(&self) -> bool {
        self.dragged.get() > 0
    }

    /// A drag started (`true`, Compose's `DragInteraction.Start`) or ended (`false`, its `Stop`
    /// or `Cancel`): for foundation's gestures, until the interactions' stream comes.
    #[doc(hidden)]
    pub fn set_dragged(&self, dragged: bool) {
        let update = move |count: &u32| if dragged { count + 1 } else { count.saturating_sub(1) };
        if compose_runtime::is_composing() {
            let state = self.dragged.clone();
            compose_runtime::CoroutineScope::new().launch(async move { state.update(update) });
        } else {
            self.dragged.update(update);
        }
    }
}

/// Where a component's interactions can be read, as Compose's `InteractionSource`: a handle,
/// cloned to share it, compared by identity.
#[derive(Clone)]
pub struct InteractionSource {
    inner: Rc<SourceInner>,
}

impl InteractionSource {
    fn new() -> InteractionSource {
        InteractionSource {
            inner: Rc::new(SourceInner {
                subscribers: RefCell::new(Vec::new()),
                listeners: RefCell::new(Vec::new()),
                ongoing: RefCell::new(Ongoing::default()),
                states: Default::default(),
            }),
        }
    }

    /// The interactions emitted from now on, as `interactions` (a `Flow`): collect them in a
    /// coroutine, `source.interactions().collect(|interaction| …).await`, or take them one at a
    /// time with `next`. Each call is a collector of its own; one that doesn't keep up keeps the
    /// latest 16.
    pub fn interactions(&self) -> Interactions {
        let subscriber = Rc::new(Subscriber::default());
        let mut subscribers = self.inner.subscribers.borrow_mut();
        subscribers.retain(|subscriber| subscriber.strong_count() > 0);
        subscribers.push(Rc::downgrade(&subscriber));
        Interactions { _source: self.clone(), subscriber }
    }

    /// Calls `callback` with each interaction emitted from now on, from the emission itself, while
    /// the returned listener is held: compose-rs's own indications' collection (D1057), which
    /// a task polling a queue would cost an executor round per emission.
    pub(crate) fn listen(&self, callback: impl FnMut(&Interaction) + 'static) -> Rc<Listener> {
        let listener = Rc::new(Listener(RefCell::new(Box::new(callback))));
        let mut listeners = self.inner.listeners.borrow_mut();
        listeners.retain(|listener| listener.strong_count() > 0);
        listeners.push(Rc::downgrade(&listener));
        listener
    }

    /// Whether the component is pressed, as `collectIsPressedAsState`: a state, `true` once the
    /// source emits a `PressInteraction::Press` and until it emits each press's release or
    /// cancellation. Its collection starts in the composition (a `LaunchedEffect`), so a press
    /// emitted before reads as released, as Compose's. Call it while composing.
    pub fn collect_is_pressed_as_state(&self) -> State<bool> {
        self.collect_as_state(Kind::Pressed)
    }

    /// Whether the component has the focus, as `collectIsFocusedAsState` (see
    /// [`collect_is_pressed_as_state`](Self::collect_is_pressed_as_state)).
    pub fn collect_is_focused_as_state(&self) -> State<bool> {
        self.collect_as_state(Kind::Focused)
    }

    /// Whether a mouse is over the component, as `collectIsHoveredAsState` (see
    /// [`collect_is_pressed_as_state`](Self::collect_is_pressed_as_state)).
    pub fn collect_is_hovered_as_state(&self) -> State<bool> {
        self.collect_as_state(Kind::Hovered)
    }

    /// Whether the component is dragged, as `collectIsDraggedAsState` (see
    /// [`collect_is_pressed_as_state`](Self::collect_is_pressed_as_state)).
    pub fn collect_is_dragged_as_state(&self) -> State<bool> {
        self.collect_as_state(Kind::Dragged)
    }

    fn collect_as_state(&self, kind: Kind) -> State<bool> {
        let source = self.clone();
        compose_runtime::produce_state(false, self.clone(), move |state| async move {
            let mut ongoing = Ongoing::default();
            source
                .interactions()
                .collect(move |interaction| {
                    ongoing.apply(&interaction);
                    let active = match kind {
                        Kind::Pressed => !ongoing.presses.is_empty(),
                        Kind::Focused => !ongoing.focuses.is_empty(),
                        Kind::Hovered => !ongoing.hovers.is_empty(),
                        Kind::Dragged => !ongoing.drags.is_empty(),
                    };
                    state.set(active);
                })
                .await
        })
    }

    /// Whether a press is under way: compose-rs's own, a state read of every press the source
    /// emitted since it was made (`collect_is_pressed_as_state` starts at its collection), so
    /// what reads it composes, measures or draws again when it changes.
    pub fn is_pressed(&self) -> bool {
        self.inner.state(Kind::Pressed).get()
    }

    /// Whether a focus is under way, as [`is_pressed`](Self::is_pressed).
    pub fn is_focused(&self) -> bool {
        self.inner.state(Kind::Focused).get()
    }

    /// Whether a hover is under way, as [`is_pressed`](Self::is_pressed).
    pub fn is_hovered(&self) -> bool {
        self.inner.state(Kind::Hovered).get()
    }

    /// Whether a drag is under way, as [`is_pressed`](Self::is_pressed).
    pub fn is_dragged(&self) -> bool {
        self.inner.state(Kind::Dragged).get()
    }

    fn send(&self, interaction: Interaction) {
        self.inner.ongoing.borrow_mut().apply(&interaction);
        for (index, state) in self.inner.states.iter().enumerate() {
            if let Some(state) = state.get() {
                let kind = [Kind::Pressed, Kind::Focused, Kind::Hovered, Kind::Dragged][index];
                let value = self.inner.is(kind);
                if state.try_get() != Some(value) {
                    // A plain write while composing too (a node detached by a recomposition):
                    // as any write in composition (D712).
                    state.set(value);
                }
            }
        }
        // By index, each upgraded in turn: nothing collected per emission, and a collector made
        // or dropped by a callback doesn't disturb the walk.
        let mut index = 0;
        loop {
            let Some(listener) = self.inner.listeners.borrow().get(index).map(Weak::upgrade) else {
                break;
            };
            index += 1;
            if let Some(listener) = listener
                && let Ok(mut callback) = listener.0.try_borrow_mut()
            {
                callback(&interaction);
            }
        }
        let mut index = 0;
        loop {
            let Some(subscriber) = self.inner.subscribers.borrow().get(index).map(Weak::upgrade)
            else {
                break;
            };
            index += 1;
            let Some(subscriber) = subscriber else { continue };
            {
                let mut queue = subscriber.queue.borrow_mut();
                if queue.len() == BUFFER_CAPACITY {
                    queue.pop_front();
                }
                queue.push_back(interaction.clone());
            }
            if let Some(waker) = subscriber.waker.borrow_mut().take() {
                waker.wake();
            }
        }
    }
}

impl PartialEq for InteractionSource {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl ComposeArg for InteractionSource {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl fmt::Debug for InteractionSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "InteractionSource({:p})", Rc::as_ptr(&self.inner))
    }
}

/// A collector of a source's interactions, as collecting Compose's `interactions` flow: the
/// interactions emitted since it was made, in order. Dropping it stops the collection.
pub struct Interactions {
    /// Keeps the source alive while collected, as a flow keeps its producer.
    _source: InteractionSource,
    subscriber: Rc<Subscriber>,
}

impl Interactions {
    fn poll_interaction(&self, context: &mut std::task::Context<'_>) -> Poll<Interaction> {
        if let Some(interaction) = self.subscriber.queue.borrow_mut().pop_front() {
            return Poll::Ready(interaction);
        }
        *self.subscriber.waker.borrow_mut() = Some(context.waker().clone());
        Poll::Pending
    }

    /// The next interaction, waiting for one.
    pub async fn next(&mut self) -> Interaction {
        std::future::poll_fn(|context| self.poll_interaction(context)).await
    }

    /// Calls `action` with each interaction, forever, as `interactions.collect { … }`: the
    /// coroutine collecting ends by being cancelled.
    pub async fn collect(mut self, mut action: impl FnMut(Interaction)) {
        loop {
            let interaction = self.next().await;
            action(interaction);
        }
    }
}

/// The interactions as a `Stream`, which never ends (the source lives while it's collected).
impl futures_core::Stream for Interactions {
    type Item = Interaction;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> Poll<Option<Interaction>> {
        self.poll_interaction(context).map(Some)
    }
}

/// A source components emit their interactions into, as Compose's `MutableInteractionSource`:
/// remember one (`remember(MutableInteractionSource::new)`) and give it to a component to read
/// what happens to it, or emit into it yourself. It derefs to its [`InteractionSource`].
#[derive(Clone, PartialEq, Debug)]
pub struct MutableInteractionSource(InteractionSource);

impl MutableInteractionSource {
    /// A new source, as `MutableInteractionSource()`.
    pub fn new() -> MutableInteractionSource {
        MutableInteractionSource(InteractionSource::new())
    }

    /// Emits `interaction` to the collectors, as `emit`: it never waits (a collector behind
    /// drops its oldest).
    pub async fn emit(&self, interaction: impl Into<Interaction>) {
        self.0.send(interaction.into());
    }

    /// Emits `interaction` at once, as `tryEmit`: always succeeds (a collector behind drops its
    /// oldest), as Compose's source, whose buffer drops the oldest too.
    pub fn try_emit(&self, interaction: impl Into<Interaction>) -> bool {
        self.0.send(interaction.into());
        true
    }

    /// The read side, as a `MutableInteractionSource` is an `InteractionSource`.
    pub fn as_source(&self) -> &InteractionSource {
        &self.0
    }
}

impl std::ops::Deref for MutableInteractionSource {
    type Target = InteractionSource;
    fn deref(&self) -> &InteractionSource {
        &self.0
    }
}

impl Default for MutableInteractionSource {
    fn default() -> Self {
        MutableInteractionSource::new()
    }
}

impl From<MutableInteractionSource> for InteractionSource {
    fn from(source: MutableInteractionSource) -> InteractionSource {
        source.0
    }
}

impl ComposeArg for MutableInteractionSource {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

// ---- A node's emissions -----------------------------------------------------------------------

/// A node's emissions, in the order it made them (D1057), as Compose's node emits from
/// coroutines launched in order on its scope: [`emit_soon`](Emitter::emit_soon) queues one for
/// the node's task (one task on the node's coroutine scope, woken for each, made at the first:
/// Compose's `coroutineScope.launch { emit(…) }`; with no scope, the node not attached, it's
/// sent at once, as Compose's `tryEmit` there), [`emit_now`](Emitter::emit_now) sends what's
/// queued, then one at once (Compose's `tryEmit`, where the node ends its interactions, and its
/// hovers, which compose-rs sends at once so a hover the frame's layout finds draws in that
/// frame), so a release then an exit go out in that order. The node's `on_reset` and `on_detach`
/// start with [`drop_queued`](Emitter::drop_queued), its scope about to be cancelled (D1061):
/// what's queued isn't sent, but its fallbacks, as Compose's nodes send at once when their scope
/// is cancelled first: a press's cancellation (`invokeOnCompletion { tryEmit(cancel) }`) and focus
/// changes (`FocusableNode`'s `emitWithFallback`), and compose-rs's press release too (D1065), so
/// a source never stays focused or pressed for an end dropped.
#[derive(Default)]
pub(crate) struct Emitter(OnceCell<Rc<EmitterQueue>>);

#[derive(Default)]
struct EmitterQueue {
    /// Each with the source it goes to (a node given a new source still sends what it queued
    /// to the old one, as Compose's launched coroutines captured it), and whether it's sent
    /// even if the node is cancelled first (its fallback).
    pending: RefCell<VecDeque<(InteractionSource, Interaction, bool)>>,
    /// The task sending them runs: on the node's scope, until `drop_queued` (the scope's about
    /// to be cancelled).
    running: Cell<bool>,
    /// What wakes it, if it's waiting.
    waker: RefCell<Option<Waker>>,
}

impl EmitterQueue {
    /// Sends what's queued, in order (each taken before it's sent: a callback may queue more).
    fn send_pending(&self) {
        loop {
            let Some((source, interaction, _)) = self.pending.borrow_mut().pop_front() else {
                break;
            };
            source.send(interaction);
        }
    }
}

/// Whether `interaction` is sent though its node is cancelled before its task sends it, as
/// Compose's components fall back to `tryEmit` for a press's cancellation and focus changes. A
/// release too: the node has forgotten its press by then, so nothing else would end it (D1065).
fn has_fallback(interaction: &Interaction) -> bool {
    matches!(
        interaction,
        Interaction::Press(PressInteraction::Cancel(_) | PressInteraction::Release(_))
            | Interaction::Focus(FocusInteraction::Focus(_) | FocusInteraction::Unfocus(_))
    )
}

impl Emitter {
    /// Queues `interaction` for `source`, sent by the node's task on `scope` when it next runs;
    /// at once without a scope (the node isn't attached).
    pub(crate) fn emit_soon(
        &self,
        scope: Option<CoroutineScope>,
        source: &InteractionSource,
        interaction: impl Into<Interaction>,
    ) {
        let interaction = interaction.into();
        let Some(scope) = scope else {
            self.emit_now(source, interaction);
            return;
        };
        let queue = self.0.get_or_init(Rc::default);
        let fallback = has_fallback(&interaction);
        queue.pending.borrow_mut().push_back((source.clone(), interaction, fallback));
        if !queue.running.get() {
            let weak = Rc::downgrade(queue);
            scope.launch(std::future::poll_fn(move |context| {
                let Some(queue) = weak.upgrade() else { return Poll::Ready(()) };
                queue.send_pending();
                *queue.waker.borrow_mut() = Some(context.waker().clone());
                Poll::Pending
            }));
            queue.running.set(true);
        } else if let Some(waker) = queue.waker.borrow_mut().take() {
            waker.wake();
        }
    }

    /// Sends what's queued, then `interaction` to `source`, now.
    pub(crate) fn emit_now(&self, source: &InteractionSource, interaction: impl Into<Interaction>) {
        if let Some(queue) = self.0.get() {
            queue.send_pending();
        }
        source.send(interaction.into());
    }

    /// The node is detached or reset (its scope, and the task on it, about to be cancelled):
    /// what's queued is dropped but its fallbacks, sent now in order. It comes before the node
    /// ends its interactions because those go through [`emit_now`](Emitter::emit_now), which
    /// sends what's queued first: dropped here, a press queued isn't sent then. And the node's
    /// ending of its focus there (`dispose_focus`, a `tryEmit` Compose's clickable has no
    /// counterpart of at detach) comes after a focus queued, in that order (D1061).
    pub(crate) fn drop_queued(&self) {
        let Some(queue) = self.0.get() else { return };
        queue.running.set(false);
        queue.waker.borrow_mut().take();
        loop {
            let Some((source, interaction, fallback)) = queue.pending.borrow_mut().pop_front()
            else {
                break;
            };
            if fallback {
                source.send(interaction);
            }
        }
    }
}

/// `Modifier.pointer_hover_icon`'s node: hit by the mouse, it sets the pointer's look.
struct HoverIconElement(crate::input::PointerIcon);

struct HoverIconNode(crate::input::PointerIcon);

impl crate::modifier::ModifierNodeElement for HoverIconElement {
    type Node = HoverIconNode;
    fn create(&self) -> HoverIconNode {
        HoverIconNode(self.0)
    }
    fn update(&self, node: &mut HoverIconNode) {
        node.0 = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        other.0 == self.0
    }
    fn name(&self) -> &'static str {
        "pointer_hover_icon"
    }
}

impl crate::modifier::ModifierNode for HoverIconNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn crate::modifier::PointerInputModifierNode> {
        Some(self)
    }
}

impl crate::modifier::PointerInputModifierNode for HoverIconNode {
    fn on_pointer_event(
        &mut self,
        _event: &mut crate::input::PointerEvent,
        _pass: crate::input::PointerEventPass,
        _bounds: crate::unit::IntSize,
        _context: &mut crate::input::EventContext<'_>,
    ) {
    }

    fn hover_icon(&self) -> Option<crate::input::PointerIcon> {
        Some(self.0)
    }
}

impl crate::modifier::Modifier {
    /// The mouse pointer looks like `icon` over the node, as `Modifier.pointerHoverIcon`.
    pub fn pointer_hover_icon(self, icon: crate::input::PointerIcon) -> crate::modifier::Modifier {
        self.then_custom(HoverIconElement(icon))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A collector is a `Stream` of interactions, as the runtime's flows are (D1050).
    #[test]
    fn interactions_are_a_stream() {
        fn stream<S: futures_core::Stream<Item = Interaction>>(_: &S) {}
        stream(&MutableInteractionSource::new().interactions());
    }

    /// What a listener heard, as kinds.
    fn heard(source: &InteractionSource) -> (Rc<Listener>, Rc<RefCell<Vec<&'static str>>>) {
        let heard = Rc::new(RefCell::new(Vec::new()));
        let sink = heard.clone();
        let listener = source.listen(move |interaction| {
            sink.borrow_mut().push(match interaction {
                Interaction::Press(PressInteraction::Press(_)) => "Press",
                Interaction::Press(PressInteraction::Release(_)) => "Release",
                Interaction::Press(PressInteraction::Cancel(_)) => "Cancel",
                Interaction::Hover(HoverInteraction::Exit(_)) => "Exit",
                Interaction::Focus(FocusInteraction::Focus(_)) => "Focus",
                Interaction::Focus(FocusInteraction::Unfocus(_)) => "Unfocus",
                _ => "other",
            })
        });
        (listener, heard)
    }

    /// A node's emissions go out in the order it made them: one sent at once (a hover's exit)
    /// sends those queued before it first (D1057).
    #[test]
    fn an_emitter_keeps_its_order() {
        let source = MutableInteractionSource::new();
        let (_listener, heard) = heard(&source);
        let emitter = Emitter::default();
        let scope = CoroutineScope::new();
        let on_scope = Some(*scope);
        let press = Press::new(Offset::ZERO);
        emitter.emit_soon(on_scope, &source, PressInteraction::Press(press));
        emitter.emit_soon(on_scope, &source, PressInteraction::Release(press));
        assert!(heard.borrow().is_empty(), "queued for the node's task");
        emitter.emit_now(&source, HoverInteraction::Exit(HoverEnter::new()));
        assert_eq!(*heard.borrow(), ["Press", "Release", "Exit"]);
        emitter.emit_soon(on_scope, &source, PressInteraction::Press(press));
        compose_runtime::platform::run_ready();
        assert_eq!(*heard.borrow(), ["Press", "Release", "Exit", "Press"], "its task sent it");
    }

    /// Its node detached, an emitter sends none of what it queued but its
    /// fallbacks, in order: press releases (D1065) and cancellations, as Compose's
    /// `invokeOnCompletion { tryEmit }`,
    /// and focus changes, as `FocusableNode.emitWithFallback` (D1057). A press queued before
    /// the node left never reaches a collector after its cancel, and a focus's end is never
    /// dropped (the source would stay focused).
    #[test]
    fn a_cancelled_emitter_sends_only_its_fallbacks() {
        let source = MutableInteractionSource::new();
        let (_listener, heard) = heard(&source);
        let emitter = Emitter::default();
        let scope = CoroutineScope::new();
        let on_scope = Some(*scope);
        let (first, second) = (Press::new(Offset::ZERO), Press::new(Offset::ZERO));
        let focus = Focus::new();
        emitter.emit_soon(on_scope, &source, FocusInteraction::Focus(focus));
        emitter.emit_soon(on_scope, &source, PressInteraction::Press(first));
        emitter.emit_soon(on_scope, &source, PressInteraction::Release(first));
        emitter.emit_soon(on_scope, &source, PressInteraction::Cancel(second));
        emitter.emit_soon(on_scope, &source, HoverInteraction::Exit(HoverEnter::new()));
        emitter.emit_soon(on_scope, &source, FocusInteraction::Unfocus(focus));
        emitter.drop_queued();
        scope.cancel();
        compose_runtime::platform::run_ready();
        assert_eq!(*heard.borrow(), ["Focus", "Release", "Cancel", "Unfocus"]);
        assert!(!source.is_pressed() && !source.is_focused());
    }

    /// A press sent and its release queued, then the node detached: the release is sent, or the
    /// source would stay pressed, its node having forgotten the press (D1065).
    #[test]
    fn a_release_queued_when_the_node_leaves_leaves_the_source_unpressed() {
        let source = MutableInteractionSource::new();
        let emitter = Emitter::default();
        let scope = CoroutineScope::new();
        let press = Press::new(Offset::ZERO);
        emitter.emit_now(&source, PressInteraction::Press(press));
        assert!(source.is_pressed());
        emitter.emit_soon(Some(*scope), &source, PressInteraction::Release(press));
        emitter.drop_queued();
        scope.cancel();
        assert!(!source.is_pressed());
    }

    /// A focus whose end the emitter falls back to sending: queued, then cancelled, the source
    /// isn't left focused.
    #[test]
    fn a_focus_ended_in_a_cancelled_queue_leaves_the_source_unfocused() {
        let source = MutableInteractionSource::new();
        let emitter = Emitter::default();
        let scope = CoroutineScope::new();
        let focus = Focus::new();
        emitter.emit_now(&source, FocusInteraction::Focus(focus));
        assert!(source.is_focused());
        emitter.emit_soon(Some(*scope), &source, FocusInteraction::Unfocus(focus));
        emitter.drop_queued();
        scope.cancel();
        assert!(!source.is_focused());
    }

    /// With no scope (its node not attached) an emission goes out at once, after what's queued,
    /// as Compose's `emitWithFallback` falls back to `tryEmit`; a new scope's task sends what
    /// comes next once the old one is cancelled (a node attached again).
    #[test]
    fn an_emitter_without_a_scope_sends_at_once_and_a_new_scope_takes_over() {
        let source = MutableInteractionSource::new();
        let (_listener, heard) = heard(&source);
        let emitter = Emitter::default();
        let press = Press::new(Offset::ZERO);
        emitter.emit_soon(None, &source, PressInteraction::Press(press));
        assert_eq!(*heard.borrow(), ["Press"]);
        let first = CoroutineScope::new();
        emitter.emit_soon(Some(*first), &source, PressInteraction::Release(press));
        compose_runtime::platform::run_ready();
        assert_eq!(*heard.borrow(), ["Press", "Release"]);
        emitter.drop_queued();
        first.cancel();
        let second = CoroutineScope::new();
        emitter.emit_soon(Some(*second), &source, PressInteraction::Press(press));
        compose_runtime::platform::run_ready();
        assert_eq!(*heard.borrow(), ["Press", "Release", "Press"]);
    }

    /// A listener dropped hears nothing more.
    #[test]
    fn a_dropped_listener_hears_nothing() {
        let source = MutableInteractionSource::new();
        let (listener, heard) = heard(&source);
        drop(listener);
        source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
        assert!(heard.borrow().is_empty());
    }
}
