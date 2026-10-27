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

use compose_runtime::{ComposeArg, Job, OwnedState, State};

use crate::unit::Offset;

/// How many interactions a collector that hasn't run yet keeps, the oldest dropped beyond, as
/// Compose's `extraBufferCapacity = 16` with `DROP_OLDEST`.
const BUFFER_CAPACITY: usize = 16;

/// What identifies one interaction, for the interaction ending it (a release names its press).
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

macro_rules! identified {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy)]
        pub struct $name {
            id: u64,
        }

        impl $name {
            /// A new one, distinct from every other.
            #[allow(clippy::new_without_default)]
            pub fn new() -> $name {
                $name { id: next_id() }
            }
        }

        /// The same interaction, not one equal to it: as Compose's classes, compared by identity.
        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.id == other.id
            }
        }

        impl Eq for $name {}

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}#{}", stringify!($name), self.id)
            }
        }
    };
}

identified!(
    /// A focus gained, as `FocusInteraction.Focus`: `FocusInteraction::Unfocus` names it.
    Focus
);
identified!(
    /// A hover started, as `HoverInteraction.Enter`: `HoverInteraction::Exit` names it.
    HoverEnter
);
identified!(
    /// A drag started, as `DragInteraction.Start`: `DragInteraction::Stop` and `Cancel` name it.
    DragStart
);

/// A press, as `PressInteraction.Press`: where it went down (in the pressed node), and the
/// identity a `PressInteraction::Release` or `Cancel` names it by.
#[derive(Clone, Copy)]
pub struct Press {
    id: u64,
    /// Where it was pressed, in the pressed node's coordinates, as `pressPosition`.
    pub press_position: Offset,
}

impl Press {
    /// A new press at `press_position`, distinct from every other, as
    /// `PressInteraction.Press(pressPosition)`.
    pub fn new(press_position: Offset) -> Press {
        Press { id: next_id(), press_position }
    }
}

/// The same press, not one at the same place: compared by identity, as Compose's class.
impl PartialEq for Press {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Press {}

impl fmt::Debug for Press {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Press#{}({:?})", self.id, self.press_position)
    }
}

/// A press and how it ended, as `PressInteraction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PressInteraction {
    /// Pressed, as `PressInteraction.Press`.
    Press(Press),
    /// The press released (a click), as `PressInteraction.Release(press)`.
    Release(Press),
    /// The press cancelled (moved off, a scroll took it), as `PressInteraction.Cancel(press)`.
    Cancel(Press),
}

impl PressInteraction {
    /// The press it is or ends.
    pub fn press(&self) -> Press {
        match *self {
            PressInteraction::Press(press)
            | PressInteraction::Release(press)
            | PressInteraction::Cancel(press) => press,
        }
    }
}

/// A focus gained or lost, as `FocusInteraction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FocusInteraction {
    /// As `FocusInteraction.Focus`.
    Focus(Focus),
    /// The focus lost, as `FocusInteraction.Unfocus(focus)`.
    Unfocus(Focus),
}

/// A mouse over the component or gone, as `HoverInteraction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HoverInteraction {
    /// As `HoverInteraction.Enter`.
    Enter(HoverEnter),
    /// The hover over, as `HoverInteraction.Exit(enter)`.
    Exit(HoverEnter),
}

/// A drag started and how it ended, as `DragInteraction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DragInteraction {
    /// As `DragInteraction.Start`.
    Start(DragStart),
    /// The drag ended, as `DragInteraction.Stop(start)`.
    Stop(DragStart),
    /// The drag cancelled, as `DragInteraction.Cancel(start)`.
    Cancel(DragStart),
}

/// Something that happened to a component, as Compose's `Interaction`: the kinds foundation
/// emits, or one of the app's own (`Custom`, compared by identity, as a Kotlin object).
#[derive(Clone)]
pub enum Interaction {
    Press(PressInteraction),
    Focus(FocusInteraction),
    Hover(HoverInteraction),
    Drag(DragInteraction),
    /// An interaction of the app's own, as a class implementing `Interaction`: downcast it.
    Custom(Rc<dyn Any>),
}

impl Interaction {
    /// An interaction of the app's own, as a Kotlin `object : Interaction`.
    pub fn custom(value: impl Any) -> Interaction {
        Interaction::Custom(Rc::new(value))
    }
}

impl PartialEq for Interaction {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Interaction::Press(a), Interaction::Press(b)) => a == b,
            (Interaction::Focus(a), Interaction::Focus(b)) => a == b,
            (Interaction::Hover(a), Interaction::Hover(b)) => a == b,
            (Interaction::Drag(a), Interaction::Drag(b)) => a == b,
            (Interaction::Custom(a), Interaction::Custom(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Debug for Interaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Interaction::Press(press) => write!(formatter, "{press:?}"),
            Interaction::Focus(focus) => write!(formatter, "{focus:?}"),
            Interaction::Hover(hover) => write!(formatter, "{hover:?}"),
            Interaction::Drag(drag) => write!(formatter, "{drag:?}"),
            Interaction::Custom(_) => formatter.write_str("Custom"),
        }
    }
}

impl From<PressInteraction> for Interaction {
    fn from(interaction: PressInteraction) -> Interaction {
        Interaction::Press(interaction)
    }
}

impl From<FocusInteraction> for Interaction {
    fn from(interaction: FocusInteraction) -> Interaction {
        Interaction::Focus(interaction)
    }
}

impl From<HoverInteraction> for Interaction {
    fn from(interaction: HoverInteraction) -> Interaction {
        Interaction::Hover(interaction)
    }
}

impl From<DragInteraction> for Interaction {
    fn from(interaction: DragInteraction) -> Interaction {
        Interaction::Drag(interaction)
    }
}

/// One collector's queue.
#[derive(Default)]
struct Subscriber {
    queue: RefCell<VecDeque<Interaction>>,
    waker: RefCell<Option<Waker>>,
}

/// The interactions under way, by kind, each by identity: what `is_pressed` and its siblings
/// read (and compose-rs's indications, `InteractionStates`).
#[derive(Default)]
pub(crate) struct Ongoing {
    presses: Vec<Press>,
    focuses: Vec<Focus>,
    hovers: Vec<HoverEnter>,
    drags: Vec<DragStart>,
}

impl Ongoing {
    pub(crate) fn pressed(&self) -> bool {
        !self.presses.is_empty()
    }

    pub(crate) fn hovered(&self) -> bool {
        !self.hovers.is_empty()
    }

    pub(crate) fn focused(&self) -> bool {
        !self.focuses.is_empty()
    }

    pub(crate) fn apply(&mut self, interaction: &Interaction) {
        fn remove<T: PartialEq>(list: &mut Vec<T>, item: &T) {
            if let Some(index) = list.iter().position(|value| value == item) {
                list.remove(index);
            }
        }
        match interaction {
            Interaction::Press(PressInteraction::Press(press)) => self.presses.push(*press),
            Interaction::Press(PressInteraction::Release(press))
            | Interaction::Press(PressInteraction::Cancel(press)) => {
                remove(&mut self.presses, press)
            }
            Interaction::Focus(FocusInteraction::Focus(focus)) => self.focuses.push(*focus),
            Interaction::Focus(FocusInteraction::Unfocus(focus)) => {
                remove(&mut self.focuses, focus)
            }
            Interaction::Hover(HoverInteraction::Enter(enter)) => self.hovers.push(*enter),
            Interaction::Hover(HoverInteraction::Exit(enter)) => remove(&mut self.hovers, enter),
            Interaction::Drag(DragInteraction::Start(start)) => self.drags.push(*start),
            Interaction::Drag(DragInteraction::Stop(start))
            | Interaction::Drag(DragInteraction::Cancel(start)) => remove(&mut self.drags, start),
            Interaction::Custom(_) => {}
        }
    }
}

/// Which of the source's derived states.
#[derive(Clone, Copy)]
enum Kind {
    Pressed,
    Focused,
    Hovered,
    Dragged,
}

/// A listener's callback (`InteractionSource::listen`): called with each interaction as it's
/// emitted, while the listener is held.
pub(crate) struct Listener(RefCell<ListenerCallback>);

type ListenerCallback = Box<dyn FnMut(&Interaction)>;

struct SourceInner {
    subscribers: RefCell<Vec<Weak<Subscriber>>>,
    listeners: RefCell<Vec<Weak<Listener>>>,
    ongoing: RefCell<Ongoing>,
    /// The derived states, made when first read.
    states: [OnceCell<OwnedState<bool>>; 4],
}

impl SourceInner {
    fn is(&self, kind: Kind) -> bool {
        let ongoing = self.ongoing.borrow();
        match kind {
            Kind::Pressed => !ongoing.presses.is_empty(),
            Kind::Focused => !ongoing.focuses.is_empty(),
            Kind::Hovered => !ongoing.hovers.is_empty(),
            Kind::Dragged => !ongoing.drags.is_empty(),
        }
    }

    fn state(&self, kind: Kind) -> &OwnedState<bool> {
        self.states[kind as usize].get_or_init(|| compose_runtime::mutable_state_of(self.is(kind)))
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
    /// a task polling a queue would cost an executor round per emission. A callback writing
    /// state while composing (an emission from a node's update) defers the write as
    /// [`write_soon`] does.
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
                write_soon(state, self.inner.is(kind));
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

/// Writes `value` now or, while composing (a node detached by a recomposition), right after.
pub(crate) fn write_soon<T: Clone + PartialEq + 'static>(state: &OwnedState<T>, value: T) {
    if state.try_get().as_ref() == Some(&value) {
        return;
    }
    if compose_runtime::is_composing() {
        let state = state.handle();
        compose_runtime::platform::launch(async move {
            let _ = state.try_set(value);
        });
    } else {
        state.set(value);
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
/// the node's task (one task, woken for each, made at the first: Compose's
/// `coroutineScope.launch { emit(…) }`), [`emit_now`](Emitter::emit_now) sends what's queued,
/// then one at once (Compose's `tryEmit`, where the node ends its interactions, and its hovers,
/// which compose-rs sends at once so a hover the frame's layout finds draws in that frame), so
/// a release then an exit go out in that order. [`cancel`](Emitter::cancel) is the node's
/// scope cancelled on detach: what's queued isn't sent, but cancellations, as Compose's
/// `invokeOnCompletion { tryEmit(cancel) }`.
#[derive(Default)]
pub(crate) struct Emitter(OnceCell<Rc<EmitterQueue>>);

#[derive(Default)]
struct EmitterQueue {
    /// Each with the source it goes to: a node given a new source still sends what it queued
    /// to the old one, as Compose's launched coroutines captured it.
    pending: RefCell<VecDeque<(InteractionSource, Interaction)>>,
    /// The task sending them, while it runs.
    job: Cell<Option<Job>>,
    waker: RefCell<Option<Waker>>,
}

impl EmitterQueue {
    /// Sends what's queued, in order (each taken before it's sent: a callback may queue more).
    fn send_pending(&self) {
        loop {
            let Some((source, interaction)) = self.pending.borrow_mut().pop_front() else {
                break;
            };
            source.send(interaction);
        }
    }
}

impl Emitter {
    /// Queues `interaction` for `source`, sent by the node's task when it next runs.
    pub(crate) fn emit_soon(
        &self,
        source: &InteractionSource,
        interaction: impl Into<Interaction>,
    ) {
        let queue = self.0.get_or_init(Rc::default);
        queue.pending.borrow_mut().push_back((source.clone(), interaction.into()));
        match queue.job.get() {
            Some(job) if job.is_active() => {
                if let Some(waker) = queue.waker.borrow_mut().take() {
                    waker.wake();
                }
            }
            _ => {
                let weak = Rc::downgrade(queue);
                let job = compose_runtime::platform::launch(std::future::poll_fn(move |context| {
                    let Some(queue) = weak.upgrade() else { return Poll::Ready(()) };
                    queue.send_pending();
                    *queue.waker.borrow_mut() = Some(context.waker().clone());
                    Poll::Pending
                }));
                queue.job.set(Some(job));
            }
        }
    }

    /// Sends what's queued, then `interaction` to `source`, now.
    pub(crate) fn emit_now(&self, source: &InteractionSource, interaction: impl Into<Interaction>) {
        if let Some(queue) = self.0.get() {
            queue.send_pending();
        }
        source.send(interaction.into());
    }

    /// The node detached or reset (its scope cancelled): what's queued is dropped but its
    /// cancellations, sent now, and the task ends.
    pub(crate) fn cancel(&self) {
        let Some(queue) = self.0.get() else { return };
        if let Some(job) = queue.job.take() {
            job.cancel();
        }
        queue.waker.borrow_mut().take();
        loop {
            let Some((source, interaction)) = queue.pending.borrow_mut().pop_front() else {
                break;
            };
            if matches!(interaction, Interaction::Press(PressInteraction::Cancel(_))) {
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
        let press = Press::new(Offset::ZERO);
        emitter.emit_soon(&source, PressInteraction::Press(press));
        emitter.emit_soon(&source, PressInteraction::Release(press));
        assert!(heard.borrow().is_empty(), "queued for the node's task");
        emitter.emit_now(&source, HoverInteraction::Exit(HoverEnter::new()));
        assert_eq!(*heard.borrow(), ["Press", "Release", "Exit"]);
        emitter.emit_soon(&source, PressInteraction::Press(press));
        compose_runtime::platform::run_ready();
        assert_eq!(*heard.borrow(), ["Press", "Release", "Exit", "Press"], "its task sent it");
    }

    /// Cancelled (its node detached), an emitter sends none of what it queued but its
    /// cancellations, as Compose's node scope cancelled with `invokeOnCompletion { tryEmit }`
    /// (D1057): a press queued before the node left never reaches a collector after it.
    #[test]
    fn a_cancelled_emitter_sends_only_its_cancellations() {
        let source = MutableInteractionSource::new();
        let (_listener, heard) = heard(&source);
        let emitter = Emitter::default();
        let (first, second) = (Press::new(Offset::ZERO), Press::new(Offset::ZERO));
        emitter.emit_soon(&source, PressInteraction::Press(first));
        emitter.emit_soon(&source, PressInteraction::Cancel(second));
        emitter.cancel();
        compose_runtime::platform::run_ready();
        assert_eq!(*heard.borrow(), ["Cancel"]);
        assert!(!source.is_pressed());
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
