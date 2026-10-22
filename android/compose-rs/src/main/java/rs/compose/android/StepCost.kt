package rs.compose.android

/**
 * How long a step of idle-time work takes (ns), as Rust's prefetch steps estimate theirs
 * (`StepCost` in lazy.rs, D341): a running average and the recent maximum, which loses an
 * eighth with each step timed. A step is expected to take the larger, so one much slower than
 * the average (a cold path, a little core) keeps the next from overrunning the next frame.
 */
internal class StepCost {
    private var average = 0L
    private var peak = 0L
    /** Steps timed, up to [SETTLED_STEPS]: an average of fewer may be one outlier. */
    private var timed = 0

    /** How long the next step is expected to take. */
    fun estimate(): Long = maxOf(average, peak)

    /** A step took [nanos]. */
    fun timed(nanos: Long) {
        average = if (timed == 0) nanos else average / 4 * 3 + nanos / 4
        peak = maxOf(nanos, peak - peak / 8)
        if (timed < SETTLED_STEPS) timed++
    }

    /**
     * The step didn't fit the time left, as the [first] step of its idle time or not: lowered
     * (slowly once settled) only when first, as Rust's, so nothing stops it for good.
     */
    fun missed(first: Boolean) {
        if (!first) return
        average = if (timed < SETTLED_STEPS) average / 4 * 3 else average / 16 * 15
        peak = maxOf(average, peak / 4 * 3)
    }

    private companion object {
        const val SETTLED_STEPS = 4
    }
}
