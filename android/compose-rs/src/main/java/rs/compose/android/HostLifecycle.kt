package rs.compose.android

import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.view.View

/** The ordinals of AndroidX's `Lifecycle.State`, what the Rust side takes. */
internal object LifecycleStates {
    const val CREATED = 2
    const val STARTED = 3
    const val RESUMED = 4
}

/** What a view's lifecycle follows, until [remove]d. */
internal interface LifecycleSource {
    /** Where it is now, by `Lifecycle.State`'s ordinal. */
    val state: Int

    fun remove()

    companion object {
        /**
         * What [view]'s lifecycle follows, telling [onState] of each change: its view tree's
         * `LifecycleOwner` if it has one, as Compose's `LocalLifecycleOwner`; else its activity's
         * callbacks; else, attached, it stays resumed.
         */
        fun of(view: View, onState: (Int) -> Unit): LifecycleSource {
            if (androidXPresent) {
                try {
                    AndroidXLifecycle.of(view, onState)?.let { return it }
                } catch (_: NoClassDefFoundError) {
                    // The app has no AndroidX lifecycle (or R8 removed it).
                    androidXPresent = false
                }
            }
            val activity = view.context.activity()
            return if (activity != null) ActivityLifecycle(activity, onState) else Always
        }

        private var androidXPresent = true
    }
}

private object Always : LifecycleSource {
    override val state: Int
        get() = LifecycleStates.RESUMED

    override fun remove() {}
}

/** A plain activity's callbacks. */
private class ActivityLifecycle(
    private val activity: Activity,
    private val onState: (Int) -> Unit,
) : LifecycleSource, Application.ActivityLifecycleCallbacks {
    /** A plain activity tells no state: its window focused, it's taken as resumed. */
    override var state: Int =
        if (activity.hasWindowFocus()) LifecycleStates.RESUMED else LifecycleStates.STARTED
        private set

    init {
        activity.registerActivityLifecycleCallbacks(this)
    }

    private fun move(to: Activity, state: Int) {
        if (to === activity && state != this.state) {
            this.state = state
            onState(state)
        }
    }

    override fun remove() = activity.unregisterActivityLifecycleCallbacks(this)

    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {}

    override fun onActivityStarted(activity: Activity) = move(activity, LifecycleStates.STARTED)

    override fun onActivityResumed(activity: Activity) = move(activity, LifecycleStates.RESUMED)

    override fun onActivityPaused(activity: Activity) = move(activity, LifecycleStates.STARTED)

    override fun onActivityStopped(activity: Activity) = move(activity, LifecycleStates.CREATED)

    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}

    override fun onActivityDestroyed(activity: Activity) {}
}
