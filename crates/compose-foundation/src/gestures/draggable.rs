//! `Modifier.draggable` and `Modifier.draggable2D`, on Compose's `DragGestureNode`
//! (`drag_gesture`), with the states their deltas go to.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{ComposeArg, CoroutineScope, composable};
use compose_ui::{Modifier, ModifierNodeElement, MutableInteractionSource, Offset, Velocity};

use super::Orientation;
use super::drag_gesture::{Deltas, DragBehavior, DragGestureNode};
use super::scrollable::LocalBoxFuture;
use crate::mutator::{MutatePriority, MutationInterrupted, MutatorMutex};

/// Where a drag's deltas go, as `DragScope`: valid in the state's `drag`.
#[derive(Clone)]
pub struct DragScope {
    on_delta: Rc<dyn Fn(f32)>,
}

impl DragScope {
    /// A scope calling `on_delta` with each drag, for a state of your own.
    pub fn new(on_delta: impl Fn(f32) + 'static) -> DragScope {
        DragScope { on_delta: Rc::new(on_delta) }
    }

    /// Drags by `pixels`, as `dragBy`.
    pub fn drag_by(&self, pixels: f32) {
        (self.on_delta)(pixels)
    }
}

/// What a state's `drag` runs.
pub type DragBlock<'a> = Box<dyn FnOnce(DragScope) -> LocalBoxFuture<'a, ()> + 'a>;

/// Where a drag's deltas go, as Compose's `DraggableState`, an interface for a drag of your own (a
/// slider's state is one). The state `DraggableState(onDelta)` makes is [`DefaultDraggableState`],
/// from [`draggable_state`]. `drag` itself is in [`DraggableStateExt`], on every state.
pub trait DraggableState {
    /// Runs `block` with this state's scope, as a mutation of `priority`: Compose's `drag`, boxed
    /// so the trait is object-safe; call [`DraggableStateExt::drag`].
    fn drag_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: DragBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>>;

    /// Drags by `delta` pixels, outside any `drag`, as `dispatchRawDelta`.
    fn dispatch_raw_delta(&self, delta: f32);
}

/// `drag` on every [`DraggableState`].
pub trait DraggableStateExt: DraggableState {
    /// Takes the drag for `block`, as `drag`: a drag going on of the same or a lower priority is
    /// interrupted, one of a higher priority makes this one return [`MutationInterrupted`].
    fn drag<'a, F, Fut>(
        &'a self,
        priority: MutatePriority,
        block: F,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>>
    where
        F: FnOnce(DragScope) -> Fut + 'a,
        Fut: std::future::Future<Output = ()> + 'a,
    {
        self.drag_boxed(priority, Box::new(move |scope| Box::pin(block(scope))))
    }
}

impl<T: DraggableState + ?Sized> DraggableStateExt for T {}

/// The state [`draggable_state`] makes, as `DraggableState(onDelta)`.
#[derive(Clone)]
pub struct DefaultDraggableState {
    on_delta: Rc<dyn Fn(f32)>,
    mutex: Rc<MutatorMutex>,
}

/// A state calling `on_delta` with each delta (pixels along the axis), as
/// `DraggableState(onDelta)`.
pub fn draggable_state(on_delta: impl Fn(f32) + 'static) -> DefaultDraggableState {
    DefaultDraggableState { on_delta: Rc::new(on_delta), mutex: Rc::default() }
}

impl DraggableState for DefaultDraggableState {
    fn drag_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: DragBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        let scope = DragScope { on_delta: self.on_delta.clone() };
        Box::pin(self.mutex.mutate_with(scope, priority, block))
    }

    fn dispatch_raw_delta(&self, delta: f32) {
        (self.on_delta)(delta)
    }
}

impl ComposeArg for DefaultDraggableState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.on_delta, &other.on_delta)
    }
}

/// A draggable state remembered at its call site, as `rememberDraggableState(onDelta)`: the
/// newest `on_delta` is called.
#[composable(non_restartable)]
pub fn remember_draggable_state(on_delta: impl Fn(f32) + 'static) -> DefaultDraggableState {
    let latest = compose_runtime::remember(|| {
        Rc::new(RefCell::new(None::<compose_runtime::Callback<dyn Fn(f32)>>))
    });
    *latest.borrow_mut() = Some(on_delta);
    compose_runtime::remember(move || {
        let latest = latest.clone();
        draggable_state(move |delta| {
            let on_delta = latest.borrow().clone();
            if let Some(on_delta) = on_delta {
                on_delta(delta);
            }
        })
    })
}

/// Called when a drag starts, with where it started and the node's scope, before the first delta,
/// as Compose's suspending `onDragStarted`, which it starts undispatched. What takes time goes in a
/// task launched in the scope, whose body runs later, after the first delta: Compose's own body
/// runs up to its first suspension before it.
pub type OnDragStarted = Rc<dyn Fn(CoroutineScope, Offset)>;
/// Called when a drag stops, with its velocity along the axis in pixels per second (0 if it's
/// cancelled) and the node's scope.
pub type OnDragStopped = Rc<dyn Fn(CoroutineScope, f32)>;

/// What `Modifier::draggable_with` takes beyond its state and axis, as `draggable`'s default
/// parameters.
#[derive(Clone)]
pub struct DraggableOptions {
    pub enabled: bool,
    /// Told of the drags, as `DragInteraction`.
    pub interaction_source: Option<MutableInteractionSource>,
    /// Drags from the press, without waiting for the slop.
    pub start_drag_immediately: bool,
    pub on_drag_started: Option<OnDragStarted>,
    pub on_drag_stopped: Option<OnDragStopped>,
    /// Negates the deltas and the velocity.
    pub reverse_direction: bool,
}

impl Default for DraggableOptions {
    fn default() -> DraggableOptions {
        DraggableOptions {
            enabled: true,
            interaction_source: None,
            start_drag_immediately: false,
            on_drag_started: None,
            on_drag_stopped: None,
            reverse_direction: false,
        }
    }
}

/// Drags along `orientation` are told to `state` as deltas, as `Modifier.draggable`: a press
/// that moves past the touch slop along the axis starts one (`on_drag_started` with where it
/// crossed the slop), with the movement past the slop as its first delta; the moves of its pointer
/// after it (consumed) are its deltas (negated if `reverse_direction`), and the release ends it
/// (`on_drag_stopped` with its velocity along the axis, in pixels per second; also called, with
/// 0, when a parent takes the moves or the gesture is cancelled).
/// A parent that takes the moves before the slop is crossed keeps the gesture. The newest
/// callbacks are called; a new `state`, `orientation` or `reverse_direction`, or disabling it, ends
/// a drag going on.
pub trait DraggableModifier {
    /// With what's most often given.
    #[allow(clippy::too_many_arguments)]
    fn draggable<S: DraggableState + ComposeArg + Clone + 'static>(
        self,
        state: S,
        orientation: Orientation,
        enabled: bool,
        reverse_direction: bool,
        on_drag_started: impl Fn(Offset) + 'static,
        on_drag_stopped: impl Fn(f32) + 'static,
    ) -> Modifier;

    /// With all of Compose's parameters.
    fn draggable_with<S: DraggableState + ComposeArg + Clone + 'static>(
        self,
        state: S,
        orientation: Orientation,
        options: DraggableOptions,
    ) -> Modifier;

    /// Drags any way are told to `state` as deltas, as `Modifier.draggable2D`; the slop is a
    /// distance.
    fn draggable_2d<S: Draggable2DState + ComposeArg + Clone + 'static>(
        self,
        state: S,
        options: Draggable2DOptions,
    ) -> Modifier;
}

impl DraggableModifier for Modifier {
    fn draggable<S: DraggableState + ComposeArg + Clone + 'static>(
        self,
        state: S,
        orientation: Orientation,
        enabled: bool,
        reverse_direction: bool,
        on_drag_started: impl Fn(Offset) + 'static,
        on_drag_stopped: impl Fn(f32) + 'static,
    ) -> Modifier {
        self.draggable_with(
            state,
            orientation,
            DraggableOptions {
                enabled,
                reverse_direction,
                on_drag_started: Some(Rc::new(move |_, position| on_drag_started(position))),
                on_drag_stopped: Some(Rc::new(move |_, velocity| on_drag_stopped(velocity))),
                ..DraggableOptions::default()
            },
        )
    }

    fn draggable_with<S: DraggableState + ComposeArg + Clone + 'static>(
        self,
        state: S,
        orientation: Orientation,
        options: DraggableOptions,
    ) -> Modifier {
        let dyn_state: Rc<dyn DraggableState> = Rc::new(state.clone());
        self.then_custom(DraggableElement { state, dyn_state, orientation, options })
    }

    fn draggable_2d<S: Draggable2DState + ComposeArg + Clone + 'static>(
        self,
        state: S,
        options: Draggable2DOptions,
    ) -> Modifier {
        let dyn_state: Rc<dyn Draggable2DState> = Rc::new(state.clone());
        self.then_custom(Draggable2DElement { state, dyn_state, options })
    }
}

fn same_option<T: ?Sized>(one: &Option<Rc<T>>, other: &Option<Rc<T>>) -> bool {
    match (one, other) {
        (None, None) => true,
        (Some(one), Some(other)) => Rc::ptr_eq(one, other),
        _ => false,
    }
}

fn same_source(
    one: &Option<MutableInteractionSource>,
    other: &Option<MutableInteractionSource>,
) -> bool {
    match (one, other) {
        (None, None) => true,
        (Some(one), Some(other)) => one.same(other),
        _ => false,
    }
}

/// What a draggable's drag reads: its newest inputs.
struct DraggableInputs {
    /// The state as given, to compare with the next one.
    state_as_given: Box<dyn std::any::Any>,
    state: Rc<dyn DraggableState>,
    orientation: Orientation,
    start_drag_immediately: bool,
    on_drag_started: Option<OnDragStarted>,
    on_drag_stopped: Option<OnDragStopped>,
    reverse_direction: bool,
}

struct DraggableBehavior(RefCell<DraggableInputs>);

impl DragBehavior for DraggableBehavior {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn drag(
        self: Rc<Self>,
        deltas: Deltas,
    ) -> LocalBoxFuture<'static, Result<(), MutationInterrupted>> {
        let state = self.0.borrow().state.clone();
        Box::pin(async move {
            let inputs = self.clone();
            state
                .drag(MutatePriority::UserInput, |scope| async move {
                    while let Some(delta) = deltas.next().await {
                        let (vertical, reverse) = {
                            let inputs = inputs.0.borrow();
                            (inputs.orientation == Orientation::Vertical, inputs.reverse_direction)
                        };
                        let along = if vertical { delta.y } else { delta.x };
                        scope.drag_by(if reverse { -along } else { along });
                    }
                })
                .await
        })
    }

    fn on_drag_started(&self, scope: CoroutineScope, position: Offset) {
        let started = self.0.borrow().on_drag_started.clone();
        if let Some(started) = started {
            started(scope, position);
        }
    }

    fn on_drag_stopped(&self, scope: CoroutineScope, velocity: Velocity) {
        let (stopped, vertical, reverse) = {
            let inputs = self.0.borrow();
            (
                inputs.on_drag_stopped.clone(),
                inputs.orientation == Orientation::Vertical,
                inputs.reverse_direction,
            )
        };
        if let Some(stopped) = stopped {
            let along = if vertical { velocity.y } else { velocity.x };
            stopped(scope, if reverse { -along } else { along });
        }
    }

    fn start_drag_immediately(&self) -> bool {
        self.0.borrow().start_drag_immediately
    }
}

struct DraggableElement<S> {
    state: S,
    dyn_state: Rc<dyn DraggableState>,
    orientation: Orientation,
    options: DraggableOptions,
}

impl<S: Clone + 'static> DraggableElement<S> {
    fn inputs(&self) -> DraggableInputs {
        DraggableInputs {
            state_as_given: Box::new(self.state.clone()),
            state: self.dyn_state.clone(),
            orientation: self.orientation,
            start_drag_immediately: self.options.start_drag_immediately,
            on_drag_started: self.options.on_drag_started.clone(),
            on_drag_stopped: self.options.on_drag_stopped.clone(),
            reverse_direction: self.options.reverse_direction,
        }
    }
}

impl<S: DraggableState + ComposeArg + Clone + 'static> ModifierNodeElement for DraggableElement<S> {
    type Node = DragGestureNode;
    fn create(&self) -> Self::Node {
        DragGestureNode::new(
            Rc::new(DraggableBehavior(RefCell::new(self.inputs()))),
            self.options.enabled,
            Some(self.orientation),
            self.options.interaction_source.clone(),
        )
    }
    fn update(&self, node: &mut Self::Node) {
        let behavior = node.behavior.clone();
        let behavior =
            behavior.as_any().downcast_ref::<DraggableBehavior>().expect("its element's");
        let reset = {
            let inputs = behavior.0.borrow();
            !inputs.state_as_given.downcast_ref::<S>().is_some_and(|old| old.same(&self.state))
                || inputs.reverse_direction != self.options.reverse_direction
        };
        *behavior.0.borrow_mut() = self.inputs();
        node.update(
            self.options.enabled,
            Some(self.orientation),
            self.options.interaction_source.clone(),
            reset,
        );
    }
    fn same(&self, other: &Self) -> bool {
        self.state.same(&other.state)
            && self.orientation == other.orientation
            && self.options.enabled == other.options.enabled
            && same_source(&self.options.interaction_source, &other.options.interaction_source)
            && self.options.start_drag_immediately == other.options.start_drag_immediately
            && same_option(&self.options.on_drag_started, &other.options.on_drag_started)
            && same_option(&self.options.on_drag_stopped, &other.options.on_drag_stopped)
            && self.options.reverse_direction == other.options.reverse_direction
    }
    fn name(&self) -> &'static str {
        "draggable"
    }
}

/// Where a 2D drag's deltas go, as `Drag2DScope`.
#[derive(Clone)]
pub struct Drag2DScope {
    on_delta: Rc<dyn Fn(Offset)>,
}

impl Drag2DScope {
    /// A scope calling `on_delta` with each drag, for a state of your own.
    pub fn new(on_delta: impl Fn(Offset) + 'static) -> Drag2DScope {
        Drag2DScope { on_delta: Rc::new(on_delta) }
    }

    /// Drags by `pixels`, as `dragBy`.
    pub fn drag_by(&self, pixels: Offset) {
        (self.on_delta)(pixels)
    }
}

/// What a 2D state's `drag` runs.
pub type Drag2DBlock<'a> = Box<dyn FnOnce(Drag2DScope) -> LocalBoxFuture<'a, ()> + 'a>;

/// Where a 2D drag's deltas go, as `Draggable2DState`, as [`DraggableState`] is for one axis.
pub trait Draggable2DState {
    /// Runs `block` with this state's scope, as a mutation of `priority`; call
    /// [`Draggable2DStateExt::drag`].
    fn drag_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: Drag2DBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>>;

    /// Drags by `delta`, outside any `drag`, as `dispatchRawDelta`.
    fn dispatch_raw_delta(&self, delta: Offset);
}

/// `drag` on every [`Draggable2DState`].
pub trait Draggable2DStateExt: Draggable2DState {
    /// Takes the drag for `block`, as `drag`.
    fn drag<'a, F, Fut>(
        &'a self,
        priority: MutatePriority,
        block: F,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>>
    where
        F: FnOnce(Drag2DScope) -> Fut + 'a,
        Fut: std::future::Future<Output = ()> + 'a,
    {
        self.drag_boxed(priority, Box::new(move |scope| Box::pin(block(scope))))
    }
}

impl<T: Draggable2DState + ?Sized> Draggable2DStateExt for T {}

/// The state [`draggable_2d_state`] makes, as `Draggable2DState(onDelta)`.
#[derive(Clone)]
pub struct DefaultDraggable2DState {
    on_delta: Rc<dyn Fn(Offset)>,
    mutex: Rc<MutatorMutex>,
}

/// A state calling `on_delta` with each delta, as `Draggable2DState(onDelta)`.
pub fn draggable_2d_state(on_delta: impl Fn(Offset) + 'static) -> DefaultDraggable2DState {
    DefaultDraggable2DState { on_delta: Rc::new(on_delta), mutex: Rc::default() }
}

impl Draggable2DState for DefaultDraggable2DState {
    fn drag_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: Drag2DBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        let scope = Drag2DScope { on_delta: self.on_delta.clone() };
        Box::pin(self.mutex.mutate_with(scope, priority, block))
    }

    fn dispatch_raw_delta(&self, delta: Offset) {
        (self.on_delta)(delta)
    }
}

impl ComposeArg for DefaultDraggable2DState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.on_delta, &other.on_delta)
    }
}

/// A 2D draggable state remembered at its call site, as `rememberDraggable2DState(onDelta)`: the
/// newest `on_delta` is called.
#[composable(non_restartable)]
pub fn remember_draggable_2d_state(on_delta: impl Fn(Offset) + 'static) -> DefaultDraggable2DState {
    let latest = compose_runtime::remember(|| {
        Rc::new(RefCell::new(None::<compose_runtime::Callback<dyn Fn(Offset)>>))
    });
    *latest.borrow_mut() = Some(on_delta);
    compose_runtime::remember(move || {
        let latest = latest.clone();
        draggable_2d_state(move |delta| {
            let on_delta = latest.borrow().clone();
            if let Some(on_delta) = on_delta {
                on_delta(delta);
            }
        })
    })
}

/// Called when a 2D drag stops, with its velocity (zero if it's cancelled) and the node's scope.
pub type OnDrag2DStopped = Rc<dyn Fn(CoroutineScope, Velocity)>;

/// What `Modifier::draggable_2d` takes beyond its state, as `draggable2D`'s default parameters.
#[derive(Clone)]
pub struct Draggable2DOptions {
    pub enabled: bool,
    pub interaction_source: Option<MutableInteractionSource>,
    pub start_drag_immediately: bool,
    pub on_drag_started: Option<OnDragStarted>,
    pub on_drag_stopped: Option<OnDrag2DStopped>,
    pub reverse_direction: bool,
}

impl Default for Draggable2DOptions {
    fn default() -> Draggable2DOptions {
        Draggable2DOptions {
            enabled: true,
            interaction_source: None,
            start_drag_immediately: false,
            on_drag_started: None,
            on_drag_stopped: None,
            reverse_direction: false,
        }
    }
}

struct Draggable2DInputs {
    /// The state as given, to compare with the next one.
    state_as_given: Box<dyn std::any::Any>,
    state: Rc<dyn Draggable2DState>,
    start_drag_immediately: bool,
    on_drag_started: Option<OnDragStarted>,
    on_drag_stopped: Option<OnDrag2DStopped>,
    reverse_direction: bool,
}

struct Draggable2DBehavior(RefCell<Draggable2DInputs>);

impl DragBehavior for Draggable2DBehavior {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn drag(
        self: Rc<Self>,
        deltas: Deltas,
    ) -> LocalBoxFuture<'static, Result<(), MutationInterrupted>> {
        let state = self.0.borrow().state.clone();
        Box::pin(async move {
            let inputs = self.clone();
            state
                .drag(MutatePriority::UserInput, |scope| async move {
                    while let Some(delta) = deltas.next().await {
                        let reverse = inputs.0.borrow().reverse_direction;
                        scope.drag_by(if reverse { Offset::ZERO - delta } else { delta });
                    }
                })
                .await
        })
    }

    fn on_drag_started(&self, scope: CoroutineScope, position: Offset) {
        let started = self.0.borrow().on_drag_started.clone();
        if let Some(started) = started {
            started(scope, position);
        }
    }

    fn on_drag_stopped(&self, scope: CoroutineScope, velocity: Velocity) {
        let (stopped, reverse) = {
            let inputs = self.0.borrow();
            (inputs.on_drag_stopped.clone(), inputs.reverse_direction)
        };
        if let Some(stopped) = stopped {
            let way = if reverse { -1.0 } else { 1.0 };
            stopped(scope, Velocity::new(velocity.x * way, velocity.y * way));
        }
    }

    fn start_drag_immediately(&self) -> bool {
        self.0.borrow().start_drag_immediately
    }
}

struct Draggable2DElement<S> {
    state: S,
    dyn_state: Rc<dyn Draggable2DState>,
    options: Draggable2DOptions,
}

impl<S: Clone + 'static> Draggable2DElement<S> {
    fn inputs(&self) -> Draggable2DInputs {
        Draggable2DInputs {
            state_as_given: Box::new(self.state.clone()),
            state: self.dyn_state.clone(),
            start_drag_immediately: self.options.start_drag_immediately,
            on_drag_started: self.options.on_drag_started.clone(),
            on_drag_stopped: self.options.on_drag_stopped.clone(),
            reverse_direction: self.options.reverse_direction,
        }
    }
}

impl<S: Draggable2DState + ComposeArg + Clone + 'static> ModifierNodeElement
    for Draggable2DElement<S>
{
    type Node = DragGestureNode;
    fn create(&self) -> Self::Node {
        DragGestureNode::new(
            Rc::new(Draggable2DBehavior(RefCell::new(self.inputs()))),
            self.options.enabled,
            None,
            self.options.interaction_source.clone(),
        )
    }
    fn update(&self, node: &mut Self::Node) {
        let behavior = node.behavior.clone();
        let behavior =
            behavior.as_any().downcast_ref::<Draggable2DBehavior>().expect("its element's");
        let reset = {
            let inputs = behavior.0.borrow();
            !inputs.state_as_given.downcast_ref::<S>().is_some_and(|old| old.same(&self.state))
                || inputs.reverse_direction != self.options.reverse_direction
        };
        *behavior.0.borrow_mut() = self.inputs();
        node.update(self.options.enabled, None, self.options.interaction_source.clone(), reset);
    }
    fn same(&self, other: &Self) -> bool {
        self.state.same(&other.state)
            && self.options.enabled == other.options.enabled
            && same_source(&self.options.interaction_source, &other.options.interaction_source)
            && self.options.start_drag_immediately == other.options.start_drag_immediately
            && same_option(&self.options.on_drag_started, &other.options.on_drag_started)
            && same_option(&self.options.on_drag_stopped, &other.options.on_drag_stopped)
            && self.options.reverse_direction == other.options.reverse_direction
    }
    fn name(&self) -> &'static str {
        "draggable2D"
    }
}
