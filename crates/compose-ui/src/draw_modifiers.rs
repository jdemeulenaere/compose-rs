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

type DrawBehind = Rc<dyn Fn(&mut DrawScope<'_>)>;
type DrawWithContent = Rc<dyn Fn(&mut ContentDrawScope<'_, '_>)>;

struct DrawBehindElement(DrawBehind);

struct DrawBehindNode(DrawBehind);

impl ModifierNodeElement for DrawBehindElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(DrawBehindNode(self.0.clone()))
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<DrawBehindNode>() {
            node.0 = self.0.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<DrawBehindElement>().is_some_and(|other| Rc::ptr_eq(&other.0, &self.0))
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
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(DrawWithContentNode(self.0.clone()))
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<DrawWithContentNode>() {
            node.0 = self.0.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other
            .downcast_ref::<DrawWithContentElement>()
            .is_some_and(|other| Rc::ptr_eq(&other.0, &self.0))
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
