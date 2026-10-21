//! `keyframes`, as Compose's `KeyframesSpec`: values at given times within a duration, eased
//! from each to the next.
//!
//! A spec stays `Copy`, as the others: its keyframes are kept in a table of this thread's, the
//! spec naming its entry. Equal keyframes share one entry, so a spec built on every pass adds
//! nothing; each distinct one stays for the thread's life (as a program's specs are constants).

use std::cell::RefCell;
use std::rc::Rc;

use crate::easing::{Easing, LINEAR_EASING};
use crate::vector::Animate;

/// Values at times within `duration_millis`, as Compose's `KeyframesSpec`: built with
/// [`keyframes`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyframesSpec {
    entry: u32,
    pub duration_millis: u32,
    pub delay_millis: u32,
}

#[derive(Clone, PartialEq, Debug)]
struct Keyframe {
    time_millis: u32,
    value: [f32; 4],
    easing: Easing,
}

#[derive(PartialEq, Debug)]
struct Entry {
    duration_millis: u32,
    delay_millis: u32,
    /// Sorted by time; a later one at the same time replaces an earlier.
    frames: Vec<Keyframe>,
}

thread_local! {
    static ENTRIES: RefCell<Vec<Rc<Entry>>> = const { RefCell::new(Vec::new()) };
}

/// What `keyframes`' builder is given, as Compose's `KeyframesSpecConfig`.
pub struct KeyframesBuilder<T: Animate> {
    pub delay_millis: u32,
    frames: Vec<Keyframe>,
    _type: std::marker::PhantomData<T>,
}

/// A keyframe just added: `.with(easing)` eases from it to the next, as Compose's `with`.
pub struct KeyframeEntry<'a> {
    frame: &'a mut Keyframe,
}

impl KeyframeEntry<'_> {
    pub fn with(self, easing: Easing) {
        self.frame.easing = easing;
    }
}

impl<T: Animate> KeyframesBuilder<T> {
    /// `value` at `time_millis`, as `value at time`. Times past the duration are ignored.
    pub fn at(&mut self, time_millis: u32, value: T) -> KeyframeEntry<'_> {
        let frame = Keyframe { time_millis, value: value.to_vector(), easing: LINEAR_EASING };
        let index = match self.frames.iter().position(|old| old.time_millis == time_millis) {
            Some(index) => {
                self.frames[index] = frame;
                index
            }
            None => {
                self.frames.push(frame);
                self.frames.len() - 1
            }
        };
        KeyframeEntry { frame: &mut self.frames[index] }
    }
}

/// An animation through the values `build` sets at times within `duration_millis`, as
/// Compose's `keyframes { durationMillis = …; value at time with easing }`: from the start
/// value (at 0, unless a keyframe sets it) to the target (at the end, unless one sets it), each
/// stretch eased by the easing of the keyframe it starts at (linear by default).
pub fn keyframes<T: Animate>(
    duration_millis: u32,
    build: impl FnOnce(&mut KeyframesBuilder<T>),
) -> KeyframesSpec {
    let mut builder =
        KeyframesBuilder { delay_millis: 0, frames: Vec::new(), _type: std::marker::PhantomData };
    build(&mut builder);
    let mut frames: Vec<Keyframe> =
        builder.frames.into_iter().filter(|frame| frame.time_millis <= duration_millis).collect();
    frames.sort_by_key(|frame| frame.time_millis);
    let entry = Entry { duration_millis, delay_millis: builder.delay_millis, frames };
    let index = ENTRIES.with(|entries| {
        let mut entries = entries.borrow_mut();
        match entries.iter().position(|old| **old == entry) {
            Some(index) => index,
            None => {
                entries.push(Rc::new(entry));
                entries.len() - 1
            }
        }
    });
    KeyframesSpec { entry: index as u32, duration_millis, delay_millis: builder.delay_millis }
}

impl KeyframesSpec {
    pub(crate) fn total_nanos(&self) -> u64 {
        (u64::from(self.delay_millis) + u64::from(self.duration_millis)) * 1_000_000
    }

    /// The value `play_nanos` in, from `start` to `target`.
    pub(crate) fn value_at<const N: usize>(
        &self,
        play_nanos: u64,
        start: [f32; N],
        target: [f32; N],
    ) -> [f32; N] {
        let entry = ENTRIES.with(|entries| entries.borrow().get(self.entry as usize).cloned());
        let Some(entry) = entry else { return target };
        let delay = u64::from(entry.delay_millis) * 1_000_000;
        let duration = u64::from(entry.duration_millis) * 1_000_000;
        let time = play_nanos.saturating_sub(delay).min(duration);
        let ms = time as f32 / 1_000_000.0;
        // The stretch the time is in: from the last keyframe at or before it (or the start) to
        // the next one (or the target at the end).
        let vector = |value: &[f32; 4]| {
            let mut out = [0.0; N];
            out.copy_from_slice(&value[..N]);
            out
        };
        let (mut from_time, mut from_value, mut easing) = (0.0, start, LINEAR_EASING);
        let (mut to_time, mut to_value) = (entry.duration_millis as f32, target);
        for frame in &entry.frames {
            let at = frame.time_millis as f32;
            if at <= ms {
                (from_time, from_value, easing) = (at, vector(&frame.value), frame.easing);
            } else {
                (to_time, to_value) = (at, vector(&frame.value));
                break;
            }
        }
        // At the end, the target unless a keyframe there says otherwise.
        if ms >= entry.duration_millis as f32 {
            return match entry.frames.last() {
                Some(frame) if frame.time_millis == entry.duration_millis => vector(&frame.value),
                _ => target,
            };
        }
        let span = to_time - from_time;
        let fraction = if span <= 0.0 { 1.0 } else { easing.transform((ms - from_time) / span) };
        let mut out = [0.0; N];
        for index in 0..N {
            out[index] = from_value[index] + (to_value[index] - from_value[index]) * fraction;
        }
        out
    }
}
