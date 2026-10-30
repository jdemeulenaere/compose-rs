package rs.compose.android

import android.annotation.TargetApi
import android.graphics.Insets
import android.os.Build
import android.view.DisplayCutout
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsAnimation

/**
 * The window's insets as the content reads them (compose-ui's `PlatformWindowInsets`), kept up
 * to date as Compose's `WindowInsetsHolder` keeps its own (its `InsetsListener`): every type's
 * insets, whether it's shown, the insets of those that hide were they shown, and where the
 * keyboard's animation starts and ends. While an animation of the insets runs (the keyboard
 * sliding in, API 30+), each of its frames' insets are the content's; the final ones once it
 * ends.
 *
 * [values] is what [ComposeRsNative.setWindowInsets] takes, in pixels, 4 per insets (left, top,
 * right, bottom): the 10 types (status bars, navigation bars, caption bar, keyboard, display
 * cutout, system bars, system gestures, mandatory system gestures, tappable element,
 * waterfall); then 5 ignoring visibility (status bars, navigation bars, caption bar, system
 * bars, tappable element); then the keyboard animation's source and target; then a mask of the
 * types hidden, by their index. They're sent when they change, so a keyboard sliding over content
 * that reads none of them costs a state write per changed frame, and no frame.
 *
 * [send] says whether an animation's callback sent them: those run in the vsync's insets animation
 * step, after the frame callbacks and with no traversal of their own (D1365).
 */
internal class ComposeRsInsets(
    private val view: View,
    private val send: (values: IntArray, animating: Boolean) -> Unit,
    private val sendCutoutPath: (FloatArray?) -> Unit,
) : View.OnApplyWindowInsetsListener, Runnable {
    val values = IntArray(SIZE)
    private val sent = IntArray(SIZE)

    /** The display cutout's outline, as [android.graphics.Path.approximate] gives it. */
    var cutoutPath: FloatArray? = null
        private set
    private var cutout: DisplayCutout? = null

    /** Whether the insets are consumed for the views below, as `consumeWindowInsets`. */
    var consumes = false
        set(value) {
            if (field != value) {
                field = value
                // The animation's dispatch mode is given when it's registered.
                if (Build.VERSION.SDK_INT >= 30) Api30.listen(this)
            }
        }

    private var prepared = false
    private var runningAnimationMask = 0
    private var savedInsets: WindowInsets? = null

    /** Listens to the view: insets, and on API 30+ their animations. */
    fun attach() {
        view.setOnApplyWindowInsetsListener(this)
        if (Build.VERSION.SDK_INT >= 30) Api30.listen(this)
    }

    /** The view is attached again: an animation that ran while it was away told it nothing. */
    fun resetState() {
        prepared = false
        runningAnimationMask = 0
        savedInsets = null
    }

    override fun onApplyWindowInsets(view: View, insets: WindowInsets): WindowInsets {
        // The most recent insets, which an animation's end takes.
        savedInsets = insets
        putIme(TARGET, insets)
        if (prepared) {
            // On R a cancelled animation may not end: what came after it is taken if nothing did.
            if (Build.VERSION.SDK_INT == 30) view.post(this)
        } else if (runningAnimationMask == 0) {
            // While an animation runs, its progress is the insets.
            putIme(SOURCE, insets)
            update(insets)
        }
        sendIfChanged()
        return consumed(insets)
    }

    override fun run() {
        if (prepared) {
            runningAnimationMask = 0
            prepared = false
            savedInsets?.let {
                putIme(SOURCE, it)
                update(it)
                savedInsets = null
                sendIfChanged()
            }
        }
    }

    private fun sendIfChanged(animating: Boolean = false) {
        if (!values.contentEquals(sent)) {
            values.copyInto(sent)
            send(values, animating)
        }
    }

    @Suppress("DEPRECATION")
    private fun consumed(insets: WindowInsets): WindowInsets = when {
        !consumes -> insets
        Build.VERSION.SDK_INT >= 30 -> WindowInsets.CONSUMED
        else -> insets.consumeDisplayCutout().consumeStableInsets().consumeSystemWindowInsets()
    }

    private fun put(index: Int, left: Int, top: Int, right: Int, bottom: Int) {
        values[index * 4] = left
        values[index * 4 + 1] = top
        values[index * 4 + 2] = right
        values[index * 4 + 3] = bottom
    }

    private fun put(index: Int, insets: Insets) =
        put(index, insets.left, insets.top, insets.right, insets.bottom)

    @Suppress("DEPRECATION")
    private fun putIme(index: Int, insets: WindowInsets) {
        if (Build.VERSION.SDK_INT >= 30) {
            put(index, insets.getInsets(WindowInsets.Type.ime()))
        } else {
            put(index, 0, 0, 0, api29ImeBottom(insets))
        }
    }

    /** API 29: the keyboard, what the system windows take below the stable insets. */
    @Suppress("DEPRECATION")
    private fun api29ImeBottom(insets: WindowInsets): Int {
        val bottom = insets.systemWindowInsets.bottom
        return if (bottom > insets.stableInsets.bottom) bottom else 0
    }

    /** Every type's insets and visibility from [insets]. */
    @Suppress("DEPRECATION")
    private fun update(insets: WindowInsets) {
        if (Build.VERSION.SDK_INT >= 30) {
            Api30.read(this, insets)
            if (insets.displayCutout != cutout) {
                cutout = insets.displayCutout
                val points = if (Build.VERSION.SDK_INT >= 31) Api31.cutoutPath(cutout) else null
                cutoutPath = points
                sendCutoutPath(points)
            }
            return
        }
        // API 29: from the system windows' insets, as WindowInsetsCompat: the status bar at the
        // top, the navigation bar up to the stable insets, the keyboard above them; the stable
        // insets stand for those ignoring visibility. Shown: what has insets, of the status and
        // navigation bars, the keyboard and a cutout; no caption bar (so no system bars, which
        // include it); the rest always.
        val system = insets.systemWindowInsets
        val stable = insets.stableInsets
        val navigationBottom = minOf(system.bottom, stable.bottom)
        val ime = api29ImeBottom(insets)
        put(STATUS_BARS, 0, system.top, 0, 0)
        put(NAVIGATION_BARS, system.left, 0, system.right, navigationBottom)
        put(IME, 0, 0, 0, ime)
        put(SYSTEM_BARS, system.left, system.top, system.right, navigationBottom)
        put(SYSTEM_GESTURES, insets.systemGestureInsets)
        put(MANDATORY_SYSTEM_GESTURES, insets.mandatorySystemGestureInsets)
        put(TAPPABLE_ELEMENT, insets.tappableElementInsets)
        val cutout = insets.displayCutout
        put(
            DISPLAY_CUTOUT,
            cutout?.safeInsetLeft ?: 0,
            cutout?.safeInsetTop ?: 0,
            cutout?.safeInsetRight ?: 0,
            cutout?.safeInsetBottom ?: 0,
        )
        put(IGNORING_VISIBILITY, 0, maxOf(stable.top, system.top), 0, 0)
        put(IGNORING_VISIBILITY + 1, stable.left, 0, stable.right, stable.bottom)
        put(IGNORING_VISIBILITY + 3, stable)
        put(IGNORING_VISIBILITY + 4, insets.tappableElementInsets)
        var hidden = (1 shl CAPTION_BAR) or (1 shl SYSTEM_BARS)
        if (system.top == 0) hidden = hidden or (1 shl STATUS_BARS)
        if (system.left == 0 && system.right == 0 && navigationBottom == 0) {
            hidden = hidden or (1 shl NAVIGATION_BARS)
        }
        if (ime == 0) hidden = hidden or (1 shl IME)
        if (cutout == null ||
            cutout.safeInsetLeft == 0 && cutout.safeInsetTop == 0 &&
            cutout.safeInsetRight == 0 && cutout.safeInsetBottom == 0
        ) {
            hidden = hidden or (1 shl DISPLAY_CUTOUT)
        }
        values[HIDDEN] = hidden
    }

    @TargetApi(31)
    private object Api31 {
        fun cutoutPath(cutout: DisplayCutout?): FloatArray? = cutout?.cutoutPath?.approximate(0.5f)
    }

    @TargetApi(30)
    private object Api30 {
        val types = intArrayOf(
            WindowInsets.Type.statusBars(),
            WindowInsets.Type.navigationBars(),
            WindowInsets.Type.captionBar(),
            WindowInsets.Type.ime(),
            WindowInsets.Type.displayCutout(),
            WindowInsets.Type.systemBars(),
            WindowInsets.Type.systemGestures(),
            WindowInsets.Type.mandatorySystemGestures(),
            WindowInsets.Type.tappableElement(),
        )
        val ignoringVisibility = intArrayOf(
            WindowInsets.Type.statusBars(),
            WindowInsets.Type.navigationBars(),
            WindowInsets.Type.captionBar(),
            WindowInsets.Type.systemBars(),
            WindowInsets.Type.tappableElement(),
        )

        fun read(target: ComposeRsInsets, insets: WindowInsets) {
            var hidden = 0
            for ((index, type) in types.withIndex()) {
                target.put(index, insets.getInsets(type))
                if (!insets.isVisible(type)) hidden = hidden or (1 shl index)
            }
            target.put(WATERFALL, insets.displayCutout?.waterfallInsets ?: Insets.NONE)
            for ((index, type) in ignoringVisibility.withIndex()) {
                target.put(IGNORING_VISIBILITY + index, insets.getInsetsIgnoringVisibility(type))
            }
            target.values[HIDDEN] = hidden
        }

        fun listen(target: ComposeRsInsets) {
            val mode = if (target.consumes) {
                WindowInsetsAnimation.Callback.DISPATCH_MODE_STOP
            } else {
                WindowInsetsAnimation.Callback.DISPATCH_MODE_CONTINUE_ON_SUBTREE
            }
            target.view.setWindowInsetsAnimationCallback(
                object : WindowInsetsAnimation.Callback(mode) {
                    override fun onPrepare(animation: WindowInsetsAnimation) {
                        target.prepared = true
                    }

                    override fun onStart(
                        animation: WindowInsetsAnimation,
                        bounds: WindowInsetsAnimation.Bounds,
                    ): WindowInsetsAnimation.Bounds {
                        target.prepared = false
                        if (animation.durationMillis != 0L) {
                            target.runningAnimationMask =
                                target.runningAnimationMask or animation.typeMask
                        }
                        return bounds
                    }

                    override fun onProgress(
                        insets: WindowInsets,
                        runningAnimations: MutableList<WindowInsetsAnimation>,
                    ): WindowInsets {
                        target.update(insets)
                        target.sendIfChanged(animating = true)
                        return target.consumed(insets)
                    }

                    override fun onEnd(animation: WindowInsetsAnimation) {
                        target.prepared = false
                        val type = animation.typeMask
                        val insets = target.savedInsets
                        target.runningAnimationMask = target.runningAnimationMask and type.inv()
                        val allFinished = target.runningAnimationMask == 0
                        if (insets != null) {
                            if (allFinished || type and WindowInsets.Type.ime() != 0) {
                                target.putIme(SOURCE, insets)
                                target.putIme(TARGET, insets)
                            }
                            if (allFinished) target.update(insets)
                            target.sendIfChanged(animating = true)
                        }
                        if (allFinished) target.savedInsets = null
                    }
                },
            )
        }
    }

    companion object {
        const val STATUS_BARS = 0
        const val NAVIGATION_BARS = 1
        const val CAPTION_BAR = 2
        const val IME = 3
        const val DISPLAY_CUTOUT = 4
        const val SYSTEM_BARS = 5
        const val SYSTEM_GESTURES = 6
        const val MANDATORY_SYSTEM_GESTURES = 7
        const val TAPPABLE_ELEMENT = 8
        const val WATERFALL = 9
        const val IGNORING_VISIBILITY = 10
        const val SOURCE = 15
        const val TARGET = 16
        const val HIDDEN = 17 * 4

        /** The ints [values] holds. */
        const val SIZE = HIDDEN + 1
    }
}
