package rs.compose.android

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.os.Build
import android.view.View
import android.window.BackEvent
import android.window.OnBackAnimationCallback
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
 * back), and the back key when it reaches the view. A gesture's progress, on API 34+, goes to
 * [BackGesture] by either way.
 */
internal interface BackRegistration {
    fun setEnabled(enabled: Boolean)

    fun remove()

    /** Passes the back the content didn't take to what's behind the view. */
    fun passOn()

    /** Whether back keys reaching the view are handled there (no dispatcher takes them). */
    val handlesKeys: Boolean

    companion object {
        /** A registration for [view], telling [gesture] of back while enabled (initially not). */
        fun create(view: View, gesture: BackGesture): BackRegistration {
            val activity = view.context.activity()
            if (activity != null && androidXPresent && AndroidXPresence.ofBackDispatcher(activity)) {
                try {
                    AndroidXBack.register(activity, gesture)?.let { return it }
                } catch (_: LinkageError) {
                    // The app has no AndroidX activity (or R8 removed it): the platform's way.
                    androidXPresent = false
                }
            }
            return PlatformBack(view, gesture)
        }

        /**
         * Whether AndroidX's activity classes may be there: found by referring to them directly
         * (so R8 keeps and renames them consistently), a `NoClassDefFoundError` meaning not.
         */
        private var androidXPresent = true
    }
}

/** What a view does with a back gesture, or a press: [onBack] is its end, taken. */
internal interface BackGesture {
    fun onBack()

    fun onBackStarted(swipeEdge: Int, progress: Float, touchX: Float, touchY: Float, frameTimeMillis: Long)

    fun onBackProgressed(swipeEdge: Int, progress: Float, touchX: Float, touchY: Float, frameTimeMillis: Long)

    fun onBackCancelled()
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
private class PlatformBack(private val view: View, private val gesture: BackGesture) : BackRegistration {
    /** The dispatcher it registered with and its callback (API 33+), unregistered from it. */
    private var registered: Pair<Any, Any>? = null
    private var enabled = false

    override val handlesKeys: Boolean
        get() = enabled

    override fun setEnabled(enabled: Boolean) {
        this.enabled = enabled
        if (Build.VERSION.SDK_INT < 33) return
        val current = registered
        if (enabled && current == null) {
            val dispatcher = view.findOnBackInvokedDispatcher() ?: return
            val callback: OnBackInvokedCallback = if (Build.VERSION.SDK_INT >= 34) {
                // The gesture's progress, as AndroidX's `OnBackInvokedInput`.
                object : OnBackAnimationCallback {
                    override fun onBackStarted(event: BackEvent) {
                        gesture.onBackStarted(
                            event.swipeEdge,
                            event.progress,
                            event.touchX,
                            event.touchY,
                            // Where it tells, as AndroidX's `NavigationEvent(BackEvent)`.
                            if (Build.VERSION.SDK_INT >= 36) event.frameTimeMillis else 0L,
                        )
                    }

                    override fun onBackProgressed(event: BackEvent) {
                        gesture.onBackProgressed(
                            event.swipeEdge,
                            event.progress,
                            event.touchX,
                            event.touchY,
                            if (Build.VERSION.SDK_INT >= 36) event.frameTimeMillis else 0L,
                        )
                    }

                    override fun onBackInvoked() = gesture.onBack()

                    override fun onBackCancelled() = gesture.onBackCancelled()
                }
            } else {
                OnBackInvokedCallback { gesture.onBack() }
            }
            dispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback)
            registered = dispatcher to callback
        } else if (!enabled && current != null) {
            (current.first as OnBackInvokedDispatcher).unregisterOnBackInvokedCallback(current.second as OnBackInvokedCallback)
            registered = null
        }
    }

    override fun remove() = setEnabled(false)

    override fun passOn() {
        // The content's handler went in the meantime: this back goes to the activity, as it
        // would have without the callback (its default finishes it).
        setEnabled(false)
        @Suppress("DEPRECATION")
        view.context.activity()?.onBackPressed()
    }
}
