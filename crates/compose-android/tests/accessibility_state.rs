//! What accessibility services read when they connect (D630), over a model of `Accessibility.kt`
//! and the frame in `ComposeRsView.kt`: a frame that may have changed the semantics
//! (`accessibility::semantics_may_have_changed`, the frame's `SEMANTICS`) marks the nodes read
//! stale, service or not, as Compose's `currentSemanticsNodesInvalidated`; with a service on, it is
//! told once laid out (`TYPE_WINDOW_CONTENT_CHANGED`, a subtree, and each scroll), and a service
//! turning on is told the content. With none on, nothing is read or sent.
//!
//! The bug this fixes: a service connecting after a change (uiautomator's dump, TalkBack started
//! after the app) read the nodes as they were when a service last looked, since a change marked
//! them stale only with a service on at that frame. The navigation benchmark's dump listed the
//! launcher's nodes over the heavy screen.

#![allow(non_snake_case)]

// The call macros name the runtime `compose`.
extern crate compose_runtime as compose;

use std::collections::HashMap;

use compose_android::accessibility::{flag, semantics_may_have_changed, write_tree};
use compose_android::serialize::Writer;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{MutableState, mutable_state_of};
use compose_ui::platform::UiHost;
use compose_ui::*;

// ---- The records, as `Accessibility.kt`'s `refresh` reads them ---------------------------------

#[derive(Clone, Debug)]
struct Record {
    id: i32,
    text: String,
    /// A scrolling container's offset.
    scroll: Option<f32>,
}

fn records(bytes: &[u8]) -> Vec<Record> {
    let mut at = 0;
    let u32_at = |at: &mut usize| {
        let value = u32::from_le_bytes(bytes[*at..*at + 4].try_into().unwrap());
        *at += 4;
        value
    };
    let mut out = Vec::new();
    while at < bytes.len() {
        let id = u32_at(&mut at) as i32;
        u32_at(&mut at);
        at += 16;
        let flags = u32_at(&mut at);
        at += 1;
        let mut strings = Vec::new();
        for _ in 0..5 {
            let length = u32_at(&mut at) as usize;
            strings.push(String::from_utf8(bytes[at..at + length].to_vec()).unwrap());
            at += length;
        }
        if flags & flag::RANGE != 0 {
            at += 12;
        }
        let scroll = (flags & flag::SCROLL != 0).then(|| {
            let value = f32::from_bits(u32_at(&mut at));
            at += 9;
            value
        });
        out.push(Record { id, text: strings.swap_remove(0), scroll });
    }
    out
}

// ---- A model of Accessibility.kt and ComposeRsView.kt's frame ---------------------------------

/// What the services are told.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Event {
    /// `TYPE_WINDOW_CONTENT_CHANGED`, `CONTENT_CHANGE_TYPE_SUBTREE`, from the view.
    ContentChanged,
    /// `TYPE_VIEW_SCROLLED` of a scrolling container, with its delta.
    Scrolled { id: i32, delta: i32 },
}

struct Kotlin {
    host: UiHost,
    /// `AccessibilityManager.isEnabled`: a service (TalkBack, uiautomator's) is connected.
    enabled: bool,
    // Accessibility.kt's state.
    stale: bool,
    nodes: Vec<Record>,
    scrolled_to: HashMap<i32, f32>,
    change_posted: bool,
    // What the services saw, and what it cost.
    events: Vec<Event>,
    /// `ComposeRsNative.semantics` calls: the tree written and read.
    reads: usize,
}

impl Kotlin {
    fn new(content: impl Fn() + 'static) -> Kotlin {
        let host = UiHost::headless(200, 400);
        host.set_content(content);
        let mut kotlin = Kotlin {
            host,
            enabled: false,
            stale: true,
            nodes: Vec::new(),
            scrolled_to: HashMap::new(),
            change_posted: false,
            events: Vec::new(),
            reads: 0,
        };
        kotlin.frames();
        kotlin
    }

    /// `ComposeRsView.frame`, until the content settles: a frame's `SEMANTICS` is told to
    /// `semanticsChanged`, whether or not a service is on.
    fn frames(&mut self) {
        for _ in 0..20 {
            if !self.host.needs_frame() {
                return;
            }
            let stats = self.host.frame();
            if semantics_may_have_changed(&stats) {
                self.semantics_changed();
            }
        }
        panic!("the content doesn't settle");
    }

    /// `Accessibility.semanticsChanged`.
    fn semantics_changed(&mut self) {
        self.stale = true;
        if !self.change_posted && self.enabled {
            self.change_posted = true;
        }
    }

    /// The runnable `semanticsChanged` posted, 100 ms later.
    fn run_posted(&mut self) {
        if self.change_posted {
            self.change_posted = false;
            self.tell();
        }
    }

    /// A service connects: `AccessibilityManager.isEnabled` is true at once (its queries can
    /// come before the state's callback).
    fn connect(&mut self) {
        self.enabled = true;
    }

    /// `onAccessibilityStateChanged(true)`, delivered after the service connected.
    fn on_accessibility_state_changed(&mut self, enabled: bool) {
        if enabled {
            self.turned_on();
        }
    }

    /// The last service disconnects: `onAccessibilityStateChanged(false)` does nothing.
    fn disconnect(&mut self) {
        self.enabled = false;
    }

    /// `Accessibility.turnedOn`.
    fn turned_on(&mut self) {
        self.stale = true;
        self.scrolled_to.clear();
        self.tell();
    }

    /// `Accessibility.tell`.
    fn tell(&mut self) {
        if !self.enabled {
            return;
        }
        self.refresh();
        let mut offsets = HashMap::new();
        for node in &self.nodes {
            let Some(scroll) = node.scroll else { continue };
            offsets.insert(node.id, scroll);
            let Some(&before) = self.scrolled_to.get(&node.id) else { continue };
            if before != scroll {
                self.events.push(Event::Scrolled { id: node.id, delta: (scroll - before) as i32 });
            }
        }
        self.scrolled_to = offsets;
        self.events.push(Event::ContentChanged);
    }

    /// `Accessibility.refresh`.
    fn refresh(&mut self) {
        if !self.stale {
            return;
        }
        self.stale = false;
        self.reads += 1;
        let mut writer = Writer::default();
        write_tree(&self.host.semantics_tree(true), &mut writer);
        self.nodes = records(&writer.bytes);
    }

    /// A service's dump (`createAccessibilityNodeInfo` from the host down): the texts listed.
    fn dump(&mut self) -> Vec<String> {
        assert!(self.enabled, "only a connected service queries");
        self.refresh();
        self.nodes
            .iter()
            .filter(|node| !node.text.is_empty())
            .map(|node| node.text.clone())
            .collect()
    }
}

// ---- The content: a launcher opening a heavy screen, as the navigation benchmark's ------------

fn screens(heavy: MutableState<bool>, scroll: ScrollState) -> impl Fn() + 'static {
    move || {
        if heavy.get() {
            Column!(modifier = Modifier.size((100.dp(), 200.dp())).vertical_scroll(scroll), {
                for row in 0..20 {
                    BasicText!("Heavy row {}", row, modifier = Modifier.height(20.dp()));
                }
            });
        } else {
            Column!({
                BasicText!("Navigation");
                BasicText!("Open", modifier = Modifier.clickable(|| {}));
            });
        }
    }
}

fn launcher(dump: &[String]) -> bool {
    dump.iter().any(|text| text == "Open")
}

fn heavy(dump: &[String]) -> bool {
    dump.iter().any(|text| text == "Heavy row 0")
}

// ---- Scenarios -------------------------------------------------------------------------------

/// The navigation benchmark's dump: a service looked at the launcher and went, the screen
/// changed, and a service connecting reads the heavy screen, even before its state's callback.
#[test]
fn a_service_connecting_after_the_screen_changed_reads_the_new_screen() {
    let screen = mutable_state_of(false);
    let scroll = ScrollState::new(0);
    let mut kotlin = Kotlin::new(screens(screen.handle(), *scroll));
    kotlin.connect();
    kotlin.on_accessibility_state_changed(true);
    let dump = kotlin.dump();
    assert!(launcher(&dump), "{dump:?}");
    kotlin.disconnect();
    let (reads, events) = (kotlin.reads, kotlin.events.len());

    screen.set(true);
    kotlin.frames();
    // With no service on, nothing was read or told.
    assert_eq!((kotlin.reads, kotlin.events.len(), kotlin.change_posted), (reads, events, false));

    // uiautomator's dump queries as it connects.
    kotlin.connect();
    let dump = kotlin.dump();
    assert!(heavy(&dump) && !launcher(&dump), "the launcher's nodes, stale: {dump:?}");
    kotlin.on_accessibility_state_changed(true);
    assert_eq!(kotlin.events.last(), Some(&Event::ContentChanged));
}

/// A service on through the change is told once, after the frames that made it, and reads the
/// new screen.
#[test]
fn a_change_with_a_service_on_is_told_once() {
    let screen = mutable_state_of(false);
    let scroll = ScrollState::new(0);
    let mut kotlin = Kotlin::new(screens(screen.handle(), *scroll));
    kotlin.connect();
    kotlin.on_accessibility_state_changed(true);
    assert!(launcher(&kotlin.dump()));
    kotlin.events.clear();

    screen.set(true);
    kotlin.frames();
    assert!(kotlin.change_posted);
    kotlin.run_posted();
    assert_eq!(kotlin.events, [Event::ContentChanged]);
    let dump = kotlin.dump();
    assert!(heavy(&dump) && !launcher(&dump), "{dump:?}");
}

/// With no service ever on, changes cost a field write: the tree is never written or read, and
/// nothing is sent.
#[test]
fn with_no_service_nothing_is_read_or_told() {
    let screen = mutable_state_of(false);
    let scroll = ScrollState::new(0);
    let mut kotlin = Kotlin::new(screens(screen.handle(), *scroll));
    for step in 0..6 {
        screen.set(step % 2 == 0);
        kotlin.frames();
        scroll.scroll_to(step * 10);
        kotlin.frames();
        kotlin.run_posted();
    }
    assert!(kotlin.stale);
    assert_eq!((kotlin.reads, kotlin.events.len(), kotlin.change_posted), (0, 0, false));
}

/// A scroll with a service on is told with its delta; scrolls while none was on aren't told
/// when one turns on (the offsets are read afresh), only the content.
#[test]
fn scrolls_made_with_no_service_on_are_not_told_on_connecting() {
    let screen = mutable_state_of(true);
    let scroll = ScrollState::new(0);
    let mut kotlin = Kotlin::new(screens(screen.handle(), *scroll));
    kotlin.connect();
    kotlin.on_accessibility_state_changed(true);
    let id = kotlin.nodes.iter().find(|node| node.scroll.is_some()).expect("a scrolling node").id;
    kotlin.events.clear();

    scroll.scroll_to(100);
    kotlin.frames();
    kotlin.run_posted();
    assert_eq!(kotlin.events, [Event::Scrolled { id, delta: 100 }, Event::ContentChanged]);

    kotlin.disconnect();
    scroll.scroll_to(150);
    kotlin.frames();
    kotlin.events.clear();
    kotlin.connect();
    kotlin.on_accessibility_state_changed(true);
    assert_eq!(kotlin.events, [Event::ContentChanged]);

    kotlin.events.clear();
    scroll.scroll_to(200);
    kotlin.frames();
    kotlin.run_posted();
    assert_eq!(kotlin.events, [Event::Scrolled { id, delta: 50 }, Event::ContentChanged]);
}

/// The frame that switches screens may have changed the semantics (the view's `SEMANTICS`), an
/// idle one not; the record written after it lists the new screen's nodes only.
#[test]
fn the_frame_switching_screens_reports_a_semantics_change() {
    let screen = mutable_state_of(false);
    let scroll = ScrollState::new(0);
    let mut host = UiHost::headless(200, 400);
    host.set_content(screens(screen.handle(), *scroll));
    host.settle(10);
    assert!(!semantics_may_have_changed(&host.frame()), "an idle frame");
    screen.set(true);
    assert!(semantics_may_have_changed(&host.frame()));
    host.settle(10);
    let mut writer = Writer::default();
    write_tree(&host.semantics_tree(true), &mut writer);
    let texts: Vec<String> = records(&writer.bytes).into_iter().map(|record| record.text).collect();
    assert!(texts.iter().any(|text| text == "Heavy row 0"), "{texts:?}");
    assert!(!texts.iter().any(|text| text == "Open" || text == "Navigation"), "{texts:?}");
}

/// The model is the Kotlin's: the lines it mirrors are there.
#[test]
fn the_kotlin_is_the_model() {
    let kotlin = |file: &str| {
        let path = format!(
            "{}/../../android/compose-rs/src/main/java/rs/compose/android/{file}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    };
    let accessibility = kotlin("Accessibility.kt");
    for line in [
        "    fun semanticsChanged() {\n        stale = true\n        if (!changePosted && manager.isEnabled) {",
        "manager.addAccessibilityStateChangeListener(this)",
        "manager.addTouchExplorationStateChangeListener(this)",
        "    override fun onAccessibilityStateChanged(enabled: Boolean) {\n        if (enabled) turnedOn()",
        "    private fun turnedOn() {\n        stale = true\n        scrolledTo = SparseArray()\n        tell()",
        "    private fun tell() {\n        if (!manager.isEnabled) return",
        "event.contentChangeTypes = AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE",
    ] {
        assert!(accessibility.contains(line), "Accessibility.kt lacks {line:?}");
    }
    let view = kotlin("ComposeRsView.kt");
    for line in [
        "if (result and Frame.SEMANTICS != 0L) accessibility.semanticsChanged()",
        "accessibility.attach()",
        "accessibility.detach()",
    ] {
        assert!(view.contains(line), "ComposeRsView.kt lacks {line:?}");
    }
}
