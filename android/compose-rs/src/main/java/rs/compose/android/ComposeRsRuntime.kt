package rs.compose.android

import android.annotation.SuppressLint
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
     * The setting `ANIMATOR_DURATION_SCALE`, as Compose's `MotionDurationScale` follows it
     * (`WindowRecomposer.android.kt`): read and watched from the first animation's read of it, so
     * a screen with no animation makes no call for it, until no view shows.
     */
    private val animatorScaleObserver = object : ContentObserver(handler) {
        override fun onChange(selfChange: Boolean, uri: Uri?) {
            ComposeRsNative.setAnimatorDurationScale(animatorScale())
        }
    }

    /** The application's context, which lives as long as this object. */
    @SuppressLint("StaticFieldLeak")
    private var applicationContext: Context? = null

    private var animatorScaleWatched = false

    private fun animatorScale(): Float =
        Settings.Global.getFloat(
            applicationContext?.contentResolver ?: return 1f,
            Settings.Global.ANIMATOR_DURATION_SCALE,
            1f,
        )

    /** The first read since it was last watched, from Rust (`bridge.rs`), on the main thread. */
    internal fun readAnimatorScale(): Float {
        val context = applicationContext ?: return 1f
        if (!animatorScaleWatched) {
            animatorScaleWatched = true
            context.contentResolver.registerContentObserver(
                Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE),
                false,
                animatorScaleObserver,
            )
        }
        return animatorScale()
    }

    internal fun attach(view: ComposeRsView) {
        if (views.add(view) && views.size == 1) {
            applicationContext = view.context.applicationContext
        }
    }

    internal fun detach(view: ComposeRsView) {
        if (views.remove(view) && views.isEmpty() && animatorScaleWatched) {
            animatorScaleWatched = false
            applicationContext?.contentResolver?.unregisterContentObserver(animatorScaleObserver)
            ComposeRsNative.readAnimatorScaleAgain()
        }
    }
}
