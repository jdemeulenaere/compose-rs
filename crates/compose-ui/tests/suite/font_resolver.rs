//! Resolving fonts: Compose's `FontListFontFamilyTypefaceAdapter`, its preload,
//! `FontFamilyResolverImpl`'s caches and synthesis, over a fake platform and fake fonts, as
//! Compose's `AsyncTestTypefaceLoader`, under the executor's manual clock.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Poll, Waker};
use std::time::Duration;

use compose_runtime::platform::{
    advance_clock, cancel_current_task, fire_timers, launch, run_ready, set_manual_clock,
};
use compose_ui::platform::{PlatformFontLoader, PlatformFonts, ResolvedFonts};
use compose_ui::{
    Font, FontFamily, FontLoadingStrategy, FontOptions, FontStyle, FontSynthesis, FontWeight,
    TextStyle, Typeface, TypefaceLoader, TypefaceResult,
};

// ---- The fake platform -----------------------------------------------------------------------

/// A typeface `synthesize` made.
#[derive(PartialEq, Debug)]
struct Synthesized {
    weight: u16,
    italic: bool,
}

struct FakePlatform {
    default: Typeface,
    /// The defaults created.
    created: Cell<u32>,
}

impl PlatformFontLoader for FakePlatform {
    fn load_blocking(&self, _font: &Font) -> Result<Option<Typeface>, String> {
        Err("only faux fonts here".to_string())
    }
    fn create_default(&self, _weight: FontWeight, _style: FontStyle) -> Typeface {
        self.created.set(self.created.get() + 1);
        self.default.clone()
    }
    fn create_named(&self, _name: &str, _weight: FontWeight, _style: FontStyle) -> Typeface {
        Typeface::new("named")
    }
    fn synthesize(&self, _typeface: &Typeface, weight: FontWeight, italic: bool) -> Typeface {
        Typeface::new(Synthesized { weight: weight.weight(), italic })
    }
}

/// A load in progress: what `complete` or `error` ends.
#[derive(Default)]
struct Pending {
    answer: RefCell<Option<Result<Option<Typeface>, String>>>,
    waker: RefCell<Option<Waker>>,
}

/// What the fonts of a test asked for, as `AsyncTestTypefaceLoader`.
#[derive(Default)]
struct Requests {
    completed: RefCell<Vec<&'static str>>,
    pending: RefCell<Vec<(&'static str, Rc<Pending>)>>,
}

impl Requests {
    fn completed(&self) -> Vec<&'static str> {
        let mut names = self.completed.borrow().clone();
        names.sort_unstable();
        names
    }

    fn pending(&self) -> Vec<&'static str> {
        self.pending.borrow().iter().map(|(name, _)| *name).collect()
    }

    fn answer(&self, name: &'static str, answer: Result<Option<Typeface>, String>) {
        let at = self
            .pending
            .borrow()
            .iter()
            .position(|(pending, _)| *pending == name)
            .unwrap_or_else(|| panic!("{name} isn't loading"));
        let (_, pending) = self.pending.borrow_mut().remove(at);
        self.completed.borrow_mut().push(name);
        *pending.answer.borrow_mut() = Some(answer);
        if let Some(waker) = pending.waker.borrow_mut().take() {
            waker.wake();
        }
    }

    fn complete(&self, name: &'static str, typeface: &Typeface) {
        self.answer(name, Ok(Some(typeface.clone())));
    }

    fn error(&self, name: &'static str) {
        self.answer(name, Err("failed to load".to_string()));
    }
}

/// A font of a test, as `BlockingFauxFont`, `OptionalFauxFont` and `AsyncFauxFont`: fonts are
/// equal by their loader here, as by their identity there.
struct Faux {
    name: &'static str,
    requests: Rc<Requests>,
    answer: Result<Option<Typeface>, String>,
}

impl TypefaceLoader for Faux {
    fn load_blocking(
        &self,
        _platform: &dyn PlatformFontLoader,
        _font: &Font,
    ) -> Result<Option<Typeface>, String> {
        self.requests.completed.borrow_mut().push(self.name);
        self.answer.clone()
    }

    fn await_load(
        self: Rc<Self>,
        _platform: Rc<dyn PlatformFontLoader>,
        _font: Font,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Typeface>, String>>>> {
        let pending = Rc::new(Pending::default());
        self.requests.pending.borrow_mut().push((self.name, pending.clone()));
        Box::pin(std::future::poll_fn(move |context| match pending.answer.borrow_mut().take() {
            Some(answer) => Poll::Ready(answer),
            None => {
                *pending.waker.borrow_mut() = Some(context.waker().clone());
                Poll::Pending
            }
        }))
    }
}

struct CancellingLoader {
    loads: Rc<Cell<u32>>,
}

impl TypefaceLoader for CancellingLoader {
    fn load_blocking(
        &self,
        _: &dyn PlatformFontLoader,
        _: &Font,
    ) -> Result<Option<Typeface>, String> {
        Err("async".to_string())
    }

    fn await_load(
        self: Rc<Self>,
        _: Rc<dyn PlatformFontLoader>,
        _: Font,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Typeface>, String>>>> {
        Box::pin(async move {
            self.loads.set(self.loads.get() + 1);
            cancel_current_task();
            std::future::pending().await
        })
    }
}

/// One test's platform, fonts and what they asked.
struct Fixture {
    default: Typeface,
    platform: Rc<FakePlatform>,
    fonts: Rc<PlatformFonts>,
    requests: Rc<Requests>,
}

impl Fixture {
    fn new() -> Fixture {
        set_manual_clock(0);
        let default = Typeface::new("default");
        let platform = Rc::new(FakePlatform { default: default.clone(), created: Cell::new(0) });
        Fixture {
            default,
            fonts: PlatformFonts::new(platform.clone()),
            platform,
            requests: Rc::default(),
        }
    }

    fn font(
        &self,
        name: &'static str,
        strategy: FontLoadingStrategy,
        answer: Result<Option<Typeface>, String>,
        weight: FontWeight,
        style: FontStyle,
    ) -> Font {
        let loader = Rc::new(Faux { name, requests: self.requests.clone(), answer });
        Font::from_loader(
            loader,
            FontOptions {
                weight,
                style,
                loading_strategy: Some(strategy),
                ..FontOptions::default()
            },
        )
    }

    fn blocking(&self, name: &'static str, typeface: &Typeface) -> Font {
        self.font(
            name,
            FontLoadingStrategy::Blocking,
            Ok(Some(typeface.clone())),
            FontWeight::NORMAL,
            FontStyle::Normal,
        )
    }

    fn optional(&self, name: &'static str, typeface: Option<&Typeface>) -> Font {
        self.font(
            name,
            FontLoadingStrategy::OptionalLocal,
            Ok(typeface.cloned()),
            FontWeight::NORMAL,
            FontStyle::Normal,
        )
    }

    fn asynchronous(&self, name: &'static str) -> Font {
        self.font(name, FontLoadingStrategy::Async, Ok(None), FontWeight::NORMAL, FontStyle::Normal)
    }

    fn resolve(&self, family: &FontFamily) -> TypefaceResult {
        self.fonts.resolve(Some(family), FontWeight::NORMAL, FontStyle::Normal, FontSynthesis::All)
    }

    /// Runs what's ready, as `testScheduler.runCurrent`.
    fn run_current(&self) {
        while run_ready() {}
    }

    /// As `advanceTimeBy(Font.MaximumAsyncTimeoutMillis)`, exactly.
    fn advance_past_the_timeout(&self) {
        self.advance(15_000);
    }

    fn advance(&self, millis: u64) {
        advance_clock(Duration::from_millis(millis));
        fire_timers();
        self.run_current();
    }

    /// A font whose load cancels itself, its task's, each time counted in `loads`: what
    /// `throw CancellationException()` is in Compose's loaders.
    fn cancelling(&self, loads: &Rc<Cell<u32>>) -> Font {
        Font::from_loader(
            Rc::new(CancellingLoader { loads: loads.clone() }),
            FontOptions {
                loading_strategy: Some(FontLoadingStrategy::Async),
                ..FontOptions::default()
            },
        )
    }

    /// `family`'s preload, started, its result to read when it ends.
    fn preload(&self, family: &FontFamily) -> Rc<RefCell<Option<Result<(), String>>>> {
        let result: Rc<RefCell<Option<Result<(), String>>>> = Rc::default();
        let (out, fonts, family) = (result.clone(), self.fonts.clone(), family.clone());
        launch(async move {
            *out.borrow_mut() = Some(fonts.preload(family).await);
        });
        self.run_current();
        result
    }
}

fn is_immutable(result: &TypefaceResult, expected: &Typeface) -> bool {
    matches!(result, TypefaceResult::Immutable { value, .. } if value == expected)
}

fn is_async(result: &TypefaceResult, expected: &Typeface) -> bool {
    matches!(result, TypefaceResult::Async(_)) && result.value() == *expected
}

fn typeface(name: &'static str) -> Typeface {
    Typeface::new(name)
}

// ---- FontListFontFamilyTypefaceAdapterTest ---------------------------------------------------

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_onlyBlockingFonts_doesNotLoad
#[test]
fn a_blocking_font_is_there_at_once() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let family = test.blocking("blocking", &expected).to_font_family();
    assert!(is_immutable(&test.resolve(&family), &expected));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_blockingAndAsyncFonts_matchesBlocking_doesLoad
#[test]
fn a_blocking_font_before_an_async_one_leaves_it_alone() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let family = FontFamily::of([test.blocking("blocking", &expected), test.asynchronous("async")]);
    assert!(is_immutable(&test.resolve(&family), &expected));
    assert!(test.requests.pending().is_empty());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_blockingAndAsyncFonts_matchesAsync_doesLoadForBoth
#[test]
fn an_async_font_shows_the_blocking_one_after_it_until_it_arrives() {
    let test = Fixture::new();
    let (initial, expected) = (typeface("mono"), typeface("serif"));
    let family = FontFamily::of([test.asynchronous("async"), test.blocking("blocking", &initial)]);
    let reply = test.resolve(&family);
    assert!(is_async(&reply, &initial));
    test.run_current();
    test.requests.complete("async", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    // From now on it's there at once.
    assert!(is_immutable(&test.resolve(&family), &expected));
    assert_eq!(test.requests.completed(), ["async", "blocking"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_blockingAndAsyncFonts_matchesAsync_onlyLoadsFirstBlockingFallback
#[test]
fn only_the_first_blocking_fallback_of_an_async_font_loads() {
    let test = Fixture::new();
    let (initial, expected) = (typeface("mono"), typeface("sans"));
    let family = FontFamily::of([
        test.asynchronous("async"),
        test.blocking("first", &initial),
        test.blocking("second", &typeface("serif")),
    ]);
    let reply = test.resolve(&family);
    assert!(is_async(&reply, &initial));
    test.requests.complete("async", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    assert_eq!(test.requests.completed(), ["async", "first"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_blockingAndAsyncFonts_differentStyles_onlyLoadsAsync
#[test]
fn a_blocking_font_of_another_weight_is_not_a_fallback() {
    let test = Fixture::new();
    let expected = typeface("sans");
    let lighter = test.font(
        "blocking",
        FontLoadingStrategy::Blocking,
        Ok(Some(typeface("mono"))),
        FontWeight::W100,
        FontStyle::Normal,
    );
    let family = FontFamily::of([test.asynchronous("async"), lighter]);
    let reply = test.resolve(&family);
    // The platform's default until then.
    assert!(is_async(&reply, &test.default));
    test.requests.complete("async", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    assert_eq!(test.requests.completed(), ["async"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_asyncFonts_differentStyles_loadsOnlyNeeded
#[test]
fn only_the_async_font_of_the_weight_asked_loads() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let lighter = test.font(
        "lighter",
        FontLoadingStrategy::Async,
        Ok(None),
        FontWeight::W100,
        FontStyle::Normal,
    );
    let family = FontFamily::of([lighter, test.asynchronous("normal")]);
    let reply = test.resolve(&family);
    assert!(is_async(&reply, &test.default));
    assert_eq!(test.requests.pending(), ["normal"]);
    test.requests.complete("normal", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    assert!(test.requests.pending().is_empty());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_fallbackAsyncFonts_sameStyle_loadsFirst
#[test]
fn a_fallback_async_font_waits_for_the_first() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let family = FontFamily::of([test.asynchronous("first"), test.asynchronous("second")]);
    let reply = test.resolve(&family);
    assert_eq!(test.requests.pending(), ["first"]);
    test.requests.complete("first", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    assert!(test.requests.pending().is_empty());
    assert_eq!(test.requests.completed(), ["first"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_fallbackAsyncFonts_sameStyle_onTimeout_loadsFallback
#[test]
fn a_timeout_moves_on_to_the_fallback() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let family = FontFamily::of([test.asynchronous("first"), test.asynchronous("second")]);
    let reply = test.resolve(&family);
    assert!(is_async(&reply, &test.default));
    // Not a moment before.
    test.advance(14_999);
    assert_eq!(test.requests.pending(), ["first"]);
    test.advance(1);
    // The first never answered: it's still asked, and given up on.
    assert_eq!(test.requests.pending(), ["first", "second"]);
    test.requests.complete("second", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    assert!(is_immutable(&test.resolve(&family), &expected));
    assert_eq!(test.requests.pending(), ["first"]);
    assert_eq!(test.requests.completed(), ["second"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_fallbackAsyncFonts_sameStyle_onError_loadsFallback
#[test]
fn an_error_moves_on_to_the_fallback() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let family = FontFamily::of([test.asynchronous("first"), test.asynchronous("second")]);
    let reply = test.resolve(&family);
    test.requests.error("first");
    test.run_current();
    test.requests.complete("second", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    assert!(test.requests.pending().is_empty());
    assert_eq!(test.requests.completed(), ["first", "second"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_fallbackAsyncFonts_sameStyle_yieldsBetweenFontLoads
#[test]
fn the_next_font_waits_for_the_tasks_turn() {
    let test = Fixture::new();
    let expected = typeface("serif");
    let family = FontFamily::of([test.asynchronous("first"), test.asynchronous("second")]);
    let reply = test.resolve(&family);
    test.requests.error("first");
    assert!(test.requests.pending().is_empty());
    test.run_current();
    assert_eq!(test.requests.pending(), ["second"]);
    test.requests.complete("second", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_optionalAndAsyncFonts_matchesOptional_doesLoad
#[test]
fn an_optional_font_there_leaves_an_async_one_alone() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let family =
        FontFamily::of([test.optional("optional", Some(&expected)), test.asynchronous("async")]);
    assert!(is_immutable(&test.resolve(&family), &expected));
    assert!(test.requests.pending().is_empty());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_optionalAndAsyncFonts_matchesAsync_doesLoadForBoth
#[test]
fn an_optional_font_missing_goes_on_to_the_async_one() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let family = FontFamily::of([test.optional("optional", None), test.asynchronous("async")]);
    let reply = test.resolve(&family);
    assert!(is_async(&reply, &test.default));
    test.requests.complete("async", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    assert_eq!(test.requests.completed(), ["async", "optional"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_optionalAndAsyncAndBlockingFonts_matchesAsync_doesLoadForAll
#[test]
fn optional_async_and_blocking_all_load_when_it_comes_to_it() {
    let test = Fixture::new();
    let (fallback, expected) = (typeface("serif"), typeface("mono"));
    let family = FontFamily::of([
        test.optional("optional", None),
        test.asynchronous("async"),
        test.blocking("blocking", &fallback),
    ]);
    let reply = test.resolve(&family);
    assert!(is_async(&reply, &fallback));
    test.requests.complete("async", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    assert_eq!(test.requests.completed(), ["async", "blocking", "optional"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_optionalAndAsyncAndBlockingFonts_matchAsync_validOptional_doesNotLoadBlocking
#[test]
fn an_optional_font_there_after_an_async_one_is_its_fallback() {
    let test = Fixture::new();
    let (optional, expected) = (typeface("sans"), typeface("mono"));
    let family = FontFamily::of([
        test.asynchronous("async"),
        test.optional("optional", Some(&optional)),
        test.blocking("blocking", &typeface("serif")),
    ]);
    let reply = test.resolve(&family);
    assert!(is_async(&reply, &optional));
    test.requests.complete("async", &expected);
    test.run_current();
    assert!(is_async(&reply, &expected));
    assert_eq!(test.requests.completed(), ["async", "optional"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, onResolve_optionalAndAsyncAndBlockingFonts_matchesOptional_doesNotLoadBlockingAsync
#[test]
fn an_optional_font_there_first_is_all_that_loads() {
    let test = Fixture::new();
    let expected = typeface("sans");
    let family = FontFamily::of([
        test.optional("optional", Some(&expected)),
        test.asynchronous("async"),
        test.blocking("blocking", &typeface("mono")),
    ]);
    assert!(is_immutable(&test.resolve(&family), &expected));
    assert!(test.requests.pending().is_empty());
    assert_eq!(test.requests.completed(), ["optional"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, cancellationIsConsideredLoadError
#[test]
fn a_load_that_cancels_itself_is_a_failure() {
    let test = Fixture::new();
    let loads = Rc::new(Cell::new(0));
    let family = test.cancelling(&loads).to_font_family();
    let reply = test.resolve(&family);
    test.run_current();
    assert!(is_async(&reply, &test.default));
    // It's over, and final: nothing loads again.
    assert!(is_immutable(&test.resolve(&family), &test.default));
    assert_eq!(loads.get(), 1);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, cancellationOfFirstRequest_cannotCancelJob
#[test]
fn a_load_that_cancels_itself_does_not_cancel_its_fallback() {
    let test = Fixture::new();
    let expected = typeface("serif");
    let loads = Rc::new(Cell::new(0));
    let family = FontFamily::of([test.cancelling(&loads), test.asynchronous("fallback")]);
    test.resolve(&family);
    test.run_current();
    assert_eq!(test.requests.pending(), ["fallback"]);
    test.requests.complete("fallback", &expected);
    test.run_current();
    assert!(is_immutable(&test.resolve(&family), &expected));
    assert_eq!(loads.get(), 1);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, cancellationOfFirstLoad_stillDoesSecond_inFallback
#[test]
fn the_failure_of_the_first_load_leaves_the_second_to_arrive() {
    let test = Fixture::new();
    let expected = typeface("serif");
    let family = FontFamily::of([test.asynchronous("first"), test.asynchronous("second")]);
    test.resolve(&family);
    test.requests.error("first");
    test.run_current();
    test.requests.complete("second", &expected);
    test.run_current();
    assert!(is_immutable(&test.resolve(&family), &expected));
}

#[test]
fn a_blocking_font_that_fails_does_not_go_on_to_the_next() {
    let test = Fixture::new();
    let throwing = test.font(
        "throwing",
        FontLoadingStrategy::Blocking,
        Err("always fails to load".to_string()),
        FontWeight::NORMAL,
        FontStyle::Normal,
    );
    let family = FontFamily::of([throwing, test.blocking("next", &typeface("next"))]);
    assert!(is_immutable(&test.resolve(&family), &test.default));
    assert_eq!(test.requests.completed(), ["throwing"]);
}

/// Fills the fonts' cache of recent results with others', past what it keeps.
fn fill_the_recent_fonts(test: &Fixture) {
    for _ in 0..17 {
        let other = test.blocking("other", &typeface("other"));
        test.resolve(&other.to_font_family());
    }
}

#[test]
fn a_failure_is_for_good_past_the_recent_fonts() {
    let test = Fixture::new();
    let font = test.asynchronous("async");
    let family = font.to_font_family();
    test.resolve(&family);
    test.requests.error("async");
    test.run_current();
    fill_the_recent_fonts(&test);
    let another = FontFamily::of([font, test.asynchronous("fallback")]);
    test.resolve(&another);
    assert_eq!(test.requests.pending(), ["fallback"]);
}

#[test]
fn fifteen_other_fonts_leave_a_font_loaded_in_the_recent_ones() {
    let test = Fixture::new();
    let font = test.blocking("kept", &typeface("kept"));
    test.resolve(&font.to_font_family());
    for _ in 0..15 {
        test.resolve(&test.blocking("other", &typeface("other")).to_font_family());
    }
    // Another family, past the requests' cache: its font is the one already loaded.
    let another = FontFamily::of([font, test.asynchronous("unused")]);
    let completed_before =
        test.requests.completed.borrow().iter().filter(|n| **n == "kept").count();
    test.resolve(&another);
    let completed = test.requests.completed.borrow().iter().filter(|n| **n == "kept").count();
    assert_eq!(completed, completed_before);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterTest.kt, throwingFont_doesntThrow
#[test]
fn a_blocking_font_that_fails_is_the_default() {
    let test = Fixture::new();
    let throwing = test.font(
        "throwing",
        FontLoadingStrategy::Blocking,
        Err("always fails to load".to_string()),
        FontWeight::NORMAL,
        FontStyle::Normal,
    );
    assert!(is_immutable(&test.resolve(&throwing.to_font_family()), &test.default));
}

#[test]
fn an_async_font_loaded_is_there_for_another_family() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let font = test.asynchronous("async");
    test.resolve(&font.to_font_family());
    test.requests.complete("async", &expected);
    test.run_current();
    let another = FontFamily::of([font, test.blocking("blocking", &typeface("serif"))]);
    assert!(is_immutable(&test.resolve(&another), &expected));
    assert!(test.requests.pending().is_empty());
}

#[test]
fn a_font_that_failed_for_good_is_skipped_at_once() {
    let test = Fixture::new();
    let font = test.asynchronous("async");
    test.resolve(&font.to_font_family());
    test.requests.error("async");
    test.run_current();
    let fallback = typeface("serif");
    let another = FontFamily::of([font, test.blocking("blocking", &fallback)]);
    assert!(is_immutable(&test.resolve(&another), &fallback));
}

// ---- AsyncFontListLoaderTest ------------------------------------------------------------------

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/AsyncFontListLoaderTest.kt, loadWithTimeoutOrNull_returnsOnSuccess
#[test]
fn a_load_gives_its_typeface() {
    let test = Fixture::new();
    let expected = typeface("loaded");
    let family = test.asynchronous("async").to_font_family();
    let reply = test.resolve(&family);
    test.requests.complete("async", &expected);
    test.run_current();
    assert_eq!(reply.value(), expected);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/AsyncFontListLoaderTest.kt, loadWithTimeoutOrNull_nullsOnException
#[test]
fn a_load_that_throws_gives_nothing() {
    let test = Fixture::new();
    let family = test.asynchronous("async").to_font_family();
    let reply = test.resolve(&family);
    test.requests.error("async");
    test.run_current();
    assert_eq!(reply.value(), test.default);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/AsyncFontListLoaderTest.kt, loadWithTimeoutOrNull_nullsOnNull
#[test]
fn a_load_that_finds_nothing_gives_nothing() {
    let test = Fixture::new();
    let family = test.asynchronous("async").to_font_family();
    let reply = test.resolve(&family);
    test.requests.answer("async", Ok(None));
    test.run_current();
    assert_eq!(reply.value(), test.default);
    // For good.
    assert!(is_immutable(&test.resolve(&family), &test.default));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/AsyncFontListLoaderTest.kt, loadWithTimeoutOrNull_nullsOnCancellation
#[test]
fn a_load_cancelled_gives_nothing() {
    let test = Fixture::new();
    let loads = Rc::new(Cell::new(0));
    let family = test.cancelling(&loads).to_font_family();
    let reply = test.resolve(&family);
    test.run_current();
    assert_eq!(reply.value(), test.default);
}

#[test]
fn a_load_that_never_ends_gives_nothing_after_the_timeout() {
    let test = Fixture::new();
    let family = test.asynchronous("async").to_font_family();
    let reply = test.resolve(&family);
    test.advance_past_the_timeout();
    assert_eq!(reply.value(), test.default);
    assert!(is_immutable(&test.resolve(&family), &test.default));
}

// ---- FontListFontFamilyTypefaceAdapterPreloadTest, FontFamilyResolverImplPreloadTest ----------

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterPreloadTest.kt, onPreload_onlyBlockingFonts_doesNotLoad
#[test]
fn a_family_of_blocking_fonts_preloads_nothing_before_it_is_asked() {
    let test = Fixture::new();
    let family = test.blocking("blocking", &typeface("mono")).to_font_family();
    assert_eq!(*test.preload(&family).borrow(), Some(Ok(())));
    // Compose's resolver warms its cache with each font.
    assert!(test.requests.pending().is_empty());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterPreloadTest.kt, onPreload_blockingAndAsyncFonts_matchesBlocking_doesLoad
#[test]
fn a_blocking_font_before_an_async_one_is_what_preloads() {
    let test = Fixture::new();
    let family =
        FontFamily::of([test.blocking("blocking", &typeface("mono")), test.asynchronous("async")]);
    assert_eq!(*test.preload(&family).borrow(), Some(Ok(())));
    assert_eq!(test.requests.completed(), ["blocking"]);
    assert!(test.requests.pending().is_empty());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterPreloadTest.kt, onPreload_blockingAndAsyncFonts_matchesAsync_doesLoadForBoth
#[test]
fn preloading_an_async_font_waits_for_it() {
    let test = Fixture::new();
    let family =
        FontFamily::of([test.asynchronous("async"), test.blocking("blocking", &typeface("mono"))]);
    let preload = test.preload(&family);
    assert_eq!(test.requests.pending(), ["async"]);
    assert_eq!(test.requests.completed(), ["blocking"]);
    assert_eq!(*preload.borrow(), None);
    test.requests.complete("async", &typeface("serif"));
    test.run_current();
    assert_eq!(*preload.borrow(), Some(Ok(())));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterPreloadTest.kt, onPreload_asyncFonts_differentStyles_loadsAll
#[test]
fn preloading_loads_the_async_font_of_each_style() {
    let test = Fixture::new();
    let lighter = test.font(
        "lighter",
        FontLoadingStrategy::Async,
        Ok(None),
        FontWeight::W100,
        FontStyle::Normal,
    );
    let family = FontFamily::of([test.asynchronous("normal"), lighter]);
    let preload = test.preload(&family);
    let mut pending = test.requests.pending();
    pending.sort_unstable();
    assert_eq!(pending, ["lighter", "normal"]);
    test.requests.complete("normal", &typeface("a"));
    test.run_current();
    assert_eq!(*preload.borrow(), None);
    test.requests.complete("lighter", &typeface("b"));
    test.run_current();
    assert_eq!(*preload.borrow(), Some(Ok(())));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterPreloadTest.kt, onPreload_fallbackAsyncFonts_sameStyle_loadsFirst
#[test]
fn preloading_loads_only_the_first_async_font_of_a_style() {
    let test = Fixture::new();
    let family = FontFamily::of([test.asynchronous("first"), test.asynchronous("second")]);
    let preload = test.preload(&family);
    assert_eq!(test.requests.pending(), ["first"]);
    test.requests.complete("first", &typeface("a"));
    test.run_current();
    assert_eq!(*preload.borrow(), Some(Ok(())));
    assert!(test.requests.pending().is_empty());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterPreloadTest.kt, onPreloadFail_timeout_throwsIllegalStateException
#[test]
fn a_timeout_fails_a_preload() {
    let test = Fixture::new();
    let family = FontFamily::of([test.asynchronous("first"), test.asynchronous("second")]);
    let preload = test.preload(&family);
    assert_eq!(test.requests.pending(), ["first"]);
    test.advance_past_the_timeout();
    assert!(matches!(&*preload.borrow(), Some(Err(error)) if error.contains("Unable to load")));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterPreloadTest.kt, onPreload_whenFontLoadError_throwsIllegalStateException
#[test]
fn an_error_fails_a_preload() {
    let test = Fixture::new();
    let family = FontFamily::of([test.asynchronous("first"), test.asynchronous("second")]);
    let preload = test.preload(&family);
    test.requests.error("first");
    test.run_current();
    assert!(matches!(&*preload.borrow(), Some(Err(error)) if error.contains("failed to load")));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterPreloadTest.kt, onPreload_optionalAndAsyncFonts_matchesOptional_doesLoad
#[test]
fn an_optional_font_there_is_what_preloads() {
    let test = Fixture::new();
    let family = FontFamily::of([
        test.optional("optional", Some(&typeface("a"))),
        test.asynchronous("async"),
    ]);
    assert_eq!(*test.preload(&family).borrow(), Some(Ok(())));
    assert_eq!(test.requests.completed(), ["optional"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontListFontFamilyTypefaceAdapterPreloadTest.kt, onPreload_optionalAndAsyncFonts_matchesAsync_doesLoadForBoth
#[test]
fn an_optional_font_missing_preloads_the_async_one() {
    let test = Fixture::new();
    let family = FontFamily::of([test.optional("optional", None), test.asynchronous("async")]);
    let preload = test.preload(&family);
    assert_eq!(test.requests.pending(), ["async"]);
    test.requests.complete("async", &typeface("a"));
    test.run_current();
    assert_eq!(*preload.borrow(), Some(Ok(())));
    assert_eq!(test.requests.completed(), ["async", "optional"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplPreloadTest.kt, preload_insertsTypefaceIntoCache
#[test]
fn a_preload_puts_a_blocking_fonts_result_in_the_cache() {
    let test = Fixture::new();
    let family = test.blocking("blocking", &typeface("mono")).to_font_family();
    let cached = || {
        test.fonts.has_cached_request(
            Some(&family),
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontSynthesis::All,
        )
    };
    assert!(!cached());
    assert_eq!(*test.preload(&family).borrow(), Some(Ok(())));
    assert!(cached());
    assert_eq!(test.requests.completed(), ["blocking"]);
}

#[test]
fn a_preloaded_font_stays_past_the_recent_fonts() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let font = test.asynchronous("async");
    let preload = test.preload(&font.to_font_family());
    test.requests.complete("async", &expected);
    test.run_current();
    assert_eq!(*preload.borrow(), Some(Ok(())));
    fill_the_recent_fonts(&test);
    // Another family of it, past the requests' cache too: there at once, nothing loaded.
    let another = FontFamily::of([font, test.blocking("fallback", &typeface("fallback"))]);
    assert!(is_immutable(&test.resolve(&another), &expected));
    assert!(test.requests.pending().is_empty());
}

#[test]
fn a_failed_preload_leaves_the_font_to_try_again() {
    let test = Fixture::new();
    let family = test.asynchronous("async").to_font_family();
    let preload = test.preload(&family);
    test.requests.error("async");
    test.run_current();
    assert!(matches!(&*preload.borrow(), Some(Err(_))));
    test.resolve(&family);
    assert_eq!(test.requests.pending(), ["async"]);
}

#[test]
fn a_preloaded_async_font_is_there_at_once() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let family = test.asynchronous("async").to_font_family();
    let preload = test.preload(&family);
    test.requests.complete("async", &expected);
    test.run_current();
    assert_eq!(*preload.borrow(), Some(Ok(())));
    assert!(is_immutable(&test.resolve(&family), &expected));
    assert!(test.requests.pending().is_empty());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplPreloadTest.kt, whenBlockingFont_neverResolvesAsync
#[test]
fn a_preload_of_a_blocking_font_starts_no_async_load() {
    let test = Fixture::new();
    let expected = typeface("mono");
    let family = FontFamily::of([test.blocking("blocking", &expected), test.asynchronous("async")]);
    test.preload(&family);
    assert!(test.requests.pending().is_empty());
    assert!(is_immutable(&test.resolve(&family), &expected));
}

// ---- FontFamilyResolverImplTest: the caches and synthesis -------------------------------------

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, resultsAreCached_defaultTypeface
#[test]
fn a_default_typeface_is_made_once() {
    let test = Fixture::new();
    for _ in 0..3 {
        test.fonts.resolve(None, FontWeight::NORMAL, FontStyle::Normal, FontSynthesis::All);
    }
    assert_eq!(test.platform.created.get(), 1);
}

#[test]
fn another_synthesis_is_another_request() {
    let test = Fixture::new();
    let family = test.blocking("blocking", &typeface("a")).to_font_family();
    let at = |synthesis| {
        test.fonts.resolve(Some(&family), FontWeight::BOLD, FontStyle::Normal, synthesis).value()
    };
    let (none, all) = (at(FontSynthesis::None), at(FontSynthesis::All));
    assert_eq!(synthesized(&none), None);
    assert_eq!(synthesized(&all), Some(&Synthesized { weight: 700, italic: false }));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, resultsNotSame_forDifferentFontWeight
#[test]
fn another_weight_is_another_request() {
    let test = Fixture::new();
    test.fonts.resolve(None, FontWeight::NORMAL, FontStyle::Normal, FontSynthesis::All);
    test.fonts.resolve(None, FontWeight::BOLD, FontStyle::Normal, FontSynthesis::All);
    assert_eq!(test.platform.created.get(), 2);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, resultsNotSame_forDifferentFontStyle
#[test]
fn another_style_is_another_request() {
    let test = Fixture::new();
    test.fonts.resolve(None, FontWeight::NORMAL, FontStyle::Normal, FontSynthesis::All);
    test.fonts.resolve(None, FontWeight::NORMAL, FontStyle::Italic, FontSynthesis::All);
    assert_eq!(test.platform.created.get(), 2);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, resultsAreCached_withCustomTypeface
#[test]
fn a_custom_font_loads_once() {
    let test = Fixture::new();
    let family = test.blocking("blocking", &typeface("a")).to_font_family();
    test.resolve(&family);
    test.resolve(&family);
    assert_eq!(test.requests.completed(), ["blocking"]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, cacheCanHoldTwoResults
#[test]
fn two_results_are_both_kept() {
    let test = Fixture::new();
    let first = test.fonts.resolve(None, FontWeight::NORMAL, FontStyle::Normal, FontSynthesis::All);
    let second =
        test.fonts.resolve(None, FontWeight::NORMAL, FontStyle::Italic, FontSynthesis::All);
    assert!(is_immutable(&first, &test.default) && is_immutable(&second, &test.default));
    test.fonts.resolve(None, FontWeight::NORMAL, FontStyle::Normal, FontSynthesis::All);
    test.fonts.resolve(None, FontWeight::NORMAL, FontStyle::Italic, FontSynthesis::All);
    assert_eq!(test.platform.created.get(), 2);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, resultsAreCached_forLoadedTypeface
#[test]
fn a_loaded_typeface_is_itself() {
    let test = Fixture::new();
    let loaded = typeface("loaded");
    let family = FontFamily::from_typeface(loaded.clone());
    assert!(is_immutable(&test.resolve(&family), &loaded));
}

/// The sixteen weights the cache holds.
fn sixteen_weights() -> impl Iterator<Item = FontWeight> {
    (1..=16).map(|weight| FontWeight::new(weight * 50))
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, resultsAreEvicted_whenCacheOverfills_cacheSize16
#[test]
fn the_seventeenth_result_evicts_the_first() {
    let test = Fixture::new();
    let resolve = |weight| {
        test.fonts.resolve(None, weight, FontStyle::Normal, FontSynthesis::All);
    };
    resolve(FontWeight::new(1));
    sixteen_weights().take(15).for_each(resolve);
    assert_eq!(test.platform.created.get(), 16);
    // Sixteen fit.
    resolve(FontWeight::new(1));
    assert_eq!(test.platform.created.get(), 16);
    sixteen_weights().skip(15).for_each(resolve);
    resolve(FontWeight::new(50));
    assert_eq!(test.platform.created.get(), 18, "the least recently used went");
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, resultsAreNotEvicted_whenCacheOverfills_ifUsedRecently_cacheSize16
#[test]
fn a_result_used_recently_is_not_evicted() {
    let test = Fixture::new();
    let resolve = |weight| {
        test.fonts.resolve(None, weight, FontStyle::Normal, FontSynthesis::All);
    };
    resolve(FontWeight::new(1));
    for weight in sixteen_weights().take(15) {
        resolve(weight);
    }
    // Used again: the next to go is the first of the others.
    resolve(FontWeight::new(1));
    resolve(FontWeight::new(800));
    let created = test.platform.created.get();
    resolve(FontWeight::new(1));
    assert_eq!(test.platform.created.get(), created);
    resolve(FontWeight::new(50));
    assert_eq!(test.platform.created.get(), created + 1);
}

/// What `font` (regular 400 unless said) resolves to for a query.
fn synthesis_of(
    font_weight: FontWeight,
    font_style: FontStyle,
    query_weight: FontWeight,
    query_style: FontStyle,
    synthesis: FontSynthesis,
) -> (Typeface, Typeface) {
    let test = Fixture::new();
    let loaded = typeface("loaded");
    let font = test.font(
        "font",
        FontLoadingStrategy::Blocking,
        Ok(Some(loaded.clone())),
        font_weight,
        font_style,
    );
    let result =
        test.fonts.resolve(Some(&font.to_font_family()), query_weight, query_style, synthesis);
    (result.value(), loaded)
}

fn synthesized(typeface: &Typeface) -> Option<&Synthesized> {
    typeface.downcast_ref::<Synthesized>()
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, fontSynthesisDefault_synthesizeTheFontToItalicBold
#[test]
fn all_synthesis_fakes_italic_and_bold() {
    let (result, _) = synthesis_of(
        FontWeight::NORMAL,
        FontStyle::Normal,
        FontWeight::BOLD,
        FontStyle::Italic,
        FontSynthesis::All,
    );
    assert_eq!(synthesized(&result), Some(&Synthesized { weight: 700, italic: true }));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, fontSynthesisStyle_synthesizeTheFontToItalic
#[test]
fn style_synthesis_fakes_italic_only() {
    let (result, _) = synthesis_of(
        FontWeight::NORMAL,
        FontStyle::Normal,
        FontWeight::BOLD,
        FontStyle::Italic,
        FontSynthesis::Style,
    );
    assert_eq!(synthesized(&result), Some(&Synthesized { weight: 400, italic: true }));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, fontSynthesisWeight_synthesizeTheFontToBold
#[test]
fn weight_synthesis_fakes_bold_only() {
    let (result, _) = synthesis_of(
        FontWeight::NORMAL,
        FontStyle::Normal,
        FontWeight::BOLD,
        FontStyle::Italic,
        FontSynthesis::Weight,
    );
    assert_eq!(synthesized(&result), Some(&Synthesized { weight: 700, italic: false }));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, fontSynthesisStyle_forMatchingItalicDoesNotSynthesize
#[test]
fn an_italic_font_asked_italic_is_not_faked() {
    let (result, loaded) = synthesis_of(
        FontWeight::W700,
        FontStyle::Italic,
        FontWeight::W700,
        FontStyle::Italic,
        FontSynthesis::Style,
    );
    assert_eq!(result, loaded);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, fontSynthesisAll_doesNotSynthesizeIfFontIsTheSame_beforeApi28
#[test]
fn a_font_as_asked_is_not_faked() {
    let (result, loaded) = synthesis_of(
        FontWeight::W700,
        FontStyle::Italic,
        FontWeight::W700,
        FontStyle::Italic,
        FontSynthesis::All,
    );
    assert_eq!(result, loaded);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, fontSynthesisNone_doesNotSynthesize
#[test]
fn no_synthesis_fakes_nothing() {
    let (result, loaded) = synthesis_of(
        FontWeight::NORMAL,
        FontStyle::Normal,
        FontWeight::BOLD,
        FontStyle::Italic,
        FontSynthesis::None,
    );
    assert_eq!(result, loaded);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/font/FontFamilyResolverImplTest.kt, fontSynthesisWeight_doesNotSynthesizeIfRequestedWeightIsLessThan600
#[test]
fn a_weight_below_600_is_not_faked() {
    let (result, loaded) = synthesis_of(
        FontWeight::NORMAL,
        FontStyle::Normal,
        FontWeight::W500,
        FontStyle::Normal,
        FontSynthesis::Weight,
    );
    assert_eq!(result, loaded);
    // At 600 it is.
    let (result, _) = synthesis_of(
        FontWeight::NORMAL,
        FontStyle::Normal,
        FontWeight::W600,
        FontStyle::Normal,
        FontSynthesis::Weight,
    );
    assert_eq!(synthesized(&result), Some(&Synthesized { weight: 600, italic: false }));
}

#[test]
fn a_font_already_bold_is_not_faked_bolder() {
    let (result, loaded) = synthesis_of(
        FontWeight::W600,
        FontStyle::Normal,
        FontWeight::W700,
        FontStyle::Normal,
        FontSynthesis::Weight,
    );
    assert_eq!(result, loaded);
}

// ---- ParagraphIntrinsicsAsyncTypefaceTest: a text's staleness --------------------------------

fn with_family(family: &FontFamily) -> TextStyle {
    TextStyle { font_family: Some(family.clone()), ..TextStyle::DEFAULT }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicsAsyncTypefaceTest.kt, hasStaleResolvedFonts_falseByDefault
#[test]
fn a_text_in_an_async_font_is_not_stale_before_it_arrives() {
    let test = Fixture::new();
    let family = test.asynchronous("async").to_font_family();
    let mut resolved = ResolvedFonts::default();
    assert_eq!(resolved.resolve(&test.fonts, &with_family(&family)), Some(test.default.clone()));
    assert!(!resolved.is_stale());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicsAsyncTypefaceTest.kt, hasStaleResolvedFonts_trueOnTypefaceUpdate_mainTypeface
#[test]
fn a_text_in_an_async_font_is_stale_when_it_arrives() {
    let test = Fixture::new();
    let family = test.asynchronous("async").to_font_family();
    let mut resolved = ResolvedFonts::default();
    resolved.resolve(&test.fonts, &with_family(&family));
    test.requests.complete("async", &typeface("arrived"));
    test.run_current();
    assert!(resolved.is_stale());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicsAsyncTypefaceTest.kt, hasStaleResolvedFonts_trueOnTypefaceUpdate_spanTypeface
#[test]
fn a_spans_async_font_arriving_makes_it_stale_too() {
    let test = Fixture::new();
    let plain = test.blocking("plain", &typeface("plain")).to_font_family();
    let family = test.asynchronous("async").to_font_family();
    let mut resolved = ResolvedFonts::default();
    resolved.resolve(&test.fonts, &with_family(&plain));
    resolved.resolve(&test.fonts, &with_family(&family));
    assert!(!resolved.is_stale());
    test.requests.complete("async", &typeface("arrived"));
    test.run_current();
    assert!(resolved.is_stale());
}

#[test]
fn the_platforms_own_families_resolve_nothing_for_a_paragraph() {
    let test = Fixture::new();
    let mut resolved = ResolvedFonts::default();
    assert_eq!(resolved.resolve(&test.fonts, &TextStyle::DEFAULT), None);
    let serif = TextStyle { font_family: Some(FontFamily::Serif), ..TextStyle::DEFAULT };
    assert_eq!(resolved.resolve(&test.fonts, &serif), None);
    assert!(!resolved.is_stale());
}
