//! Custom parent data, as Compose's `ParentDataModifierNode`, and `Modifier.layout_id`
//! (docs/parity/design/modifier-nodes.md §3.10, step MN-10). The built-in `weight` and `align`
//! keep their fixed `ParentData`, which `Row`, `Column` and `Box` read without boxing.

use std::any::Any;
use std::fmt::Debug;
use std::rc::Rc;

use crate::delegation::dispatch_for_kind;
use crate::layout::{IntrinsicMeasureScope, Measurable, Placeable};
use crate::modifier::{InspectorInfo, Modifier, ModifierNode, ModifierNodeElement};
use crate::node_chain::{ChainEntry, EntryId, NodeKinds};
use crate::tree::{NodeId, UiTree};
use crate::unit::Density;

/// A node giving data to its layout node's parent, as Compose's `ParentDataModifierNode`: the
/// parent's measure policy reads it with `IntrinsicMeasureScope::parent_data_any`. Each node of
/// a chain modifies what the nodes after it gave, from the tail to the head, so the outermost
/// has the last word. An update of the node by its element tells the parent by itself; a node
/// changing its data otherwise says so with `NodeContext::invalidate_parent_data`.
pub trait ParentDataModifierNode {
    /// The parent data, given `parent_data`, what the nodes after it in the chain gave, as
    /// `Density.modifyParentData(parentData)`.
    fn modify_parent_data(
        &self,
        density: Density,
        parent_data: Option<Rc<dyn Any>>,
    ) -> Option<Rc<dyn Any>>;
}

/// The parent data of `Modifier.layout_id`, as Compose's `LayoutIdParentData`: what
/// `IntrinsicMeasureScope::layout_id` reads. Compose's is an interface any parent data may
/// implement; Rust can't ask an `Any` for a trait it implements, so here it's a type of its own.
#[derive(Clone)]
pub struct LayoutIdParentData {
    pub layout_id: Rc<dyn Any>,
}

impl Debug for LayoutIdParentData {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("LayoutIdParentData").finish_non_exhaustive()
    }
}

impl Modifier {
    /// Tags the layout node with `layout_id`, which its parent's policy finds it by
    /// (`IntrinsicMeasureScope::layout_id`), as `Modifier.layoutId`.
    pub fn layout_id<T: PartialEq + Debug + 'static>(self, layout_id: T) -> Modifier {
        self.then_custom(LayoutIdElement(Rc::new(layout_id)))
    }
}

struct LayoutIdElement<T>(Rc<T>);

/// As Compose's `LayoutIdModifier`, whose parent data is itself: here it keeps its data, the
/// same `Rc` while its id holds.
struct LayoutIdNode<T> {
    id: Rc<T>,
    data: Rc<dyn Any>,
}

fn layout_id_data<T: 'static>(id: &Rc<T>) -> Rc<dyn Any> {
    let layout_id: Rc<dyn Any> = id.clone();
    Rc::new(LayoutIdParentData { layout_id })
}

impl<T: PartialEq + Debug + 'static> ModifierNodeElement for LayoutIdElement<T> {
    type Node = LayoutIdNode<T>;
    fn create(&self) -> LayoutIdNode<T> {
        LayoutIdNode { id: self.0.clone(), data: layout_id_data(&self.0) }
    }
    fn update(&self, node: &mut LayoutIdNode<T>) {
        node.id = self.0.clone();
        node.data = layout_id_data(&self.0);
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn name(&self) -> &'static str {
        "layoutId"
    }
    fn inspectable_properties(&self, info: &mut InspectorInfo) {
        info.name = Some("layoutId");
        info.value = Some(format!("{:?}", self.0));
    }
}

impl<T: 'static> ModifierNode for LayoutIdNode<T> {
    fn as_parent_data(&self) -> Option<&dyn ParentDataModifierNode> {
        Some(self)
    }
}

impl<T: 'static> ParentDataModifierNode for LayoutIdNode<T> {
    fn modify_parent_data(&self, _: Density, _: Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>> {
        Some(self.data.clone())
    }
}

impl From<Placeable> for Measurable {
    fn from(placeable: Placeable) -> Measurable {
        placeable.measurable
    }
}

impl From<&Placeable> for Measurable {
    fn from(placeable: &Placeable) -> Measurable {
        placeable.measurable
    }
}

impl IntrinsicMeasureScope<'_> {
    /// The data `measurable`'s modifier nodes give its parent (`ParentDataModifierNode`), as
    /// `IntrinsicMeasurable.parentData` (and `Placeable.parentData`, given its placeable): `None`
    /// when they give none. A layout modifier's content has its layout node's, as each of
    /// Compose's coordinators folds the whole chain. The built-in `weight` and `align` are
    /// `parent_data`.
    pub fn parent_data_any(&self, measurable: impl Into<Measurable>) -> Option<Rc<dyn Any>> {
        self.tree.get(measurable.into().node)?.custom_parent_data.clone()
    }

    /// The id `measurable`'s `Modifier.layout_id` gives it, as `Measurable.layoutId`: the
    /// `layout_id` of a `LayoutIdParentData` parent data.
    pub fn layout_id(&self, measurable: impl Into<Measurable>) -> Option<Rc<dyn Any>> {
        let data = self.parent_data_any(measurable)?;
        let data = (*data).downcast_ref::<LayoutIdParentData>()?;
        Some(data.layout_id.clone())
    }
}

/// `chain`'s parent data folded from the tail to the head, as `NodeCoordinator.parentData`: each
/// node's, or each of its delegates' that gives some.
fn fold_parent_data(chain: &mut [ChainEntry], density: Density) -> Option<Rc<dyn Any>> {
    let mut data = None;
    for entry in chain.iter_mut().rev() {
        if !entry.kinds.any(NodeKinds::PARENT_DATA) {
            continue;
        }
        dispatch_for_kind(
            &mut *entry.node,
            entry.own,
            NodeKinds::PARENT_DATA,
            EntryId::NO_DELEGATE,
            &mut |node, _| {
                if let Some(node) = node.as_parent_data() {
                    data = node.modify_parent_data(density, data.take());
                }
            },
        );
    }
    data
}

impl UiTree {
    /// Folds `node`'s custom parent data again. Returns whether its parent measures again: it
    /// had or has some, as Compose's `updateParentData`, which compares nothing.
    pub(crate) fn refresh_custom_parent_data(&mut self, node: NodeId) -> bool {
        let density = self.density;
        let Some(value) = self.get_mut(node) else { return false };
        if !value.kinds.any(NodeKinds::PARENT_DATA) {
            return value.custom_parent_data.take().is_some();
        }
        let data = fold_parent_data(&mut value.chain, density);
        let had = std::mem::replace(&mut value.custom_parent_data, data).is_some();
        had || value.custom_parent_data.is_some()
    }
}
