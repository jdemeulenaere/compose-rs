//! compose-rs's own performance hint session for the RenderThread and the main thread
//! (docs/07-platforms.md, "Android"; D390, D440, D500).
//!
//! Android boosts a thread (raises its `uclamp.min`, so the scheduler runs it on faster cores at
//! higher frequencies) when a performance hint session holding it reports work running long
//! against its target (ADPF). HWUI runs one session per window for the main thread and the
//! RenderThread, reporting each frame's duration from the main thread's start to the
//! RenderThread's end against 70 % of its deadline (about 1.4 vsync intervals): an app whose
//! main-thread frames are short is rarely boosted, and its RenderThread's frames then take
//! longer for the same work (docs/14, "The scroll's RenderThread"). So a view showing compose-rs
//! with the `performance_hint` optimization (an app's opt-in: off by default, as Compose has no
//! such session, D581) keeps a session of its own for the RenderThread and the main thread,
//! reporting both threads' CPU time over each frame against [`TARGET_NANOS`]. The power HAL
//! applies the highest boost of a thread's sessions.
//!
//! The session holds the main thread too (D440): with the RenderThread boosted, its part of
//! each frame is shorter, so HWUI's session, which boosted both threads, hardly boosts any more,
//! and the main thread would be left on the little cores (on the stress list, 79 % of its time,
//! its median frame 1.54 ms against 1.07 ms in the session).
//!
//! Why both threads' time, against one 120 Hz interval (D500): under the target, the power
//! HAL's controller adds to the vote what a report exceeds the one before by, and takes off a
//! share of the margin under the target, so the vote settles where the reports' mean is about
//! 0.4 of the target for frames varying as ours do. Against 6.5 ms, the RenderThread's time
//! alone settled near 2.6 ms whatever the screen: the less work a screen gave it, the less it
//! was boosted, and the slower that work ran (docs/14, "The session's target"). Both threads'
//! time is the frame's work, as HWUI reports it, and against 8.33 ms that level is where the
//! scrolls' frames are: the vote follows the load, low in a light animation, high on a list.
//!
//! What is reported: the RenderThread's and the main thread's CPU time since the last
//! compose-rs frame, read when the next one starts (one `clock_gettime` of each thread's CPU
//! clock). While compose-rs makes a frame every vsync, that is the RenderThread's work for the
//! frame before, which ends well before the next vsync (with whatever else it drew in between:
//! other views' frames in the same window, its own animations), and the main thread's since the
//! last frame started (the frame's, and the idle-time work after it: prefetch). After a gap of
//! more than [`MOST_FRAME_GAP`], the time since isn't one frame's: it starts a new count,
//! unreported. No listener, no other thread, and no call into Java: `frame` reads it.
//!
//! The session is the process's (the RenderThread is): open while a view with hints enabled is
//! shown, created at the second frame after that (not on the first frame's path, the window's
//! first traversal at startup; the RenderThread found among the process's threads once), closed
//! when the last such view hides or goes. Below API 33 (no `APerformanceHint`), or if the system
//! gives no session, nothing happens.

// Only Android has the system side.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use std::time::{Duration, Instant};

/// What a frame's work (both threads' CPU time) is reported against: one 120 Hz vsync interval
/// (D500; D390's 6.5 ms was for the RenderThread's alone).
pub(crate) const TARGET_NANOS: i64 = 8_333_333;

/// Frames of other views within this time of a reported one are the same frame (two views
/// showing compose-rs in one window run their frames back to back, the second after the first
/// ends): not reported again. Half a 120 Hz vsync interval.
const SAME_FRAME: Duration = Duration::from_millis(4);

/// Frames further apart than this (one and a half 60 Hz intervals) don't report the
/// RenderThread's time between them: it may hold several frames, or other content's.
const MOST_FRAME_GAP: Duration = Duration::from_millis(25);

/// Every this many reports, the session's thread is checked to still be the RenderThread (a new
/// one is found, and the session made again, if not).
const CHECK_THREAD_EVERY: u32 = 1024;

/// Sessions made again after a failure (a report or a CPU time read failing) while shown, before
/// giving up until the views hide and show again.
const MOST_REOPENS: u32 = 3;

/// What the hint needs of the system: the threads, their CPU clocks, and the NDK's
/// `APerformanceHint` sessions. A fake in tests.
pub(crate) trait HintPlatform {
    type Session;
    /// The calling thread's id.
    fn current_thread(&self) -> i32;
    /// The process's RenderThread's id, if it has one.
    fn render_thread(&self) -> Option<i32>;
    /// Whether thread `tid` is (still) the RenderThread.
    fn is_render_thread(&self, tid: i32) -> bool;
    /// Thread `tid`'s CPU time so far, in nanoseconds (`None`: gone).
    fn thread_cpu_nanos(&self, tid: i32) -> Option<i64>;
    /// A session for `tids` with a target of `target_nanos`.
    fn create_session(&self, tids: &[i32], target_nanos: i64) -> Option<Self::Session>;
    /// Reports a frame's actual work duration; false if the session failed.
    fn report(&self, session: &Self::Session, nanos: i64) -> bool;
    fn close_session(&self, session: Self::Session);
}

struct Open<S> {
    session: S,
    tid: i32,
    /// The RenderThread's CPU time at the last frame.
    cpu_nanos: i64,
    /// The main thread (the one making the frames).
    main_tid: i32,
    /// The main thread's CPU time at the last frame.
    main_cpu_nanos: i64,
    reports: u32,
}

/// The process's RenderThread hint: shown views count, and frames report.
pub(crate) struct RenderThreadHint<P: HintPlatform> {
    platform: P,
    target_nanos: i64,
    shown: usize,
    open: Option<Open<P::Session>>,
    /// The RenderThread found last, checked by name before it's used again.
    render_thread: Option<i32>,
    /// Sessions made again since the views showed.
    reopens: u32,
    /// No session to be had until the views hide and show again.
    gave_up: bool,
    /// When the last frame since the views showed started.
    last_frame: Option<Instant>,
}

impl<P: HintPlatform> RenderThreadHint<P> {
    pub(crate) fn new(platform: P, target_nanos: i64) -> Self {
        Self {
            platform,
            target_nanos,
            shown: 0,
            open: None,
            render_thread: None,
            reopens: 0,
            gave_up: false,
            last_frame: None,
        }
    }

    /// A view with hints enabled shows: its frames report.
    pub(crate) fn show(&mut self) {
        self.shown += 1;
        if self.shown == 1 {
            self.gave_up = false;
            self.reopens = 0;
        }
    }

    /// A view with hints enabled hides or goes: with the last one, the session closes.
    pub(crate) fn hide(&mut self) {
        self.shown = self.shown.saturating_sub(1);
        if self.shown == 0 {
            self.close();
            self.last_frame = None;
        }
    }

    /// Whether a session is open.
    #[cfg(test)]
    fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// A shown view's frame starts at `now`, on the main thread: both threads' work since the
    /// last frame is reported (the session made first, if none is open: not at the first frame
    /// since the views showed).
    pub(crate) fn frame(&mut self, now: Instant) {
        if self.shown == 0 || self.gave_up {
            return;
        }
        let since_last = self.last_frame.map(|last| now.saturating_duration_since(last));
        if since_last.is_some_and(|since| since < SAME_FRAME) {
            return;
        }
        self.last_frame = Some(now);
        let Some(since_last) = since_last else {
            return;
        };
        let Some(open) = self.open.as_mut() else {
            self.open_session();
            return;
        };
        let (Some(cpu_nanos), Some(main_cpu_nanos)) = (
            self.platform.thread_cpu_nanos(open.tid),
            self.platform.thread_cpu_nanos(open.main_tid),
        ) else {
            self.reopen();
            return;
        };
        let work = cpu_nanos - open.cpu_nanos + main_cpu_nanos - open.main_cpu_nanos;
        open.cpu_nanos = cpu_nanos;
        open.main_cpu_nanos = main_cpu_nanos;
        if since_last > MOST_FRAME_GAP {
            return;
        }
        open.reports = open.reports.wrapping_add(1);
        if open.reports % CHECK_THREAD_EVERY == 0 && !self.platform.is_render_thread(open.tid) {
            self.render_thread = None;
            self.reopen();
            return;
        }
        // Nothing ran: a duration of 0 is invalid, and says nothing.
        if work > 0 && !self.platform.report(&open.session, work) {
            self.reopen();
        }
    }

    fn open_session(&mut self) {
        let _section = compose_ui::platform::trace::section(c"compose-rs: hint session");
        let cached = self.render_thread.filter(|&tid| self.platform.is_render_thread(tid));
        self.render_thread = cached.or_else(|| self.platform.render_thread());
        let opened = self.render_thread.and_then(|tid| {
            let cpu_nanos = self.platform.thread_cpu_nanos(tid)?;
            // The main thread: the one making the frames.
            let main_tid = self.platform.current_thread();
            let main_cpu_nanos = self.platform.thread_cpu_nanos(main_tid)?;
            let session = self.platform.create_session(&[tid, main_tid], self.target_nanos)?;
            Some(Open { session, tid, cpu_nanos, main_tid, main_cpu_nanos, reports: 0 })
        });
        match opened {
            Some(open) => {
                log::debug!(
                    "a performance hint session for the RenderThread ({}) and the main thread",
                    open.tid
                );
                self.open = Some(open);
            }
            None => self.gave_up = true,
        }
    }

    /// The session failed, or its thread isn't the RenderThread any more: closed, and made
    /// again at the next frame, a few times at most.
    fn reopen(&mut self) {
        self.close();
        self.reopens += 1;
        if self.reopens > MOST_REOPENS {
            self.gave_up = true;
        }
    }

    fn close(&mut self) {
        if let Some(open) = self.open.take() {
            self.platform.close_session(open.session);
        }
    }
}

/// The id of the thread named `name` among `task_directory`'s (`/proc/self/task`: a directory
/// per thread, its name in `comm`), the lowest if several.
pub(crate) fn find_thread(task_directory: &std::path::Path, name: &str) -> Option<i32> {
    let entries = std::fs::read_dir(task_directory).ok()?;
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<i32>().ok())
        .filter(|&tid| thread_is_named(task_directory, tid, name))
        .min()
}

/// Whether thread `tid` in `task_directory` is named `name`.
pub(crate) fn thread_is_named(task_directory: &std::path::Path, tid: i32, name: &str) -> bool {
    std::fs::read_to_string(task_directory.join(tid.to_string()).join("comm"))
        .is_ok_and(|comm| comm.trim_end_matches('\n') == name)
}

/// The process's hint, on the main thread (where views show, hide and make frames): loaded at
/// the first view shown; `None` inside where the system has no `APerformanceHint`.
#[cfg(target_os = "android")]
mod process {
    use std::cell::RefCell;

    use super::{RenderThreadHint, TARGET_NANOS, ndk::Ndk};

    thread_local! {
        static HINT: RefCell<Option<Option<RenderThreadHint<Ndk>>>> = const { RefCell::new(None) };
    }

    pub(crate) fn with(f: impl FnOnce(&mut RenderThreadHint<Ndk>)) {
        HINT.with_borrow_mut(|hint| {
            let hint = hint.get_or_insert_with(|| {
                Ndk::load().map(|ndk| RenderThreadHint::new(ndk, TARGET_NANOS))
            });
            if let Some(hint) = hint {
                f(hint);
            }
        });
    }
}

/// A view with hints enabled shows (see [`RenderThreadHint::show`]). Main thread.
pub(crate) fn show() {
    #[cfg(target_os = "android")]
    process::with(|hint| hint.show());
}

/// A view with hints enabled hides or goes (see [`RenderThreadHint::hide`]). Main thread.
pub(crate) fn hide() {
    #[cfg(target_os = "android")]
    process::with(|hint| hint.hide());
}

/// A shown view's frame starts (see [`RenderThreadHint::frame`]). Main thread.
pub(crate) fn frame() {
    #[cfg(target_os = "android")]
    process::with(|hint| hint.frame(Instant::now()));
}

#[cfg(target_os = "android")]
mod ndk {
    use std::ffi::{c_char, c_int, c_long, c_void};
    use std::path::Path;

    const TASKS: &str = "/proc/self/task";
    const RENDER_THREAD: &str = "RenderThread";

    #[repr(C)]
    struct Timespec {
        seconds: c_long,
        nanoseconds: c_long,
    }

    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn clock_gettime(clock: c_int, time: *mut Timespec) -> c_int;
        fn gettid() -> c_int;
    }

    const RTLD_NOW: c_int = 2;

    type GetManager = unsafe extern "C" fn() -> *mut c_void;
    type CreateSession = unsafe extern "C" fn(*mut c_void, *const i32, usize, i64) -> *mut c_void;
    type ReportActualWorkDuration = unsafe extern "C" fn(*mut c_void, i64) -> c_int;
    type CloseSession = unsafe extern "C" fn(*mut c_void);

    /// The NDK's `APerformanceHint` (API 33), looked up in `libandroid.so` (the library's
    /// minimum API level is lower).
    pub(crate) struct Ndk {
        manager: *mut c_void,
        create_session: CreateSession,
        report: ReportActualWorkDuration,
        close_session: CloseSession,
    }

    pub(crate) struct Session(*mut c_void);

    impl Ndk {
        /// `None` below API 33, or without a manager (a device without hint sessions).
        pub(crate) fn load() -> Option<Ndk> {
            // SAFETY: dlopen and dlsym with NUL-terminated names; each symbol is the NDK's
            // function of the type it's cast to.
            unsafe {
                let library = dlopen(c"libandroid.so".as_ptr(), RTLD_NOW);
                if library.is_null() {
                    return None;
                }
                let symbol = |name: &std::ffi::CStr| {
                    let address = dlsym(library, name.as_ptr());
                    (!address.is_null()).then_some(address)
                };
                let get_manager: GetManager =
                    std::mem::transmute(symbol(c"APerformanceHint_getManager")?);
                let create_session: CreateSession =
                    std::mem::transmute(symbol(c"APerformanceHint_createSession")?);
                let report: ReportActualWorkDuration =
                    std::mem::transmute(symbol(c"APerformanceHint_reportActualWorkDuration")?);
                let close_session: CloseSession =
                    std::mem::transmute(symbol(c"APerformanceHint_closeSession")?);
                let manager = get_manager();
                if manager.is_null() {
                    return None;
                }
                Some(Ndk { manager, create_session, report, close_session })
            }
        }
    }

    impl super::HintPlatform for Ndk {
        type Session = Session;

        fn current_thread(&self) -> i32 {
            // SAFETY: no arguments; bionic's `gettid` can't fail.
            unsafe { gettid() }
        }

        fn render_thread(&self) -> Option<i32> {
            super::find_thread(Path::new(TASKS), RENDER_THREAD)
        }

        fn is_render_thread(&self, tid: i32) -> bool {
            super::thread_is_named(Path::new(TASKS), tid, RENDER_THREAD)
        }

        fn thread_cpu_nanos(&self, tid: i32) -> Option<i64> {
            // The thread's CPU clock, as bionic's `pthread_getcpuclockid` makes it from a tid:
            // the kernel's per-thread scheduler clock (a thread of this process).
            let clock = (!tid << 3) | 6;
            let mut time = Timespec { seconds: 0, nanoseconds: 0 };
            // SAFETY: `time` is a valid timespec to write.
            if unsafe { clock_gettime(clock, &mut time) } != 0 {
                return None;
            }
            // (`c_long` is 32 bits on 32-bit Android.)
            #[allow(clippy::useless_conversion)]
            Some(i64::from(time.seconds) * 1_000_000_000 + i64::from(time.nanoseconds))
        }

        fn create_session(&self, tids: &[i32], target_nanos: i64) -> Option<Session> {
            // SAFETY: the manager from `APerformanceHint_getManager`, and `tids.len()` ids.
            let session = unsafe {
                (self.create_session)(self.manager, tids.as_ptr(), tids.len(), target_nanos)
            };
            (!session.is_null()).then_some(Session(session))
        }

        fn report(&self, session: &Session, nanos: i64) -> bool {
            // SAFETY: an open session.
            unsafe { (self.report)(session.0, nanos) == 0 }
        }

        fn close_session(&self, session: Session) {
            // SAFETY: an open session, closed once (taken by value).
            unsafe { (self.close_session)(session.0) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct State {
        render_thread: Option<i32>,
        /// How often the RenderThread was looked for among the threads.
        scans: u32,
        /// Threads that aren't the RenderThread any more.
        renamed: Vec<i32>,
        cpu: std::collections::HashMap<i32, i64>,
        create_fails: bool,
        report_fails: bool,
        next_session: u32,
        created: Vec<(u32, Vec<i32>, i64)>,
        reports: Vec<(u32, i64)>,
        closed: Vec<u32>,
    }

    #[derive(Clone, Default)]
    struct Fake(Rc<RefCell<State>>);

    impl HintPlatform for Fake {
        type Session = u32;
        fn current_thread(&self) -> i32 {
            MAIN
        }
        fn render_thread(&self) -> Option<i32> {
            let mut state = self.0.borrow_mut();
            state.scans += 1;
            state.render_thread
        }
        fn is_render_thread(&self, tid: i32) -> bool {
            let state = self.0.borrow();
            state.render_thread == Some(tid) && !state.renamed.contains(&tid)
        }
        fn thread_cpu_nanos(&self, tid: i32) -> Option<i64> {
            self.0.borrow().cpu.get(&tid).copied()
        }
        fn create_session(&self, tids: &[i32], target_nanos: i64) -> Option<u32> {
            let mut state = self.0.borrow_mut();
            if state.create_fails {
                return None;
            }
            state.next_session += 1;
            let id = state.next_session;
            state.created.push((id, tids.to_vec(), target_nanos));
            Some(id)
        }
        fn report(&self, session: &u32, nanos: i64) -> bool {
            let mut state = self.0.borrow_mut();
            state.reports.push((*session, nanos));
            !state.report_fails
        }
        fn close_session(&self, session: u32) {
            self.0.borrow_mut().closed.push(session);
        }
    }

    const RENDER: i32 = 4321;
    /// The thread making the frames.
    const MAIN: i32 = 1234;
    const MS: i64 = 1_000_000;
    /// 120 Hz.
    const INTERVAL: Duration = Duration::from_nanos(8_333_333);

    fn platform() -> Fake {
        let fake = Fake::default();
        fake.0.borrow_mut().render_thread = Some(RENDER);
        fake.0.borrow_mut().cpu.insert(RENDER, 1_000 * MS);
        fake.0.borrow_mut().cpu.insert(MAIN, 500 * MS);
        fake
    }

    /// Thread `tid` runs `nanos` more.
    fn runs(fake: &Fake, tid: i32, nanos: i64) {
        *fake.0.borrow_mut().cpu.get_mut(&tid).unwrap() += nanos;
    }

    /// A view shows and makes two frames from `start`, which open the session: when the next
    /// frame is due.
    fn shown_and_opened(hint: &mut RenderThreadHint<Fake>, start: Instant) -> Instant {
        hint.show();
        hint.frame(start);
        hint.frame(start + INTERVAL);
        start + 2 * INTERVAL
    }

    #[test]
    fn nothing_happens_until_a_view_shows() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        hint.frame(Instant::now());
        hint.frame(Instant::now() + INTERVAL);
        assert!(fake.0.borrow().created.is_empty());
        assert!(!hint.is_open());
    }

    #[test]
    fn the_second_frame_shown_opens_a_session_for_the_render_thread_and_the_main_thread() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), 4 * MS);
        hint.show();
        assert!(fake.0.borrow().created.is_empty(), "made at a frame, not on show");
        let start = Instant::now();
        // The first frame (at startup, the window's first traversal's) opens nothing.
        hint.frame(start);
        assert!(fake.0.borrow().created.is_empty());
        assert_eq!(fake.0.borrow().scans, 0);
        hint.frame(start + INTERVAL);
        // The main thread: the one making the frames.
        assert_eq!(fake.0.borrow().created, vec![(1, vec![RENDER, MAIN], 4 * MS)]);
        // Nothing to report yet: the CPU time is the start.
        assert!(fake.0.borrow().reports.is_empty());
    }

    #[test]
    fn the_target_is_one_120_hz_interval() {
        assert_eq!(TARGET_NANOS, 8_333_333);
    }

    #[test]
    fn each_frame_reports_both_threads_cpu_time_since_the_last() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let next = shown_and_opened(&mut hint, Instant::now());
        runs(&fake, RENDER, 3 * MS);
        runs(&fake, MAIN, 2 * MS);
        hint.frame(next);
        runs(&fake, RENDER, 2 * MS);
        runs(&fake, MAIN, MS / 2);
        hint.frame(next + INTERVAL);
        assert_eq!(fake.0.borrow().reports, vec![(1, 5 * MS), (1, 2 * MS + MS / 2)]);
    }

    #[test]
    fn the_main_threads_work_alone_is_reported() {
        // Between two frames, the RenderThread drew nothing; the main thread ran the frame and
        // idle-time work (prefetch).
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let next = shown_and_opened(&mut hint, Instant::now());
        runs(&fake, MAIN, 4 * MS);
        hint.frame(next);
        assert_eq!(fake.0.borrow().reports, vec![(1, 4 * MS)]);
    }

    #[test]
    fn the_main_threads_cpu_time_counts_from_the_sessions_creation() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        hint.show();
        let start = Instant::now();
        hint.frame(start);
        // Before the session: not reported.
        runs(&fake, MAIN, 30 * MS);
        hint.frame(start + INTERVAL);
        runs(&fake, MAIN, MS);
        runs(&fake, RENDER, 2 * MS);
        hint.frame(start + 2 * INTERVAL);
        assert_eq!(fake.0.borrow().reports, vec![(1, 3 * MS)]);
    }

    #[test]
    fn a_main_thread_clock_failing_makes_the_session_again() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let next = shown_and_opened(&mut hint, Instant::now());
        let main_cpu = fake.0.borrow_mut().cpu.remove(&MAIN).unwrap();
        runs(&fake, RENDER, 3 * MS);
        hint.frame(next);
        assert!(fake.0.borrow().reports.is_empty());
        assert_eq!(fake.0.borrow().closed, vec![1]);
        // While it fails, no session can be made: given up until shown again.
        hint.frame(next + INTERVAL);
        assert!(!hint.is_open());
        fake.0.borrow_mut().cpu.insert(MAIN, main_cpu);
        hint.frame(next + 2 * INTERVAL);
        assert!(!hint.is_open());
        hint.hide();
        shown_and_opened(&mut hint, next + 3 * INTERVAL);
        assert!(hint.is_open());
    }

    #[test]
    fn a_frame_without_work_reports_nothing() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let next = shown_and_opened(&mut hint, Instant::now());
        hint.frame(next);
        assert!(fake.0.borrow().reports.is_empty());
    }

    #[test]
    fn a_frame_after_a_gap_starts_a_new_count_unreported() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let next = shown_and_opened(&mut hint, Instant::now());
        // Between the frames, the RenderThread drew other content (or several frames), and the
        // main thread ran other work.
        runs(&fake, RENDER, 40 * MS);
        runs(&fake, MAIN, 25 * MS);
        let after_gap = next + Duration::from_millis(500);
        hint.frame(after_gap);
        assert!(fake.0.borrow().reports.is_empty());
        runs(&fake, RENDER, 3 * MS);
        hint.frame(after_gap + INTERVAL);
        assert_eq!(fake.0.borrow().reports, vec![(1, 3 * MS)]);
        // A skipped vsync at 120 Hz is no gap.
        runs(&fake, RENDER, 2 * MS);
        hint.frame(after_gap + 3 * INTERVAL);
        assert_eq!(fake.0.borrow().reports, vec![(1, 3 * MS), (1, 2 * MS)]);
    }

    #[test]
    fn two_views_frames_in_one_vsync_report_once() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        hint.show();
        let next = shown_and_opened(&mut hint, Instant::now());
        runs(&fake, RENDER, 3 * MS);
        hint.frame(next);
        // The second view's frame starts when the first's ends.
        hint.frame(next + Duration::from_micros(3_000));
        runs(&fake, RENDER, 3 * MS);
        hint.frame(next + INTERVAL);
        assert_eq!(fake.0.borrow().reports, vec![(1, 3 * MS), (1, 3 * MS)]);
    }

    #[test]
    fn the_session_closes_when_the_last_view_hides_and_opens_again_on_show() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        hint.show();
        let start = Instant::now();
        let next = shown_and_opened(&mut hint, start);
        hint.hide();
        assert!(hint.is_open(), "a view still shows");
        hint.hide();
        assert!(!hint.is_open());
        assert_eq!(fake.0.borrow().closed, vec![1]);
        // Hidden: frames report nothing (a frame made while hiding).
        runs(&fake, RENDER, 3 * MS);
        hint.frame(next);
        hint.frame(next + INTERVAL);
        assert!(fake.0.borrow().reports.is_empty());
        assert_eq!(fake.0.borrow().created.len(), 1);
        // Shown again: a new session, its CPU time from the frame making it, and the
        // RenderThread found before, checked by name rather than looked for again.
        let next = shown_and_opened(&mut hint, next + 2 * INTERVAL);
        runs(&fake, RENDER, 2 * MS);
        hint.frame(next);
        assert_eq!(fake.0.borrow().created.len(), 2);
        assert_eq!(fake.0.borrow().reports, vec![(2, 2 * MS)]);
        assert_eq!(fake.0.borrow().scans, 1);
    }

    #[test]
    fn hiding_more_than_shown_is_harmless() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        hint.hide();
        shown_and_opened(&mut hint, Instant::now());
        assert!(hint.is_open());
    }

    #[test]
    fn without_a_session_from_the_system_it_gives_up_until_shown_again() {
        let fake = platform();
        fake.0.borrow_mut().create_fails = true;
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let next = shown_and_opened(&mut hint, Instant::now());
        // Not tried on every frame.
        fake.0.borrow_mut().create_fails = false;
        hint.frame(next);
        assert!(!hint.is_open());
        hint.hide();
        shown_and_opened(&mut hint, next + INTERVAL);
        assert!(hint.is_open());
    }

    #[test]
    fn without_a_render_thread_it_gives_up_until_shown_again() {
        let fake = platform();
        fake.0.borrow_mut().render_thread = None;
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let next = shown_and_opened(&mut hint, Instant::now());
        hint.frame(next);
        assert!(fake.0.borrow().created.is_empty());
        assert_eq!(fake.0.borrow().scans, 1);
        assert!(!hint.is_open());
    }

    #[test]
    fn a_failed_report_makes_the_session_again_a_few_times_at_most() {
        let fake = platform();
        fake.0.borrow_mut().report_fails = true;
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let mut now = shown_and_opened(&mut hint, Instant::now());
        for _ in 0..20 {
            runs(&fake, RENDER, 3 * MS);
            hint.frame(now);
            now += INTERVAL;
        }
        let state = fake.0.borrow();
        assert_eq!(state.created.len() as u32, MOST_REOPENS + 1);
        assert_eq!(state.closed.len() as u32, MOST_REOPENS + 1);
        assert!(!hint.is_open());
    }

    #[test]
    fn a_new_render_thread_gets_a_new_session() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let mut now = shown_and_opened(&mut hint, Instant::now());
        // The RenderThread is replaced: the old one is renamed.
        fake.0.borrow_mut().renamed.push(RENDER);
        fake.0.borrow_mut().render_thread = Some(RENDER + 1);
        fake.0.borrow_mut().cpu.insert(RENDER + 1, 0);
        for _ in 0..CHECK_THREAD_EVERY {
            runs(&fake, RENDER, MS);
            hint.frame(now);
            now += INTERVAL;
        }
        assert_eq!(fake.0.borrow().closed, vec![1]);
        hint.frame(now);
        assert_eq!(
            fake.0.borrow().created.last(),
            Some(&(2, vec![RENDER + 1, MAIN], TARGET_NANOS))
        );
    }

    #[test]
    fn a_render_thread_gone_gets_a_new_session() {
        let fake = platform();
        let mut hint = RenderThreadHint::new(fake.clone(), TARGET_NANOS);
        let next = shown_and_opened(&mut hint, Instant::now());
        fake.0.borrow_mut().cpu.remove(&RENDER);
        fake.0.borrow_mut().render_thread = Some(RENDER + 1);
        fake.0.borrow_mut().cpu.insert(RENDER + 1, 0);
        hint.frame(next);
        assert_eq!(fake.0.borrow().closed, vec![1]);
        hint.frame(next + INTERVAL);
        assert_eq!(
            fake.0.borrow().created.last().map(|created| created.1.clone()),
            Some(vec![RENDER + 1, MAIN])
        );
    }

    #[test]
    fn the_render_thread_is_found_by_name() {
        let directory =
            std::env::temp_dir().join(format!("compose-rs-tasks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        for (tid, name) in [(100, "feed"), (130, "RenderThread"), (120, "hwuiTask0"), (7, "Binder")]
        {
            std::fs::create_dir_all(directory.join(tid.to_string())).unwrap();
            std::fs::write(directory.join(tid.to_string()).join("comm"), format!("{name}\n"))
                .unwrap();
        }
        // An entry that isn't a thread.
        std::fs::create_dir_all(directory.join("self")).unwrap();
        assert_eq!(find_thread(&directory, "RenderThread"), Some(130));
        assert!(thread_is_named(&directory, 130, "RenderThread"));
        assert!(!thread_is_named(&directory, 120, "RenderThread"));
        assert!(!thread_is_named(&directory, 999, "RenderThread"));
        assert_eq!(find_thread(&directory, "GLThread"), None);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
