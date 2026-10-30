//! `keyframes`, as Compose's `KeyframesSpec`: values at given times within a duration, eased
//! from each to the next; and `keyframes_with_spline`, as `KeyframesWithSplineSpec`: through them
//! along a smooth curve.
//!
//! The spec holds its keyframes behind an `Rc`: cloning it is cheap, and two specs built alike
//! are equal (where Compose's, without `equals`, are equal only to themselves), so a spec built
//! on every pass doesn't restart what plays it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::easing::{Easing, LINEAR_EASING};
use crate::mono_spline::MonoSpline;
use crate::spec::{MILLIS_TO_NANOS, VectorizedAnimationSpec, lerp};
use crate::vector::{Animate, AnimationVector};

/// Values at times within `duration_millis`, as Compose's `KeyframesSpec`: built with
/// [`keyframes`].
#[derive(Clone, Debug)]
pub struct KeyframesSpec {
    entry: Rc<Entry>,
    duration_millis: u32,
    delay_millis: u32,
}

/// Built alike: the same duration, delay and keyframes.
impl PartialEq for KeyframesSpec {
    fn eq(&self, other: &KeyframesSpec) -> bool {
        Rc::ptr_eq(&self.entry, &other.entry) || *self.entry == *other.entry
    }
}

#[derive(Clone, PartialEq, Debug)]
struct Keyframe {
    time_millis: i32,
    value: AnimationVector,
    easing: Easing,
}

#[derive(PartialEq, Debug)]
struct Entry {
    duration_millis: u32,
    delay_millis: u32,
    /// The keyframes, sorted by time; a later one at the same time replaced an earlier.
    frames: Vec<Keyframe>,
    /// The times of the stretches: the keyframes', with 0 and the duration, sorted.
    timestamps: Vec<i32>,
}

impl Entry {
    fn frame_at(&self, time_millis: i32) -> Option<&Keyframe> {
        self.frames
            .binary_search_by_key(&time_millis, |frame| frame.time_millis)
            .ok()
            .map(|index| &self.frames[index])
    }
}

/// What `keyframes`' builder is given, as Compose's `KeyframesSpecConfig`: its duration and
/// delay to set, and the keyframes to add.
pub struct KeyframesBuilder<T: Animate> {
    /// How long the keyframes play, as `durationMillis` (300 by default).
    pub duration_millis: u32,
    /// How long they wait first, as `delayMillis`.
    pub delay_millis: u32,
    frames: Vec<Keyframe>,
    _type: std::marker::PhantomData<T>,
}

/// A keyframe just added: `.using(easing)` eases from it to the next, as Compose's `using`.
pub struct KeyframeEntry<'a> {
    frame: &'a mut Keyframe,
}

impl KeyframeEntry<'_> {
    /// Eases the stretch from this keyframe to the next along `easing` (linear by default).
    pub fn using(self, easing: Easing) {
        self.frame.easing = easing;
    }
}

impl<T: Animate> KeyframesBuilder<T> {
    /// `value` at `time_millis`, as `value at time`. A later one at the same time replaces it;
    /// times outside 0 to the duration are ignored.
    pub fn at(&mut self, time_millis: i32, value: T) -> KeyframeEntry<'_> {
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

    /// `value` at `fraction` of the duration set so far, rounded to a millisecond, as
    /// `value atFraction fraction`.
    pub fn at_fraction(&mut self, fraction: f32, value: T) -> KeyframeEntry<'_> {
        let time = (f64::from(self.duration_millis as f32 * fraction) + 0.5).floor() as i32;
        self.at(time, value)
    }
}

/// An animation through the values `build` sets at times within its duration, as Compose's
/// `keyframes { durationMillis = …; value at time using easing }`: from the start value (at 0,
/// unless a keyframe sets it) to the target (at the end, unless one sets it), each stretch
/// eased by the easing of the keyframe it starts at (linear by default).
pub fn keyframes<T: Animate>(build: impl FnOnce(&mut KeyframesBuilder<T>)) -> KeyframesSpec {
    let entry = Rc::new(built(build));
    let (duration_millis, delay_millis) = (entry.duration_millis, entry.delay_millis);
    KeyframesSpec { entry, duration_millis, delay_millis }
}

/// What `build` sets, sorted, with the times of its stretches.
fn built<T: Animate>(build: impl FnOnce(&mut KeyframesBuilder<T>)) -> Entry {
    let mut builder = KeyframesBuilder {
        duration_millis: crate::spec::DEFAULT_DURATION_MILLIS,
        delay_millis: 0,
        frames: Vec::new(),
        _type: std::marker::PhantomData,
    };
    build(&mut builder);
    let KeyframesBuilder { duration_millis, delay_millis, mut frames, .. } = builder;
    frames.sort_by_key(|frame| frame.time_millis);
    let duration = duration_millis.min(i32::MAX as u32) as i32;
    let mut timestamps: Vec<i32> = frames.iter().map(|frame| frame.time_millis).collect();
    for time in [0, duration] {
        if !timestamps.contains(&time) {
            timestamps.push(time);
        }
    }
    timestamps.sort_unstable();
    Entry { duration_millis, delay_millis, frames, timestamps }
}

impl KeyframesSpec {
    /// How long it plays, after its delay.
    pub fn duration_millis(&self) -> u32 {
        self.duration_millis
    }

    pub fn delay_millis(&self) -> u32 {
        self.delay_millis
    }

    /// The play time in milliseconds within the duration, as Compose's `clampPlayTime`.
    fn clamp_play_time(&self, play_time_millis: i64) -> i64 {
        (play_time_millis - i64::from(self.delay_millis)).clamp(0, i64::from(self.duration_millis))
    }

    fn value_from_millis_in(
        &self,
        entry: &Entry,
        play_time_millis: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
    ) -> AnimationVector {
        let time = self.clamp_play_time(play_time_millis) as i32;
        if let Some(frame) = entry.frame_at(time) {
            return frame.value;
        }
        if time >= self.duration_millis as i32 {
            return target_value;
        }
        if time <= 0 {
            return initial_value;
        }
        // The stretch from the last time at or before it to the next.
        let index = match entry.timestamps.binary_search(&time) {
            Ok(index) => index,
            Err(insertion) => insertion - 1,
        };
        let (from, to) = (entry.timestamps[index], entry.timestamps[index + 1]);
        let start = entry.frame_at(from);
        let easing = start.map_or(&LINEAR_EASING, |frame| &frame.easing);
        let fraction = easing.transform((time - from) as f32 / (to - from) as f32);
        let start = start.map_or(initial_value, |frame| frame.value);
        let end = entry.frame_at(to).map_or(target_value, |frame| frame.value);
        initial_value.map(|index, _| lerp(component(start, index), component(end, index), fraction))
    }
}

/// A vector's component, 0 past its size.
fn component(vector: AnimationVector, index: usize) -> f32 {
    vector.as_slice().get(index).copied().unwrap_or(0.0)
}

impl VectorizedAnimationSpec for KeyframesSpec {
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        _: AnimationVector,
    ) -> AnimationVector {
        self.value_from_millis_in(
            &self.entry,
            play_time_nanos / MILLIS_TO_NANOS,
            initial_value,
            target_value,
        )
    }

    /// As Compose's `VectorizedKeyframesSpec`: from the value a millisecond before, both read at
    /// play times the delay is taken off again, and with the sign Compose gives it (the earlier
    /// value minus the later).
    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        _: AnimationVector,
    ) -> AnimationVector {
        let entry = &self.entry;
        let clamped = self.clamp_play_time(play_time_nanos / MILLIS_TO_NANOS);
        let start = self.value_from_millis_in(entry, clamped - 1, initial_value, target_value);
        let end = self.value_from_millis_in(entry, clamped, initial_value, target_value);
        initial_value.map(|index, _| (component(start, index) - component(end, index)) * 1000.0)
    }

    fn duration_nanos(&self, _: AnimationVector, _: AnimationVector, _: AnimationVector) -> i64 {
        (i64::from(self.delay_millis) + i64::from(self.duration_millis)) * MILLIS_TO_NANOS
    }
}

/// Values at times within a duration on a smooth curve across them all, as Compose's
/// `KeyframesWithSplineSpec`: built with [`keyframes_with_spline`] or
/// [`periodic_keyframes_with_spline`]. Built alike, two are equal (D816).
#[derive(Clone, Debug)]
pub struct KeyframesWithSplineSpec {
    inner: Rc<SplineInner>,
}

#[derive(Debug)]
struct SplineInner {
    entry: Entry,
    /// How much the first and last velocities change to be the same, or NaN: not periodic.
    periodic_bias: f32,
    /// The spline of the last start and end values, which are part of it unless keyframes set them,
    /// as `VectorizedMonoSplineKeyframesSpec`'s fields.
    spline: RefCell<Option<CachedSpline>>,
}

#[derive(Debug)]
struct CachedSpline {
    initial_value: AnimationVector,
    target_value: AnimationVector,
    spline: Rc<MonoSpline>,
}

impl PartialEq for KeyframesWithSplineSpec {
    fn eq(&self, other: &KeyframesWithSplineSpec) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
            || (self.inner.entry == other.inner.entry
                && self.inner.periodic_bias.to_bits() == other.inner.periodic_bias.to_bits())
    }
}

/// An animation through the values `build` sets on a smooth curve across them all, as Compose's
/// `keyframesWithSpline { … }`: a monotonic spline, each stretch's time eased by the easing of the
/// keyframe it starts at (linear by default). The initial velocity is ignored.
pub fn keyframes_with_spline<T: Animate>(
    build: impl FnOnce(&mut KeyframesBuilder<T>),
) -> KeyframesWithSplineSpec {
    spline_spec(f32::NAN, build)
}

/// [`keyframes_with_spline`] starting and ending at one velocity, for smooth repeats, as Compose's
/// `keyframesWithSpline(periodicBias) { … }`: at 0 only the first velocity changes to the last's, at
/// 1 only the last to the first's, at 0.5 both to their average.
pub fn periodic_keyframes_with_spline<T: Animate>(
    periodic_bias: f32,
    build: impl FnOnce(&mut KeyframesBuilder<T>),
) -> KeyframesWithSplineSpec {
    spline_spec(periodic_bias, build)
}

fn spline_spec<T: Animate>(
    periodic_bias: f32,
    build: impl FnOnce(&mut KeyframesBuilder<T>),
) -> KeyframesWithSplineSpec {
    let entry = built(build);
    KeyframesWithSplineSpec {
        inner: Rc::new(SplineInner { entry, periodic_bias, spline: RefCell::new(None) }),
    }
}

impl KeyframesWithSplineSpec {
    /// How long it plays, after its delay.
    pub fn duration_millis(&self) -> u32 {
        self.inner.entry.duration_millis
    }

    pub fn delay_millis(&self) -> u32 {
        self.inner.entry.delay_millis
    }

    /// The play time within the duration, as Compose's `clampPlayTime`.
    fn clamped(&self, play_time_nanos: i64) -> i32 {
        let entry = &self.inner.entry;
        (play_time_nanos / MILLIS_TO_NANOS - i64::from(entry.delay_millis))
            .clamp(0, i64::from(entry.duration_millis)) as i32
    }

    /// The spline from `initial_value` to `target_value`, as `init`.
    fn spline(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
    ) -> Rc<MonoSpline> {
        let inner = &*self.inner;
        let mut cached = inner.spline.borrow_mut();
        if let Some(cached) = &*cached
            && cached.initial_value == initial_value
            && cached.target_value == target_value
        {
            return cached.spline.clone();
        }
        let entry = &inner.entry;
        let values = entry.timestamps.iter().map(|time| match entry.frame_at(*time) {
            Some(frame) => frame.value,
            None if *time == 0 => initial_value,
            None => target_value,
        });
        let values: Vec<AnimationVector> = values.collect();
        let times: Vec<f32> =
            entry.timestamps.iter().map(|time| *time as f32 / SECONDS_TO_MILLIS).collect();
        let spline = Rc::new(MonoSpline::new(times, values, inner.periodic_bias));
        *cached = Some(CachedSpline { initial_value, target_value, spline: spline.clone() });
        spline
    }

    /// The entry index with `time_millis` at or after it and before the next, as
    /// `findEntryForTimeMillis`.
    fn index_of(&self, time_millis: i32) -> usize {
        match self.inner.entry.timestamps.binary_search(&time_millis) {
            Ok(index) => index,
            Err(insertion) => insertion.saturating_sub(1),
        }
    }

    /// `time_millis` in seconds, eased within its stretch, as `getEasedTimeFromIndex`.
    fn eased_time(&self, index: usize, time_millis: i32) -> f32 {
        let entry = &self.inner.entry;
        if index + 1 >= entry.timestamps.len() {
            return time_millis as f32 / SECONDS_TO_MILLIS;
        }
        let (from, to) = (entry.timestamps[index], entry.timestamps[index + 1]);
        if time_millis == from {
            return from as f32 / SECONDS_TO_MILLIS;
        }
        let range = (to - from) as f32;
        let easing = entry.frame_at(from).map_or(&LINEAR_EASING, |frame| &frame.easing);
        let eased = easing.transform((time_millis - from) as f32 / range);
        (range * eased + from as f32) / SECONDS_TO_MILLIS
    }
}

/// Compose's `SecondsToMillis`.
const SECONDS_TO_MILLIS: f32 = 1000.0;

impl VectorizedAnimationSpec for KeyframesWithSplineSpec {
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        _: AnimationVector,
    ) -> AnimationVector {
        let time = self.clamped(play_time_nanos);
        if let Some(frame) = self.inner.entry.frame_at(time) {
            return frame.value;
        }
        if time >= self.duration_millis() as i32 {
            return target_value;
        }
        if time <= 0 {
            return initial_value;
        }
        let spline = self.spline(initial_value, target_value);
        let index = self.index_of(time);
        spline.position(self.eased_time(index, time), index, initial_value)
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        let time = self.clamped(play_time_nanos);
        let spline = self.spline(initial_value, target_value);
        let index = self.index_of(time);
        spline.slope(self.eased_time(index, time), index, initial_velocity)
    }

    fn duration_nanos(&self, _: AnimationVector, _: AnimationVector, _: AnimationVector) -> i64 {
        let entry = &self.inner.entry;
        (i64::from(entry.delay_millis) + i64::from(entry.duration_millis)) * MILLIS_TO_NANOS
    }
}
