//! Review of D281: the services' locals read the composing tree's services in subcompositions
//! (lazy items, `BoxWithConstraints`) and overlays (`Dialog`, `Popup`).

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Dialog, IntSize, LOCAL_CLIPBOARD_MANAGER, LOCAL_URI_HANDLER, Popup, UnitExt, UriHandler,
    platform::UiHost,
};

#[derive(Default)]
struct Fake {
    opened: RefCell<Vec<String>>,
}

impl UriHandler for Fake {
    fn open_uri(&self, uri: &str) {
        self.opened.borrow_mut().push(uri.to_string());
    }
}

#[composable]
fn Link(uri: &'static str) {
    let handler = LOCAL_URI_HANDLER.current();
    Box!(modifier = Modifier.size(20.dp()).test_tag(uri).clickable(move || handler.open_uri(uri)));
}

fn host_with(fake: &Rc<Fake>) -> UiHost {
    let host = UiHost::headless(400, 400);
    let handler: Rc<dyn UriHandler> = fake.clone();
    host.with_tree(|tree| tree.set_uri_handler(handler));
    host
}

#[test]
fn lazy_items_reach_the_hosts_uri_handler() {
    let fake = Rc::new(Fake::default());
    let mut ui = ComposeTestRule::with_host(host_with(&fake), || {
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            scope.item(|| Link("lazy"));
        });
    });
    ui.on_node_with_tag("lazy").perform_click();
    assert_eq!(*fake.opened.borrow(), ["lazy"]);
}

#[test]
fn box_with_constraints_reaches_the_hosts_uri_handler() {
    let fake = Rc::new(Fake::default());
    let mut ui = ComposeTestRule::with_host(host_with(&fake), || {
        BoxWithConstraints!(
            modifier = Modifier.fill_max_size(),
            |_scope: BoxWithConstraintsScope| {
                Link("bwc");
            }
        );
    });
    ui.on_node_with_tag("bwc").perform_click();
    assert_eq!(*fake.opened.borrow(), ["bwc"]);
}

#[test]
fn popups_reach_the_hosts_uri_handler() {
    let fake = Rc::new(Fake::default());
    let mut ui = ComposeTestRule::with_host(host_with(&fake), || {
        Box!(modifier = Modifier.fill_max_size(), {
            Popup!(on_dismiss_request = || {}, {
                Link("popup");
            });
        });
    });
    ui.on_node_with_tag("popup").perform_click();
    assert_eq!(*fake.opened.borrow(), ["popup"]);
}

#[test]
fn dialogs_reach_the_hosts_uri_handler() {
    let fake = Rc::new(Fake::default());
    let mut ui = ComposeTestRule::with_host(host_with(&fake), || {
        Dialog!(on_dismiss_request = || {}, {
            Link("dialog");
        });
    });
    ui.on_node_with_tag("dialog").perform_click();
    assert_eq!(*fake.opened.borrow(), ["dialog"]);
}

#[test]
fn a_lazy_items_clipboard_is_the_trees() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            scope.item(|| {
                let clipboard = LOCAL_CLIPBOARD_MANAGER.current();
                Box!(
                    modifier = Modifier
                        .size(20.dp())
                        .test_tag("copy")
                        .clickable(move || clipboard.set_text("from lazy"))
                );
            });
        });
    });
    ui.on_node_with_tag("copy").perform_click();
    let clipboard = ui.host().with_tree(|tree| tree.clipboard());
    assert_eq!(clipboard.get_text().as_deref(), Some("from lazy"));
}

/// Records, per composed item, whether its handler is the host's fake (opening records it).
#[composable]
fn PrefetchedItem(index: usize) {
    let handler = LOCAL_URI_HANDLER.current();
    // Whether `handler` is the host's: open a sentinel and see if the shared fake got it.
    handler.open_uri(&format!("probe {index}"));
    Box!(modifier = Modifier.size((100.dp(), 20.dp())).test_tag(format!("row {index}")));
}

#[test]
fn prefetched_items_reach_the_hosts_uri_handler() {
    let fake = Rc::new(Fake::default());
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::with_host(host_with(&fake), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, PrefetchedItem);
            }
        );
    });
    state.dispatch_raw_delta(30.0);
    ui.wait_for_idle();
    let before = fake.opened.borrow().len();
    let prefetched =
        ui.host().prefetch(std::time::Instant::now() + std::time::Duration::from_secs(60));
    let after = fake.opened.borrow().len();
    let _ = prefetched;
    assert!(
        after > before,
        "prefetch composed items with another handler (or composed none): {:?}",
        fake.opened.borrow()
    );
}
