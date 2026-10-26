//! Selectors: how an interaction finds its nodes in the tree, as Compose's `SemanticsSelector`
//! chains (a matcher, then steps: an index, the parent, the children, the siblings…).

use std::rc::Rc;

use crate::matcher::{Matcher, NodeRef, TreeView};

/// A step from one node to others, as Compose's `addSelectionFromSingleNode` selectors.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Relative {
    Parent,
    Children,
    Child,
    Siblings,
    Sibling,
    Ancestors,
    Descendants,
}

impl Relative {
    fn name(self) -> &'static str {
        match self {
            Relative::Parent => "parent",
            Relative::Children => "children",
            Relative::Child => "child",
            Relative::Siblings => "siblings",
            Relative::Sibling => "sibling",
            Relative::Ancestors => "ancestors",
            Relative::Descendants => "descendants",
        }
    }

    fn select<'a>(self, node: NodeRef<'a>) -> Vec<NodeRef<'a>> {
        match self {
            Relative::Parent => node.parent().into_iter().collect(),
            Relative::Children | Relative::Child => node.children().collect(),
            Relative::Siblings | Relative::Sibling => node.siblings().collect(),
            Relative::Ancestors => node.ancestors().collect(),
            Relative::Descendants => node.descendants().collect(),
        }
    }
}

#[derive(Clone)]
enum Step {
    /// The nodes of the tree matching.
    Matching(Matcher),
    /// From the one node selected before.
    Relative(Relative),
    /// The node at an index of those selected before.
    Index(usize),
    /// The last of those selected before.
    Last,
    /// Those selected before that match.
    Filter(Matcher),
}

/// What a selector picked, with the reason it picked nothing when it has one (an index past
/// the nodes), as `SelectionResult`.
pub(crate) struct Selection<'a> {
    pub(crate) nodes: Vec<NodeRef<'a>>,
    pub(crate) custom_error: Option<String>,
}

/// A chain of steps finding nodes in a tree, with its description.
#[derive(Clone)]
pub(crate) struct Selector {
    pub(crate) description: String,
    before: Option<Rc<Selector>>,
    step: Step,
}

impl Selector {
    pub(crate) fn matching(matcher: Matcher) -> Selector {
        Selector {
            description: matcher.description.clone(),
            before: None,
            step: Step::Matching(matcher),
        }
    }

    fn then(&self, description: String, step: Step) -> Selector {
        Selector { description, before: Some(Rc::new(self.clone())), step }
    }

    pub(crate) fn relative(&self, relative: Relative) -> Selector {
        self.then(format!("({}).{}", self.description, relative.name()), Step::Relative(relative))
    }

    pub(crate) fn index(&self, index: usize) -> Selector {
        self.then(format!("({})[{index}]", self.description), Step::Index(index))
    }

    pub(crate) fn last(&self) -> Selector {
        self.then(format!("({}).last", self.description), Step::Last)
    }

    pub(crate) fn filter(&self, name: &'static str, matcher: Matcher) -> Selector {
        let description = format!("({}).{name}({})", self.description, matcher.description);
        self.then(description, Step::Filter(matcher))
    }

    /// The nodes this selects in `tree`; an error when a step from one node finds not exactly
    /// one before it, as Compose's.
    pub(crate) fn select<'a>(&self, tree: &'a TreeView<'a>) -> Result<Selection<'a>, String> {
        let before = match &self.before {
            Some(before) => Some(before.select(tree)?),
            None => None,
        };
        let nodes = match &self.step {
            Step::Matching(matcher) => {
                tree.all().filter(|node| matcher.matches_node(*node)).collect()
            }
            Step::Relative(relative) => {
                let before = before.expect("a step after another");
                if before.nodes.len() != 1 {
                    return Err(before.custom_error.unwrap_or_else(|| {
                        format!(
                            "expected exactly 1 node for ({}), found {}",
                            self.before.as_ref().map_or("", |b| b.description.as_str()),
                            before.nodes.len()
                        )
                    }));
                }
                relative.select(before.nodes[0])
            }
            Step::Index(index) => {
                let before = before.expect("a step after another");
                return Ok(match before.nodes.get(*index) {
                    Some(node) => Selection { nodes: vec![*node], custom_error: None },
                    None => Selection {
                        nodes: Vec::new(),
                        custom_error: Some(format!(
                            "can't retrieve node at index '{index}' of '{}': {}",
                            self.before.as_ref().map_or("", |b| b.description.as_str()),
                            if before.nodes.is_empty() {
                                "there are no existing nodes for that selector".to_string()
                            } else {
                                format!("there are only {} nodes", before.nodes.len())
                            }
                        )),
                    },
                });
            }
            Step::Last => {
                let before = before.expect("a step after another");
                before.nodes.last().copied().into_iter().collect()
            }
            Step::Filter(matcher) => {
                let before = before.expect("a step after another");
                before.nodes.into_iter().filter(|node| matcher.matches_node(*node)).collect()
            }
        };
        Ok(Selection { nodes, custom_error: None })
    }
}
