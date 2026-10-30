package rs.compose.android

import android.annotation.SuppressLint
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
            if (androidXPresent && AndroidXPresence.ofLifecycle(view)) {
                try {
                    AndroidXLifecycle.of(view, onState)?.let { return it }
                } catch (_: LinkageError) {
                    // The app has no AndroidX lifecycle (or R8 removed it).
                    androidXPresent = false
                }
            }
            val activity = view.context.activity()
            return if (activity != null) ActivityLifecycle(activity, view, onState) else Always
        }

        private var androidXPresent = true
    }
}

private object Always : LifecycleSource {
    override val state: Int
        get() = LifecycleStates.RESUMED

    override fun remove() {}
}

/**
 * A view of a plain activity, following its [ActivityStates].
 */
private class ActivityLifecycle(
    activity: Activity,
    view: View,
    private val onState: (Int) -> Unit,
) : LifecycleSource {
    private val states = ActivityStates.of(activity, view).also { it.listeners.add(onState) }

    override val state: Int
        get() = states.state

    override fun remove() {
        states.listeners.remove(onState)
    }
}

/**
 * A plain activity's state, heard from its callbacks from when its first view attached until it's
 * destroyed, so a view attaching again knows what was missed. Nothing tells the state before that,
 * and the callbacks of a first launch have passed by the time its window attaches its views, with
 * the window's focus still to come: visible, an activity is taken as resumed until it says
 * otherwise (D840).
 */
private class ActivityStates(
    private val activity: Activity,
    var state: Int,
) : Application.ActivityLifecycleCallbacks {
    val listeners = ArrayList<(Int) -> Unit>(1)

    init {
        activity.registerActivityLifecycleCallbacks(this)
    }

    private fun move(state: Int) {
        if (state != this.state) {
            this.state = state
            for (listener in listeners.toTypedArray()) listener(state)
        }
    }

    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {}

    override fun onActivityStarted(activity: Activity) = move(LifecycleStates.STARTED)

    override fun onActivityResumed(activity: Activity) = move(LifecycleStates.RESUMED)

    override fun onActivityPaused(activity: Activity) = move(LifecycleStates.STARTED)

    override fun onActivityStopped(activity: Activity) = move(LifecycleStates.CREATED)

    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}

    override fun onActivityDestroyed(activity: Activity) {
        activity.unregisterActivityLifecycleCallbacks(this)
        all.remove(activity)
    }

    companion object {
        private val all = HashMap<Activity, ActivityStates>()

        fun of(activity: Activity, view: View): ActivityStates =
            all.getOrPut(activity) {
                val visible = view.windowVisibility == View.VISIBLE
                ActivityStates(
                    activity,
                    if (visible) LifecycleStates.RESUMED else LifecycleStates.CREATED,
                )
            }
    }
}

/**
 * Whether an app may have AndroidX's owners, told without touching AndroidX: where it's missing,
 * each first use costs a `NoClassDefFoundError` at a cold start. An AndroidX activity is one by the
 * names of its interfaces; a tree with an owner set by hand under another context has AndroidX's
 * tag ids among the app's resources. (A hand-set owner under a plain activity in an app with
 * obfuscated resource names isn't found: its callbacks are followed, D840.)
 */
internal object AndroidXPresence {
    private var lifecycleIds = -1
    private var viewModelIds = -1

    fun ofLifecycle(view: View): Boolean {
        if (view.context.activity().implements("androidx.lifecycle.LifecycleOwner")) return true
        if (lifecycleIds < 0) lifecycleIds = view.resourceId("view_tree_lifecycle_owner")
        return lifecycleIds != 0
    }

    fun ofViewModels(view: View): Boolean {
        if (view.context.activity().implements("androidx.lifecycle.ViewModelStoreOwner")) return true
        if (viewModelIds < 0) viewModelIds = view.resourceId("view_tree_view_model_store_owner")
        return viewModelIds != 0
    }

    @SuppressLint("DiscouragedApi")
    private fun View.resourceId(name: String): Int =
        resources.getIdentifier(name, "id", context.packageName)

    private fun Activity?.implements(name: String): Boolean {
        var type: Class<*>? = (this ?: return false).javaClass
        while (type != null) {
            if (type.interfaces.any { it.name == name }) return true
            type = type.superclass
        }
        return false
    }
}
