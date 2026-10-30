//! Reusable content, as Compose's `ReusableContent` and `ReusableContentHost`: content composed
//! in the place of other content keeps that content's groups and nodes, updated, rather than
//! removing them and creating new ones. A lazy list's items (D430) reuse those that left.

use std::hash::Hash;

use crate::composer::current;
use crate::hash::{hash_key, key_of};

/// The group key of `ReusableContent` and `ReusableContentHost`, as Compose's `reuseKey`.
const REUSE_KEY: u64 = key_of("compose_runtime::reuseKey");

/// Composes `content` in a reusable group identified by `key`, as Compose's
/// `ReusableContent(key) { … }`: when `key` changes, the content composed before is reused
/// rather than removed. Its remembered values are forgotten and computed anew (remember
/// observers get `on_forgotten`, effects are disposed and launched again), its composables run,
/// and its nodes are kept and updated in place (the nodes of `Layout`, which are reusable;
/// `Applier::reuse` resets them first). Groups inside match as usual: a `key(…)` that changed
/// gets new content.
///
/// Matched by position, not by `key`: unlike `key(…)`, two of them in a loop don't move with
/// their keys (put each in a `key(…)` for that).
#[allow(non_snake_case)]
pub fn ReusableContent(key: impl Hash, content: impl FnOnce()) {
    let composer = current();
    composer.start_reusable_group(REUSE_KEY, hash_key(&key));
    content();
    composer.end_reusable_group();
}

/// Composes `content` while `active`, as Compose's `ReusableContentHost(active) { … }`. While
/// not active, the content composed before stays, deactivated, for reuse: its groups and nodes
/// are kept (the applier deactivates the nodes: `Applier::deactivate`), its remember observers
/// forgotten (effects disposed) and its recompose scopes released, so nothing in it runs. Active
/// again, the content is reused as by [`ReusableContent`] with a new key.
#[allow(non_snake_case)]
pub fn ReusableContentHost(active: bool, content: impl FnOnce()) {
    let composer = current();
    composer.start_reusable_group(REUSE_KEY, u64::from(active));
    if active {
        content();
    } else {
        // Compose's `changed(active)` is true only while reusing (slots read as empty then): the
        // group's data key changed as `active` did, or content around is reused. The composer
        // knows: given false, it deactivates then.
        composer.deactivate_to_end_group(false);
    }
    composer.end_reusable_group();
}

/// The group of a [`ReusableComposition`](crate::ReusableComposition)'s content, as
/// `startReuseFromRoot`: `set_content_with_reuse` is no key of Compose's, so a composite key hash
/// doesn't hold these (`composite_key_hash_at`).
pub(crate) const REUSE_ROOT_KEY: u64 = key_of("compose_runtime::reuseFromRoot");

/// A reusable composition's content: as [`ReusableContentHost`] around [`ReusableContent`], the
/// content reused when `generation` moves, without what they'd add to a hash.
#[allow(non_snake_case)]
pub(crate) fn ReusableRootContent(active: bool, generation: u64, content: impl FnOnce()) {
    let composer = current();
    composer.start_reusable_group(REUSE_ROOT_KEY, u64::from(active));
    if active {
        composer.start_reusable_group(REUSE_ROOT_KEY, generation);
        content();
        composer.end_reusable_group();
    } else {
        composer.deactivate_to_end_group(false);
    }
    composer.end_reusable_group();
}
