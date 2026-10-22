package rs.compose.android

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.os.Build
import android.view.View
import android.window.OnBackInvokedCallback
import android.window.OnBackInvokedDispatcher

/**
 * Back for a `ComposeRsView`, intercepted only while the content has an enabled `BackHandler`
 * (otherwise the activity gets it and finishes, as usual).
 *
 * In an AndroidX `ComponentActivity` (Kotlin Compose apps), a callback on its
 * `OnBackPressedDispatcher`, so it takes its place among the activity's own (Kotlin
 * `BackHandler`s, fragments), the last added first. Without AndroidX (the library doesn't depend
 * on it), the window's `OnBackInvokedDispatcher` (API 33+, for apps that opted into predictive
 * back), and the back key when it reaches the view.
 */
internal interface BackRegistration {
    fun setEnabled(enabled: Boolean)

    fun remove()

    /** Passes the back the content didn't take to what's behind the view. */
    fun passOn()

    /** Whether back keys reaching the view are handled there (no dispatcher takes them). */
    val handlesKeys: Boolean

    companion object {
        /** A registration for [view], calling [onBack] on back while enabled (initially not). */
        fun create(view: View, onBack: () -> Unit): BackRegistration {
            val activity = view.context.activity()
            if (activity != null && androidXPresent) {
                AndroidXBack.register(activity, onBack)?.let { return it }
            }
            return PlatformBack(view, onBack)
        }

        private val androidXPresent: Boolean by lazy {
            try {
                Class.forName("androidx.activity.OnBackPressedDispatcherOwner")
                true
            } catch (_: ClassNotFoundException) {
                false
            }
        }
    }
}

internal fun Context.activity(): Activity? {
    var context: Context? = this
    while (context is ContextWrapper) {
        if (context is Activity) return context
        context = context.baseContext
    }
    return null
}

/** The window's `OnBackInvokedDispatcher` (API 33+), and back keys reaching the view. */
private class PlatformBack(private val view: View, private val onBack: () -> Unit) : BackRegistration {
    private var registered: Any? = null
    private var enabled = false

    override val handlesKeys: Boolean
        get() = enabled

    override fun setEnabled(enabled: Boolean) {
        this.enabled = enabled
        if (Build.VERSION.SDK_INT < 33) return
        val dispatcher = view.findOnBackInvokedDispatcher() ?: return
        val current = registered as OnBackInvokedCallback?
        if (enabled && current == null) {
            val callback = OnBackInvokedCallback { onBack() }
            dispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback)
            registered = callback
        } else if (!enabled && current != null) {
            dispatcher.unregisterOnBackInvokedCallback(current)
            registered = null
        }
    }

    override fun remove() = setEnabled(false)

    override fun passOn() {
        // Nothing enabled any more (the handler went in the meantime): the next back goes on.
        setEnabled(false)
    }
}
