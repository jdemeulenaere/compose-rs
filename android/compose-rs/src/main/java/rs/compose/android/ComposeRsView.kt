package rs.compose.android

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.res.Configuration
import android.graphics.Canvas
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.Parcelable
import android.os.SystemClock
import android.os.Trace
import android.util.Log
import android.view.Choreographer
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.ViewTreeObserver
import android.view.WindowInsets
import android.view.accessibility.AccessibilityManager
import android.view.accessibility.AccessibilityNodeProvider
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager

/**
 * A view showing a compose-rs app (`compose_android::export_apps!` names them), as a plain
 * `View`: in a Kotlin Compose UI through `AndroidView`, it's transformed, clipped and scrolled as
 * any content (docs/07-platforms.md, "Embedding").
 *
 * On a Choreographer frame (asked for when the app's state changes), Rust recomposes, lays out
 * and records a display list; [onDraw] replays it on the view's (hardware) canvas. After a frame,
 * until the next one should start, lazy lists that scrolled compose and measure the items they'll
 * reach next (prefetch, as Compose's `AndroidPrefetchScheduler`). While the window is hidden, no
 * frame is asked for: the one missed is made when it shows. The composition lives while the
 * view is attached to a window.
 *
 * What the content's `remember_saveable`s hold survives the composition: kept when the view
 * detaches (and given back when it attaches again), and saved in the view's instance state (an
 * activity recreated for a configuration change, or after the process was killed), which needs
 * the view to have an id: it takes `R.id.compose_rs_view` unless given another (two views in one
 * window need ids of their own). Back goes to the content's `BackHandler`s while one is enabled.
 * The app receives properties ([setProp]) and sends events ([setOnEventListener]).
 */
class ComposeRsView(context: Context, private val app: String) : View(context) {
    private val measurer =
        TextMeasurer(checkBoring = context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0)
    private val replayer = Replayer(measurer)
    /** Over the Rust host's drawing (see [ComposeRsNative.takeDrawing]). */
    private var drawing: java.nio.ByteBuffer? = null
    private var host = 0L
    private var frameRequested = false
    /** A frame was asked for while the window was hidden: made when it shows ([requestFrame]). */
    private var frameWanted = false
    private var textInput = false
    private val inputMethods = context.getSystemService(InputMethodManager::class.java)

    private val frameCallback = Choreographer.FrameCallback { time ->
        noteFrameTime(time)
        frame()
    }

    /**
     * The last Choreographer callback's frame time and when it started (`System.nanoTime`):
     * where the next frame starts comes from both ([ComposeRsNative.nextFrameNanos]), as the
     * frame time may lag its vsync by an interval (buffer stuffing recovery: D340).
     */
    private var frameTimeNanos = 0L
    private var frameStartedNanos = 0L
    private var prefetchScheduled = false
    private val prefetchRunnable = Runnable { prefetch() }

    /** Idle times in a row that had no time before the next frame (see [prefetch]). */
    private var idleTimesWithoutTime = 0

    /** How long drawing an item ahead and recording its groups takes ([recordAhead]). */
    private val drawAheadCost = StepCost()

    /** Prefetch work left: tried again after the next frame. */
    private val prefetchFrameCallback = Choreographer.FrameCallback { time ->
        noteFrameTime(time)
        post(prefetchRunnable)
    }

    private fun noteFrameTime(time: Long) {
        frameTimeNanos = time
        frameStartedNanos = System.nanoTime()
    }
    private val accessibility = Accessibility(this)
    private val accessibilityManager = context.getSystemService(AccessibilityManager::class.java)
    /** A change of semantics is told to accessibility services at most this often. */
    private var semanticsChangePosted = false
    private val semanticsChanged = Runnable {
        semanticsChangePosted = false
        accessibility.invalidate()
    }

    /** What the last composition saved (or the instance state restored), for the next one. */
    private var savedState: ByteArray? = null

    /** Back, intercepted while the content has an enabled `BackHandler`. */
    private var back: BackRegistration? = null
    private var backEnabled = false

    /** The window's insets, 4 per kind (see [ComposeRsNative.setWindowInsets]). */
    private val insets = IntArray(20)

    /** The properties given to the content, set again on each composition. */
    private val props = LinkedHashMap<String, Any>()
    private var eventListener: OnEventListener? = null
    private val mainHandler = Handler(Looper.getMainLooper())

    /** Receives the events the content sends (`LOCAL_HOST_EVENTS`). */
    fun interface OnEventListener {
        fun onEvent(name: String, payload: String)
    }

    init {
        isFocusable = true
        isFocusableInTouchMode = true
        // Its instance state is saved only with an id.
        if (id == NO_ID) id = R.id.compose_rs_view
        setOnApplyWindowInsetsListener { _, windowInsets ->
            updateInsets(windowInsets)
            windowInsets
        }
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        ComposeRsRuntime.attach(this)
        requestApplyInsets()
        if (width > 0 && height > 0) create()
    }

    override fun onDetachedFromWindow() {
        ComposeRsRuntime.detach(this)
        destroy()
        back?.remove()
        back = null
        backEnabled = false
        super.onDetachedFromWindow()
    }

    override fun onSizeChanged(width: Int, height: Int, oldWidth: Int, oldHeight: Int) {
        super.onSizeChanged(width, height, oldWidth, oldHeight)
        if (host == 0L) {
            if (isAttachedToWindow && width > 0 && height > 0) create()
        } else {
            ComposeRsNative.resize(host, width, height)
            requestFrame()
        }
    }

    private fun create() {
        if (host != 0L) return
        val metrics = resources.displayMetrics
        host = ComposeRsNative.create(
            this,
            measurer,
            measurer.results,
            app,
            width,
            height,
            metrics.density,
            resources.configuration.fontScale,
            layoutDirection == LAYOUT_DIRECTION_RTL,
            savedState,
        )
        savedState = null
        sendConfiguration(resources.configuration)
        ComposeRsNative.setWindowInsets(host, insets)
        for ((name, value) in props) sendProp(name, value)
        // Composed, laid out and recorded now (during the window's layout), so the first frame
        // the window draws shows the content, as a Compose view's does.
        frame()
    }

    private fun destroy() {
        if (host == 0L) return
        // Given back if it's composed again (attached again, or a new view of the same id).
        savedState = ComposeRsNative.saveState(host)
        cancelFrames()
        ComposeRsNative.destroy(host)
        host = 0L
        drawing = null
        measurer.clear()
        replayer.clear()
        if (textInput) {
            textInput = false
            inputMethods.hideSoftInputFromWindow(windowToken, 0)
        }
    }

    override fun onConfigurationChanged(configuration: Configuration) {
        super.onConfigurationChanged(configuration)
        if (host != 0L) {
            ComposeRsNative.setDensity(host, resources.displayMetrics.density, configuration.fontScale)
            sendConfiguration(configuration)
            requestFrame()
        }
    }

    private fun sendConfiguration(configuration: Configuration) {
        val night = configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
        ComposeRsNative.setConfiguration(
            host,
            configuration.screenWidthDp,
            configuration.screenHeightDp,
            configuration.orientation == Configuration.ORIENTATION_LANDSCAPE,
            configuration.fontScale,
            resources.displayMetrics.density,
            night,
        )
    }

    // ---- Saved state ---------------------------------------------------------------------

    override fun onSaveInstanceState(): Parcelable {
        val bundle = Bundle()
        bundle.putParcelable(SUPER_STATE, super.onSaveInstanceState())
        val state = if (host != 0L) ComposeRsNative.saveState(host) else savedState
        if (state != null) bundle.putByteArray(COMPOSE_RS_STATE, state)
        return bundle
    }

    override fun onRestoreInstanceState(state: Parcelable?) {
        if (state !is Bundle) {
            super.onRestoreInstanceState(state)
            return
        }
        @Suppress("DEPRECATION")
        super.onRestoreInstanceState(state.getParcelable(SUPER_STATE))
        val restored = state.getByteArray(COMPOSE_RS_STATE) ?: return
        savedState = restored
        // Already composed (restored late): composed again from the saved state.
        if (host != 0L) {
            cancelFrames()
            ComposeRsNative.destroy(host)
            host = 0L
            drawing = null
            measurer.clear()
            replayer.clear()
            create()
            invalidate()
        }
    }

    // ---- Back ----------------------------------------------------------------------------

    private fun updateBack(enabled: Boolean) {
        if (enabled == backEnabled) return
        backEnabled = enabled
        val registration = back ?: BackRegistration.create(this) { handleBack() }.also { back = it }
        registration.setEnabled(enabled)
    }

    private fun handleBack() {
        if (host != 0L && ComposeRsNative.onBackPressed(host)) {
            ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
            return
        }
        // The handler went in the meantime: back goes on, to the activity.
        backEnabled = false
        back?.passOn()
    }

    // ---- Insets, properties and events ----------------------------------------------------

    private fun updateInsets(windowInsets: WindowInsets) {
        fun put(kind: Int, left: Int, top: Int, right: Int, bottom: Int) {
            insets[kind * 4] = left
            insets[kind * 4 + 1] = top
            insets[kind * 4 + 2] = right
            insets[kind * 4 + 3] = bottom
        }
        if (Build.VERSION.SDK_INT >= 30) {
            val kinds = intArrayOf(
                WindowInsets.Type.statusBars(),
                WindowInsets.Type.navigationBars(),
                WindowInsets.Type.captionBar(),
                WindowInsets.Type.ime(),
                WindowInsets.Type.displayCutout(),
            )
            for ((kind, type) in kinds.withIndex()) {
                val value = windowInsets.getInsets(type)
                put(kind, value.left, value.top, value.right, value.bottom)
            }
        } else {
            // API 29: the system windows' insets, the status bar at the top.
            @Suppress("DEPRECATION")
            val system = windowInsets.systemWindowInsets
            put(0, 0, system.top, 0, 0)
            put(1, system.left, 0, system.right, system.bottom)
            val cutout = windowInsets.displayCutout
            put(4, cutout?.safeInsetLeft ?: 0, cutout?.safeInsetTop ?: 0, cutout?.safeInsetRight ?: 0, cutout?.safeInsetBottom ?: 0)
        }
        if (host != 0L) {
            ComposeRsNative.setWindowInsets(host, insets)
            requestFrame()
        }
    }

    /**
     * Sets the property [name] for the content (`LOCAL_HOST_PROPS`), recomposing its readers.
     * Any thread: from another, it's posted to the main thread.
     */
    fun setProp(name: String, value: String) = putProp(name, value)

    fun setProp(name: String, value: Double) = putProp(name, value)

    fun setProp(name: String, value: Int) = putProp(name, value.toDouble())

    fun setProp(name: String, value: Boolean) = putProp(name, value)

    /** Removes the property [name]. On the main thread (or posted to it). */
    fun removeProp(name: String) {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            mainHandler.post { removeProp(name) }
            return
        }
        props.remove(name)
        if (host != 0L) ComposeRsNative.setProp(host, name, 0, null, 0.0)
    }

    /** A property set from another thread is posted to the main thread, where Rust runs. */
    private fun putProp(name: String, value: Any) {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            mainHandler.post { putProp(name, value) }
            return
        }
        props[name] = value
        if (host != 0L) sendProp(name, value)
    }

    private fun sendProp(name: String, value: Any) {
        when (value) {
            is String -> ComposeRsNative.setProp(host, name, 1, value, 0.0)
            is Double -> ComposeRsNative.setProp(host, name, 2, null, value)
            is Boolean -> ComposeRsNative.setProp(host, name, 3, null, if (value) 1.0 else 0.0)
        }
    }

    /** Receives the events the content sends (`LOCAL_HOST_EVENTS.current().send(name, payload)`). */
    fun setOnEventListener(listener: OnEventListener?) {
        eventListener = listener
    }

    /**
     * An event from the content (Rust calls it, in the middle of a call into Rust): delivered
     * from the main looper once that call returned, so the listener may call into the view
     * (set a property, detach it) without re-entering Rust.
     */
    fun dispatchHostEvent(name: String, payload: String) {
        mainHandler.post { eventListener?.onEvent(name, payload) }
    }

    /**
     * Asks for a frame (Rust calls it when the app's state changes). While the window is hidden
     * (the activity stopped, the screen off), none: the frame is noted, and made when it shows
     * again ([onWindowVisibilityChanged]), as Compose pauses its frame clock when stopped (D342).
     */
    fun requestFrame() {
        if (frameRequested || host == 0L) return
        if (windowVisibility != VISIBLE) {
            frameWanted = true
            return
        }
        frameRequested = true
        Choreographer.getInstance().postFrameCallback(frameCallback)
    }

    /**
     * Hidden, the window asks for no frame: an animation would otherwise run a frame on every
     * vsync, drawing nothing. Shown again, the frame it missed (state changed, animations) is
     * made in the traversal that shows the window, after its layout and before it draws, so it
     * shows the content as of now; animations go on from the frame's time, as their clock is
     * the time itself.
     */
    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        if (host == 0L) return
        if (visibility == VISIBLE) {
            if (frameWanted) {
                frameWanted = false
                // Asked for as any frame, and made sooner, before the traversal showing the
                // window draws, if one does (it dispatched this before its layout: a size changed
                // while hidden is applied first).
                requestFrame()
                if (!showFramePending) {
                    showFramePending = true
                    viewTreeObserver.addOnPreDrawListener(showFrame)
                }
            }
            // Prefetch work left when the window hid.
            if (prefetchLeft) {
                prefetchLeft = false
                schedulePrefetch()
            }
        } else {
            if (frameRequested) {
                Choreographer.getInstance().removeFrameCallback(frameCallback)
                frameRequested = false
                frameWanted = true
            }
            // A hidden window prefetches nothing (prefetch() checks, too).
            Choreographer.getInstance().removeFrameCallback(prefetchFrameCallback)
            removeCallbacks(prefetchRunnable)
            if (prefetchScheduled) prefetchLeft = true
            prefetchScheduled = false
            idleTimesWithoutTime = 0
        }
    }

    /** The frame missed while hidden, made before the traversal that shows the window draws. */
    private var showFramePending = false

    /** Prefetch work was left when the window hid: scheduled again when it shows. */
    private var prefetchLeft = false
    private val showFrame = object : ViewTreeObserver.OnPreDrawListener {
        override fun onPreDraw(): Boolean {
            viewTreeObserver.removeOnPreDrawListener(this)
            showFramePending = false
            if (host != 0L && frameRequested) {
                Choreographer.getInstance().removeFrameCallback(frameCallback)
                frame()
            }
            return true
        }
    }

    /** No frame, prefetch or frame on show is waiting any more (the host goes). */
    private fun cancelFrames() {
        val choreographer = Choreographer.getInstance()
        choreographer.removeFrameCallback(frameCallback)
        frameRequested = false
        frameWanted = false
        choreographer.removeFrameCallback(prefetchFrameCallback)
        removeCallbacks(prefetchRunnable)
        prefetchScheduled = false
        prefetchLeft = false
        idleTimesWithoutTime = 0
        cancelShowFrame()
    }

    private fun cancelShowFrame() {
        if (showFramePending) {
            showFramePending = false
            viewTreeObserver.removeOnPreDrawListener(showFrame)
        }
    }

    private fun frame() {
        frameRequested = false
        // Any frame is the one missed while hidden (if the traversal showing the window ended
        // before drawing, the frame callback made it: its listener mustn't make the next one).
        cancelShowFrame()
        if (host == 0L) return
        val result = ComposeRsNative.frame(host)
        if (result and Frame.DREW != 0L) invalidate()
        if (result and Frame.AGAIN != 0L) requestFrame()
        if (result and Frame.PREFETCH != 0L) schedulePrefetch()
        updateBack(result and Frame.BACK != 0L)
        ComposeRsRuntime.schedule(Frame.nextTimer(result))
        updateTextInput(result and Frame.TEXT_INPUT != 0L, result and Frame.RESET_INPUT != 0L)
        // What the content asked of the keyboard (after the frame's events, which woke it).
        if (result and Frame.SHOW_KEYBOARD != 0L && textInput) {
            requestFocus()
            inputMethods.showSoftInput(this, 0)
        } else if (result and Frame.HIDE_KEYBOARD != 0L) {
            inputMethods.hideSoftInputFromWindow(windowToken, 0)
        }
        if (result and Frame.SEMANTICS != 0L && accessibilityManager.isEnabled && !semanticsChangePosted) {
            semanticsChangePosted = true
            postDelayed(semanticsChanged, 100)
        }
    }

    /** Prefetches once the frame is done (a message after the Choreographer's callbacks). */
    private fun schedulePrefetch() {
        if (prefetchScheduled) return
        prefetchScheduled = true
        post(prefetchRunnable)
    }

    private fun prefetch() {
        // A hidden window draws no frames to prepare for, as Compose's.
        if (host == 0L || windowVisibility != VISIBLE) {
            if (host != 0L) prefetchLeft = true
            prefetchScheduled = false
            idleTimesWithoutTime = 0
            return
        }
        // Until the next frame: the vsync after the last frame's, found from its frame time and
        // start, as a frame time shifted by buffer stuffing recovery would leave no time at all
        // (D340). The shifted time itself is the animations', not changed here.
        val refreshRate = display?.refreshRate?.coerceAtLeast(30f) ?: 60f
        val interval = (1_000_000_000 / refreshRate).toLong()
        // (`display.refreshRate`: right after a change of rate, a frame time from before it may
        // put the next frame up to an interval late, once.)
        val nextFrameNanos = ComposeRsNative.nextFrameNanos(frameTimeNanos, interval, frameStartedNanos)
        if (nextFrameNanos - System.nanoTime() <= RECORD_MARGIN_NANOS) {
            // No time, or less than the margin, which no step starts in and which would only
            // lower Rust's estimates (the frame ran past the next vsync, or came from a layout,
            // not a callback): tried after the next frame, which gives a fresh time; a few times at
            // most, then the next frame with prefetch work schedules it again, rather than
            // asking for every vsync for good.
            if (++idleTimesWithoutTime > MOST_IDLE_TIMES_WITHOUT_TIME) {
                idleTimesWithoutTime = 0
                prefetchScheduled = false
            } else {
                Choreographer.getInstance().postFrameCallback(prefetchFrameCallback)
            }
            return
        }
        idleTimesWithoutTime = 0
        // Recording stops a margin before it, as Rust's prefetch steps do: its estimate is an
        // average, and a recording running past the vsync delays the frame (D341).
        val recordDeadline = nextFrameNanos - RECORD_MARGIN_NANOS
        // Paragraphs the last idle time laid out but had no time to record go first, then the
        // groups of the items it measured: their item is the next one shown.
        var workLeft = measurer.recordLaidOut(recordDeadline) ||
            recordAhead(recordDeadline, firstInIdleTime = measurer.recordedLast == 0)
        if (!workLeft) {
            val available = nextFrameNanos - System.nanoTime()
            workLeft = available <= RECORD_MARGIN_NANOS || prefetchAndRecord(available, recordDeadline)
        }
        if (!workLeft) {
            prefetchScheduled = false
        } else {
            Choreographer.getInstance().postFrameCallback(prefetchFrameCallback)
        }
    }

    /**
     * Prefetches for [available] nanoseconds, then records the paragraphs it laid out and the
     * groups of the items it measured until [deadlineNanos], so the frame showing their item
     * only draws them. Returns whether work is left (prefetching, or paragraphs or groups to
     * record).
     */
    private fun prefetchAndRecord(available: Long, deadlineNanos: Long): Boolean {
        measurer.collectingLaidOut = true
        val prefetchLeft = try {
            ComposeRsNative.prefetch(host, available)
        } finally {
            measurer.collectingLaidOut = false
        }
        val recordingLeft = measurer.recordLaidOut(deadlineNanos) || recordAhead(deadlineNanos, firstInIdleTime = false)
        return prefetchLeft || recordingLeft
    }

    /**
     * Draws ahead the items a prefetch measured, oldest first, while one is expected to end
     * before [deadlineNanos] ([drawAheadCost], as Rust's prefetch steps: D341): Rust draws an
     * item's groups and writes their records, the replayer records their render nodes, so the
     * frame placing the item draws them by reference without recording (D350). [firstInIdleTime]: nothing
     * ran yet in this idle time. A step that doesn't fit after other work waits for the next idle
     * time; one that doesn't fit an idle time of its own gives up the items waiting (the frames
     * draw them), as Rust's prefetch gives up until the next scroll, rather than asking for every
     * vsync until a lowered estimate lets one start and overrun it. Returns whether some are
     * left, for the next idle time.
     */
    private fun recordAhead(deadlineNanos: Long, firstInIdleTime: Boolean): Boolean {
        if (!ComposeRsNative.hasDrawAhead(host)) return false
        Trace.beginSection("compose-rs: record ahead")
        try {
            var first = firstInIdleTime
            while (true) {
                val start = System.nanoTime()
                if (start + drawAheadCost.estimate() >= deadlineNanos) {
                    drawAheadCost.missed(first)
                    if (!first) return true
                    ComposeRsNative.cancelDrawAhead(host)
                    return false
                }
                first = false
                val taken = ComposeRsNative.drawAhead(host)
                val length = (taken and 0xFFFFFFFFL).toInt()
                if (length > 0) {
                    if (taken and ComposeRsNative.NEW_DRAWING_BUFFER != 0L || drawing == null) {
                        drawing = ComposeRsNative.drawingBuffer(host)
                    }
                    val buffer = drawing ?: return false
                    buffer.limit(length)
                    // Timed only when recorded: a step with nothing recorded is no sample.
                    if (replayer.recordAhead(buffer)) drawAheadCost.timed(System.nanoTime() - start)
                }
                if (taken and ComposeRsNative.MORE_AHEAD == 0L) return false
            }
        } finally {
            Trace.endSection()
        }
    }

    // ---- Accessibility -------------------------------------------------------------------

    override fun getAccessibilityNodeProvider(): AccessibilityNodeProvider = accessibility

    override fun dispatchHoverEvent(event: MotionEvent): Boolean {
        if (accessibilityManager.isTouchExplorationEnabled && accessibility.hover(event)) return true
        return super.dispatchHoverEvent(event)
    }

    internal fun semantics() = if (host != 0L) ComposeRsNative.semantics(host) else null

    internal fun performSemanticsAction(id: Int, action: Int, value: Float): Boolean =
        host != 0L && ComposeRsNative.performAction(host, id, action, value)

    override fun onDraw(canvas: Canvas) {
        if (host == 0L) return
        val taken = ComposeRsNative.takeDrawing(host)
        if (taken and ComposeRsNative.NEW_DRAWING_BUFFER != 0L || drawing == null) {
            drawing = ComposeRsNative.drawingBuffer(host)
        }
        val drawing = drawing ?: return
        drawing.limit((taken and 0xFFFFFFFFL).toInt())
        Trace.beginSection("compose-rs: replay")
        replayer.replay(canvas, drawing)
        Trace.endSection()
    }

    // ---- Touch ---------------------------------------------------------------------------

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (host == 0L) return false
        val now = SystemClock.uptimeMillis()
        fun age(time: Long) = (now - time).coerceAtLeast(0L) * 1_000_000L
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                val index = event.actionIndex
                ComposeRsNative.touch(host, 0, event.getPointerId(index), event.getX(index), event.getY(index), age(event.eventTime))
            }
            MotionEvent.ACTION_MOVE -> {
                // The samples batched since the last event, then the latest, at their times: a
                // fling's velocity comes from all of them.
                for (sample in 0 until event.historySize) {
                    val time = age(event.getHistoricalEventTime(sample))
                    for (index in 0 until event.pointerCount) {
                        ComposeRsNative.touch(
                            host,
                            1,
                            event.getPointerId(index),
                            event.getHistoricalX(index, sample),
                            event.getHistoricalY(index, sample),
                            time,
                        )
                    }
                }
                for (index in 0 until event.pointerCount) {
                    ComposeRsNative.touch(host, 1, event.getPointerId(index), event.getX(index), event.getY(index), age(event.eventTime))
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> {
                val index = event.actionIndex
                ComposeRsNative.touch(host, 2, event.getPointerId(index), event.getX(index), event.getY(index), age(event.eventTime))
            }
            MotionEvent.ACTION_CANCEL -> {
                for (index in 0 until event.pointerCount) {
                    ComposeRsNative.touch(host, 3, event.getPointerId(index), 0f, 0f, 0L)
                }
            }
        }
        // A timeout a gesture started (a long press): a tick when it ends.
        ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
        return true
    }

    /** A mouse wheel (or a trackpad's scroll), in pixels, as Compose's scroll events. */
    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        if (host != 0L && event.actionMasked == MotionEvent.ACTION_SCROLL) {
            val step = 64f * resources.displayMetrics.density
            ComposeRsNative.scroll(
                host,
                event.x,
                event.y,
                -event.getAxisValue(MotionEvent.AXIS_HSCROLL) * step,
                -event.getAxisValue(MotionEvent.AXIS_VSCROLL) * step,
            )
            return true
        }
        return super.onGenericMotionEvent(event)
    }

    // ---- Keys and text -------------------------------------------------------------------

    private fun keyCode(code: Int): Int = when (code) {
        KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> 1
        KeyEvent.KEYCODE_SPACE -> 2
        KeyEvent.KEYCODE_TAB -> 3
        KeyEvent.KEYCODE_DEL -> 4
        KeyEvent.KEYCODE_FORWARD_DEL -> 5
        KeyEvent.KEYCODE_ESCAPE -> 6
        KeyEvent.KEYCODE_DPAD_LEFT -> 7
        KeyEvent.KEYCODE_DPAD_RIGHT -> 8
        KeyEvent.KEYCODE_DPAD_UP -> 9
        KeyEvent.KEYCODE_DPAD_DOWN -> 10
        KeyEvent.KEYCODE_MOVE_HOME -> 11
        KeyEvent.KEYCODE_MOVE_END -> 12
        KeyEvent.KEYCODE_PAGE_UP -> 13
        KeyEvent.KEYCODE_PAGE_DOWN -> 14
        else -> 0
    }

    internal fun sendKey(event: KeyEvent): Boolean {
        if (host == 0L) return false
        val down = event.action == KeyEvent.ACTION_DOWN
        if (event.action == KeyEvent.ACTION_MULTIPLE) return false
        // The key's character without Ctrl, Alt or Meta (which map to none), so a shortcut's
        // key (Ctrl+C) is known; Rust types no text while they're held.
        val shortcutMask = KeyEvent.META_CTRL_MASK or KeyEvent.META_ALT_MASK or KeyEvent.META_META_MASK
        val consumed = ComposeRsNative.key(
            host,
            keyCode(event.keyCode),
            event.getUnicodeChar(event.metaState and shortcutMask.inv()),
            down,
            event.metaState,
        )
        ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
        return consumed
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        // The back key, where no dispatcher takes back (no AndroidX, no predictive back): the
        // content's, on its release, while a handler is enabled.
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            if (back?.handlesKeys == true) {
                event.startTracking()
                return true
            }
            return super.onKeyDown(keyCode, event)
        }
        return sendKey(event) || super.onKeyDown(keyCode, event)
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            if (back?.handlesKeys == true && event.isTracking && !event.isCanceled) {
                handleBack()
                return true
            }
            return super.onKeyUp(keyCode, event)
        }
        return sendKey(event) || super.onKeyUp(keyCode, event)
    }

    internal fun commitText(text: CharSequence) {
        if (host != 0L) ComposeRsNative.text(host, text.toString())
    }

    internal fun composeText(text: CharSequence, cursor: Int) {
        if (host != 0L) ComposeRsNative.preedit(host, text.toString(), cursor)
    }

    override fun onRtlPropertiesChanged(layoutDirection: Int) {
        super.onRtlPropertiesChanged(layoutDirection)
        if (host != 0L) {
            ComposeRsNative.setRtl(host, layoutDirection == LAYOUT_DIRECTION_RTL)
            requestFrame()
        }
    }

    private fun updateTextInput(active: Boolean, reset: Boolean) {
        if (active != textInput) {
            textInput = active
            if (active) {
                requestFocus()
                inputMethods.restartInput(this)
                inputMethods.showSoftInput(this, 0)
            } else {
                inputMethods.hideSoftInputFromWindow(windowToken, 0)
                inputMethods.restartInput(this)
            }
        } else if (reset && active) {
            inputMethods.restartInput(this)
        }
    }

    override fun onCheckIsTextEditor(): Boolean = textInput

    /**
     * The focused field's keyboard: its type, action key, capitalization, auto-correction and
     * lines, as Compose's `EditorInfo.update` (computed in Rust, `input_method.rs`).
     */
    override fun onCreateInputConnection(info: EditorInfo): InputConnection? {
        if (!textInput || host == 0L) return null
        val options = ComposeRsNative.textInputOptions(host)
        info.inputType = options.toInt()
        info.imeOptions = (options shr 32).toInt()
        return ComposeRsInputConnection(this)
    }

    /** The keyboard's action key: the field's action for it (its `KeyboardActions`). */
    internal fun performEditorAction(action: Int) {
        if (host == 0L) return
        ComposeRsNative.performEditorAction(host, action)
        ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
    }

    // ---- The clipboard (Rust calls these) ------------------------------------------------

    fun clipboardText(): String? {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        return clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString()
    }

    fun setClipboardText(text: String) {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        clipboard.setPrimaryClip(ClipData.newPlainText("text", text))
    }

    // ---- Links and haptics (Rust calls these: `LOCAL_URI_HANDLER`, `LOCAL_HAPTIC_FEEDBACK`) --

    /**
     * Opens [uri] with an `ACTION_VIEW` intent, as Compose's `AndroidUriHandler`; Rust checked
     * its scheme (`PlatformUriHandler`). Rust calls it in the middle of a call into Rust: posted
     * to the main looper (D226). A failure (nothing opens it, a `file:` URI the app may not
     * expose, a permission) is logged, never thrown at the looper.
     */
    fun openUri(uri: String) {
        mainHandler.post {
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(uri))
            if (findActivity(context) == null) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            try {
                context.startActivity(intent)
            } catch (exception: RuntimeException) {
                val scheme = Uri.parse(uri).scheme
                Log.w("compose-rs", "not opening a $scheme: URI: ${exception.javaClass.simpleName}")
            }
        }
    }

    /** The activity [context] wraps, if any (an `AndroidView`'s context is often a wrapper). */
    private fun findActivity(context: Context): android.app.Activity? {
        var current: Context? = context
        while (current is android.content.ContextWrapper) {
            if (current is android.app.Activity) return current
            current = current.baseContext
        }
        return null
    }

    /**
     * Performs the haptic feedback of [type] (`HapticFeedbackType`'s index in Compose's order),
     * with Compose's fallbacks before the API level that has the constant. Posted, as [openUri].
     */
    fun performHapticFeedbackType(type: Int) {
        val constant = hapticFeedbackConstant(type) ?: return
        mainHandler.post { performHapticFeedback(constant) }
    }

    private fun hapticFeedbackConstant(type: Int): Int? {
        val sdk = Build.VERSION.SDK_INT
        return when (type) {
            0 -> HapticFeedbackConstants.LONG_PRESS
            1 -> HapticFeedbackConstants.TEXT_HANDLE_MOVE
            2 -> if (sdk >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY
            3 -> HapticFeedbackConstants.CONTEXT_CLICK
            4 -> if (sdk >= 30) HapticFeedbackConstants.GESTURE_END else HapticFeedbackConstants.CONTEXT_CLICK
            5 -> if (sdk >= 34) HapticFeedbackConstants.GESTURE_THRESHOLD_ACTIVATE else HapticFeedbackConstants.CONTEXT_CLICK
            6 -> HapticFeedbackConstants.KEYBOARD_TAP
            7 -> if (sdk >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
            8 -> if (sdk >= 34) HapticFeedbackConstants.SEGMENT_FREQUENT_TICK else HapticFeedbackConstants.CLOCK_TICK
            9 -> if (sdk >= 34) HapticFeedbackConstants.SEGMENT_TICK else HapticFeedbackConstants.CLOCK_TICK
            10 -> if (sdk >= 34) HapticFeedbackConstants.TOGGLE_OFF else HapticFeedbackConstants.VIRTUAL_KEY_RELEASE
            11 -> if (sdk >= 34) HapticFeedbackConstants.TOGGLE_ON else HapticFeedbackConstants.VIRTUAL_KEY
            12 -> HapticFeedbackConstants.VIRTUAL_KEY
            else -> null
        }
    }

    private companion object {
        const val SUPER_STATE = "rs.compose.android.super"
        const val COMPOSE_RS_STATE = "rs.compose.android.state"

        /** What recording paragraphs while idle leaves before the next frame, as Rust's prefetch. */
        const val RECORD_MARGIN_NANOS = 500_000L

        /** Idle times in a row without time before [prefetch] waits for a frame (D340). */
        const val MOST_IDLE_TIMES_WITHOUT_TIME = 3
    }
}

/**
 * The soft keyboard's edits, as compose-rs takes them: committed text, the composing text (shown
 * in place, underlined), deletions as Backspace and Delete keys. The field's text stays on the
 * Rust side, so the keyboard sees none (as a terminal's input connection).
 */
internal class ComposeRsInputConnection(private val view: ComposeRsView) : BaseInputConnection(view, false) {
    private var composing: CharSequence = ""

    override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
        if (composing.isNotEmpty()) {
            composing = ""
            view.composeText("", -1)
        }
        view.commitText(text)
        return true
    }

    override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
        composing = text
        view.composeText(text, -1)
        return true
    }

    override fun finishComposingText(): Boolean {
        if (composing.isNotEmpty()) {
            val text = composing
            composing = ""
            view.composeText("", -1)
            view.commitText(text)
        }
        return true
    }

    override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
        repeat(beforeLength) { key(KeyEvent.KEYCODE_DEL) }
        repeat(afterLength) { key(KeyEvent.KEYCODE_FORWARD_DEL) }
        return true
    }

    /**
     * The connection closes (the keyboard restarts input, focus moves): what it was composing
     * is the field's to commit or drop (it already did), not to commit again.
     */
    override fun closeConnection() {
        composing = ""
        super.closeConnection()
    }

    override fun sendKeyEvent(event: KeyEvent): Boolean {
        view.sendKey(event)
        return true
    }

    /**
     * The keyboard's action key (Done, Search, Next…), as Compose's `RecordingInputConnection`:
     * always taken. (Enter, which keyboards send as a key, performs a single line's action in
     * the field itself.)
     */
    override fun performEditorAction(editorAction: Int): Boolean {
        view.performEditorAction(editorAction)
        return true
    }

    private fun key(code: Int) {
        view.sendKey(KeyEvent(KeyEvent.ACTION_DOWN, code))
        view.sendKey(KeyEvent(KeyEvent.ACTION_UP, code))
    }
}
