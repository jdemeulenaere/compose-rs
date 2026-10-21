//! compose-rs on desktop (docs/07-platforms.md, "Desktop"): `application` runs an application
//! of `Window`s, as Compose Desktop's `application { Window(…) }`, and `run_app` one window;
//! everything runs on the main thread: events, recomposition, layout, drawing with Skia, and
//! presenting. Windows sleep until there is a reason to draw.
//!
//! Frames are rendered on Skia's CPU raster backend and presented through `softbuffer`; GPU
//! surfaces are a later step (D93).
//!
//! Environment variables: `COMPOSE_SCREENSHOT=path.png` writes the first window's last frame
//! when the app exits; `COMPOSE_FRAME_STATS=1` prints each frame's timings;
//! `COMPOSE_EXIT_AFTER_FRAMES=n` exits after `n` frames, or once idle (for smoke tests).

#![forbid(unsafe_code)]

extern crate compose_runtime as compose;

mod accessibility;
mod application;
mod input;

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use application::{
    Application, Window, WindowApplier, WindowHandle, WindowSize, WindowState, exit_application,
    remember_window_state,
};
use compose_skia::{RasterRenderer, SkiaTextMeasurer};
pub use compose_ui::{Color, LayoutDirection};
use compose_ui::{Density, IntSize, Renderer, UiHost};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

/// What wakes the event loop besides the windows' own events.
enum UserEvent {
    /// A task woke, from any thread, or the application's state changed.
    Wake,
    /// An assistive technology asked for a window's tree or an action.
    Accessibility(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> UserEvent {
        UserEvent::Accessibility(event)
    }
}

/// How `run_app`'s window opens, as Compose Desktop's `Window(…)` parameters.
#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub title: String,
    pub size: WindowSize,
    /// What's drawn under the content.
    pub background: Color,
    pub resizable: bool,
    /// The content's layout direction (`LOCAL_LAYOUT_DIRECTION` at the root): right-to-left
    /// for an Arabic or Hebrew interface.
    pub layout_direction: compose_ui::LayoutDirection,
}

impl Default for WindowOptions {
    fn default() -> Self {
        WindowOptions {
            title: "compose-rs".into(),
            size: WindowSize { width: 800.0, height: 600.0 },
            background: Color::WHITE,
            resizable: true,
            layout_direction: compose_ui::LayoutDirection::Ltr,
        }
    }
}

/// The system clipboard, through `arboard`; a clipboard in memory if the system's can't be
/// opened.
struct SystemClipboard {
    system: Option<arboard::Clipboard>,
    fallback: compose_ui::MemoryClipboard,
}

impl SystemClipboard {
    fn new() -> SystemClipboard {
        SystemClipboard { system: arboard::Clipboard::new().ok(), fallback: Default::default() }
    }
}

impl compose_ui::Clipboard for SystemClipboard {
    fn text(&mut self) -> Option<String> {
        match &mut self.system {
            Some(system) => system.get_text().ok(),
            None => self.fallback.text(),
        }
    }

    fn set_text(&mut self, text: &str) {
        match &mut self.system {
            Some(system) => {
                let _ = system.set_text(text);
            }
            None => self.fallback.set_text(text),
        }
    }
}

/// Timings of one frame, for `COMPOSE_FRAME_STATS`.
#[derive(Clone, Copy, Debug, Default)]
struct FrameTimes {
    compose: Duration,
    render: Duration,
    present: Duration,
    /// From the first input event handled since the last frame to the present.
    input_to_present: Option<Duration>,
}

/// An open window: its winit window, its composition's host, and what draws and feeds it.
struct Shown {
    handle: Rc<WindowHandle>,
    window: Arc<winit::window::Window>,
    surface: softbuffer::Surface<Arc<winit::window::Window>, Arc<winit::window::Window>>,
    host: UiHost,
    renderer: RasterRenderer,
    input: input::InputState,
    /// Where the input method's candidate window was last placed.
    ime_area: Option<compose_ui::Rect>,
    /// The bridge to screen readers, which asks for the tree only while one is on.
    accessibility: accesskit_winit::Adapter,
    /// The focused node the accessibility tree last had.
    accessibility_focus: Option<compose_ui::NodeId>,
    /// The mouse pointer's look last set.
    pointer_icon: compose_ui::PointerIcon,
    /// The properties last applied to the winit window.
    title: String,
    resizable: bool,
    size: WindowSize,
    frames: u64,
    last_frame: Instant,
}

struct App {
    application: Application,
    windows: HashMap<WindowId, Shown>,
    /// The order windows were opened in (the first is the one screenshots show).
    opened: Vec<WindowId>,
    stats: bool,
    frames: u64,
    exit_after: Option<u64>,
    screenshot: Option<std::path::PathBuf>,
    first_input: Option<Instant>,
    /// Wakes the loop from any thread.
    proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    /// The display's refresh interval: animation frames come at most this often.
    frame_interval: Duration,
    /// The windows have been opened once.
    started: bool,
}

impl App {
    /// Opens the windows the application's composition has and closes the ones it lost; exits
    /// when none is left.
    fn sync_windows(&mut self, event_loop: &ActiveEventLoop) {
        let handles = self.application.windows();
        if handles.is_empty() {
            event_loop.exit();
            return;
        }
        // Gone or hidden: closed.
        let gone: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, shown)| {
                !shown.handle.is_visible()
                    || !handles.iter().any(|handle| Rc::ptr_eq(handle, &shown.handle))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.close(id);
        }
        for handle in handles {
            let shown = self.windows.values().any(|shown| Rc::ptr_eq(&shown.handle, &handle));
            if !shown && handle.is_visible() {
                self.open(event_loop, handle);
            }
        }
        // Properties the application changed.
        for shown in self.windows.values_mut() {
            let props = shown.handle.props.borrow();
            if props.title != shown.title {
                shown.title = props.title.clone();
                shown.window.set_title(&props.title);
            }
            if props.resizable != shown.resizable {
                shown.resizable = props.resizable;
                shown.window.set_resizable(props.resizable);
            }
            drop(props);
            let size = shown.handle.window_state().size();
            if size != shown.size {
                shown.size = size;
                let _ = shown
                    .window
                    .request_inner_size(winit::dpi::LogicalSize::new(size.width, size.height));
            }
        }
    }

    fn close(&mut self, id: WindowId) {
        if let Some(mut shown) = self.windows.remove(&id) {
            if self.opened.first() == Some(&id) {
                self.write_screenshot_of(&mut shown);
            }
            self.opened.retain(|opened| *opened != id);
            // The composition goes before the window, on this thread.
            drop(shown);
        }
    }

    fn open(&mut self, event_loop: &ActiveEventLoop, handle: Rc<WindowHandle>) {
        let props = handle.props.borrow();
        let size = handle.window_state().size();
        let attributes = winit::window::Window::default_attributes()
            .with_title(props.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(size.width, size.height))
            .with_resizable(props.resizable)
            // Shown once the accessibility adapter is attached, as AccessKit asks.
            .with_visible(false);
        let window =
            Arc::new(event_loop.create_window(attributes).expect("compose-desktop: a window"));
        let accessibility = accesskit_winit::Adapter::with_event_loop_proxy(
            event_loop,
            &window,
            self.proxy.clone(),
        );
        window.set_visible(true);
        window.set_ime_allowed(true);
        let context = softbuffer::Context::new(window.clone()).expect("compose-desktop: a display");
        let surface =
            softbuffer::Surface::new(&context, window.clone()).expect("compose-desktop: a surface");
        let physical = window.inner_size();
        let density = Density::new(window.scale_factor() as f32);
        let mut host = UiHost::new(
            IntSize::new(physical.width as i32, physical.height as i32),
            density,
            Box::new(SkiaTextMeasurer::new()),
        );
        host.with_tree(|tree| tree.set_clipboard(Box::new(SystemClipboard::new())));
        host.set_layout_direction(props.layout_direction);
        let waker = window.clone();
        host.set_frame_waker(move || waker.request_redraw());
        if !self.started {
            let refresh = window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .filter(|millihertz| *millihertz > 0);
            self.frame_interval = refresh.map_or(Duration::from_micros(16_667), |millihertz| {
                Duration::from_secs_f64(1000.0 / f64::from(millihertz))
            });
        }
        let content = handle.clone();
        host.set_content(move || content.compose_content());
        let renderer = RasterRenderer::new(physical.width as i32, physical.height as i32);
        window.request_redraw();
        let (title, resizable) = (props.title.clone(), props.resizable);
        drop(props);
        let id = window.id();
        self.opened.push(id);
        self.windows.insert(
            id,
            Shown {
                handle,
                window,
                surface,
                host,
                renderer,
                input: input::InputState::default(),
                ime_area: None,
                accessibility,
                accessibility_focus: None,
                pointer_icon: compose_ui::PointerIcon::Default,
                title,
                resizable,
                size,
                frames: 0,
                last_frame: Instant::now(),
            },
        );
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        let Some(shown) = self.windows.get_mut(&id) else { return };
        let start = Instant::now();
        shown.last_frame = start;
        let stats = shown.host.frame();
        let composed = Instant::now();
        let size = shown.window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        let resized = shown.renderer.size() != (size.width as i32, size.height as i32);
        shown.renderer.resize(size.width as i32, size.height as i32);
        // Render only what changed (or a new size); present either way (the system may have
        // asked for the window's content again).
        if stats.drew || resized || shown.frames == 0 {
            let background = shown.handle.props.borrow().background;
            let renderer = &mut shown.renderer;
            shown.host.with_tree(|tree| {
                renderer.render(tree.display_list(), background, 1.0, tree.text_measurer_ref())
            });
        }
        let rendered = Instant::now();
        if shown.surface.resize(width, height).is_ok()
            && let Ok(mut buffer) = shown.surface.buffer_mut()
        {
            // Straight from the raster surface into the window's buffer, allocating nothing.
            shown.renderer.copy_pixels_to(&mut buffer);
            for pixel in buffer.iter_mut() {
                // softbuffer takes 0RGB.
                *pixel &= 0x00FF_FFFF;
            }
            let _ = buffer.present();
        }
        let presented = Instant::now();
        // The input method's candidate window goes at the focused field's cursor.
        let area = shown.host.text_input_area();
        if area != shown.ime_area {
            shown.ime_area = area;
            if let Some(area) = area {
                shown.window.set_ime_cursor_area(
                    winit::dpi::PhysicalPosition::new(area.left as f64, area.top as f64),
                    winit::dpi::PhysicalSize::new(
                        (area.right - area.left).max(1.0) as f64,
                        (area.bottom - area.top).max(1.0) as f64,
                    ),
                );
            }
        }
        // Screen readers see what changed (only while one is on: nothing is built otherwise).
        let focus = shown.host.with_tree(|tree| tree.focused());
        if stats.recomposed || stats.measured || stats.drew || focus != shown.accessibility_focus {
            shown.accessibility_focus = focus;
            let (host, title) = (&shown.host, &shown.title);
            shown.accessibility.update_if_active(|| accessibility::tree_update(host, title));
        }
        shown.frames += 1;
        self.frames += 1;
        if self.stats {
            let times = FrameTimes {
                compose: composed - start,
                render: rendered - composed,
                present: presented - rendered,
                input_to_present: self.first_input.take().map(|input| presented - input),
            };
            eprintln!(
                "frame {}: recomposed {} ({} bodies), measured {}, drew {}; compose {:?}, render {:?}, present {:?}, input to present {:?}",
                self.frames,
                stats.recomposed,
                stats.bodies_run,
                stats.measured,
                stats.drew,
                times.compose,
                times.render,
                times.present,
                times.input_to_present
            );
        } else {
            self.first_input = None;
        }
        // Effects that wrote state during the frame ask for the next one (animation frames
        // are paced by `about_to_wait`).
        if shown.host.has_pending_work() {
            shown.window.request_redraw();
        }
        if self.exit_after.is_some_and(|limit| self.frames >= limit) {
            self.exit(event_loop);
        }
    }

    fn write_screenshot_of(&self, shown: &mut Shown) {
        if let Some(path) = &self.screenshot
            && let Err(error) = std::fs::write(path, shown.renderer.png())
        {
            eprintln!("compose-desktop: writing the screenshot to {}: {error}", path.display());
        }
    }

    /// Closes every window and ends the loop.
    fn exit(&mut self, event_loop: &ActiveEventLoop) {
        let opened = self.opened.clone();
        for id in opened {
            self.close(id);
        }
        event_loop.exit();
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.started {
            self.sync_windows(event_loop);
            self.started = true;
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(shown) = self.windows.get_mut(&id) {
            shown.accessibility.process_event(&shown.window, &event);
        }
        match event {
            // The application decides: its `on_close_request` removes the window, or not.
            WindowEvent::CloseRequested => {
                if let Some(shown) = self.windows.get(&id) {
                    shown.handle.request_close();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop, id),
            WindowEvent::Resized(size) => {
                if let Some(shown) = self.windows.get_mut(&id) {
                    shown.host.resize(IntSize::new(size.width as i32, size.height as i32));
                    // The window's state follows the user's resize.
                    let scale = shown.window.scale_factor() as f32;
                    let logical = WindowSize {
                        width: size.width as f32 / scale,
                        height: size.height as f32 / scale,
                    };
                    if (logical.width - shown.size.width).abs() >= 0.5
                        || (logical.height - shown.size.height).abs() >= 0.5
                    {
                        shown.size = logical;
                        shown.handle.window_state().set_size(logical);
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(shown) = self.windows.get_mut(&id) {
                    shown.host.set_density(Density::new(scale_factor as f32));
                }
            }
            event => {
                let Some(shown) = self.windows.get_mut(&id) else { return };
                match input::handle(&mut shown.input, &mut shown.host, &event) {
                    input::Outcome::Close => shown.handle.request_close(),
                    // Quitting asks every window, so each can keep the application open.
                    input::Outcome::Quit => {
                        for shown in self.windows.values() {
                            shown.handle.request_close();
                        }
                    }
                    input::Outcome::Handled => {
                        self.first_input.get_or_insert_with(Instant::now);
                        // The pointer's look where it now is.
                        let icon = shown.host.pointer_icon();
                        if icon != shown.pointer_icon {
                            shown.pointer_icon = icon;
                            shown.window.set_cursor(match icon {
                                compose_ui::PointerIcon::Default => {
                                    winit::window::CursorIcon::Default
                                }
                                compose_ui::PointerIcon::Text => winit::window::CursorIcon::Text,
                                compose_ui::PointerIcon::Hand => winit::window::CursorIcon::Pointer,
                                compose_ui::PointerIcon::Crosshair => {
                                    winit::window::CursorIcon::Crosshair
                                }
                            });
                        }
                    }
                    input::Outcome::Ignored => {}
                }
            }
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        let UserEvent::Accessibility(event) = event else { return };
        let Some(shown) = self.windows.get_mut(&event.window_id) else { return };
        match event.window_event {
            accesskit_winit::WindowEvent::InitialTreeRequested => {
                let (host, title) = (&shown.host, &shown.title);
                shown.accessibility.update_if_active(|| accessibility::tree_update(host, title));
            }
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                if accessibility::perform(&mut shown.host, &request) {
                    shown.window.request_redraw();
                }
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if !self.started {
            return;
        }
        // A smoke test exits once idle after its frames.
        let idle = self.windows.values().all(|shown| !shown.host.needs_frame());
        if self.exit_after.is_some() && self.frames >= 1 && idle {
            self.exit(event_loop);
            return;
        }
        // Tasks whose `delay` ended, and the ones woken meanwhile, take their step; their state
        // writes ask for a frame.
        compose_runtime::executor::fire_timers();
        compose_runtime::executor::run_ready();
        if application::take_exit_request() {
            self.exit(event_loop);
            return;
        }
        // The application's state changed: its windows come and go.
        if self.application.recompose() {
            self.sync_windows(event_loop);
        }
        let now = Instant::now();
        let mut next_frame = None;
        for shown in self.windows.values() {
            // A field committed or dropped its composition itself (a click, a focus change): the
            // input method starts over, so it doesn't commit the text again.
            if shown.host.take_input_method_reset() {
                shown.window.set_ime_allowed(false);
                shown.window.set_ime_allowed(true);
            }
            if shown.host.has_pending_work() {
                shown.window.request_redraw();
            } else if compose_runtime::executor::has_frame_awaiters() {
                // An animation: its next frame a refresh interval after the last one.
                let due = shown.last_frame + self.frame_interval;
                if now >= due {
                    shown.window.request_redraw();
                } else {
                    next_frame = Some(next_frame.map_or(due, |next: Instant| next.min(due)));
                }
            }
        }
        // Sleep until an event, a redraw request, the next animation frame or the next
        // `delay`'s end: an idle application costs nothing.
        let timer = compose_runtime::executor::next_timer().map(|wait| now + wait);
        match [next_frame, timer].into_iter().flatten().min() {
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        let opened = self.opened.clone();
        for id in opened {
            self.close(id);
        }
    }
}

/// Runs an application of windows, as Compose Desktop's `application { … }`: `content` holds
/// `Window`s, each opened while it's in the composition; it returns once none is left or
/// `exit_application` is called. On the calling thread, which must be the main thread on macOS.
pub fn application(content: impl Fn() + 'static) {
    let event_loop =
        EventLoop::<UserEvent>::with_user_event().build().expect("compose-desktop: an event loop");
    let proxy = event_loop.create_proxy();
    // A task woken (from any thread), the application's state changed, or an exit asked for:
    // a turn of the loop (`about_to_wait`).
    let wake = std::sync::Mutex::new(proxy.clone());
    compose_runtime::executor::set_wake_handler(move || {
        let _ = wake.lock().map(|proxy| proxy.send_event(UserEvent::Wake));
    });
    let application = Application::new(content);
    let wake = proxy.clone();
    application.set_wake(move || {
        let _ = wake.send_event(UserEvent::Wake);
    });
    let wake = proxy.clone();
    application::set_wake(move || {
        let _ = wake.send_event(UserEvent::Wake);
    });
    let mut app = App {
        application,
        windows: HashMap::new(),
        opened: Vec::new(),
        stats: std::env::var_os("COMPOSE_FRAME_STATS").is_some(),
        frames: 0,
        exit_after: std::env::var("COMPOSE_EXIT_AFTER_FRAMES")
            .ok()
            .and_then(|frames| frames.parse().ok()),
        screenshot: std::env::var_os("COMPOSE_SCREENSHOT").map(Into::into),
        first_input: None,
        proxy,
        frame_interval: Duration::from_micros(16_667),
        started: false,
    };
    event_loop.run_app(&mut app).expect("compose-desktop: the event loop");
}

/// Opens one window showing `content` and runs until it's closed (the close button, Cmd+W or
/// Cmd+Q, Ctrl+W on other systems): `application` with one `Window` whose close request exits.
pub fn run_app(options: WindowOptions, content: impl Fn() + 'static) {
    let content = compose_runtime::callback(content);
    application(move || {
        let state = remember_window_state(options.size);
        Window(
            compose_runtime::callback(exit_application),
            state,
            true,
            options.title.clone(),
            options.resizable,
            options.background,
            options.layout_direction,
            content.clone(),
        );
    });
}
