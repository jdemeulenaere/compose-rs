//! Semantics matchers, as Compose's `SemanticsMatcher` and its `has…` functions.

use std::fmt;
use std::rc::Rc;

use compose_ui::{Role, SemanticsConfiguration};

/// A test of a node's semantics, with a description for failure messages.
#[derive(Clone)]
pub struct Matcher {
    pub description: String,
    test: Rc<dyn Fn(&SemanticsConfiguration) -> bool>,
}

impl Matcher {
    pub fn new(
        description: impl Into<String>,
        test: impl Fn(&SemanticsConfiguration) -> bool + 'static,
    ) -> Matcher {
        Matcher { description: description.into(), test: Rc::new(test) }
    }

    pub fn matches(&self, config: &SemanticsConfiguration) -> bool {
        (self.test)(config)
    }

    /// Both, as `and`.
    pub fn and(self, other: Matcher) -> Matcher {
        let description = format!("{} and {}", self.description, other.description);
        Matcher::new(description, move |config| self.matches(config) && other.matches(config))
    }

    pub fn or(self, other: Matcher) -> Matcher {
        let description = format!("{} or {}", self.description, other.description);
        Matcher::new(description, move |config| self.matches(config) || other.matches(config))
    }
}

/// Neither, as `not`: `!has_text("a")`.
impl std::ops::Not for Matcher {
    type Output = Matcher;

    fn not(self) -> Matcher {
        let description = format!("not ({})", self.description);
        Matcher::new(description, move |config| !self.matches(config))
    }
}

impl fmt::Debug for Matcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.description)
    }
}

/// A node with exactly `text` among its texts (or its editable text), as `hasText`.
pub fn has_text(text: &str) -> Matcher {
    let text = text.to_string();
    Matcher::new(format!("with text {text:?}"), move |config| {
        config.all_text().any(|candidate| candidate == text)
    })
}

/// A node with a text containing `text`, as `hasText(text, substring = true)`.
pub fn has_text_containing(text: &str) -> Matcher {
    let text = text.to_string();
    Matcher::new(format!("with text containing {text:?}"), move |config| {
        config.all_text().any(|candidate| candidate.contains(text.as_str()))
    })
}

/// As `hasTestTag`.
pub fn has_test_tag(tag: &str) -> Matcher {
    let tag = tag.to_string();
    Matcher::new(format!("with tag {tag:?}"), move |config| {
        config.test_tag.as_deref() == Some(tag.as_str())
    })
}

pub fn has_role(role: Role) -> Matcher {
    Matcher::new(format!("with role {role:?}"), move |config| config.role == Some(role))
}

/// As `hasClickAction`.
pub fn has_click_action() -> Matcher {
    Matcher::new("with a click action", |config| config.clickable)
}

pub fn is_enabled() -> Matcher {
    Matcher::new("enabled", |config| !config.disabled)
}

pub fn is_not_enabled() -> Matcher {
    Matcher::new("not enabled", |config| config.disabled)
}

pub fn is_focused() -> Matcher {
    Matcher::new("focused", |config| config.focused)
}

pub fn is_focusable() -> Matcher {
    Matcher::new("focusable", |config| config.focusable)
}
