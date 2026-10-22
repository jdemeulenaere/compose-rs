//! When the next frame starts, for the work done between frames (prefetch, D340).
//!
//! A Choreographer frame's time isn't always its vsync's: after a display stall, Android 16's
//! "buffer stuffing recovery" hands every frame callback the vsync's time less a refresh
//! interval, until the app goes idle, so animations catch up smoothly. The next frame then
//! starts two intervals after that time, not one. The vsync's phase is found again as
//! Choreographer's own jitter correction finds it: the last vsync before the callback started.

/// When the frame after the one whose callback got `frame_time` and started at `started`
/// should start (`System.nanoTime`), with vsyncs every `interval`: the vsync after the last one
/// at or before `started`, on `frame_time`'s phase. That is `frame_time + interval` unless the
/// frame time lags the callback by an interval or more (shifted by buffer stuffing recovery, or
/// the callback came late). Taken from the callback's start rather than from when the idle work
/// runs: a frame requested during a callback that ended after the next vsync is due at that
/// vsync, already past. Without a frame time (0) or an interval, 0: no time.
pub(crate) fn next_frame_nanos(frame_time: i64, interval: i64, started: i64) -> i64 {
    if frame_time <= 0 || interval <= 0 {
        return 0;
    }
    let intervals = started.saturating_sub(frame_time).max(0) / interval;
    frame_time.saturating_add(interval.saturating_mul(intervals.saturating_add(1)))
}

#[cfg(test)]
mod tests {
    use super::next_frame_nanos;

    const MS: i64 = 1_000_000;
    /// 120 Hz.
    const INTERVAL: i64 = 8_333_333;
    /// A vsync's time.
    const VSYNC: i64 = 1_000_000 * MS;

    #[test]
    fn a_frame_on_time_is_followed_an_interval_later() {
        // The callback runs about 1 ms after its vsync.
        assert_eq!(next_frame_nanos(VSYNC, INTERVAL, VSYNC + MS), VSYNC + INTERVAL);
        assert_eq!(next_frame_nanos(VSYNC, INTERVAL, VSYNC), VSYNC + INTERVAL);
        assert_eq!(next_frame_nanos(VSYNC, INTERVAL, VSYNC + INTERVAL - 1), VSYNC + INTERVAL);
    }

    #[test]
    fn a_time_shifted_by_buffer_stuffing_recovery_keeps_the_vsyncs_phase() {
        // Recovery hands the callback the vsync's time less an interval.
        let shifted = VSYNC - INTERVAL;
        let next = next_frame_nanos(shifted, INTERVAL, VSYNC + MS);
        assert_eq!(next, VSYNC + INTERVAL);
        // The old budget, the shifted time plus an interval, was past before the idle time
        // began: no prefetch for as long as recovery lasted.
        let idle_starts = VSYNC + 3 * MS;
        assert!(shifted + INTERVAL - idle_starts < 0);
        assert_eq!(next - idle_starts, INTERVAL - 3 * MS);
    }

    #[test]
    fn a_callback_late_by_intervals_is_followed_by_the_vsync_after_it_started() {
        // Choreographer moves a late frame's time to the last vsync before it ran; if it
        // doesn't (a shifted time on top), the phase still holds.
        for late in 1..5 {
            let started = VSYNC + late * INTERVAL + 2 * MS;
            let next = next_frame_nanos(VSYNC, INTERVAL, started);
            assert_eq!(next, VSYNC + (late + 1) * INTERVAL);
            assert!(next > started && next - started <= INTERVAL);
        }
    }

    #[test]
    fn without_a_frame_time_or_an_interval_there_is_no_time() {
        assert_eq!(next_frame_nanos(0, INTERVAL, VSYNC), 0);
        assert_eq!(next_frame_nanos(VSYNC, 0, VSYNC), 0);
        // A start before the frame time (not on one clock) counts as the frame's own.
        assert_eq!(next_frame_nanos(VSYNC, INTERVAL, VSYNC - MS), VSYNC + INTERVAL);
    }
}
