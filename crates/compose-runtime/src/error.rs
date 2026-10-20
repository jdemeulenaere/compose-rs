//! Protocol misuse.
//!
//! Misuse comes from generated code or hand-written primitives, never from correct user code, so
//! it is a [`ComposeError`] value internally and a panic with a clear message at the boundary.

use std::fmt;

/// A violation of the composer protocol or of a runtime rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposeError {
    /// The composer was used while no composition pass was running on this thread.
    NotComposing,
    /// A pass was started while another one was running on this thread.
    PassInProgress,
    /// `end_group` or `end_restart_group` had no matching start, or closed the wrong kind of group.
    UnbalancedGroups,
    /// A pass ended with groups still open.
    UnclosedGroups,
    /// A group was started, or a slot used, after `skip_to_group_end`.
    UseAfterSkip,
    /// A slot was written out of order: `remember`'s calculation called `remember` or a composable.
    SlotOutOfOrder,
    /// A restart function did not start its restart group first.
    RestartWithoutGroup,
    /// The composition was poisoned by a panic during an earlier pass; it can only be dropped.
    Poisoned,
    /// The composition was dropped during its own pass.
    DisposedDuringPass,
    /// A node of a type the applier does not handle was emitted.
    WrongNodeType,
    /// The applier was accessed as the wrong type.
    WrongApplierType,
    /// The applier was used while it was already in use.
    ApplierBusy,
}

impl fmt::Display for ComposeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            ComposeError::NotComposing => "the composer was used outside a composition pass",
            ComposeError::PassInProgress => {
                "a composition pass is already running on this thread (one pass at a time)"
            }
            ComposeError::UnbalancedGroups => {
                "unbalanced groups: an end without a matching start, or the wrong kind of end"
            }
            ComposeError::UnclosedGroups => "the pass ended with groups still open",
            ComposeError::UseAfterSkip => {
                "a group was started or a slot used after skip_to_group_end"
            }
            ComposeError::SlotOutOfOrder => {
                "slots were written out of order: remember's calculation must not call remember \
                 or composables"
            }
            ComposeError::RestartWithoutGroup => {
                "a restart function must start its restart group before anything else"
            }
            ComposeError::Poisoned => {
                "the composition was poisoned by a panic during an earlier pass; it can only be \
                 dropped"
            }
            ComposeError::DisposedDuringPass => "a composition was dropped during its own pass",
            ComposeError::WrongNodeType => {
                "a node of a type the applier does not handle was emitted"
            }
            ComposeError::WrongApplierType => "the applier was accessed as the wrong type",
            ComposeError::ApplierBusy => "the applier was used while it was already in use",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ComposeError {}

/// Panics with the error's message: the public boundary of protocol misuse.
#[cold]
#[track_caller]
pub(crate) fn misuse(error: ComposeError) -> ! {
    panic!("compose-runtime: {error}")
}
