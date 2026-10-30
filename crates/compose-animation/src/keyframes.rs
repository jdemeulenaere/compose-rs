//! `keyframes`, as Compose's `KeyframesSpec`: values at given times within a duration, eased
//! from each to the next; and `keyframes_with_spline`, as `KeyframesWithSplineSpec`: through them
//! along a smooth curve.
//!
//! The spec holds its keyframes behind an `Rc`: cloning it is cheap, and two specs built alike
//! are equal (where Compose's, without `equals`, are equal only to themselves), so a spec built
//! on every pass doesn't restart what plays it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::arc_spline::{ArcMode, ArcSpline};
use crate::easing::{Easing, FAST_OUT_SLOW_IN_EASING, LINEAR_EASING};
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
    arc_mode: ArcMode,
}

#[derive(Debug)]
struct Entry {
    duration_millis: u32,
    delay_millis: u32,
    /// The keyframes, sorted by time; a later one at the same time replaced an earlier.
    frames: Vec<Keyframe>,
    /// The times of the stretches: the keyframes', with 0 and the duration, sorted.
    timestamps: Vec<i32>,
    /// The easing of a stretch no keyframe starts, as `defaultEasing`.
    default_easing: Easing,
    /// The arc mode of the same, as `initialArcMode`.
    initial_arc_mode: ArcMode,
    /// Whether some stretch isn't a line, as `requiresArcSpline`.
    requires_arcs: bool,
    /// The arcs of the last start and end values, as `VectorizedKeyframesSpec`'s fields.
    arcs: RefCell<Option<CachedArcs>>,
}

#[derive(Debug)]
struct CachedArcs {
    initial_value: AnimationVector,
    target_value: AnimationVector,
    spline: Rc<ArcSpline>,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Entry) -> bool {
        self.duration_millis == other.duration_millis
            && self.delay_millis == other.delay_millis
            && self.frames == other.frames
            && self.default_easing == other.default_easing
            && self.initial_arc_mode == other.initial_arc_mode
    }
}

impl Entry {
    fn frame_at(&self, time_millis: i32) -> Option<&Keyframe> {
        self.frames
            .binary_search_by_key(&time_millis, |frame| frame.time_millis)
            .ok()
            .map(|index| &self.frames[index])
    }

    /// The entry index with `time_millis` at or after it and before the next, as
    /// `findEntryForTimeMillis`.
    fn index_of(&self, time_millis: i32) -> usize {
        match self.timestamps.binary_search(&time_millis) {
            Ok(index) => index,
            Err(insertion) => insertion.saturating_sub(1),
        }
    }

    /// `time_millis` in seconds, eased within its stretch, as `getEasedTimeFromIndex`.
    fn eased_time(&self, index: usize, time_millis: i32) -> f32 {
        if index + 1 >= self.timestamps.len() {
            return time_millis as f32 / SECONDS_TO_MILLIS;
        }
        let (from, to) = (self.timestamps[index], self.timestamps[index + 1]);
        if time_millis == from {
            return from as f32 / SECONDS_TO_MILLIS;
        }
        let range = (to - from) as f32;
        let easing = self.frame_at(from).map_or(&self.default_easing, |frame| &frame.easing);
        let eased = easing.transform((time_millis - from) as f32 / range);
        (range * eased + from as f32) / SECONDS_TO_MILLIS
    }

    /// The times of the stretches in seconds.
    fn times_in_seconds(&self) -> Vec<f32> {
        self.timestamps.iter().map(|time| *time as f32 / SECONDS_TO_MILLIS).collect()
    }

    /// The value at each of the times: a keyframe's, else the start's at 0 or the end's.
    fn values_with(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
    ) -> Vec<AnimationVector> {
        self.timestamps
            .iter()
            .map(|time| match self.frame_at(*time) {
                Some(frame) => frame.value,
                None if *time == 0 => initial_value,
                None => target_value,
            })
            .collect()
    }

    /// The arcs from `initial_value` to `target_value`, as `init`, kept until they change.
    fn arc_spline(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
    ) -> Rc<ArcSpline> {
        let mut cached = self.arcs.borrow_mut();
        if let Some(cached) = &*cached
            && cached.initial_value == initial_value
            && cached.target_value == target_value
        {
            return cached.spline.clone();
        }
        let modes: Vec<ArcMode> = self
            .timestamps
            .iter()
            .map(|time| self.frame_at(*time).map_or(self.initial_arc_mode, |frame| frame.arc_mode))
            .collect();
        let values = self.values_with(initial_value, target_value);
        let spline = Rc::new(ArcSpline::new(&modes, &self.times_in_seconds(), &values));
        *cached = Some(CachedArcs { initial_value, target_value, spline: spline.clone() });
        spline
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
    pub fn using(self, easing: Easing) -> Self {
        self.frame.easing = easing;
        self
    }

    /// Goes from this keyframe to the next along `arc_mode` (a line by default), as Compose's
    /// `using` of an `ArcMode`: for `keyframes`; `keyframes_with_spline`'s curve is across them all,
    /// and ignores it, where Compose's config doesn't offer it.
    pub fn using_arc(self, arc_mode: ArcMode) -> Self {
        self.frame.arc_mode = arc_mode;
        self
    }
}

impl<T: Animate> KeyframesBuilder<T> {
    /// `value` at `time_millis`, as `value at time`. A later one at the same time replaces it;
    /// times outside 0 to the duration are ignored.
    pub fn at(&mut self, time_millis: i32, value: T) -> KeyframeEntry<'_> {
        let frame = Keyframe {
            time_millis,
            value: value.to_vector(),
            easing: LINEAR_EASING,
            arc_mode: ArcMode::ArcLinear,
        };
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
    let requires_arcs = frames.iter().any(|frame| frame.arc_mode != ArcMode::ArcLinear);
    Entry {
        duration_millis,
        delay_millis,
        frames,
        timestamps,
        default_easing: LINEAR_EASING,
        initial_arc_mode: ArcMode::ArcLinear,
        requires_arcs,
        arcs: RefCell::new(None),
    }
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
        if entry.requires_arcs {
            let spline = entry.arc_spline(initial_value, target_value);
            let position = spline.position(entry.eased_time(entry.index_of(time), time));
            return initial_value.map(|index, _| position[index]);
        }
        // The stretch from the last time at or before it to the next.
        let index = entry.index_of(time);
        let (from, to) = (entry.timestamps[index], entry.timestamps[index + 1]);
        let start = entry.frame_at(from);
        let easing = start.map_or(&entry.default_easing, |frame| &frame.easing);
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
        if entry.requires_arcs {
            let time = clamped as i32;
            let spline = entry.arc_spline(initial_value, target_value);
            let slope = spline.slope(entry.eased_time(entry.index_of(time), time));
            return initial_value.map(|index, _| slope[index]);
        }
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
        let values = entry.values_with(initial_value, target_value);
        let times = entry.times_in_seconds();
        let spline = Rc::new(MonoSpline::new(times, values, inner.periodic_bias));
        *cached = Some(CachedSpline { initial_value, target_value, spline: spline.clone() });
        spline
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
        let entry = &self.inner.entry;
        let index = entry.index_of(time);
        spline.position(entry.eased_time(index, time), index, initial_value)
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
        let entry = &self.inner.entry;
        let index = entry.index_of(time);
        spline.slope(entry.eased_time(index, time), index, initial_velocity)
    }

    fn duration_nanos(&self, _: AnimationVector, _: AnimationVector, _: AnimationVector) -> i64 {
        let entry = &self.inner.entry;
        (i64::from(entry.delay_millis) + i64::from(entry.duration_millis)) * MILLIS_TO_NANOS
    }
}

/// An arc from the start value to the target, as Compose's `ArcAnimationSpec`: built with
/// [`ArcAnimationSpec::new`], [`duration`](Self::duration), [`delay`](Self::delay) and
/// [`easing`](Self::easing), playing as keyframes with none, as Compose's `vectorize`. For positions,
/// and experimental in Compose (`@ExperimentalAnimationSpecApi`): see [`ArcMode`].
#[derive(Clone, PartialEq, Debug)]
pub struct ArcAnimationSpec {
    mode: ArcMode,
    duration_millis: u32,
    delay_millis: u32,
    easing: Easing,
    keyframes: KeyframesSpec,
}

impl ArcAnimationSpec {
    /// `mode` over 300 ms, eased by `FAST_OUT_SLOW_IN_EASING`, with no delay: `ArcAnimationSpec(mode)`.
    pub fn new(mode: ArcMode) -> ArcAnimationSpec {
        let duration = crate::spec::DEFAULT_DURATION_MILLIS;
        ArcAnimationSpec::with(mode, duration, 0, FAST_OUT_SLOW_IN_EASING)
    }

    fn with(mode: ArcMode, duration_millis: u32, delay_millis: u32, easing: Easing) -> Self {
        let duration = duration_millis.min(i32::MAX as u32) as i32;
        let entry = Rc::new(Entry {
            duration_millis,
            delay_millis,
            frames: Vec::new(),
            timestamps: vec![0, duration],
            default_easing: easing.clone(),
            initial_arc_mode: mode,
            requires_arcs: mode != ArcMode::ArcLinear,
            arcs: RefCell::new(None),
        });
        let keyframes = KeyframesSpec { entry, duration_millis, delay_millis };
        ArcAnimationSpec { mode, duration_millis, delay_millis, easing, keyframes }
    }

    /// The same over `duration_millis`, as `durationMillis`.
    pub fn duration(self, duration_millis: u32) -> ArcAnimationSpec {
        ArcAnimationSpec::with(self.mode, duration_millis, self.delay_millis, self.easing)
    }

    /// The same after `delay_millis`, as `delayMillis`.
    pub fn delay(self, delay_millis: u32) -> ArcAnimationSpec {
        ArcAnimationSpec::with(self.mode, self.duration_millis, delay_millis, self.easing)
    }

    /// The same along `easing`, as `easing`.
    pub fn easing(self, easing: Easing) -> ArcAnimationSpec {
        ArcAnimationSpec::with(self.mode, self.duration_millis, self.delay_millis, easing)
    }
}

impl VectorizedAnimationSpec for ArcAnimationSpec {
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        self.keyframes.value_from_nanos(
            play_time_nanos,
            initial_value,
            target_value,
            initial_velocity,
        )
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        self.keyframes.velocity_from_nanos(
            play_time_nanos,
            initial_value,
            target_value,
            initial_velocity,
        )
    }

    fn duration_nanos(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> i64 {
        self.keyframes.duration_nanos(initial_value, target_value, initial_velocity)
    }
}

impl From<ArcAnimationSpec> for KeyframesSpec {
    fn from(spec: ArcAnimationSpec) -> KeyframesSpec {
        spec.keyframes
    }
}
