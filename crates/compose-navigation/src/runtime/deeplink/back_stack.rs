//! `BackStackMatcher.kt`.

use std::cell::OnceCell;
use std::cmp::Ordering;
use std::marker::PhantomData;
use std::rc::Rc;

use super::matcher::{DeepLinkMatchResult, DeepLinkMatcher};
use super::request::DeepLinkRequest;

type BackStackBuilder<T, K> = Rc<dyn Fn(&dyn DeepLinkMatchResult<T>) -> Vec<K>>;

/// A matcher building a back stack when another matches, as `BackStackMatcher`: made by
/// [`DeepLinkMatcher::with_back_stack`].
pub struct BackStackMatcher<M, T, K> {
    matcher: M,
    back_stack_builder: BackStackBuilder<T, K>,
    key: PhantomData<T>,
}

impl<M, T, K> BackStackMatcher<M, T, K> {
    pub(crate) fn new(
        matcher: M,
        back_stack_builder: impl Fn(&dyn DeepLinkMatchResult<T>) -> Vec<K> + 'static,
    ) -> BackStackMatcher<M, T, K> {
        BackStackMatcher {
            matcher,
            back_stack_builder: Rc::new(back_stack_builder),
            key: PhantomData,
        }
    }
}

impl<M, T, K> DeepLinkMatcher<T> for BackStackMatcher<M, T, K>
where
    M: DeepLinkMatcher<T>,
    M::Result: DeepLinkMatchResult<T>,
    T: 'static,
    K: 'static,
{
    type Result = BackStackMatchResult<T, K>;

    fn match_request(&self, request: &DeepLinkRequest) -> Option<BackStackMatchResult<T, K>> {
        let result = self.matcher.r#match(request)?;
        Some(BackStackMatchResult {
            match_result: Rc::new(result),
            back_stack_builder: self.back_stack_builder.clone(),
            back_stack: OnceCell::new(),
        })
    }
}

/// A [`BackStackMatcher`]'s result: what matched, ranking as it, and its back stack.
pub struct BackStackMatchResult<T, K> {
    match_result: Rc<dyn DeepLinkMatchResult<T>>,
    back_stack_builder: BackStackBuilder<T, K>,
    back_stack: OnceCell<Vec<K>>,
}

impl<T: 'static, K: 'static> BackStackMatchResult<T, K> {
    #[doc(hidden)]
    pub fn new(
        match_result: impl DeepLinkMatchResult<T>,
        back_stack_builder: impl Fn(&dyn DeepLinkMatchResult<T>) -> Vec<K> + 'static,
    ) -> BackStackMatchResult<T, K> {
        BackStackMatchResult {
            match_result: Rc::new(match_result),
            back_stack_builder: Rc::new(back_stack_builder),
            back_stack: OnceCell::new(),
        }
    }

    /// The back stack of this match, built when first asked for.
    pub fn back_stack(&self) -> &[K] {
        self.back_stack.get_or_init(|| (self.back_stack_builder)(&*self.match_result))
    }

    /// What matched.
    pub fn match_result(&self) -> &dyn DeepLinkMatchResult<T> {
        &*self.match_result
    }
}

impl<T: 'static, K: 'static> DeepLinkMatchResult<T> for BackStackMatchResult<T, K> {
    fn key(&self) -> &T {
        self.match_result.key()
    }

    fn compare_to(&self, other: &dyn DeepLinkMatchResult<T>) -> Ordering {
        self.match_result.compare_to(other.wrapped().unwrap_or(other))
    }

    fn wrapped(&self) -> Option<&dyn DeepLinkMatchResult<T>> {
        Some(&*self.match_result)
    }
}
