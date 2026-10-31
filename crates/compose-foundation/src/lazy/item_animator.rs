//! `Modifier::animate_item` and what runs it, as Compose's `LazyLayoutItemAnimator` and
//! `LazyLayoutItemAnimation`: placement animations, fades in and out, and items kept composed and
//! placed while they move out of the viewport.
//!
//! How it fits this host, where items compose in rounds at frame boundaries rather than in the
//! measurement (D1472):
//!
//! - The modifier gives its node's parent data, a layer and, with a fade out, a capture of what
//!   it draws. An item's node reads its children's parent data when measured ([`ItemChildren`]);
//!   a list none of whose items has the modifier stays [idle](ItemAnimator::idle), and pays that
//!   branch and the one asking whether it is.
//! - The measurement calls [`ItemAnimator::on_measured`] once it settled: a round lacking items
//!   isn't one, it only adds up the scroll it consumed. A move out of the viewport to an item not
//!   composed asks for it ([`ItemAnimator::kept`]), and the next round of the frame finds it.
//! - A placement's delta and a fade's alpha are state the item's layer reads while drawing, not
//!   a placement as Compose's: animating, an item is neither composed nor measured again, and the
//!   list only when an animation that it read ends.
//! - An item removed from the data goes at once, as Compose's: what fades out is what it last
//!   drew, drawn after the list's content ([`ItemAnimator::draw_disappearing`]), by a node the list
//!   has only once an item has had the modifier ([`displaying`]).
//!
//! All of this keeps to keys and offsets, not to how an item got composed: subcomposition (C8)
//! takes `kept` away and little else.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use compose_animation::{Animatable, AnimationEndReason, AnimationSpec, Spring, spring_with};
use compose_runtime::{CoroutineScope, Job, Owned, OwnedState, mutable_state_of};
use compose_ui::{
    Constraints, Density, DrawCapture, DrawScope, IntOffset, IntSize, Modifier, ModifierNode,
    ModifierNodeElement, NodeHandle, Offset, Rect,
};

use super::{ItemLayout, Items, KEY_WINDOW, LazyGridItemScope, LazyItemScope, LazyListInner};

const NOT_INITIALIZED: IntOffset = IntOffset { x: i32::MAX, y: i32::MAX };

/// How many items' nodes are known before those gone are forgotten.
const STALE_ITEMS_CHECK: usize = 256;

/// The scopes `animate_item` can be used in, as the three scopes declaring Compose's.
pub trait AnimateItemScope {}

impl AnimateItemScope for LazyItemScope {}
impl AnimateItemScope for LazyGridItemScope {}

/// A fade's default spec, as `animateItem`'s `fadeInSpec` and `fadeOutSpec`.
fn default_fade_spec() -> AnimationSpec {
    spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW).into()
}

/// A placement's default spec, as `animateItem`'s `placementSpec`.
fn default_placement_spec() -> AnimationSpec {
    spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW)
        .visibility_threshold(IntOffset::new(1, 1))
        .into()
}

/// What an interrupted placement goes on with unless its spec is a spring, as Compose's
/// `InterruptionSpec`.
fn interruption_spec() -> AnimationSpec {
    default_placement_spec()
}

/// `Modifier.animateItem`, on the modifiers of an item's outermost layouts.
pub trait AnimateItemModifier {
    /// Animates the item's appearance, placement and disappearance with the default springs, as
    /// `Modifier.animateItem()`. For the outermost layout of an item's content: its parent is
    /// what reads it. Items need keys for a move to be told from another item coming.
    fn animate_item(self, scope: &impl AnimateItemScope) -> Modifier;

    /// The same with `fade_in_spec`, `placement_spec` and `fade_out_spec`, each `None` for no
    /// animation of its kind: all three `None`, the modifier is left as it was.
    fn animate_item_with(
        self,
        scope: &impl AnimateItemScope,
        fade_in_spec: Option<AnimationSpec>,
        placement_spec: Option<AnimationSpec>,
        fade_out_spec: Option<AnimationSpec>,
    ) -> Modifier;
}

impl AnimateItemModifier for Modifier {
    fn animate_item(self, scope: &impl AnimateItemScope) -> Modifier {
        self.animate_item_with(
            scope,
            Some(default_fade_spec()),
            Some(default_placement_spec()),
            Some(default_fade_spec()),
        )
    }

    fn animate_item_with(
        self,
        _scope: &impl AnimateItemScope,
        fade_in_spec: Option<AnimationSpec>,
        placement_spec: Option<AnimationSpec>,
        fade_out_spec: Option<AnimationSpec>,
    ) -> Modifier {
        if fade_in_spec.is_none() && placement_spec.is_none() && fade_out_spec.is_none() {
            return self;
        }
        let fades_out = fade_out_spec.is_some();
        let capture = DrawCapture::without_own_layer();
        // The layer's block is made with each modifier and compares as another, so it reaches its
        // node's lasting data through this, which the element's `create` and `update` fill.
        let link: Rc<RefCell<Option<Rc<AnimateItemData>>>> = Rc::default();
        let block_link = link.clone();
        let modifier = self
            .then_custom(AnimateItemElement {
                specs: AnimateItemSpecs {
                    fade_in: fade_in_spec,
                    placement: placement_spec,
                    fade_out: fade_out_spec,
                },
                capture: capture.clone(),
                link,
            })
            .graphics_layer(move |layer| {
                if let Some(data) = &*block_link.borrow() {
                    data.apply_to_layer(layer);
                }
            });
        if fades_out { modifier.then_custom(ItemCaptureElement(capture)) } else { modifier }
    }
}

#[derive(Clone, PartialEq, Default)]
struct AnimateItemSpecs {
    fade_in: Option<AnimationSpec>,
    placement: Option<AnimationSpec>,
    fade_out: Option<AnimationSpec>,
}

/// A child of an item that has the modifier: its specs, what its layer shows and what it last drew,
/// as long as its modifier node lives.
pub(super) struct AnimateItemData {
    specs: RefCell<AnimateItemSpecs>,
    animation: RefCell<Option<Rc<ItemAnimation>>>,
    /// Read by the layer's block, so another animation taking the child runs it again.
    linked: OwnedState<u32>,
    capture: DrawCapture,
}

impl AnimateItemData {
    fn link(&self, animation: Option<Rc<ItemAnimation>>) {
        let mut linked = self.animation.borrow_mut();
        let same = match (&*linked, &animation) {
            (None, None) => true,
            (Some(one), Some(other)) => Rc::ptr_eq(one, other),
            _ => false,
        };
        if !same {
            *linked = animation;
            self.linked.update(|count| count.wrapping_add(1));
        }
    }

    fn apply_to_layer(&self, layer: &mut compose_ui::GraphicsLayerScope) {
        self.linked.get();
        if let Some(animation) = &*self.animation.borrow() {
            let delta = animation.placement_delta_state.get();
            let (sign_x, sign_y) = animation.mirror.get();
            layer.translation_x = (delta.x * sign_x) as f32;
            layer.translation_y = (delta.y * sign_y) as f32;
            layer.alpha = animation.alpha.get();
        }
    }
}

struct AnimateItemElement {
    specs: AnimateItemSpecs,
    capture: DrawCapture,
    link: Rc<RefCell<Option<Rc<AnimateItemData>>>>,
}

struct AnimateItemNode {
    data: Rc<AnimateItemData>,
}

impl ModifierNodeElement for AnimateItemElement {
    type Node = AnimateItemNode;

    fn create(&self) -> AnimateItemNode {
        let data = Rc::new(AnimateItemData {
            specs: RefCell::new(self.specs.clone()),
            animation: RefCell::new(None),
            linked: mutable_state_of(0),
            capture: self.capture.clone(),
        });
        *self.link.borrow_mut() = Some(data.clone());
        AnimateItemNode { data }
    }

    fn update(&self, node: &mut AnimateItemNode) {
        *node.data.specs.borrow_mut() = self.specs.clone();
        *self.link.borrow_mut() = Some(node.data.clone());
    }

    /// Never the same: `update` is what links the new layer block to the node's data. Its parent
    /// data stays the same object, so its parent isn't measured again for this.
    fn same(&self, _other: &Self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "animate_item"
    }
}

impl ModifierNode for AnimateItemNode {
    fn as_parent_data(&self) -> Option<&dyn compose_ui::ParentDataModifierNode> {
        Some(self)
    }
}

impl compose_ui::ParentDataModifierNode for AnimateItemNode {
    fn modify_parent_data(
        &self,
        _density: Density,
        _parent_data: Option<Rc<dyn std::any::Any>>,
    ) -> Option<Rc<dyn std::any::Any>> {
        Some(self.data.clone())
    }
}

/// Keeps what the child draws under its layer, for its fade out. It keeps the capture it was made
/// with: what it holds doesn't start over when the item composes again.
struct ItemCaptureElement(DrawCapture);

struct ItemCaptureNode(DrawCapture);

impl ModifierNodeElement for ItemCaptureElement {
    type Node = ItemCaptureNode;

    fn create(&self) -> ItemCaptureNode {
        ItemCaptureNode(self.0.clone())
    }

    fn update(&self, _node: &mut ItemCaptureNode) {}

    fn same(&self, _other: &Self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "animate_item_capture"
    }
}

impl ModifierNode for ItemCaptureNode {
    fn is_repaint_boundary(&self) -> bool {
        true
    }

    fn draw_capture(&self) -> Option<&DrawCapture> {
        Some(&self.0)
    }
}

/// How a child of an item stands in it.
#[derive(Clone)]
pub(super) struct ChildRecord {
    pub data: Option<Rc<AnimateItemData>>,
    /// Along the item in the list's order (not mirrored when reversed), as Compose's offsets.
    pub logical: IntOffset,
    /// Where it is in its item's node.
    pub physical: IntOffset,
    pub size: IntSize,
}

/// While it lives, placements and appearances don't animate, as `skipItemPlacementAnimation` in an
/// animated scroll to an item.
pub(super) struct SkipAnimation(Rc<ItemAnimator>);

impl SkipAnimation {
    pub(super) fn new(animator: Rc<ItemAnimator>) -> SkipAnimation {
        animator.skip_animation.set(true);
        SkipAnimation(animator)
    }
}

impl Drop for SkipAnimation {
    fn drop(&mut self) {
        self.0.skip_animation.set(false);
    }
}

/// What an item's node keeps of its children for the animator, in the node's measure policy: a
/// key and an empty list on an item without the modifier.
pub(super) struct ItemChildren {
    key: Cell<u64>,
    children: RefCell<Vec<ChildRecord>>,
}

impl ItemChildren {
    pub(super) fn new() -> ItemChildren {
        ItemChildren { key: Cell::new(0), children: RefCell::default() }
    }

    fn has_animations(&self) -> bool {
        self.children.borrow().iter().any(|child| child.data.is_some())
    }

    /// What the item's node found, once it placed its children: for an item with the modifier or
    /// that had it.
    pub(super) fn set(&self, children: Vec<ChildRecord>) {
        *self.children.borrow_mut() = children;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.children.borrow().is_empty()
    }

    pub(super) fn key(&self) -> u64 {
        self.key.get()
    }
}

/// The states of an item's fade, as `TransitionState` and `InterruptibleItemAnimation`'s state in
/// one: they change together here, nothing running between them.
#[derive(Clone, Copy, PartialEq)]
enum Fade {
    New,
    Idle,
    Entering,
    Exiting,
    Finished,
}

/// The animations of one child of an item, as Compose's `LazyLayoutItemAnimation`.
pub(super) struct ItemAnimation {
    this: Weak<ItemAnimation>,
    list: Weak<LazyListInner>,
    scope: CoroutineScope,
    enter: RefCell<Option<AnimationSpec>>,
    exit: RefCell<Option<AnimationSpec>>,
    placement_spec: RefCell<Option<AnimationSpec>>,
    fade: Cell<Fade>,
    fade_job: Cell<Option<Job>>,
    fade_animatable: Animatable<f32>,
    alpha: OwnedState<f32>,
    released: Cell<bool>,
    running_moving_away: Cell<bool>,
    placement_in_progress: Cell<bool>,
    /// The list reads whether it's in progress: it measures again when it ends.
    placement_read: Cell<bool>,
    placement_animatable: Animatable<IntOffset>,
    /// The delta, as state for the layer's block and as a value for the measurement's arithmetic.
    placement_delta_state: OwnedState<IntOffset>,
    placement_delta: Cell<IntOffset>,
    /// The last offset known in the list's coordinates, the target of a running placement.
    target_offset: Cell<IntOffset>,
    /// Which way a list's offsets go on screen: -1 along a reversed axis.
    mirror: Cell<(i32, i32)>,
    /// Where it's placed in the list, its delta left out.
    placement_offset: Cell<IntOffset>,
    /// Past these along the list, it has gone out of view.
    bounds: Cell<(i32, i32, bool)>,
    /// What it fades out as.
    capture: RefCell<Option<DrawCapture>>,
    size: Cell<IntSize>,
}

impl ItemAnimation {
    fn new(list: Weak<LazyListInner>, scope: CoroutineScope) -> Rc<ItemAnimation> {
        Rc::new_cyclic(|this| ItemAnimation {
            this: this.clone(),
            list,
            scope,
            enter: RefCell::new(None),
            exit: RefCell::new(None),
            placement_spec: RefCell::new(None),
            fade: Cell::new(Fade::New),
            fade_job: Cell::new(None),
            fade_animatable: Animatable::new(1.0),
            alpha: mutable_state_of(1.0),
            released: Cell::new(false),
            running_moving_away: Cell::new(false),
            placement_in_progress: Cell::new(false),
            placement_read: Cell::new(false),
            placement_animatable: Animatable::new(IntOffset::ZERO),
            placement_delta_state: mutable_state_of(IntOffset::ZERO),
            placement_delta: Cell::new(IntOffset::ZERO),
            target_offset: Cell::new(NOT_INITIALIZED),
            mirror: Cell::new((1, 1)),
            placement_offset: Cell::new(IntOffset::ZERO),
            bounds: Cell::new((i32::MIN, i32::MAX, true)),
            capture: RefCell::new(None),
            size: Cell::new(IntSize::ZERO),
        })
    }

    fn strong(&self) -> Rc<ItemAnimation> {
        self.this.upgrade().expect("compose-foundation: an animation in use")
    }

    fn set_delta(&self, delta: IntOffset) {
        if self.placement_delta.replace(delta) != delta {
            self.placement_delta_state.set(delta);
        }
    }

    fn set_alpha(&self, alpha: f32) {
        self.alpha.set(alpha);
    }

    fn is_disappearing(&self) -> bool {
        self.fade.get() == Fade::Exiting
    }

    fn is_disappearance_finished(&self) -> bool {
        self.fade.get() == Fade::Finished
    }

    fn measure_again(&self) {
        if let Some(list) = self.list.upgrade() {
            list.request_remeasure();
        }
    }

    fn apply_scroll_offset(&self, offset: IntOffset) {
        self.target_offset.set(self.target_offset.get() + offset);
    }

    /// Ends the placement at once, its delta gone.
    fn cancel_placement(&self) {
        if self.placement_in_progress.replace(false) {
            self.placement_animatable.snap_to(IntOffset::ZERO);
            self.set_delta(IntOffset::ZERO);
            self.running_moving_away.set(false);
            if self.placement_read.replace(false) {
                self.measure_again();
            }
        }
    }

    /// Both where it is and where it's going are past the list's bounds: as `place`'s check.
    fn check_bounds(&self) {
        if !self.placement_in_progress.get() {
            return;
        }
        let (min, max, vertical) = self.bounds.get();
        let size = self.size.get();
        let min = min - if vertical { size.height } else { size.width };
        let target = self.target_offset.get();
        let animated = target + self.placement_delta.get();
        let main = |offset: IntOffset| if vertical { offset.y } else { offset.x };
        if (main(target) <= min && main(animated) <= min)
            || (main(target) >= max && main(animated) >= max)
        {
            self.cancel_placement();
        }
    }

    fn animate_placement_delta(&self, delta: IntOffset, moving_away: bool) {
        let Some(spec) = self.placement_spec.borrow().clone() else { return };
        let total = self.placement_delta.get() - delta;
        self.set_delta(total);
        self.placement_in_progress.set(true);
        self.running_moving_away.set(moving_away);
        let this = self.strong();
        self.scope.launch(async move {
            let animatable = &this.placement_animatable;
            let running = animatable.is_running();
            let spec = match spec {
                AnimationSpec::Spring(_) => spec,
                _ if running => interruption_spec(),
                other => other,
            };
            if !running {
                animatable.snap_to(total);
            }
            let target = animatable.value() - total;
            let velocity = animatable.velocity();
            let result = animatable
                .animate_to_with(target, spec, velocity, |animatable| {
                    this.set_delta(animatable.value() - target);
                    this.check_bounds();
                })
                .await;
            if result.end_reason == AnimationEndReason::Finished
                && this.placement_in_progress.replace(false)
            {
                this.running_moving_away.set(false);
                if this.placement_read.replace(false) {
                    this.measure_again();
                }
            }
        });
    }

    fn cancel_fade(&self) {
        if let Some(job) = self.fade_job.take() {
            job.cancel();
        }
    }

    fn animate_enter_transition(&self) {
        if self.released.get() || self.fade.get() == Fade::Entering {
            return;
        }
        let was = self.fade.get();
        let Some(spec) = self.enter.borrow().clone() else {
            self.cancel_fade();
            if matches!(was, Fade::Exiting | Fade::Finished) {
                self.set_alpha(1.0);
                self.fade_animatable.snap_to(1.0);
            }
            self.fade.set(Fade::Idle);
            return;
        };
        self.fade.set(Fade::Entering);
        if was != Fade::Exiting {
            self.set_alpha(0.0);
            self.fade_animatable.snap_to(0.0);
        }
        self.run_fade(1.0, spec, Fade::Idle);
    }

    fn animate_exit_transition(&self) {
        if self.released.get() || self.fade.get() == Fade::Exiting {
            return;
        }
        let Some(spec) = self.exit.borrow().clone() else {
            self.cancel_fade();
            self.fade.set(Fade::Finished);
            return;
        };
        self.fade.set(Fade::Exiting);
        self.run_fade(0.0, spec, Fade::Finished);
    }

    /// To `target`, a running fade interrupted by this one, which goes on from the frame it was at.
    fn run_fade(&self, target: f32, spec: AnimationSpec, end: Fade) {
        let this = self.strong();
        let job = self.scope.launch(async move {
            let animatable = &this.fade_animatable;
            let velocity = animatable.velocity();
            let result = animatable
                .animate_to_with(target, spec, velocity, |animatable| {
                    this.set_alpha(animatable.value());
                })
                .await;
            if result.end_reason == AnimationEndReason::Finished {
                this.fade.set(end);
                if end == Fade::Finished {
                    // What the list read (`is_disappearing`) changed.
                    this.measure_again();
                }
            }
        });
        self.fade_job.set(Some(job));
    }

    fn release(&self) {
        if self.placement_in_progress.replace(false) {
            self.placement_animatable.stop();
        }
        self.cancel_fade();
        self.fade.set(Fade::New);
        self.released.set(true);
        self.running_moving_away.set(false);
        self.set_delta(IntOffset::ZERO);
        self.set_alpha(1.0);
        self.target_offset.set(NOT_INITIALIZED);
        *self.capture.borrow_mut() = None;
        *self.enter.borrow_mut() = None;
        *self.exit.borrow_mut() = None;
        *self.placement_spec.borrow_mut() = None;
    }
}

/// What the animator keeps per item key while it has animations, as `ItemInfo`.
struct ItemInfo {
    animations: Vec<Option<Rc<ItemAnimation>>>,
    constraints: Option<Constraints>,
    cross_offset: i32,
    lane: usize,
    span: usize,
    layout_min: i32,
    layout_max: i32,
    /// Its index when last seen: a move is told from this, over the rounds it waits to compose.
    index: usize,
    /// Where it moved to, waiting to compose there.
    waiting_at: Option<usize>,
}

impl ItemInfo {
    fn new(index: usize) -> ItemInfo {
        ItemInfo {
            animations: Vec::new(),
            constraints: None,
            cross_offset: 0,
            lane: 0,
            span: 1,
            layout_min: 0,
            layout_max: 0,
            index,
            waiting_at: None,
        }
    }

    fn release_all(&mut self) {
        for animation in self.animations.drain(..).flatten() {
            animation.release();
        }
    }
}

/// An item laid out in view (or beyond it, for a focus search), told to the animator.
pub(super) struct Positioned {
    pub index: usize,
    pub key: u64,
    /// Along the axis from the content's start (the end when reversed), as Compose's offsets, and
    /// across it, the content padding left out.
    pub main: i32,
    pub cross: i32,
    pub main_size_with_spacing: i32,
    pub lane: usize,
    pub span: usize,
    pub constraints: Option<Constraints>,
}

/// An item that moved out of view, placed where the animator says.
pub(super) struct MovedAway {
    pub index: usize,
    pub key: u64,
    pub main: i32,
    pub cross: i32,
    pub to_start: bool,
}

/// One settled measurement, as `onMeasured`'s arguments.
pub(super) struct Pass<'a> {
    pub items: &'a Rc<Items>,
    pub vertical: bool,
    pub reverse: bool,
    /// Which way its offsets go on screen, per axis: -1 where they're mirrored.
    pub mirror: (i32, i32),
    pub main_size: i32,
    pub lane_count: usize,
    pub layout_min: i32,
    pub layout_max: i32,
}

/// The items' keys around where the list was, kept while they're current: an item's key function
/// isn't run on what the data no longer has, as Compose's key map.
struct KeyIndex {
    items: Rc<Items>,
    keys: std::cell::OnceCell<HashMap<u64, usize>>,
    first: Cell<usize>,
}

impl KeyIndex {
    fn new(items: Rc<Items>, first: usize) -> KeyIndex {
        KeyIndex { items, keys: Default::default(), first: Cell::new(first) }
    }

    fn keys(&self) -> &HashMap<u64, usize> {
        self.keys.get_or_init(|| {
            let first = self.first.get();
            let start = first.saturating_sub(KEY_WINDOW);
            let end = (first + KEY_WINDOW + KEY_SLIDE).min(self.items.count);
            (start..end).map(|index| (self.items.key(index), index)).collect()
        })
    }
}

/// How far past the first visible item the key window goes beyond `KEY_WINDOW`, as Compose's
/// `NearestItemsSlidingWindowSize`.
const KEY_SLIDE: usize = 30;

/// The animator of a list, a grid or a staggered grid, as `LazyLayoutItemAnimator`.
pub(super) struct ItemAnimator {
    /// A child with the modifier has been measured.
    any_specs: Cell<bool>,
    /// The same as state, for the list's composition: it draws what's removed from then on.
    displays: OwnedState<bool>,
    info: RefCell<HashMap<u64, ItemInfo>>,
    /// Each item with the modifier's node, by key.
    items: RefCell<HashMap<u64, Weak<ItemLayout>>>,
    previous: RefCell<Option<KeyIndex>>,
    first_visible: Cell<usize>,
    previous_main_size: Cell<Option<i32>>,
    disappearing: RefCell<Vec<Rc<ItemAnimation>>>,
    /// Those moved out of view, with where they went, composed apart from the range.
    kept: RefCell<Vec<(u64, usize)>>,
    /// The scroll consumed since the last settled measurement, along the axis.
    consumed: Cell<i32>,
    /// As `skipItemPlacementAnimation`: an animated scroll to an item is running.
    pub(super) skip_animation: Cell<bool>,
    scope: RefCell<Option<Owned<CoroutineScope>>>,
    display: RefCell<Option<NodeHandle>>,
}

impl Default for ItemAnimator {
    fn default() -> ItemAnimator {
        ItemAnimator {
            any_specs: Cell::new(false),
            displays: mutable_state_of(false),
            info: RefCell::default(),
            items: RefCell::default(),
            previous: RefCell::default(),
            first_visible: Cell::new(0),
            previous_main_size: Cell::new(None),
            disappearing: RefCell::default(),
            kept: RefCell::default(),
            consumed: Cell::new(0),
            skip_animation: Cell::new(false),
            scope: RefCell::default(),
            display: RefCell::default(),
        }
    }
}

/// `modifier` for a list's layout, with what draws its items removed once one of them has
/// `animate_item`: the list's last modifier, so no other node moves for it. As
/// `DisplayingDisappearingItemsElement`, which every list has in Compose; a drawing node costs a list
/// on each frame, so none has it here until it's of use.
pub(super) fn displaying(
    modifier: Modifier,
    inner: &LazyListInner,
    state: super::LazyListState,
) -> Modifier {
    if inner.item_animator.displays.get() {
        modifier.then_custom(DisplayingElement { state })
    } else {
        modifier
    }
}

struct DisplayingElement {
    state: super::LazyListState,
}

struct DisplayingNode {
    state: super::LazyListState,
}

impl ModifierNodeElement for DisplayingElement {
    type Node = DisplayingNode;

    fn create(&self) -> DisplayingNode {
        DisplayingNode { state: self.state }
    }

    fn update(&self, node: &mut DisplayingNode) {
        node.state = self.state;
    }

    fn same(&self, other: &Self) -> bool {
        self.state == other.state
    }

    fn name(&self) -> &'static str {
        "displaying_disappearing_items"
    }
}

impl ModifierNode for DisplayingNode {
    fn as_draw(&mut self) -> Option<&mut dyn compose_ui::DrawModifierNode> {
        Some(self)
    }

    fn on_attach(&mut self, context: &mut compose_ui::NodeContext<'_>) {
        if let Some(inner) = self.state.inner() {
            inner.item_animator.set_display(Some(context.handle()));
        }
    }
}

impl compose_ui::DrawModifierNode for DisplayingNode {
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        if let Some(inner) = self.state.inner() {
            inner.item_animator.draw_disappearing(scope);
        }
    }
}

impl ItemAnimator {
    /// Whether no item has ever had the modifier: what a list without it pays is this branch.
    #[inline]
    pub(super) fn idle(&self) -> bool {
        !self.any_specs.get()
    }

    /// `items` are those of a settled measurement of an idle list, for its first animated item
    /// to tell whether it's new.
    #[inline]
    pub(super) fn note_items(&self, items: &Rc<Items>, first: usize) {
        let mut previous = self.previous.borrow_mut();
        if !previous.as_ref().is_some_and(|it| Rc::ptr_eq(&it.items, items)) {
            *previous = Some(KeyIndex::new(items.clone(), first));
        }
    }

    /// The list's scroll modifier node, to be told what it draws changed.
    pub(super) fn set_display(&self, display: Option<NodeHandle>) {
        *self.display.borrow_mut() = display;
    }

    fn invalidate_display(&self) {
        if let Some(display) = &*self.display.borrow() {
            display.invalidate_draw();
        }
    }

    /// An item's node measured, having the modifier or having had it.
    pub(super) fn register(&self, key: u64, item: Weak<ItemLayout>) {
        if !self.any_specs.replace(true) {
            self.displays.set(true);
        }
        let mut items = self.items.borrow_mut();
        // Those of slots gone, now and then.
        if items.len() >= STALE_ITEMS_CHECK {
            items.retain(|_, item| item.strong_count() > 0);
        }
        items.insert(key, item);
    }

    /// `item`'s node now holds the item of `key`, as composed.
    #[inline]
    pub(super) fn item_composed(&self, item: &ItemLayout, key: u64) {
        let children = &item.children;
        if children.key.replace(key) != key && !children.is_empty() {
            self.register(key, item.this.clone());
        }
    }

    pub(super) fn add_consumed_scroll(&self, consumed: i32) {
        self.consumed.set(self.consumed.get() + consumed);
    }

    /// The items kept composed apart from the range because they're moving out of view.
    #[inline]
    pub(super) fn kept(&self) -> std::cell::Ref<'_, Vec<(u64, usize)>> {
        self.kept.borrow()
    }

    #[inline]
    pub(super) fn has_kept(&self) -> bool {
        !self.kept.borrow().is_empty()
    }

    /// As `reset`: the next offsets of the items aren't animated (a jump to another position).
    pub(super) fn reset(&self) {
        if self.idle() {
            *self.previous.borrow_mut() = None;
            return;
        }
        self.release_animations();
        *self.previous.borrow_mut() = None;
        self.first_visible.set(usize::MAX);
    }

    /// The list has left the composition: nothing goes on.
    pub(super) fn on_list_left(&self) {
        self.release_animations();
        if let Some(scope) = self.scope.borrow_mut().take() {
            scope.cancel();
        }
        self.previous_main_size.set(None);
        *self.previous.borrow_mut() = None;
        *self.display.borrow_mut() = None;
    }

    fn release_animations(&self) {
        let mut info = self.info.borrow_mut();
        for item in info.values_mut() {
            item.release_all();
        }
        info.clear();
        self.disappearing.borrow_mut().clear();
        self.kept.borrow_mut().clear();
    }

    fn coroutine_scope(&self) -> CoroutineScope {
        **self.scope.borrow_mut().get_or_insert_with(CoroutineScope::new)
    }

    fn children_of(&self, key: u64) -> Option<Rc<ItemLayout>> {
        // Its node may hold another item by now, reused.
        let item = self.items.borrow().get(&key).and_then(Weak::upgrade)?;
        (item.children.key() == key).then_some(item)
    }

    fn has_animations(&self, key: u64) -> bool {
        self.children_of(key).is_some_and(|item| item.children.has_animations())
    }

    /// As `minSizeToFitDisappearingItems`.
    pub(super) fn min_size_to_fit_disappearing(&self) -> IntSize {
        let mut size = IntSize::ZERO;
        for animation in self.disappearing.borrow().iter() {
            let target = animation.target_offset.get();
            let own = animation.size.get();
            size = IntSize::new(
                size.width.max(target.x + own.width),
                size.height.max(target.y + own.height),
            );
        }
        size
    }

    /// Draws what the items removed last drew, over the list's content, as
    /// `onDrawDisappearingItems`.
    pub(super) fn draw_disappearing(&self, scope: &mut DrawScope<'_>) {
        for animation in self.disappearing.borrow().iter() {
            let Some(capture) = animation.capture.borrow().clone() else { continue };
            let at = animation.placement_offset.get();
            let delta = animation.placement_delta_state.get();
            let (sign_x, sign_y) = animation.mirror.get();
            let alpha = animation.alpha.get();
            let size = animation.size.get();
            let (x, y) = ((at.x + delta.x * sign_x) as f32, (at.y + delta.y * sign_y) as f32);
            scope.translate(x, y, |scope| {
                let bounds = Rect::from_offset_size(Offset::ZERO, size.into());
                scope.save_layer_alpha(bounds, alpha);
                scope.draw_captured(&capture);
                scope.restore();
            });
        }
    }

    /// Where the item's children went, once the measurement placed it at `item_x`, `item_y` in the
    /// list: an animated one out of view with its target ends here.
    pub(super) fn note_placed(&self, key: u64, item_x: i32, item_y: i32, bounds: (i32, i32, bool)) {
        let info = self.info.borrow();
        let (Some(info), Some(item)) = (info.get(&key), self.children_of(key)) else { return };
        let children = item.children.children.borrow();
        for (animation, child) in info.animations.iter().zip(children.iter()) {
            if let Some(animation) = animation {
                animation
                    .placement_offset
                    .set(IntOffset::new(item_x + child.physical.x, item_y + child.physical.y));
                animation.bounds.set(bounds);
                animation.check_bounds();
            }
        }
    }

    /// As `onMeasured`, in a settled measurement: `positioned` are the items placed, in order;
    /// `measure` gives the size with spacing of a composed item by index, `None` if it isn't
    /// composed. Returns the items moving out of view, in the order to place them: those to the
    /// start first.
    pub(super) fn on_measured(
        &self,
        list: &Rc<LazyListInner>,
        pass: &Pass<'_>,
        positioned: &[Positioned],
        measure: &mut dyn FnMut(usize, &MovedItem) -> Option<i32>,
    ) -> Vec<MovedAway> {
        let consumed = self.consumed.replace(0);
        let first = positioned.first().map_or(0, |item| item.index);
        let current = KeyIndex::new(pass.items.clone(), first);
        let previous = self.previous.replace(None);
        let same_items = previous.as_ref().is_some_and(|it| Rc::ptr_eq(&it.items, pass.items));
        let result = self.step(
            list,
            pass,
            positioned,
            measure,
            consumed,
            &current,
            previous.as_ref(),
            same_items,
        );
        // Their keys now, while they're the data's: the next change looks them up. Only if the
        // list has moved far: a scroll's keys stay what they were.
        let kept_previous = previous.filter(|previous| {
            same_items
                && previous.keys.get().is_some()
                && previous.first.get().abs_diff(first) <= KEY_SLIDE
        });
        *self.previous.borrow_mut() = Some(kept_previous.unwrap_or_else(|| {
            current.keys();
            current
        }));
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn step(
        &self,
        list: &Rc<LazyListInner>,
        pass: &Pass<'_>,
        positioned: &[Positioned],
        measure: &mut dyn FnMut(usize, &MovedItem) -> Option<i32>,
        consumed: i32,
        current: &KeyIndex,
        previous: Option<&KeyIndex>,
        same_items: bool,
    ) -> Vec<MovedAway> {
        let run = !self.skip_animation.get();
        let vertical = pass.vertical;
        let along = |amount: i32| {
            if vertical { IntOffset::new(0, amount) } else { IntOffset::new(amount, 0) }
        };
        // Reversed, offsets count from the end, so resizing the list moves them all: not a move.
        let main_size_change = if pass.reverse {
            pass.main_size - self.previous_main_size.get().unwrap_or(pass.main_size)
        } else {
            0
        };
        self.previous_main_size.set(Some(pass.main_size));
        let size_offset = along(main_size_change);
        let any_animations = positioned.iter().any(|item| self.has_animations(item.key));
        if !any_animations && self.info.borrow().is_empty() {
            self.release_animations();
            return Vec::new();
        }
        let previous_first = self.first_visible.get();
        self.first_visible.set(positioned.first().map_or(0, |item| item.index));
        let scroll_offset = along(consumed);
        let index_before = |key: u64, index: usize| -> Option<usize> {
            let previous = previous?;
            if same_items { Some(index) } else { previous.keys().get(&key).copied() }
        };
        let mirror = pass.mirror;

        let mut moving_away: HashSet<u64> = self.info.borrow().keys().copied().collect();
        let mut from_start: Vec<(usize, usize)> = Vec::new();
        let mut from_end: Vec<(usize, usize)> = Vec::new();
        {
            let mut info_map = self.info.borrow_mut();
            for (position, item) in positioned.iter().enumerate() {
                moving_away.remove(&item.key);
                let Some(children) = self.children_of(item.key) else {
                    if let Some(mut gone) = info_map.remove(&item.key) {
                        gone.release_all();
                    }
                    continue;
                };
                if !children.children.has_animations() {
                    if let Some(mut gone) = info_map.remove(&item.key) {
                        gone.release_all();
                    }
                    continue;
                }
                let before = if same_items {
                    info_map.get(&item.key).map_or(Some(item.index), |info| Some(info.index))
                } else {
                    index_before(item.key, item.index)
                };
                let appears = before.is_none() && previous.is_some() && run;
                match info_map.get_mut(&item.key) {
                    None => {
                        let mut info = ItemInfo::new(item.index);
                        self.update_animation(
                            &mut info,
                            list,
                            &children.children,
                            item,
                            pass,
                            mirror,
                        );
                        if let Some(before) = before.filter(|before| *before != item.index) {
                            if before < previous_first {
                                from_start.push((position, before));
                            } else {
                                from_end.push((position, before));
                            }
                        } else {
                            initialize(&info, &children.children, item, item.main, vertical);
                            if appears {
                                for animation in info.animations.iter().flatten() {
                                    animation.animate_enter_transition();
                                }
                            }
                        }
                        info_map.insert(item.key, info);
                    }
                    Some(info) => {
                        self.update_animation(info, list, &children.children, item, pass, mirror);
                        for animation in info.animations.iter().flatten() {
                            if animation.target_offset.get() != NOT_INITIALIZED {
                                animation.apply_scroll_offset(scroll_offset);
                            }
                        }
                        if appears {
                            for animation in info.animations.iter().flatten() {
                                if animation.is_disappearing() {
                                    self.disappearing
                                        .borrow_mut()
                                        .retain(|other| !Rc::ptr_eq(other, animation));
                                    self.invalidate_display();
                                }
                                animation.animate_enter_transition();
                            }
                        }
                        start_placement(
                            info,
                            &children.children,
                            item_offset(item, vertical),
                            false,
                            size_offset,
                        );
                    }
                }
            }
        }

        let mut lanes = vec![0; pass.lane_count.max(1)];
        if previous.is_some() && run {
            from_start.sort_by_key(|entry| std::cmp::Reverse(entry.1));
            for &(position, _) in &from_start {
                let item = &positioned[position];
                let accumulated = add_to_lanes(&mut lanes, item.lane, item.span, item);
                self.move_in(item, pass.layout_min - accumulated, vertical);
            }
            lanes.fill(0);
            from_end.sort_by_key(|entry| entry.1);
            for &(position, _) in &from_end {
                let item = &positioned[position];
                let accumulated = add_to_lanes(&mut lanes, item.lane, item.span, item);
                let main = pass.layout_max + accumulated - item.main_size_with_spacing;
                self.move_in(item, main, vertical);
            }
            lanes.fill(0);
        }

        // Those in the map no longer placed: removed, or gone out of view.
        let mut to_start: Vec<Waiting> = Vec::new();
        let mut to_end: Vec<Waiting> = Vec::new();
        let mut kept = Vec::new();
        let kept_before = self.kept.borrow().len();
        for key in moving_away {
            let mut info_map = self.info.borrow_mut();
            let Some(info) = info_map.get_mut(&key) else { continue };
            info.span = info.span.min(pass.lane_count.max(1));
            info.lane = info.lane.min(pass.lane_count.max(1) - info.span);
            let new_index = if same_items {
                info.waiting_at.or(Some(info.index)).filter(|index| *index < pass.items.count)
            } else {
                current.keys().get(&key).copied()
            };
            let Some(new_index) = new_index else {
                if self.disappear(info) {
                    continue;
                }
                info.release_all();
                info_map.remove(&key);
                continue;
            };
            let in_progress = info.animations.iter().flatten().any(|animation| {
                animation.placement_read.set(true);
                animation.placement_in_progress.get()
            });
            if !in_progress && new_index == info.index {
                // Scrolled away, or at its new place: nothing goes on for it. Compose measures it
                // once more; not to compose it for that, here it just goes.
                info.release_all();
                info_map.remove(&key);
                continue;
            }
            let moved = MovedItem { lane: info.lane, span: info.span };
            let Some(size) = measure(new_index, &moved) else {
                // Not composed: the next round of this frame, its offsets kept as they were.
                info.waiting_at = Some(new_index);
                kept.push((key, new_index));
                continue;
            };
            info.waiting_at = None;
            // What it's been composed with now, as for those placed.
            if let Some(children) = self.children_of(key) {
                let as_placed = Positioned {
                    index: new_index,
                    key,
                    main: 0,
                    cross: info.cross_offset,
                    main_size_with_spacing: size,
                    lane: info.lane,
                    span: info.span,
                    constraints: info.constraints,
                };
                self.update_animation(info, list, &children.children, &as_placed, pass, mirror);
            }
            let waiting = Waiting { key, index: new_index, size };
            kept.push((key, new_index));
            if new_index < self.first_visible.get() {
                to_start.push(waiting);
            } else {
                to_end.push(waiting);
            }
        }

        let mut result = Vec::with_capacity(to_start.len() + to_end.len());
        to_start.sort_by_key(|waiting| std::cmp::Reverse(waiting.index));
        for waiting in &to_start {
            let main = {
                let info_map = self.info.borrow();
                let info = &info_map[&waiting.key];
                let accumulated =
                    add_sized_to_lanes(&mut lanes, info.lane, info.span, waiting.size);
                info.layout_min - accumulated
            };
            result.push(self.moved_away(waiting, main, true, size_offset, vertical));
        }
        lanes.fill(0);
        to_end.sort_by_key(|waiting| waiting.index);
        for waiting in &to_end {
            let main = {
                let info_map = self.info.borrow();
                let info = &info_map[&waiting.key];
                let accumulated =
                    add_sized_to_lanes(&mut lanes, info.lane, info.span, waiting.size);
                info.layout_max - waiting.size + accumulated
            };
            result.push(self.moved_away(waiting, main, false, size_offset, vertical));
        }
        // The start's in the items' order, before those placed.
        let start_count = to_start.len();
        result[..start_count].reverse();

        // Those no longer kept leave composition; those newly kept come.
        let changed = kept.len() != kept_before
            || kept.iter().zip(self.kept.borrow().iter()).any(|(one, other)| one != other);
        if changed {
            *self.kept.borrow_mut() = kept;
            list.compose_again.update(|count| count.wrapping_add(1));
        }
        for item in positioned {
            if let Some(info) = self.info.borrow_mut().get_mut(&item.key) {
                info.index = item.index;
            }
        }
        // Removed items' indices don't matter; those moved out are at their new ones now.
        for moved in &result {
            if let Some(info) = self.info.borrow_mut().get_mut(&moved.key) {
                info.index = moved.index;
            }
        }
        result
    }

    /// An item removed: whether a fade out holds it.
    fn disappear(&self, info: &mut ItemInfo) -> bool {
        let mut in_progress = false;
        for slot in info.animations.iter_mut() {
            let Some(animation) = slot.clone() else { continue };
            if animation.is_disappearing() {
                in_progress = true;
            } else if animation.is_disappearance_finished() {
                animation.release();
                *slot = None;
                self.disappearing.borrow_mut().retain(|other| !Rc::ptr_eq(other, &animation));
                self.invalidate_display();
            } else {
                animation.animate_exit_transition();
                if animation.is_disappearing() {
                    self.disappearing.borrow_mut().push(animation);
                    self.invalidate_display();
                    in_progress = true;
                } else {
                    animation.release();
                    *slot = None;
                }
            }
        }
        in_progress
    }

    fn move_in(&self, item: &Positioned, main: i32, vertical: bool) {
        let mut info_map = self.info.borrow_mut();
        let Some(info) = info_map.get_mut(&item.key) else { return };
        let Some(children) = self.children_of(item.key) else { return };
        initialize(info, &children.children, item, main, vertical);
        start_placement(
            info,
            &children.children,
            item_offset(item, vertical),
            false,
            IntOffset::ZERO,
        );
    }

    fn moved_away(
        &self,
        waiting: &Waiting,
        main: i32,
        to_start: bool,
        size_offset: IntOffset,
        vertical: bool,
    ) -> MovedAway {
        let mut info_map = self.info.borrow_mut();
        let info = info_map.get_mut(&waiting.key).expect("compose-foundation: an item's info");
        let children = self.children_of(waiting.key);
        let offset = if vertical {
            IntOffset::new(info.cross_offset, main)
        } else {
            IntOffset::new(main, info.cross_offset)
        };
        if let Some(children) = &children {
            start_placement(info, &children.children, offset, true, size_offset);
        }
        MovedAway {
            index: waiting.index,
            key: waiting.key,
            main,
            cross: info.cross_offset,
            to_start,
        }
    }

    fn update_animation(
        &self,
        info: &mut ItemInfo,
        list: &Rc<LazyListInner>,
        children: &ItemChildren,
        item: &Positioned,
        pass: &Pass<'_>,
        mirror: (i32, i32),
    ) {
        let moving_away = info.animations.iter().flatten().any(|it| it.running_moving_away.get());
        if !moving_away {
            info.layout_min = pass.layout_min;
            info.layout_max = pass.layout_max;
        }
        info.constraints = item.constraints;
        info.cross_offset = item.cross;
        info.lane = item.lane;
        info.span = item.span;
        let children = children.children.borrow();
        for extra in info.animations.iter_mut().skip(children.len()) {
            if let Some(extra) = extra.take() {
                extra.release();
            }
        }
        info.animations.resize_with(children.len(), || None);
        for (slot, child) in info.animations.iter_mut().zip(children.iter()) {
            let Some(data) = &child.data else {
                if let Some(gone) = slot.take() {
                    gone.release();
                }
                continue;
            };
            let animation = slot
                .get_or_insert_with(|| {
                    ItemAnimation::new(Rc::downgrade(list), self.coroutine_scope())
                })
                .clone();
            let specs = data.specs.borrow();
            animation.enter.borrow_mut().clone_from(&specs.fade_in);
            animation.exit.borrow_mut().clone_from(&specs.fade_out);
            animation.placement_spec.borrow_mut().clone_from(&specs.placement);
            drop(specs);
            animation.capture.borrow_mut().replace(data.capture.clone());
            animation.size.set(child.size);
            animation.mirror.set(mirror);
            data.link(Some(animation));
        }
    }
}

struct Waiting {
    key: u64,
    index: usize,
    size: i32,
}

/// What the measurement needs to measure an item moved out of view.
pub(super) struct MovedItem {
    pub lane: usize,
    pub span: usize,
}

fn item_offset(item: &Positioned, vertical: bool) -> IntOffset {
    if vertical {
        IntOffset::new(item.cross, item.main)
    } else {
        IntOffset::new(item.main, item.cross)
    }
}

/// The animations' first target, the item at `main`, as `initializeAnimation`.
fn initialize(
    info: &ItemInfo,
    children: &ItemChildren,
    item: &Positioned,
    main: i32,
    vertical: bool,
) {
    let base = item_offset(&Positioned { main, ..copy_positioned(item) }, vertical);
    let children = children.children.borrow();
    for (animation, child) in info.animations.iter().zip(children.iter()) {
        if let Some(animation) = animation {
            animation.target_offset.set(base + child.logical);
        }
    }
}

fn copy_positioned(item: &Positioned) -> Positioned {
    Positioned {
        index: item.index,
        key: item.key,
        main: item.main,
        cross: item.cross,
        main_size_with_spacing: item.main_size_with_spacing,
        lane: item.lane,
        span: item.span,
        constraints: item.constraints,
    }
}

/// The item's now at `base`: a placement starts for each child whose offset changed, the part of
/// the change that's the list's resize left out, as `startPlacementAnimationsIfNeeded`.
fn start_placement(
    info: &ItemInfo,
    children: &ItemChildren,
    base: IntOffset,
    moving_away: bool,
    size_offset: IntOffset,
) {
    let children = children.children.borrow();
    for (animation, child) in info.animations.iter().zip(children.iter()) {
        if let Some(animation) = animation {
            let new_target = base + child.logical;
            let current = animation.target_offset.get();
            if current != NOT_INITIALIZED && current != new_target {
                let delta = new_target - current - size_offset;
                if delta != IntOffset::ZERO {
                    animation.animate_placement_delta(delta, moving_away);
                }
            }
            animation.target_offset.set(new_target);
        }
    }
}

fn add_to_lanes(lanes: &mut [i32], lane: usize, span: usize, item: &Positioned) -> i32 {
    add_sized_to_lanes(lanes, lane, span, item.main_size_with_spacing)
}

fn add_sized_to_lanes(lanes: &mut [i32], lane: usize, span: usize, size: i32) -> i32 {
    let mut most = 0;
    for offset in lanes.iter_mut().skip(lane).take(span) {
        *offset += size;
        most = most.max(*offset);
    }
    most
}
