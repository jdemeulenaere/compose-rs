//! `Modifier.parentSemantics` and `Modifier.childSemantics`, as Compose's
//! `internal/ChildParentSemantics.kt`: the semantics of something around a clickable, which go on
//! the clickable when there's one inside it, since a clickable merges its own descendants only.

use std::cell::Cell;
use std::rc::Rc;

use compose_ui::{
    ModifierNode, ModifierNodeElement, NodeContext, NodeHandle, SemanticsConfiguration,
    SemanticsModifierNode, TraversableNode, TraverseKey,
};

pub(crate) type Properties = Rc<dyn Fn(&mut SemanticsConfiguration)>;

static PARENT_SEMANTICS: TraverseKey = TraverseKey::new("ParentSemanticsNodeKey");

/// Applies `.0` here, merging what's inside, unless a `ChildSemantics` inside has them.
pub(crate) struct ParentSemantics(pub(crate) Properties);

pub(crate) struct ParentSemanticsNode {
    properties: Properties,
    /// The children that have them.
    consumed: Rc<Cell<u32>>,
    handle: Option<NodeHandle>,
}

impl ModifierNodeElement for ParentSemantics {
    type Node = ParentSemanticsNode;
    fn create(&self) -> ParentSemanticsNode {
        ParentSemanticsNode { properties: self.0.clone(), consumed: Rc::default(), handle: None }
    }
    fn update(&self, node: &mut ParentSemanticsNode) {
        node.properties = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    fn name(&self) -> &'static str {
        "parentSemantics"
    }
}

impl ModifierNode for ParentSemanticsNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn as_traversable(&mut self) -> Option<&mut dyn TraversableNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.handle = Some(context.handle());
    }
}

impl TraversableNode for ParentSemanticsNode {
    fn traverse_key(&self) -> &'static TraverseKey {
        &PARENT_SEMANTICS
    }
}

impl SemanticsModifierNode for ParentSemanticsNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        if self.consumed.get() == 0 {
            (self.properties)(config);
        }
    }
    fn should_merge_descendant_semantics(&self) -> bool {
        true
    }
}

/// On a clickable: applies the semantics of the `ParentSemantics` around, if any.
pub(crate) struct ChildSemantics;

/// What the child took from the nearest parent, to read and to give back.
struct Taken {
    properties: Properties,
    consumed: Rc<Cell<u32>>,
    handle: Option<NodeHandle>,
}

#[derive(Default)]
pub(crate) struct ChildSemanticsNode {
    taken: Vec<Taken>,
}

impl ModifierNodeElement for ChildSemantics {
    type Node = ChildSemanticsNode;
    fn create(&self) -> ChildSemanticsNode {
        ChildSemanticsNode::default()
    }
    fn update(&self, _: &mut ChildSemanticsNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "childSemantics"
    }
}

impl ModifierNode for ChildSemanticsNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        context.traverse_ancestors_with_key(&PARENT_SEMANTICS, |node, _| {
            if let Some(parent) = node.downcast_ref::<ParentSemanticsNode>() {
                parent.consumed.set(parent.consumed.get() + 1);
                if let Some(handle) = &parent.handle {
                    handle.invalidate_semantics();
                }
                self.taken.push(Taken {
                    properties: parent.properties.clone(),
                    consumed: parent.consumed.clone(),
                    handle: parent.handle,
                });
            }
            false
        });
    }
    fn on_detach(&mut self, _: &mut NodeContext<'_>) {
        for taken in self.taken.drain(..) {
            taken.consumed.set(taken.consumed.get() - 1);
            if let Some(handle) = &taken.handle {
                handle.invalidate_semantics();
            }
        }
    }
}

impl SemanticsModifierNode for ChildSemanticsNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        for taken in &self.taken {
            (taken.properties)(config);
        }
    }
}
