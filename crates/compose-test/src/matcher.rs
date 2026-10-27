//! Semantics matchers, as Compose's `SemanticsMatcher` and the `has…`/`is…` filters of its
//! `Filters.kt`: a test of a node in its tree (its configuration, its bounds, its parent,
//! children and siblings), with a description for failure messages.

use std::fmt;
use std::rc::Rc;

use compose_ui::platform::{NodeId, SemanticsNode};
use compose_ui::{
    ImeAction, ProgressBarRangeInfo, Rect, Role, SemanticsActions, SemanticsConfiguration,
    SemanticsProperties, SemanticsPropertyKey, SemanticsValue, SharedString, ToggleableState,
};

/// A semantics tree flattened in tree order (depth-first, a node before its children), with
/// each node's parent and children: what finders and matchers walk.
pub(crate) struct TreeView<'a> {
    nodes: Vec<&'a SemanticsNode>,
    parents: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
}

impl<'a> TreeView<'a> {
    pub(crate) fn new(root: &'a SemanticsNode) -> TreeView<'a> {
        let mut view = TreeView { nodes: Vec::new(), parents: Vec::new(), children: Vec::new() };
        view.add(root, None);
        view
    }

    fn add(&mut self, node: &'a SemanticsNode, parent: Option<usize>) {
        let index = self.nodes.len();
        self.nodes.push(node);
        self.parents.push(parent);
        self.children.push(Vec::new());
        if let Some(parent) = parent {
            self.children[parent].push(index);
        }
        for child in &node.children {
            self.add(child, Some(index));
        }
    }

    pub(crate) fn node(&self, index: usize) -> NodeRef<'_> {
        NodeRef { tree: self, index }
    }

    /// Every node, in tree order.
    pub(crate) fn all(&self) -> impl Iterator<Item = NodeRef<'_>> {
        (0..self.nodes.len()).map(|index| self.node(index))
    }
}

/// A semantics node in its tree, as Compose's `SemanticsNode` seen by a matcher: its
/// properties, and its parent, children and siblings in the same tree (merged or not).
#[derive(Clone, Copy)]
pub struct NodeRef<'a> {
    tree: &'a TreeView<'a>,
    index: usize,
}

impl<'a> NodeRef<'a> {
    /// The node (with its subtree).
    pub fn node(&self) -> &'a SemanticsNode {
        self.tree.nodes[self.index]
    }

    pub fn id(&self) -> NodeId {
        self.node().id
    }

    /// Its configuration: merged in the merged tree.
    pub fn config(&self) -> &'a SemanticsConfiguration {
        &self.node().config
    }

    /// Its bounds in the root, in pixels, unclipped.
    pub fn bounds(&self) -> Rect {
        self.node().bounds
    }

    /// Whether it's the tree's root, as `SemanticsNode.isRoot`.
    pub fn is_root(&self) -> bool {
        self.tree.parents[self.index].is_none()
    }

    pub fn parent(&self) -> Option<NodeRef<'a>> {
        self.tree.parents[self.index].map(|index| NodeRef { tree: self.tree, index })
    }

    pub fn children(&self) -> impl Iterator<Item = NodeRef<'a>> + 'a {
        let tree = self.tree;
        tree.children[self.index].iter().map(move |&index| NodeRef { tree, index })
    }

    /// Its parent's other children, in order.
    pub fn siblings(&self) -> impl Iterator<Item = NodeRef<'a>> + 'a {
        let tree = self.tree;
        let me = self.index;
        let siblings: &'a [usize] = match tree.parents[me] {
            Some(parent) => &tree.children[parent],
            None => &[],
        };
        siblings
            .iter()
            .filter(move |&&index| index != me)
            .map(move |&index| NodeRef { tree, index })
    }

    /// Its parent, then the parent's parent, up to the root.
    pub fn ancestors(&self) -> impl Iterator<Item = NodeRef<'a>> + 'a {
        let tree = self.tree;
        std::iter::successors(self.parent(), move |node| {
            tree.parents[node.index].map(|index| NodeRef { tree, index })
        })
    }

    /// Its descendants, depth-first in order.
    pub fn descendants(&self) -> impl Iterator<Item = NodeRef<'a>> + 'a {
        let tree = self.tree;
        let mut stack: Vec<usize> = tree.children[self.index].iter().rev().copied().collect();
        std::iter::from_fn(move || {
            let index = stack.pop()?;
            stack.extend(tree.children[index].iter().rev().copied());
            Some(NodeRef { tree, index })
        })
    }
}

/// A test of a semantics node, with a description for failure messages, as Compose's
/// `SemanticsMatcher`.
#[derive(Clone)]
pub struct Matcher {
    pub description: String,
    test: Rc<dyn Fn(NodeRef<'_>) -> bool>,
}

impl Matcher {
    /// A matcher of a node's configuration alone.
    pub fn new(
        description: impl Into<String>,
        test: impl Fn(&SemanticsConfiguration) -> bool + 'static,
    ) -> Matcher {
        Matcher { description: description.into(), test: Rc::new(move |node| test(node.config())) }
    }

    /// A matcher of a node in its tree, as `SemanticsMatcher(description) { node -> … }`.
    pub fn with_node(
        description: impl Into<String>,
        test: impl Fn(NodeRef<'_>) -> bool + 'static,
    ) -> Matcher {
        Matcher { description: description.into(), test: Rc::new(test) }
    }

    /// Whether `node` matches, as `matches(node)`.
    pub fn matches_node(&self, node: NodeRef<'_>) -> bool {
        (self.test)(node)
    }

    /// Whether any of `nodes` matches, as `matchesAny`.
    pub fn matches_any<'a>(&self, nodes: impl IntoIterator<Item = NodeRef<'a>>) -> bool {
        nodes.into_iter().any(|node| self.matches_node(node))
    }

    /// Both, as `and`.
    pub fn and(self, other: Matcher) -> Matcher {
        let description = format!("({}) && ({})", self.description, other.description);
        Matcher::with_node(description, move |node| {
            self.matches_node(node) && other.matches_node(node)
        })
    }

    /// Either, as `or`.
    pub fn or(self, other: Matcher) -> Matcher {
        let description = format!("({}) || ({})", self.description, other.description);
        Matcher::with_node(description, move |node| {
            self.matches_node(node) || other.matches_node(node)
        })
    }
}

/// Neither, as `not`: `!has_text("a")`.
impl std::ops::Not for Matcher {
    type Output = Matcher;

    fn not(self) -> Matcher {
        let description = format!("NOT ({})", self.description);
        Matcher::with_node(description, move |node| !self.matches_node(node))
    }
}

/// Both, as Kotlin's infix `and`: `has_text("a") & is_enabled()`.
impl std::ops::BitAnd for Matcher {
    type Output = Matcher;

    fn bitand(self, other: Matcher) -> Matcher {
        self.and(other)
    }
}

/// Either, as Kotlin's infix `or`: `has_text("a") | has_text("b")`.
impl std::ops::BitOr for Matcher {
    type Output = Matcher;

    fn bitor(self, other: Matcher) -> Matcher {
        self.or(other)
    }
}

impl fmt::Debug for Matcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.description)
    }
}

fn contains(candidate: &str, text: &str, ignore_case: bool) -> bool {
    if ignore_case {
        candidate.to_lowercase().contains(&text.to_lowercase())
    } else {
        candidate.contains(text)
    }
}

fn equals(candidate: &str, text: &str, ignore_case: bool) -> bool {
    if ignore_case { candidate.to_lowercase() == text.to_lowercase() } else { candidate == text }
}

// ---- Keys --------------------------------------------------------------------------------------

/// A node whose `key` is `expected`, as `SemanticsMatcher.expectValue`: a key it doesn't set
/// doesn't match.
pub fn expect_value<T: SemanticsValue>(
    key: &'static SemanticsPropertyKey<T>,
    expected: T,
) -> Matcher {
    let description = format!("{} = '{}'", key.name(), expected.clone().into_slot().display());
    Matcher::new(description, move |config| config.get(key) == Some(&expected))
}

/// A node that sets `key`, as `SemanticsMatcher.keyIsDefined`.
pub fn key_is_defined<T: SemanticsValue>(key: &'static SemanticsPropertyKey<T>) -> Matcher {
    Matcher::new(format!("{} is defined", key.name()), move |config| config.contains(key))
}

/// A node that doesn't set `key`, as `SemanticsMatcher.keyNotDefined`.
pub fn key_not_defined<T: SemanticsValue>(key: &'static SemanticsPropertyKey<T>) -> Matcher {
    Matcher::new(format!("{} is NOT defined", key.name()), move |config| !config.contains(key))
}

// ---- Text and descriptions -----------------------------------------------------------------

/// The texts a text matcher reads, as Compose's `hasText`: the input text, the editable text,
/// then the texts.
fn texts(config: &SemanticsConfiguration) -> impl Iterator<Item = &str> {
    config
        .get(&SemanticsProperties::INPUT_TEXT)
        .into_iter()
        .chain(config.get(&SemanticsProperties::EDITABLE_TEXT))
        .chain(config.get(&SemanticsProperties::TEXT).into_iter().flatten())
        .map(|text| text.as_str())
}

/// A node with exactly `text` among its texts (or its editable text), as `hasText`.
pub fn has_text(text: &str) -> Matcher {
    has_text_with(text, false, false)
}

/// A node with a text containing `text`, as `hasText(text, substring = true)`.
pub fn has_text_containing(text: &str) -> Matcher {
    has_text_with(text, true, false)
}

/// As `hasText(text, substring, ignoreCase)`: one of the node's texts, its editable text or
/// its input text is `text` (or contains it, with `substring`), ignoring case with
/// `ignore_case`.
pub fn has_text_with(text: &str, substring: bool, ignore_case: bool) -> Matcher {
    let text = text.to_string();
    let description = match (substring, ignore_case) {
        (false, false) => format!("with text {text:?}"),
        (true, false) => format!("with text containing {text:?}"),
        (false, true) => format!("with text {text:?} (ignoring case)"),
        (true, true) => format!("with text containing {text:?} (ignoring case)"),
    };
    Matcher::new(description, move |config| {
        texts(config).any(|candidate| {
            if substring {
                contains(candidate, &text, ignore_case)
            } else {
                equals(candidate, &text, ignore_case)
            }
        })
    })
}

/// A node whose texts are exactly `texts`, compared as sets (each holds the other's), as
/// `hasTextExactly(…, includeEditableText)`: its editable text counts with
/// `include_editable_text`.
pub fn has_text_exactly(texts: &[&str], include_editable_text: bool) -> Matcher {
    let expected: Vec<String> = texts.iter().map(|text| text.to_string()).collect();
    Matcher::new(format!("with texts exactly {expected:?}"), move |config| {
        let actual: Vec<&str> = if include_editable_text {
            config.all_text().collect()
        } else {
            config
                .get(&SemanticsProperties::TEXT)
                .into_iter()
                .flatten()
                .map(|text| text.as_str())
                .collect()
        };
        same_set(&actual, &expected)
    })
}

/// Whether each list holds every item of the other, as Kotlin's `containsAll` both ways.
pub(crate) fn same_set(actual: &[&str], expected: &[String]) -> bool {
    actual.iter().all(|item| expected.iter().any(|candidate| candidate == item))
        && expected.iter().all(|item| actual.contains(&item.as_str()))
}

fn content_descriptions(config: &SemanticsConfiguration) -> impl Iterator<Item = &str> {
    config
        .get(&SemanticsProperties::CONTENT_DESCRIPTION)
        .into_iter()
        .flatten()
        .map(|text| text.as_str())
}

/// A node with exactly `description` among its content descriptions, as
/// `hasContentDescription`.
pub fn has_content_description(description: &str) -> Matcher {
    has_content_description_with(description, false, false)
}

/// As `hasContentDescription(value, substring, ignoreCase)`.
pub fn has_content_description_with(
    description: &str,
    substring: bool,
    ignore_case: bool,
) -> Matcher {
    let description = description.to_string();
    let text = if substring {
        format!("with content description containing {description:?}")
    } else {
        format!("with content description {description:?}")
    };
    Matcher::new(text, move |config| {
        content_descriptions(config).any(|candidate| {
            if substring {
                contains(candidate, &description, ignore_case)
            } else {
                equals(candidate, &description, ignore_case)
            }
        })
    })
}

/// A node whose content descriptions are exactly `descriptions`, as sets, as
/// `hasContentDescriptionExactly`.
pub fn has_content_description_exactly(descriptions: &[&str]) -> Matcher {
    let expected: Vec<String> = descriptions.iter().map(|text| text.to_string()).collect();
    Matcher::new(format!("with content descriptions exactly {expected:?}"), move |config| {
        let actual: Vec<&str> = content_descriptions(config).collect();
        same_set(&actual, &expected)
    })
}

/// As `hasStateDescription`.
pub fn has_state_description(description: &str) -> Matcher {
    let description = SharedString::from(description.to_string());
    Matcher::new(format!("with state description {:?}", description.as_str()), move |config| {
        config.get(&SemanticsProperties::STATE_DESCRIPTION) == Some(&description)
    })
}

/// As `hasTestTag`.
pub fn has_test_tag(tag: &str) -> Matcher {
    let tag = SharedString::from(tag.to_string());
    Matcher::new(format!("with tag {:?}", tag.as_str()), move |config| {
        config.get(&SemanticsProperties::TEST_TAG) == Some(&tag)
    })
}

/// A node of `role` (compose-rs's: Compose's tests use `expectValue(Role, …)`).
pub fn has_role(role: Role) -> Matcher {
    Matcher::new(format!("with role {role:?}"), move |config| {
        config.get(&SemanticsProperties::ROLE) == Some(&role)
    })
}

/// As `hasProgressBarRangeInfo`.
pub fn has_progress_bar_range_info(info: ProgressBarRangeInfo) -> Matcher {
    Matcher::new(format!("with progress {info:?}"), move |config| {
        config.get(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO) == Some(&info)
    })
}

/// As `hasImeAction`.
pub fn has_ime_action(action: ImeAction) -> Matcher {
    expect_value(&SemanticsProperties::IME_ACTION, action)
}

// ---- State -----------------------------------------------------------------------------------

/// As `isEnabled`: `Disabled` isn't set.
pub fn is_enabled() -> Matcher {
    Matcher::new("is enabled", |config| !config.contains(&SemanticsProperties::DISABLED))
}

/// As `isNotEnabled`.
pub fn is_not_enabled() -> Matcher {
    Matcher::new("is disabled", |config| config.contains(&SemanticsProperties::DISABLED))
}

/// A node with a toggle state, as `isToggleable`.
pub fn is_toggleable() -> Matcher {
    Matcher::new("is toggleable", |config| config.contains(&SemanticsProperties::TOGGLEABLE_STATE))
}

pub fn is_on() -> Matcher {
    Matcher::new("is on", |config| {
        config.get(&SemanticsProperties::TOGGLEABLE_STATE) == Some(&ToggleableState::On)
    })
}

pub fn is_off() -> Matcher {
    Matcher::new("is off", |config| {
        config.get(&SemanticsProperties::TOGGLEABLE_STATE) == Some(&ToggleableState::Off)
    })
}

/// A node with a selected state, as `isSelectable`.
pub fn is_selectable() -> Matcher {
    Matcher::new("is selectable", |config| config.contains(&SemanticsProperties::SELECTED))
}

pub fn is_selected() -> Matcher {
    Matcher::new("is selected", |config| config.get(&SemanticsProperties::SELECTED) == Some(&true))
}

pub fn is_not_selected() -> Matcher {
    Matcher::new("is not selected", |config| {
        config.get(&SemanticsProperties::SELECTED) == Some(&false)
    })
}

/// As `isFocusable`: `Focused` is set (either way).
pub fn is_focusable() -> Matcher {
    Matcher::new("is focusable", |config| config.contains(&SemanticsProperties::FOCUSED))
}

pub fn is_not_focusable() -> Matcher {
    Matcher::new("is not focusable", |config| !config.contains(&SemanticsProperties::FOCUSED))
}

pub fn is_focused() -> Matcher {
    Matcher::new("is focused", |config| config.get(&SemanticsProperties::FOCUSED) == Some(&true))
}

/// A focusable node without the focus, as `isNotFocused` (`Focused` is false).
pub fn is_not_focused() -> Matcher {
    Matcher::new("is not focused", |config| {
        config.get(&SemanticsProperties::FOCUSED) == Some(&false)
    })
}

/// A dialog's node, as `isDialog`.
pub fn is_dialog() -> Matcher {
    Matcher::new("is dialog", |config| config.contains(&SemanticsProperties::IS_DIALOG))
}

/// A popup's node, as `isPopup`.
pub fn is_popup() -> Matcher {
    Matcher::new("is popup", |config| config.contains(&SemanticsProperties::IS_POPUP))
}

/// As `isHeading`.
pub fn is_heading() -> Matcher {
    key_is_defined(&SemanticsProperties::HEADING)
}

/// As `isHiddenFromAccessibility`.
pub fn is_hidden_from_accessibility() -> Matcher {
    key_is_defined(&SemanticsProperties::HIDE_FROM_ACCESSIBILITY)
}

/// An editable text field, as `isEditable` (`IsEditable` is true).
pub fn is_editable() -> Matcher {
    Matcher::new("is editable", |config| {
        config.get(&SemanticsProperties::IS_EDITABLE) == Some(&true)
    })
}

// ---- Actions ---------------------------------------------------------------------------------

/// As `hasClickAction`.
pub fn has_click_action() -> Matcher {
    Matcher::new("has click action", |config| config.contains(&SemanticsActions::ON_CLICK))
}

/// As `hasNoClickAction`.
pub fn has_no_click_action() -> Matcher {
    Matcher::new("has no click action", |config| !config.contains(&SemanticsActions::ON_CLICK))
}

/// A node that scrolls, as `hasScrollAction` (`ScrollBy`).
pub fn has_scroll_action() -> Matcher {
    Matcher::new("has scroll action", |config| config.contains(&SemanticsActions::SCROLL_BY))
}

/// As `hasNoScrollAction`.
pub fn has_no_scroll_action() -> Matcher {
    Matcher::new("has no scroll action", |config| !config.contains(&SemanticsActions::SCROLL_BY))
}

/// A node whose text accessibility can set (a text field), as `hasSetTextAction`.
pub fn has_set_text_action() -> Matcher {
    Matcher::new("has set text action", |config| config.contains(&SemanticsActions::SET_TEXT))
}

/// As `hasInsertTextAtCursorAction`.
pub fn has_insert_text_at_cursor_action() -> Matcher {
    key_is_defined(&SemanticsActions::INSERT_TEXT_AT_CURSOR)
}

/// As `hasPerformImeAction`.
pub fn has_perform_ime_action() -> Matcher {
    key_is_defined(&SemanticsActions::ON_IME_ACTION)
}

/// A node that can take the focus, as `hasRequestFocusAction`.
pub fn has_request_focus_action() -> Matcher {
    Matcher::new("has request focus action", |config| {
        config.contains(&SemanticsActions::REQUEST_FOCUS)
    })
}

/// As `hasScrollToIndexAction`.
pub fn has_scroll_to_index_action() -> Matcher {
    key_is_defined(&SemanticsActions::SCROLL_TO_INDEX)
}

/// As `hasScrollToKeyAction`: `ScrollToIndex` and `IndexForKey`.
pub fn has_scroll_to_key_action() -> Matcher {
    key_is_defined(&SemanticsActions::SCROLL_TO_INDEX)
        & key_is_defined(&SemanticsProperties::INDEX_FOR_KEY)
}

/// As `hasScrollToNodeAction`: `ScrollToIndex`, `ScrollBy` and a scroll range.
pub fn has_scroll_to_node_action() -> Matcher {
    key_is_defined(&SemanticsActions::SCROLL_TO_INDEX)
        & key_is_defined(&SemanticsActions::SCROLL_BY)
        & (key_is_defined(&SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE)
            | key_is_defined(&SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE))
}

// ---- The tree --------------------------------------------------------------------------------

/// The root, as `isRoot`.
pub fn is_root() -> Matcher {
    Matcher::with_node("is root", |node| node.is_root())
}

/// A node whose parent matches, as `hasParent`.
pub fn has_parent(matcher: Matcher) -> Matcher {
    Matcher::with_node(format!("has parent that ({})", matcher.description), move |node| {
        node.parent().is_some_and(|parent| matcher.matches_node(parent))
    })
}

/// A node with a child that matches, as `hasAnyChild`.
pub fn has_any_child(matcher: Matcher) -> Matcher {
    Matcher::with_node(format!("has any child that ({})", matcher.description), move |node| {
        matcher.matches_any(node.children())
    })
}

/// A node with a sibling that matches, as `hasAnySibling`.
pub fn has_any_sibling(matcher: Matcher) -> Matcher {
    Matcher::with_node(format!("has any sibling that ({})", matcher.description), move |node| {
        matcher.matches_any(node.siblings())
    })
}

/// A node with an ancestor that matches, as `hasAnyAncestor`.
pub fn has_any_ancestor(matcher: Matcher) -> Matcher {
    Matcher::with_node(format!("has any ancestor that ({})", matcher.description), move |node| {
        matcher.matches_any(node.ancestors())
    })
}

/// A node with a descendant that matches, as `hasAnyDescendant`.
pub fn has_any_descendant(matcher: Matcher) -> Matcher {
    Matcher::with_node(format!("has any descendant that ({})", matcher.description), move |node| {
        matcher.matches_any(node.descendants())
    })
}
