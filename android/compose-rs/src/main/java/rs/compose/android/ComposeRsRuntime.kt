package rs.compose.android

import android.content.Context
import android.database.ContentObserver
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The executor's side of the main thread: woken tasks and `delay`s run in ticks posted to the
 * main looper, and the views showing compose-rs get a frame when a task waits for one.
 */
object ComposeRsRuntime {
    private val handler = Handler(Looper.getMainLooper())
    private val posted = AtomicBoolean(false)
    private val views = LinkedHashSet<ComposeRsView>()
    /** When the next timer tick is posted for (uptime ms), or 0. */
    private var timerAt = 0L

    /** Loads the app's compose-rs library (`System.loadLibrary`), once. */
    @JvmStatic
    fun loadLibrary(name: String) {
        System.loadLibrary(name)
    }

    private fun tick() {
        val result = ComposeRsNative.tick()
        if (result and 1L != 0L) {
            for (view in views) view.requestFrame()
        }
        schedule((result shr 1) - 1)
    }

    private val wakeTick = Runnable {
        posted.set(false)
        tick()
    }

    private val timerTick = Runnable {
        timerAt = 0L
        tick()
    }

    /** A task woke (on any thread). */
    internal fun wake() {
        if (posted.compareAndSet(false, true)) handler.post(wakeTick)
    }

    /** The next timer ends in [millis] (-1: none): a tick then. */
    internal fun schedule(millis: Long) {
        if (millis < 0) return
        val at = SystemClock.uptimeMillis() + millis
        if (timerAt != 0L && timerAt <= at) return
        timerAt = at
        handler.removeCallbacks(timerTick)
        handler.postAtTime(timerTick, at)
    }

    /**
     * The developer option's and battery saver's `ANIMATOR_DURATION_SCALE`, as Compose's
     * `MotionDurationScale` follows it (`WindowRecomposer.android.kt`): watched while a view
     * shows, read again when the first one attaches.
     */
    private val animatorScaleObserver = object : ContentObserver(handler) {
        override fun onChange(selfChange: Boolean, uri: Uri?) = passAnimatorScale()
    }
    private var animatorScaleContext: Context? = null

    private fun passAnimatorScale() {
        val context = animatorScaleContext ?: return
        ComposeRsNative.setAnimatorDurationScale(
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            ),
        )
    }

    internal fun attach(view: ComposeRsView) {
        if (views.add(view) && views.size == 1) {
            val context = view.context.applicationContext
            animatorScaleContext = context
            context.contentResolver.registerContentObserver(
                Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE),
                false,
                animatorScaleObserver,
            )
            passAnimatorScale()
        }
    }

    internal fun detach(view: ComposeRsView) {
        if (views.remove(view) && views.isEmpty()) {
            animatorScaleContext?.contentResolver?.unregisterContentObserver(animatorScaleObserver)
            animatorScaleContext = null
        }
    }
}
