//! compose-rs on desktop (docs/07-platforms.md, "Desktop"): `run_app` opens a winit window and
//! runs everything on the main thread: events, recomposition, layout, drawing with Skia, and
//! presenting. The window sleeps until there is a reason to draw.
//!
//! Frames are rendered on Skia's CPU raster backend and presented through `softbuffer`; GPU
//! surfaces are a later step (D93).
//!
//! Environment variables: `COMPOSE_SCREENSHOT=path.png` writes the last frame when the app
//! exits; `COMPOSE_FRAME_STATS=1` prints each frame's timings; `COMPOSE_EXIT_AFTER_FRAMES=n`
//! exits after `n` frames, or once idle (for smoke tests).

#![forbid(unsafe_code)]

mod input;

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use compose_skia::{RasterRenderer, SkiaTextMeasurer};
use compose_ui::{Color, Density, IntSize, Renderer, UiHost};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

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

/// How the window opens, as Compose Desktop's `Window(…)` parameters.
#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub title: String,
    pub size: WindowSize,
    /// What's drawn under the content.
    pub background: Color,
    pub resizable: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        WindowOptions {
            title: "compose-rs".into(),
            size: WindowSize { width: 800.0, height: 600.0 },
            background: Color::WHITE,
            resizable: true,
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

struct Shown {
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    host: UiHost,
    renderer: RasterRenderer,
    input: input::InputState,
    /// Where the input method's candidate window was last placed.
    ime_area: Option<compose_ui::Rect>,
}

struct App<C: Fn() + 'static> {
    options: WindowOptions,
    content: Rc<C>,
    shown: Option<Shown>,
    stats: bool,
    frames: u64,
    exit_after: Option<u64>,
    screenshot: Option<std::path::PathBuf>,
    first_input: Option<Instant>,
    /// Wakes the loop from any thread.
    proxy: winit::event_loop::EventLoopProxy<()>,
    /// The display's refresh interval: animation frames come at most this often.
    frame_interval: Duration,
    last_frame: Instant,
}

impl<C: Fn() + 'static> App<C> {
    fn open(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes()
            .with_title(self.options.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.options.size.width,
                self.options.size.height,
            ))
            .with_resizable(self.options.resizable);
        let window =
            Arc::new(event_loop.create_window(attributes).expect("compose-desktop: a window"));
        window.set_ime_allowed(true);
        let context = softbuffer::Context::new(window.clone()).expect("compose-desktop: a display");
        let surface =
            softbuffer::Surface::new(&context, window.clone()).expect("compose-desktop: a surface");
        let size = window.inner_size();
        let density = Density::new(window.scale_factor() as f32);
        let host = UiHost::new(
            IntSize::new(size.width as i32, size.height as i32),
            density,
            Box::new(SkiaTextMeasurer::new()),
        );
        host.with_tree(|tree| tree.set_clipboard(Box::new(SystemClipboard::new())));
        let waker = window.clone();
        host.set_frame_waker(move || waker.request_redraw());
        // A task woken (from any thread) or waiting for a frame: a turn of the loop, which runs
        // it or paces the frame (`about_to_wait`).
        let proxy = std::sync::Mutex::new(self.proxy.clone());
        compose_runtime::executor::set_wake_handler(move || {
            let _ = proxy.lock().map(|proxy| proxy.send_event(()));
        });
        let refresh = window
            .current_monitor()
            .and_then(|monitor| monitor.refresh_rate_millihertz())
            .filter(|millihertz| *millihertz > 0);
        self.frame_interval = refresh.map_or(Duration::from_micros(16_667), |millihertz| {
            Duration::from_secs_f64(1000.0 / f64::from(millihertz))
        });
        let content = self.content.clone();
        host.set_content(move || content());
        let renderer = RasterRenderer::new(size.width as i32, size.height as i32);
        window.request_redraw();
        self.shown = Some(Shown {
            window,
            surface,
            host,
            renderer,
            input: input::InputState::default(),
            ime_area: None,
        });
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(shown) = &mut self.shown else { return };
        let start = Instant::now();
        self.last_frame = start;
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
        if stats.drew || resized || self.frames == 0 {
            let background = self.options.background;
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
            event_loop.exit();
        }
    }

    fn write_screenshot(&mut self) {
        if let (Some(path), Some(shown)) = (&self.screenshot, &mut self.shown)
            && let Err(error) = std::fs::write(path, shown.renderer.png())
        {
            eprintln!("compose-desktop: writing the screenshot to {}: {error}", path.display());
        }
    }
}

impl<C: Fn() + 'static> ApplicationHandler for App<C> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.shown.is_none() {
            self.open(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            WindowEvent::Resized(size) => {
                if let Some(shown) = &mut self.shown {
                    shown.host.resize(IntSize::new(size.width as i32, size.height as i32));
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(shown) = &mut self.shown {
                    shown.host.set_density(Density::new(scale_factor as f32));
                }
            }
            event => {
                let Some(shown) = &mut self.shown else { return };
                match input::handle(&mut shown.input, &mut shown.host, &event) {
                    input::Outcome::Exit => event_loop.exit(),
                    input::Outcome::Handled => {
                        self.first_input.get_or_insert_with(Instant::now);
                    }
                    input::Outcome::Ignored => {}
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // A smoke test exits once idle after its frames.
        let idle = self.shown.as_ref().is_some_and(|shown| !shown.host.needs_frame());
        if self.exit_after.is_some() && self.frames >= 1 && idle {
            event_loop.exit();
        }
        // Tasks whose `delay` ended, and the ones woken meanwhile, take their step; their state
        // writes ask for a frame.
        compose_runtime::executor::fire_timers();
        compose_runtime::executor::run_ready();
        // A field committed or dropped its composition itself (a click, a focus change): the
        // input method starts over, so it doesn't commit the text again.
        if let Some(shown) = &self.shown
            && shown.host.take_input_method_reset()
        {
            shown.window.set_ime_allowed(false);
            shown.window.set_ime_allowed(true);
        }
        let now = Instant::now();
        let mut next_frame = None;
        if let Some(shown) = &self.shown {
            if shown.host.has_pending_work() {
                shown.window.request_redraw();
            } else if compose_runtime::executor::has_frame_awaiters() {
                // An animation: its next frame a refresh interval after the last one.
                let due = self.last_frame + self.frame_interval;
                if now >= due {
                    shown.window.request_redraw();
                } else {
                    next_frame = Some(due);
                }
            }
        }
        // Sleep until an event, a redraw request, the next animation frame or the next
        // `delay`'s end: an idle window costs nothing.
        let timer = compose_runtime::executor::next_timer().map(|wait| now + wait);
        match [next_frame, timer].into_iter().flatten().min() {
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.write_screenshot();
        // The composition goes before the window, on this thread.
        self.shown = None;
    }
}

/// Opens a window showing `content` and runs until it's closed (the close button, Cmd+W or
/// Cmd+Q, Ctrl+W on other systems), on the calling thread, which must be the main thread on
/// macOS.
pub fn run_app(options: WindowOptions, content: impl Fn() + 'static) {
    let event_loop = EventLoop::new().expect("compose-desktop: an event loop");
    let proxy = event_loop.create_proxy();
    let mut app = App {
        options,
        content: Rc::new(content),
        shown: None,
        stats: std::env::var_os("COMPOSE_FRAME_STATS").is_some(),
        frames: 0,
        exit_after: std::env::var("COMPOSE_EXIT_AFTER_FRAMES")
            .ok()
            .and_then(|frames| frames.parse().ok()),
        screenshot: std::env::var_os("COMPOSE_SCREENSHOT").map(Into::into),
        first_input: None,
        proxy,
        frame_interval: Duration::from_micros(16_667),
        last_frame: Instant::now(),
    };
    event_loop.run_app(&mut app).expect("compose-desktop: the event loop");
}
