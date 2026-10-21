//! `application { Window(…) }`, as Compose Desktop's: a composition of its own whose nodes are
//! windows. Each `Window` composable is a node of the application's composition; its content is
//! composed in the window's own `UiHost`, and reads the application's state like any content.
//!
//! The event loop (`lib.rs`) opens a window for each `Window` node, closes the ones that leave
//! the composition, and exits once none is left or `exit_application` is called. This module
//! has no winit in it, so tests drive it headless.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::{
    Applier, Callback, ComposeArg, Composition, NodeRef, OwnedState, SideEffect, composable,
    current, key_of, mutable_state_of, remember,
};
use compose_ui::{Color, LayoutDirection};

/// A window's size, in `Dp`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WindowSize {
    pub width: f32,
    pub height: f32,
}

impl From<(u32, u32)> for WindowSize {
    fn from((width, height): (u32, u32)) -> WindowSize {
        WindowSize { width: width as f32, height: height as f32 }
    }
}

impl ComposeArg for WindowSize {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A window's state, as Compose Desktop's `WindowState`: its size, which the window follows
/// when the application sets it, and which follows the window when the user resizes it.
#[derive(Clone)]
pub struct WindowState {
    size: Rc<OwnedState<WindowSize>>,
}

impl WindowState {
    pub fn new(size: WindowSize) -> WindowState {
        WindowState { size: Rc::new(mutable_state_of(size)) }
    }

    /// The window's size (a state read).
    pub fn size(&self) -> WindowSize {
        self.size.get()
    }

    /// Resizes the window.
    pub fn set_size(&self, size: WindowSize) {
        self.size.set(size);
    }
}

impl ComposeArg for WindowState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.size, &other.size)
    }
}

/// A window state remembered at its call site, as `rememberWindowState(size = …)`.
#[composable(non_restartable)]
pub fn remember_window_state(
    #[default(crate::WindowSize { width: 800.0, height: 600.0 })] size: WindowSize,
) -> WindowState {
    remember(|| WindowState::new(size))
}

/// What a `Window` node shows: written after each application pass (in a side effect), read by
/// the event loop.
pub(crate) struct WindowProps {
    pub(crate) title: String,
    pub(crate) visible: bool,
    pub(crate) resizable: bool,
    pub(crate) background: Color,
    pub(crate) layout_direction: LayoutDirection,
    pub(crate) on_close_request: Callback,
}

/// A `Window` node: its properties, its state, and its content, which its own composition shows.
pub struct WindowHandle {
    pub(crate) id: u64,
    pub(crate) props: RefCell<WindowProps>,
    pub(crate) state: RefCell<WindowState>,
    content: RefCell<Callback>,
    /// Bumped when the content changes: the window's composition reads it, so it recomposes.
    content_version: OwnedState<u64>,
}

impl WindowHandle {
    /// The window's content, as its composition shows it: recomposed when the application
    /// gives it new content.
    pub fn compose_content(&self) {
        self.content_version.get();
        let content = self.content.borrow().clone();
        content();
    }

    /// Asks the application to close the window: its `on_close_request`.
    pub fn request_close(&self) {
        let on_close_request = self.props.borrow().on_close_request.clone();
        on_close_request();
    }

    pub fn title(&self) -> String {
        self.props.borrow().title.clone()
    }

    pub fn is_visible(&self) -> bool {
        self.props.borrow().visible
    }

    pub fn window_state(&self) -> WindowState {
        self.state.borrow().clone()
    }
}

thread_local! {
    static NEXT_WINDOW: Cell<u64> = const { Cell::new(1) };
    /// `exit_application` was called.
    static EXIT_REQUESTED: Cell<bool> = const { Cell::new(false) };
    /// Wakes the event loop (set by `application`).
    static WAKE: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Ends the application, as Compose Desktop's `ApplicationScope.exitApplication`: its windows
/// close and `application` returns.
pub fn exit_application() {
    EXIT_REQUESTED.with(|exit| exit.set(true));
    let wake = WAKE.with(|wake| wake.borrow().clone());
    if let Some(wake) = wake {
        wake();
    }
}

pub(crate) fn take_exit_request() -> bool {
    EXIT_REQUESTED.with(|exit| exit.replace(false))
}

pub(crate) fn set_wake(wake: impl Fn() + 'static) {
    WAKE.with(|slot| *slot.borrow_mut() = Some(Rc::new(wake)));
}

/// A window, as Compose Desktop's `Window(onCloseRequest, state, visible, title, resizable)`:
/// shown while it's in the application's composition (and `visible`), its `content` composed
/// in a composition of its own. Closing it (its close button, Cmd+W or Ctrl+W) calls
/// `on_close_request`, which removes it from the composition or not (asking to save, say).
/// `background` (white) and `layout_direction` (the window's `LOCAL_LAYOUT_DIRECTION`) are
/// compose-rs's.
#[composable]
#[allow(non_snake_case)]
pub fn Window(
    on_close_request: impl Fn() + 'static,
    #[default(crate::remember_window_state(crate::WindowSize { width: 800.0, height: 600.0 }))]
    state: WindowState,
    #[default(true)] visible: bool,
    #[default("Untitled".to_string())] title: String,
    #[default(true)] resizable: bool,
    #[default(crate::Color::WHITE)] background: Color,
    #[default(crate::LayoutDirection::Ltr)] layout_direction: LayoutDirection,
    content: impl Fn() + 'static,
) {
    let composer = current();
    assert!(
        composer.applier_is::<WindowApplier>(),
        "compose-desktop: a Window is composed in an `application`'s content, not in a window's \
         content nor in other UI"
    );
    let node = composer.start_node(key_of("compose_desktop::Window"), || None::<Rc<WindowHandle>>);
    let handle = composer.with_applier(|applier: &mut WindowApplier| applier.handle(node));
    composer.end_group();
    let Some(handle) = handle else { return };
    let content = compose_runtime::callback(content);
    let on_close_request = compose_runtime::callback(on_close_request);
    // After the pass: what the event loop reads, and the content its composition shows.
    SideEffect(move || {
        *handle.props.borrow_mut() = WindowProps {
            title: title.clone(),
            visible,
            resizable,
            background,
            layout_direction,
            on_close_request: on_close_request.clone(),
        };
        let same_state = handle.state.borrow().same(&state);
        if !same_state {
            *handle.state.borrow_mut() = state.clone();
        }
        let same_content = handle.content.borrow().ptr_eq(&content);
        if !same_content {
            *handle.content.borrow_mut() = content.clone();
            handle.content_version.set(handle.content_version.get() + 1);
        }
    });
}

/// The windows of the application's composition, in order.
#[derive(Default)]
pub struct WindowApplier {
    windows: Rc<RefCell<Vec<Rc<WindowHandle>>>>,
    /// Inserted, not yet released.
    attached: Vec<Rc<WindowHandle>>,
}

/// The root's handle (the windows are its children).
const ROOT: NodeRef = NodeRef::new(0);

impl WindowApplier {
    fn handle(&self, node: NodeRef) -> Option<Rc<WindowHandle>> {
        self.attached.iter().find(|handle| handle.id == node.bits()).cloned()
    }
}

impl Applier for WindowApplier {
    /// A new `Window` node (`None`, made into a handle on insertion).
    type Node = Option<Rc<WindowHandle>>;

    fn root(&self) -> NodeRef {
        ROOT
    }

    fn insert(&mut self, parent: NodeRef, index: usize, _node: Self::Node) -> NodeRef {
        assert!(parent == ROOT, "compose-desktop: a Window holds no windows of the application");
        let id = NEXT_WINDOW.with(|next| next.replace(next.get() + 1));
        let handle = Rc::new(WindowHandle {
            id,
            props: RefCell::new(WindowProps {
                title: String::new(),
                // Shown once its first pass's properties are in.
                visible: false,
                resizable: true,
                background: Color::WHITE,
                layout_direction: LayoutDirection::Ltr,
                on_close_request: compose_runtime::callback(|| {}),
            }),
            state: RefCell::new(WindowState::new(WindowSize { width: 800.0, height: 600.0 })),
            content: RefCell::new(compose_runtime::callback(|| {})),
            content_version: mutable_state_of(0),
        });
        self.attached.push(handle.clone());
        let mut windows = self.windows.borrow_mut();
        let index = index.min(windows.len());
        windows.insert(index, handle);
        NodeRef::new(id)
    }

    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        debug_assert!(parent == ROOT);
        let mut windows = self.windows.borrow_mut();
        let end = (index + count).min(windows.len());
        windows.drain(index.min(end)..end);
    }

    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        debug_assert!(parent == ROOT);
        let mut windows = self.windows.borrow_mut();
        let moved: Vec<_> = windows.drain(from..from + count).collect();
        let to = if to > from { to - count } else { to };
        for (offset, window) in moved.into_iter().enumerate() {
            windows.insert(to + offset, window);
        }
    }

    fn release(&mut self, node: NodeRef) {
        self.attached.retain(|handle| handle.id != node.bits());
        self.windows.borrow_mut().retain(|handle| handle.id != node.bits());
    }
}

/// The application's composition: its windows, recomposed when the state it read changes.
pub struct Application {
    composition: Composition,
    windows: Rc<RefCell<Vec<Rc<WindowHandle>>>>,
}

impl Application {
    /// Composes `content` (its `Window`s) at once.
    pub fn new(content: impl Fn() + 'static) -> Application {
        let applier = WindowApplier::default();
        let windows = applier.windows.clone();
        let composition = Composition::new(applier);
        composition.set_content(content);
        Application { composition, windows }
    }

    /// Calls `wake` when the application's state changes: it recomposes at the loop's next turn.
    pub fn set_wake(&self, wake: impl Fn() + 'static) {
        self.composition.set_frame_requester(wake);
    }

    /// Recomposes if its state changed. Returns whether it did.
    pub fn recompose(&self) -> bool {
        self.composition.has_invalidations() && self.composition.recompose()
    }

    /// Its windows, in composition order.
    pub fn windows(&self) -> Vec<Rc<WindowHandle>> {
        self.windows.borrow().clone()
    }
}
