//! Custom drawing, as Compose's `Modifier.drawBehind` and `Modifier.drawWithContent`: lambdas
//! drawing into the node's draw scope, observed so the state they read draws again.

use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::display_list::DisplayList;
use crate::draw::DrawScope;
use crate::modifier::{DrawModifierNode, Modifier, ModifierNode, ModifierNodeElement};

/// A draw scope that also draws what's inside the modifier (the rest of the node and its
/// children) where `draw_content` is called, as Compose's `ContentDrawScope`.
pub struct ContentDrawScope<'a, 'b> {
    pub(crate) scope: &'b mut DrawScope<'a>,
    pub(crate) content: &'b mut dyn FnMut(&mut DisplayList),
}

impl ContentDrawScope<'_, '_> {
    /// Draws the content here, as `drawContent()`.
    pub fn draw_content(&mut self) {
        (self.content)(self.scope.list);
    }
}

impl<'a> Deref for ContentDrawScope<'a, '_> {
    type Target = DrawScope<'a>;
    fn deref(&self) -> &DrawScope<'a> {
        self.scope
    }
}

impl<'a> DerefMut for ContentDrawScope<'a, '_> {
    fn deref_mut(&mut self) -> &mut DrawScope<'a> {
        self.scope
    }
}

impl Modifier {
    /// Draws with `on_draw` behind the content, as `Modifier.drawBehind`.
    pub fn draw_behind(self, on_draw: impl Fn(&mut DrawScope<'_>) + 'static) -> Modifier {
        self.then_custom(DrawBehindElement(Rc::new(on_draw)))
    }

    /// Draws with `on_draw`, which draws the content where it calls `draw_content` (or not at
    /// all), as `Modifier.drawWithContent`.
    pub fn draw_with_content(
        self,
        on_draw: impl Fn(&mut ContentDrawScope<'_, '_>) + 'static,
    ) -> Modifier {
        self.then_custom(DrawWithContentElement(Rc::new(on_draw)))
    }
}

impl Modifier {
    /// Draws with what `on_build_draw_cache` returns, as `Modifier.drawWithCache`: it runs when
    /// the drawing's size, density or direction changes, or state it read does, and its result
    /// (`on_draw_behind` or `on_draw_with_content`) draws each time, so objects built from the
    /// size (paths, brushes) aren't built again for each draw.
    pub fn draw_with_cache(
        self,
        on_build_draw_cache: impl Fn(&mut CacheDrawScope) -> DrawResult + 'static,
    ) -> Modifier {
        self.then_custom(DrawWithCacheElement(Rc::new(on_build_draw_cache)))
    }
}

/// What `draw_with_cache`'s block works with, as Compose's `CacheDrawScope`.
pub struct CacheDrawScope {
    size: crate::unit::Size,
    density: crate::unit::Density,
    layout_direction: crate::alignment::LayoutDirection,
}

impl CacheDrawScope {
    pub fn size(&self) -> crate::unit::Size {
        self.size
    }

    pub fn density(&self) -> crate::unit::Density {
        self.density
    }

    pub fn layout_direction(&self) -> crate::alignment::LayoutDirection {
        self.layout_direction
    }

    /// Draws with `on_draw` behind the content, as `onDrawBehind`.
    pub fn on_draw_behind(&self, on_draw: impl Fn(&mut DrawScope<'_>) + 'static) -> DrawResult {
        DrawResult::Behind(Rc::new(on_draw))
    }

    /// Draws with `on_draw`, which draws the content where it calls `draw_content`, as
    /// `onDrawWithContent`.
    pub fn on_draw_with_content(
        &self,
        on_draw: impl Fn(&mut ContentDrawScope<'_, '_>) + 'static,
    ) -> DrawResult {
        DrawResult::WithContent(Rc::new(on_draw))
    }
}

/// What `draw_with_cache`'s block returns: how to draw, as Compose's `DrawResult`.
#[derive(Clone)]
pub enum DrawResult {
    Behind(DrawBehind),
    WithContent(DrawWithContent),
}

type BuildDrawCache = Rc<dyn Fn(&mut CacheDrawScope) -> DrawResult>;

struct DrawWithCacheElement(BuildDrawCache);

/// The cache: the result of the block for a size, density and direction, and whether state
/// the block read changed since (its own observer hears it, and bumps `version`, read while
/// drawing, so the node draws again).
struct DrawWithCacheNode {
    build: BuildDrawCache,
    cached: Option<(CacheKey, DrawResult)>,
    stale: Rc<std::cell::Cell<bool>>,
    version: compose_runtime::OwnedState<u64>,
    observer: Option<compose_runtime::platform::ReadObserver>,
}

type CacheKey = (crate::unit::Size, crate::unit::Density, crate::alignment::LayoutDirection);

impl ModifierNodeElement for DrawWithCacheElement {
    type Node = DrawWithCacheNode;
    fn create(&self) -> DrawWithCacheNode {
        DrawWithCacheNode {
            build: self.0.clone(),
            cached: None,
            stale: Rc::default(),
            version: compose_runtime::mutable_state_of(0),
            observer: None,
        }
    }
    fn update(&self, node: &mut DrawWithCacheNode) {
        node.build = self.0.clone();
        node.cached = None;
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&other.0, &self.0)
    }
    fn name(&self) -> &'static str {
        "draw_with_cache"
    }
}

impl ModifierNode for DrawWithCacheNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_release(&mut self, _context: &mut crate::modifier::ReleaseContext<'_>) {
        self.observer = None;
        self.cached = None;
    }
}

impl DrawWithCacheNode {
    /// The result for `key`, built again if the key or what the block read changed.
    fn result(&mut self, key: CacheKey) -> DrawResult {
        // Read while drawing: a write the block's observer heard draws the node again.
        self.version.get();
        if let Some((cached_key, result)) = &self.cached
            && *cached_key == key
            && !self.stale.get()
        {
            return result.clone();
        }
        let observer = self.observer.get_or_insert_with(|| {
            let (stale, version) = (self.stale.clone(), self.version.handle());
            compose_runtime::platform::ReadObserver::new(move || {
                stale.set(true);
                version.update(|version| version.wrapping_add(1));
            })
        });
        self.stale.set(false);
        let mut scope = CacheDrawScope { size: key.0, density: key.1, layout_direction: key.2 };
        let build = self.build.clone();
        let result = observer.observe(0, || build(&mut scope));
        observer.drain_invalidated(|_| {});
        self.cached = Some((key, result.clone()));
        result
    }
}

impl DrawModifierNode for DrawWithCacheNode {
    fn draws_content(&self) -> bool {
        true
    }
    fn draw_with_content(&mut self, scope: &mut ContentDrawScope<'_, '_>) {
        let key = (scope.size(), scope.density(), scope.layout_direction());
        match self.result(key) {
            DrawResult::Behind(on_draw) => {
                on_draw(scope);
                scope.draw_content();
            }
            DrawResult::WithContent(on_draw) => on_draw(scope),
        }
    }
}

/// `background`'s element and node in one, for a gradient brush.
#[derive(Clone, PartialEq)]
pub(crate) struct BackgroundBrushElement {
    pub(crate) brush: crate::brush::Brush,
    pub(crate) shape: crate::graphics::Shape,
    pub(crate) alpha: f32,
}

impl ModifierNodeElement for BackgroundBrushElement {
    type Node = Self;
    fn create(&self) -> Self {
        self.clone()
    }
    fn update(&self, node: &mut Self) {
        *node = self.clone();
    }
    fn same(&self, other: &Self) -> bool {
        other == self
    }
    fn name(&self) -> &'static str {
        "background"
    }
}

impl ModifierNode for BackgroundBrushElement {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for BackgroundBrushElement {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let paint = scope.brush_paint(&self.brush, self.alpha, crate::graphics::PaintStyle::Fill);
        let outline =
            self.shape.outline_in(scope.size(), &scope.density(), scope.layout_direction());
        scope.draw_outline(outline, paint);
    }
}

type DrawBehind = Rc<dyn Fn(&mut DrawScope<'_>)>;
type DrawWithContent = Rc<dyn Fn(&mut ContentDrawScope<'_, '_>)>;

struct DrawBehindElement(DrawBehind);

struct DrawBehindNode(DrawBehind);

impl ModifierNodeElement for DrawBehindElement {
    type Node = DrawBehindNode;
    fn create(&self) -> DrawBehindNode {
        DrawBehindNode(self.0.clone())
    }
    fn update(&self, node: &mut DrawBehindNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&other.0, &self.0)
    }
    fn name(&self) -> &'static str {
        "draw_behind"
    }
}

impl ModifierNode for DrawBehindNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for DrawBehindNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        (self.0)(scope);
    }
}

struct DrawWithContentElement(DrawWithContent);

struct DrawWithContentNode(DrawWithContent);

impl ModifierNodeElement for DrawWithContentElement {
    type Node = DrawWithContentNode;
    fn create(&self) -> DrawWithContentNode {
        DrawWithContentNode(self.0.clone())
    }
    fn update(&self, node: &mut DrawWithContentNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&other.0, &self.0)
    }
    fn name(&self) -> &'static str {
        "draw_with_content"
    }
}

impl ModifierNode for DrawWithContentNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for DrawWithContentNode {
    fn draws_content(&self) -> bool {
        true
    }
    fn draw_with_content(&mut self, scope: &mut ContentDrawScope<'_, '_>) {
        (self.0)(scope);
    }
}
