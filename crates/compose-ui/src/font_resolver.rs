//! Resolving a family, a weight and a style to a typeface, as Compose's `FontFamilyResolverImpl`,
//! `FontListFontFamilyTypefaceAdapter` and their caches: which font, loaded how, what shows until
//! it's there. Compose's caches are global and its loader's per context; here both are a text
//! stack's ([`PlatformFonts`]), which is what a measurer resolves its styles' fonts with (D1120).

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use compose_runtime::platform::{launch, poll_now};
use compose_runtime::{Job, OwnedState, delay, mutable_state_of};

use crate::font::{
    Font, FontFamily, FontLoadingStrategy, FontMatcher, FontSource, FontStyle, FontSynthesis,
    FontWeight, PlatformFontLoader, Typeface, synthesize_typeface,
};
use crate::text_style::TextStyle;

/// What a family resolves to, as Compose's `State<Any>` from `FontFamily.Resolver.resolve`: a
/// typeface now and for good, or the one showing until an `Async` font arrives.
#[derive(Clone)]
pub enum TypefaceResult {
    Immutable {
        value: Typeface,
        /// Whether a next request may be given this: not a load cut short.
        cacheable: bool,
    },
    Async(Rc<AsyncFontListLoader>),
}

impl TypefaceResult {
    /// The typeface now. Of an `Async` one it's a state's read: what reads it is told of a new
    /// value.
    pub fn value(&self) -> Typeface {
        match self {
            TypefaceResult::Immutable { value, .. } => value.clone(),
            TypefaceResult::Async(loader) => loader.value.get(),
        }
    }

    pub fn cacheable(&self) -> bool {
        match self {
            TypefaceResult::Immutable { cacheable, .. } => *cacheable,
            TypefaceResult::Async(loader) => loader.cacheable.get(),
        }
    }
}

/// An `Async` load in progress, as Compose's `AsyncFontListLoader`: the fonts to try in order and
/// the state of what shows.
pub struct AsyncFontListLoader {
    value: OwnedState<Typeface>,
    cacheable: Cell<bool>,
}

/// A least recently used list, as the `LruCache` Compose's caches are: few entries, and keys with
/// a costly hash.
struct Recent<K, V> {
    capacity: usize,
    /// The least recently used first.
    entries: Vec<(K, V)>,
}

impl<K: PartialEq, V: Clone> Recent<K, V> {
    fn new(capacity: usize) -> Recent<K, V> {
        Recent { capacity, entries: Vec::new() }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        // From the most recent: the one asked again, nearly always.
        let found = self.entries.iter().rposition(|(kept, _)| kept == key)?;
        let entry = self.entries.remove(found);
        let value = entry.1.clone();
        self.entries.push(entry);
        Some(value)
    }

    fn put(&mut self, key: K, value: V) {
        self.remove(&key);
        if self.entries.len() == self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((key, value));
    }

    fn remove(&mut self, key: &K) {
        self.entries.retain(|(kept, _)| kept != key);
    }
}

/// How many results each cache keeps, as Compose's (based on Android's `TypefaceCompat`).
const CACHE_SIZE: usize = 16;

/// A font loaded, or, `None`, that won't load: never tried again.
type LoadedFont = Option<Typeface>;

/// The typefaces fonts loaded to, as Compose's `AsyncTypefaceCache`.
#[derive(Default)]
struct FontResults {
    recent: Option<Recent<Font, LoadedFont>>,
    /// Failures and preloads, for good.
    permanent: Vec<(Font, LoadedFont)>,
}

impl FontResults {
    fn get(&mut self, font: &Font) -> Option<LoadedFont> {
        if let Some(found) = self.recent.as_mut().and_then(|recent| recent.get(font)) {
            return Some(found);
        }
        self.permanent.iter().find(|(kept, _)| kept == font).map(|(_, result)| result.clone())
    }

    fn put(&mut self, font: &Font, result: LoadedFont, forever: bool) {
        if result.is_none() || forever {
            self.permanent.retain(|(kept, _)| kept != font);
            self.permanent.push((font.clone(), result));
        } else {
            self.recent.get_or_insert_with(|| Recent::new(CACHE_SIZE)).put(font.clone(), result);
        }
    }
}

/// What a resolution is asked, as Compose's `TypefaceRequest`.
#[derive(Clone)]
struct TypefaceRequest {
    family: Option<FontFamily>,
    weight: FontWeight,
    style: FontStyle,
    synthesis: FontSynthesis,
}

impl PartialEq for TypefaceRequest {
    /// The cheap fields first: the family may be a list of fonts.
    fn eq(&self, other: &Self) -> bool {
        self.weight == other.weight
            && self.style == other.style
            && self.synthesis == other.synthesis
            && self.family == other.family
    }
}

/// A text stack's fonts: its loader and what's been loaded and resolved.
pub struct PlatformFonts {
    loader: Rc<dyn PlatformFontLoader>,
    requests: RefCell<Recent<TypefaceRequest, TypefaceResult>>,
    results: RefCell<FontResults>,
}

impl PlatformFonts {
    pub fn new(loader: Rc<dyn PlatformFontLoader>) -> Rc<PlatformFonts> {
        Rc::new(PlatformFonts {
            loader,
            requests: RefCell::new(Recent::new(CACHE_SIZE)),
            results: RefCell::default(),
        })
    }

    /// See [`PlatformFontLoader::release_dropped`].
    pub fn release_dropped(&self) {
        self.loader.release_dropped();
    }

    /// Whether what `family` at `weight` and `style` resolves to is cached: for tests, as
    /// Compose's look into its `TypefaceRequestCache`.
    #[doc(hidden)]
    pub fn has_cached_request(
        &self,
        family: Option<&FontFamily>,
        weight: FontWeight,
        style: FontStyle,
        synthesis: FontSynthesis,
    ) -> bool {
        let request = TypefaceRequest { family: family.cloned(), weight, style, synthesis };
        self.requests.borrow_mut().get(&request).is_some()
    }

    /// `family` at `weight` and `style`, as Compose's `FontFamily.Resolver.resolve`.
    pub fn resolve(
        self: &Rc<Self>,
        family: Option<&FontFamily>,
        weight: FontWeight,
        style: FontStyle,
        synthesis: FontSynthesis,
    ) -> TypefaceResult {
        self.resolve_request(TypefaceRequest { family: family.cloned(), weight, style, synthesis })
    }

    fn resolve_request(self: &Rc<Self>, request: TypefaceRequest) -> TypefaceResult {
        {
            let mut requests = self.requests.borrow_mut();
            if let Some(found) = requests.get(&request) {
                if found.cacheable() {
                    return found;
                }
                requests.remove(&request);
            }
        }
        // Not under the borrow: it's the font's load, and a fallback's resolution.
        let result = self.resolve_uncached(&request);
        let mut requests = self.requests.borrow_mut();
        // A load finishing at once has put its final result: not overwritten.
        if requests.get(&request).is_none() && result.cacheable() {
            requests.put(request, result.clone());
        }
        result
    }

    fn resolve_uncached(self: &Rc<Self>, request: &TypefaceRequest) -> TypefaceResult {
        let immutable = |value| TypefaceResult::Immutable { value, cacheable: true };
        match &request.family {
            None | Some(FontFamily::Default) => {
                immutable(self.loader.create_default(request.weight, request.style))
            }
            Some(FontFamily::Named(name)) => {
                immutable(self.loader.create_named(name, request.weight, request.style))
            }
            Some(FontFamily::Loaded(typeface)) => immutable(typeface.clone()),
            Some(FontFamily::FontList(family)) => {
                let matched =
                    FontMatcher::match_font(family.fonts(), request.weight, request.style);
                let (to_load, typeface) = self.first_immediately_available(&matched, request);
                match to_load {
                    None => immutable(typeface),
                    Some(to_load) => self.load_async(to_load, typeface, request),
                }
            }
            Some(generic) => {
                let name = generic.generic_name().expect("the other families are matched above");
                immutable(self.loader.create_named(name, request.weight, request.style))
            }
        }
    }

    /// The default family's typeface for what `request` asks: what a font that won't load is.
    fn default_typeface(self: &Rc<Self>, request: &TypefaceRequest) -> Typeface {
        self.resolve_request(TypefaceRequest { family: None, ..request.clone() }).value()
    }

    fn cached_font(&self, font: &Font) -> Option<LoadedFont> {
        self.results.borrow_mut().get(font)
    }

    fn cache_font(&self, font: &Font, result: LoadedFont, forever: bool) {
        self.results.borrow_mut().put(font, result, forever);
    }

    fn load_blocking(&self, font: &Font) -> Result<LoadedFont, String> {
        match font.source() {
            FontSource::Loader(loader) => loader.load_blocking(&*self.loader, font),
            _ => self.loader.load_blocking(font),
        }
    }

    fn await_load(&self, font: &Font) -> Pin<Box<dyn Future<Output = Result<LoadedFont, String>>>> {
        match font.source() {
            FontSource::Loader(loader) => {
                loader.clone().await_load(self.loader.clone(), font.clone())
            }
            _ => self.loader.clone().await_load(font.clone()),
        }
    }

    fn synthesized(&self, typeface: &Typeface, font: &Font, request: &TypefaceRequest) -> Typeface {
        synthesize_typeface(
            &*self.loader,
            request.synthesis,
            typeface,
            font,
            request.weight,
            request.style,
        )
    }

    /// What shows now of `fonts` and the `Async` ones before it to load, as Compose's
    /// `firstImmediatelyAvailable`: the first `Blocking` font (a failure is the default), the
    /// first `OptionalLocal` there, the first `Async` loaded; else the default.
    fn first_immediately_available(
        self: &Rc<Self>,
        fonts: &[Font],
        request: &TypefaceRequest,
    ) -> (Option<Vec<Font>>, Typeface) {
        let mut to_load: Option<Vec<Font>> = None;
        for font in fonts {
            match font.loading_strategy() {
                FontLoadingStrategy::Blocking => {
                    let loaded = match self.cached_font(font) {
                        Some(loaded) => loaded,
                        None => {
                            let loaded = self
                                .load_blocking(font)
                                .unwrap_or_else(|_| Some(self.default_typeface(request)));
                            self.cache_font(font, loaded.clone(), false);
                            loaded
                        }
                    };
                    let typeface = loaded.unwrap_or_else(|| self.default_typeface(request));
                    return (to_load, self.synthesized(&typeface, font, request));
                }
                FontLoadingStrategy::OptionalLocal => {
                    let loaded = match self.cached_font(font) {
                        Some(loaded) => loaded,
                        None => {
                            // An optional font shouldn't fail: a failed load if it does.
                            let loaded = self.load_blocking(font).unwrap_or(None);
                            self.cache_font(font, loaded.clone(), false);
                            loaded
                        }
                    };
                    if let Some(typeface) = loaded {
                        return (to_load, self.synthesized(&typeface, font, request));
                    }
                }
                FontLoadingStrategy::Async => match self.cached_font(font) {
                    None => to_load.get_or_insert_with(Vec::new).push(font.clone()),
                    // A permanent failure: it will never load.
                    Some(None) => {}
                    Some(Some(typeface)) => {
                        return (to_load, self.synthesized(&typeface, font, request));
                    }
                },
            }
        }
        (to_load, self.default_typeface(request))
    }

    /// `fonts` loading in a task of their own, started now as far as it goes without waiting.
    fn load_async(
        self: &Rc<Self>,
        fonts: Vec<Font>,
        initial: Typeface,
        request: &TypefaceRequest,
    ) -> TypefaceResult {
        let loader = Rc::new(AsyncFontListLoader {
            value: mutable_state_of(initial),
            cacheable: Cell::new(true),
        });
        let completion = LoadCompletion {
            loader: loader.clone(),
            fonts: Rc::downgrade(self),
            request: request.clone(),
            active: Cell::new(false),
        };
        let (stack, request) = (self.clone(), request.clone());
        let job = launch(async move {
            for font in &fonts {
                // What's not `Async` here has been the fallback already, or was never reached.
                if font.loading_strategy() != FontLoadingStrategy::Async {
                    continue;
                }
                let typeface = stack.load_with_timeout_or_none(font).await;
                if let Some(typeface) = typeface {
                    completion.loader.value.set(stack.synthesized(&typeface, font, &request));
                    break;
                }
                // The next font waits its turn, as Compose's `yield`.
                yield_once().await;
            }
            completion.active.set(true);
        });
        poll_now(job);
        TypefaceResult::Async(loader)
    }

    /// `font` loaded and cached, as Compose's `loadWithTimeoutOrNull` in `runCached`: a timeout,
    /// an error, a load that cancels itself or nothing is a permanent failure.
    async fn load_with_timeout_or_none(&self, font: &Font) -> LoadedFont {
        if let Some(cached) = self.cached_font(font) {
            return cached;
        }
        let load = child_task(self.await_load(font));
        let loaded = match with_timeout_or_none(Box::pin(load)).await {
            None | Some(None) => None,
            Some(Some(Ok(loaded))) => loaded,
            Some(Some(Err(error))) => {
                // It's no fatal error, and nothing else hears it.
                log::warn!("Unable to load font {font:?}: {error}");
                None
            }
        };
        self.cache_font(font, loaded.clone(), false);
        loaded
    }

    /// Loads `family`'s `Async` fonts before they're asked for, as Compose's
    /// `FontFamily.Resolver.preload`, for good; `Err` if one fails, what Compose throws. Its other
    /// families have nothing to load.
    pub async fn preload(self: Rc<Self>, family: FontFamily) -> Result<(), String> {
        let FontFamily::FontList(list) = &family else { return Ok(()) };
        let fonts = list.fonts();
        let mut styles: Vec<(FontWeight, FontStyle)> = Vec::new();
        for font in fonts {
            let style = (font.weight(), font.style());
            if font.loading_strategy() == FontLoadingStrategy::Async && !styles.contains(&style) {
                styles.push(style);
            }
        }
        let mut loads: Vec<Font> = Vec::new();
        for (weight, style) in styles {
            let matched = FontMatcher::match_font(fonts, weight, style);
            let request = TypefaceRequest {
                family: Some(family.clone()),
                weight,
                style,
                synthesis: FontSynthesis::All,
            };
            // Satisfied by another font, that's fine: it's cached for the caller.
            let (to_load, _) = self.first_immediately_available(&matched, &request);
            if let Some(first) = to_load.and_then(|fonts| fonts.into_iter().next())
                && !loads.contains(&first)
            {
                loads.push(first);
            }
        }
        let mut pending: Vec<_> = loads
            .into_iter()
            .map(|font| {
                let stack = self.clone();
                let load: Pin<Box<dyn Future<Output = Result<(), String>>>> =
                    Box::pin(async move { stack.preload_font(&font).await });
                load
            })
            .collect();
        // Together, as Compose's `async`s; the first failure is the result, as `joinAll`'s.
        std::future::poll_fn(|context| {
            let mut index = 0;
            while index < pending.len() {
                match pending[index].as_mut().poll(context) {
                    Poll::Ready(Ok(())) => {
                        drop(pending.swap_remove(index));
                    }
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending => index += 1,
                }
            }
            if pending.is_empty() { Poll::Ready(Ok(())) } else { Poll::Pending }
        })
        .await?;
        for font in fonts {
            let request = TypefaceRequest {
                family: Some(family.clone()),
                weight: font.weight(),
                style: font.style(),
                synthesis: FontSynthesis::All,
            };
            let prior = self.requests.borrow_mut().get(&request);
            if prior.is_none() {
                let result = self.resolve_uncached(&request);
                // An `Async` one shouldn't be left: they're all in the permanent cache.
                if !matches!(result, TypefaceResult::Async(_)) {
                    self.requests.borrow_mut().put(request, result);
                }
            }
        }
        Ok(())
    }

    /// As `runCached(font, forever = true) { withTimeout { awaitLoad } }`.
    async fn preload_font(&self, font: &Font) -> Result<(), String> {
        if let Some(cached) = self.cached_font(font) {
            return if cached.is_some() {
                Ok(())
            } else {
                Err(format!("Unable to load font {font:?}"))
            };
        }
        let load = child_task(self.await_load(font));
        let loaded = with_timeout_or_none(Box::pin(load)).await;
        match loaded {
            Some(Some(Ok(Some(typeface)))) => {
                self.cache_font(font, Some(typeface), true);
                Ok(())
            }
            other => {
                // A timeout or a failure is thrown, and leaves nothing behind in Compose's: a
                // later resolution may try again.
                let reason = match other {
                    Some(Some(Err(error))) => format!(": {error}"),
                    _ => String::new(),
                };
                Err(format!("Unable to load font {font:?}{reason}"))
            }
        }
    }
}

/// What a font load's end does, whichever way it ends, as the `finally` of Compose's
/// `AsyncFontListLoader.load`: dropped with its task if that's cancelled, its result not kept.
struct LoadCompletion {
    loader: Rc<AsyncFontListLoader>,
    fonts: Weak<PlatformFonts>,
    request: TypefaceRequest,
    /// It got to its end, not cancelled.
    active: Cell<bool>,
}

impl Drop for LoadCompletion {
    fn drop(&mut self) {
        self.loader.cacheable.set(false);
        let Some(fonts) = self.fonts.upgrade() else { return };
        // Read first: a read may run what's observing it, not under `requests`' borrow.
        let value = self.loader.value.get();
        let mut requests = fonts.requests.borrow_mut();
        if self.active.get() {
            // From now on immediately what it came to.
            requests
                .put(self.request.clone(), TypefaceResult::Immutable { value, cacheable: true });
        } else {
            requests.remove(&self.request);
        }
    }
}

/// What a [`child_task`] and what waits for it share.
struct ChildState<T> {
    result: RefCell<Option<T>>,
    ended: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}

/// Marks a child's end, however it ends: dropped with its future if it's cancelled.
struct ChildEnd<T>(Rc<ChildState<T>>);

impl<T> Drop for ChildEnd<T> {
    fn drop(&mut self) {
        self.0.ended.set(true);
        if let Some(waker) = self.0.waker.borrow_mut().take() {
            waker.wake();
        }
    }
}

/// A [`child_task`], ending in its output, or `None` if it was cancelled. Dropped, it cancels it.
struct ChildTask<T> {
    state: Rc<ChildState<T>>,
    job: Job,
}

impl<T> Future for ChildTask<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<T>> {
        if self.state.ended.get() {
            return Poll::Ready(self.state.result.borrow_mut().take());
        }
        *self.state.waker.borrow_mut() = Some(context.waker().clone());
        Poll::Pending
    }
}

impl<T> Drop for ChildTask<T> {
    fn drop(&mut self) {
        self.job.cancel();
    }
}

/// `future` as a task of its own, started as far as it goes: a load cancelling itself (Compose's
/// loader throwing `CancellationException`) ends it, not the fonts waiting on it, as Compose's
/// `withTimeoutOrNull` catches what the load threw.
fn child_task<T: 'static>(future: Pin<Box<dyn Future<Output = T>>>) -> ChildTask<T> {
    let state = Rc::new(ChildState {
        result: RefCell::new(None),
        ended: Cell::new(false),
        waker: RefCell::new(None),
    });
    let end = ChildEnd(state.clone());
    let job = launch(async move {
        let output = future.await;
        *end.0.result.borrow_mut() = Some(output);
    });
    poll_now(job);
    ChildTask { state, job }
}

/// Lets the other tasks run once.
async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(|context| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await
}

/// `future`'s output, or `None` after Compose's `Font.MaximumAsyncTimeoutMillis`.
async fn with_timeout_or_none<T>(future: Pin<Box<dyn Future<Output = T>>>) -> Option<T> {
    let mut future = future;
    let mut timeout = Box::pin(delay(Duration::from_millis(Font::MAXIMUM_ASYNC_TIMEOUT_MILLIS)));
    std::future::poll_fn(|context| {
        if let Poll::Ready(output) = future.as_mut().poll(context) {
            return Poll::Ready(Some(output));
        }
        match timeout.as_mut().poll(context) {
            Poll::Ready(()) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}

/// The fonts a paragraph's styles resolved to: it's out of date when one of them changed, as
/// Compose's `hasStaleResolvedFonts`. Empty for the platform's own families, which go on as they
/// did and cost nothing.
#[derive(Clone, Default)]
pub struct ResolvedFonts(Vec<(TypefaceResult, Typeface)>);

impl ResolvedFonts {
    /// The typeface of `style`'s family if it's one of fonts or a typeface: `None` for the
    /// platform's own. A paragraph laid out in it reads its state, and is told when it changes.
    pub fn resolve(&mut self, fonts: &Rc<PlatformFonts>, style: &TextStyle) -> Option<Typeface> {
        let family = style.font_family.as_ref()?;
        if !matches!(family, FontFamily::FontList(_) | FontFamily::Loaded(_)) {
            return None;
        }
        let result = fonts.resolve(
            Some(family),
            style.font_weight.unwrap_or(FontWeight::NORMAL),
            style.font_style.unwrap_or(FontStyle::Normal),
            style.font_synthesis.unwrap_or(FontSynthesis::All),
        );
        let typeface = result.value();
        self.0.push((result, typeface.clone()));
        Some(typeface)
    }

    /// Whether a font arrived since. The read of a state: what asks is told when one does.
    pub fn is_stale(&self) -> bool {
        self.0.iter().any(|(result, typeface)| result.value() != *typeface)
    }

    /// Whether none was resolved: all the platform's own families.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A loader for a text stack with no fonts of its own: nothing loads, and every typeface is one
/// that nothing draws. What a resolver has over a measurer knowing no fonts.
pub struct NoFontLoader;

impl PlatformFontLoader for NoFontLoader {
    fn load_blocking(&self, _font: &Font) -> Result<Option<Typeface>, String> {
        Ok(None)
    }
    fn create_default(&self, _weight: FontWeight, _style: FontStyle) -> Typeface {
        Typeface::new(())
    }
    fn create_named(&self, _name: &str, _weight: FontWeight, _style: FontStyle) -> Typeface {
        Typeface::new(())
    }
}
