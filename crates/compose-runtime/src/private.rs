//! What `#[composable]` generated code calls. Not a public API.
//!
//! Groups are guards closed on drop, so `return`, `break`, `continue` and `?` in a composable's
//! body keep them balanced. While a panic unwinds they don't close: the pass is poisoned anyway,
//! and touching the runtime then could panic again.

use std::any::Any;

pub use crate::compose_arg::ComposeArg;
pub use crate::composer::{Composer, current};
pub use crate::hash::key_of;

/// The restart group of a restartable composable.
pub struct RestartGroup {
    composer: Composer,
    restart: fn(&dyn Any),
}

impl RestartGroup {
    #[inline]
    pub fn start(composer: Composer, key: u64, restart: fn(&dyn Any)) -> RestartGroup {
        composer.start_restart_group(key);
        RestartGroup { composer, restart }
    }
}

impl Drop for RestartGroup {
    #[inline]
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.composer.end_restart_group(self.restart);
        }
    }
}

/// The group of a non-restartable composable.
pub struct ReplaceGroup {
    composer: Composer,
}

impl ReplaceGroup {
    #[inline]
    pub fn start(key: u64) -> ReplaceGroup {
        let composer = current();
        composer.start_replace_group(key);
        ReplaceGroup { composer }
    }
}

impl Drop for ReplaceGroup {
    #[inline]
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.composer.end_group();
        }
    }
}

/// A region group around a branch or a loop. Outside a composition pass it does nothing: the
/// macro's guess may put one in a handler closure that runs later, outside any pass.
pub struct Region {
    composer: Option<Composer>,
}

#[inline]
pub fn region(key: u64) -> Region {
    let composer = crate::composer::try_current();
    if let Some(composer) = composer {
        composer.start_replace_group(key);
    }
    Region { composer }
}

impl Drop for Region {
    #[inline]
    fn drop(&mut self) {
        if let Some(composer) = self.composer
            && !std::thread::panicking()
        {
            composer.end_group();
        }
    }
}

/// A clone of an argument, to keep. A function rather than `Clone::clone`, so generated code
/// doesn't trip `clippy::clone_on_copy` in the user's crate.
#[inline]
pub fn keep<T: Clone>(value: &T) -> T {
    value.clone()
}

/// A clone of a restart group's kept arguments, for its restart function.
#[inline]
pub fn kept<K: Clone + 'static>(kept: &dyn Any) -> K {
    kept.downcast_ref::<K>().expect("compose-runtime: kept arguments of another type").clone()
}
